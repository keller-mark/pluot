// A CurveLayer that renders SVG-like vector paths (lines, cubic/quadratic Bezier
// curves and elliptical arcs) as stroked curves.
//
// We use line_layer.rs and its shader (shaders/line_layer.wgsl) as a reference.
// In contrast to LineLayer, CurveLayer accepts a sequence of path drawing commands
// (the post-parsed form of an SVG path string such as the violin-plot outline
// "M49.39,230.8L48.419,228.65C47.448,226.499,...Z"). On the CPU we flatten every
// command into a list of cubic Bezier segments (lines and quadratics become cubics;
// arcs become one or more cubics). The flattened control points are uploaded to the
// GPU, and the vertex shader evaluates each Bezier and extrudes the resulting
// sub-segments into quads. This keeps the bulk of the per-curve work in the vertex
// shader so rendering stays efficient and scalable.
//
// Both pixel and data units are supported (matching LineLayer); we assume the
// position and size values share the same units.

// TODO: Rename line_width to stroke_width.
// TODO: support both stroked and filled modes.
// TODO: Support separate stroke/fill colors and opacity values.

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::render_traits::{DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, PickableLayer, PreparedLayer, ViewParams, AspectRatioMode, AspectRatioAlignmentMode, UnitsMode, MarginParams};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::render_types::GpuContext;
use crate::wgpu;
use crate::two::shapes::{TwoColor, TwoElement, TwoGroup, TwoPath};
use crate::two::svg::{update_svg, SvgContext};
use crate::positioning::get_point_position;

/// A single drawing command, the post-parsed form of an SVG path segment.
/// All coordinates are absolute (relative SVG commands should be resolved before
/// being passed in). This mirrors the subset of SVG path commands we support.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PathCommand {
    /// Start a new sub-path at (x, y). Corresponds to "M".
    MoveTo { x: f32, y: f32 },
    /// Straight line from the current point to (x, y). Corresponds to "L".
    LineTo { x: f32, y: f32 },
    /// Cubic Bezier with control points (x1, y1), (x2, y2) and endpoint (x, y).
    /// Corresponds to "C".
    CubicTo { x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32 },
    /// Quadratic Bezier with control point (x1, y1) and endpoint (x, y).
    /// Corresponds to "Q".
    QuadraticTo { x1: f32, y1: f32, x: f32, y: f32 },
    /// Elliptical arc to endpoint (x, y). Corresponds to "A".
    ArcTo {
        rx: f32,
        ry: f32,
        /// Rotation of the ellipse's x-axis, in degrees.
        #[serde(default)]
        x_axis_rotation: f32,
        large_arc: bool,
        sweep: bool,
        x: f32,
        y: f32,
    },
    /// Close the current sub-path with a straight line back to its start.
    /// Corresponds to "Z".
    Close,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct CurveLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub line_width: f32,
    pub line_width_unit_mode: UnitsMode,
    pub model_matrix: Option<[f32; 16]>, // Column-major 4x4 matrix

    /// The path to draw, as a sequence of absolute drawing commands.
    pub commands: Arc<Vec<PathCommand>>,
    /// Number of straight sub-segments used to approximate each cubic Bezier
    /// segment. Higher values produce smoother curves at the cost of more
    /// instanced draws.
    pub subdivisions: u32,
    /// RGBA stroke color in [0, 1]. Defaults to opaque black.
    pub color: [f32; 4],
}

impl Default for CurveLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            line_width: 1.0,
            line_width_unit_mode: UnitsMode::Pixels,
            model_matrix: None,
            commands: Arc::new(vec![]),
            subdivisions: 32,
            color: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

/// A flattened cubic Bezier segment: control points p0, p1, p2, p3.
type CubicSegment = [(f32, f32); 4];

/// Convert a quadratic Bezier (p0, control, p1) into an equivalent cubic Bezier.
fn quadratic_to_cubic(p0: (f32, f32), c: (f32, f32), p1: (f32, f32)) -> CubicSegment {
    let c1 = (
        p0.0 + 2.0 / 3.0 * (c.0 - p0.0),
        p0.1 + 2.0 / 3.0 * (c.1 - p0.1),
    );
    let c2 = (
        p1.0 + 2.0 / 3.0 * (c.0 - p1.0),
        p1.1 + 2.0 / 3.0 * (c.1 - p1.1),
    );
    [p0, c1, c2, p1]
}

/// Signed angle (radians) between vectors u and v.
fn vector_angle(ux: f64, uy: f64, vx: f64, vy: f64) -> f64 {
    let dot = ux * vx + uy * vy;
    let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
    let mut ang = (dot / len).clamp(-1.0, 1.0).acos();
    if ux * vy - uy * vx < 0.0 {
        ang = -ang;
    }
    ang
}

/// Convert an elliptical arc (SVG "A" command) into a sequence of cubic Bezier
/// segments, using the endpoint-to-center parameterization from the SVG spec
/// implementation notes (https://www.w3.org/TR/SVG/implnote.html#ArcImplementationNotes).
fn arc_to_cubics(
    start: (f32, f32),
    rx: f32,
    ry: f32,
    x_axis_rotation_deg: f32,
    large_arc: bool,
    sweep: bool,
    end: (f32, f32),
) -> Vec<CubicSegment> {
    let (x1, y1) = (start.0 as f64, start.1 as f64);
    let (x2, y2) = (end.0 as f64, end.1 as f64);

    // Degenerate radii (or a zero-length arc) collapse to a straight line.
    let mut rx = (rx as f64).abs();
    let mut ry = (ry as f64).abs();
    if rx < 1e-12 || ry < 1e-12 || (x1 == x2 && y1 == y2) {
        return vec![line_to_cubic(start, end)];
    }

    let phi = (x_axis_rotation_deg as f64).to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();

    // Step 1: compute (x1', y1') in the rotated coordinate system.
    let dx = (x1 - x2) / 2.0;
    let dy = (y1 - y2) / 2.0;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;

    // Correct out-of-range radii.
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }

    // Step 2: compute the center (cx', cy') in the rotated system.
    let rx2 = rx * rx;
    let ry2 = ry * ry;
    let x1p2 = x1p * x1p;
    let y1p2 = y1p * y1p;
    let mut num = rx2 * ry2 - rx2 * y1p2 - ry2 * x1p2;
    if num < 0.0 {
        num = 0.0;
    }
    let denom = rx2 * y1p2 + ry2 * x1p2;
    let mut coef = (num / denom).sqrt();
    if large_arc == sweep {
        coef = -coef;
    }
    let cxp = coef * rx * y1p / ry;
    let cyp = -coef * ry * x1p / rx;

    // Step 3: compute the center (cx, cy) in the original system.
    let cx = cos_phi * cxp - sin_phi * cyp + (x1 + x2) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (y1 + y2) / 2.0;

    // Step 4: compute the start angle and sweep angle.
    let theta1 = vector_angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dtheta = vector_angle(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    let tau = std::f64::consts::PI * 2.0;
    if !sweep && dtheta > 0.0 {
        dtheta -= tau;
    } else if sweep && dtheta < 0.0 {
        dtheta += tau;
    }

    // Split into segments each spanning at most 90 degrees for good accuracy.
    let n_segs = (dtheta.abs() / (std::f64::consts::FRAC_PI_2))
        .ceil()
        .max(1.0) as usize;
    let delta = dtheta / n_segs as f64;
    // Control-point distance factor for approximating an arc of `delta` with a cubic.
    let alpha = (4.0 / 3.0) * (delta / 4.0).tan();

    // Point on the ellipse at parameter angle `t`.
    let point = |t: f64| -> (f64, f64) {
        let (st, ct) = t.sin_cos();
        (
            cos_phi * rx * ct - sin_phi * ry * st + cx,
            sin_phi * rx * ct + cos_phi * ry * st + cy,
        )
    };
    // Derivative (tangent) of the ellipse at parameter angle `t`.
    let deriv = |t: f64| -> (f64, f64) {
        let (st, ct) = t.sin_cos();
        (
            -cos_phi * rx * st - sin_phi * ry * ct,
            -sin_phi * rx * st + cos_phi * ry * ct,
        )
    };

    let mut segments = Vec::with_capacity(n_segs);
    let mut angle = theta1;
    let mut p0 = point(angle);
    let mut d0 = deriv(angle);
    for _ in 0..n_segs {
        let angle2 = angle + delta;
        let p3 = point(angle2);
        let d3 = deriv(angle2);
        let p1 = (p0.0 + alpha * d0.0, p0.1 + alpha * d0.1);
        let p2 = (p3.0 - alpha * d3.0, p3.1 - alpha * d3.1);
        segments.push([
            (p0.0 as f32, p0.1 as f32),
            (p1.0 as f32, p1.1 as f32),
            (p2.0 as f32, p2.1 as f32),
            (p3.0 as f32, p3.1 as f32),
        ]);
        angle = angle2;
        p0 = p3;
        d0 = d3;
    }
    segments
}

/// Represent a straight line as a (degenerate) cubic Bezier so it can share the
/// same evaluation path on the GPU. With the control points at the endpoints, the
/// traced geometry is exactly the line segment.
fn line_to_cubic(p0: (f32, f32), p1: (f32, f32)) -> CubicSegment {
    [p0, p0, p1, p1]
}

/// Flatten a sequence of drawing commands into per-sub-path lists of cubic Bezier
/// segments. A new sub-path begins at each MoveTo; Close adds a closing segment
/// back to the sub-path start.
fn commands_to_subpaths(commands: &[PathCommand]) -> Vec<Vec<CubicSegment>> {
    let mut subpaths: Vec<Vec<CubicSegment>> = Vec::new();
    let mut current: Vec<CubicSegment> = Vec::new();
    let mut cursor = (0.0f32, 0.0f32);
    let mut subpath_start = (0.0f32, 0.0f32);

    for command in commands {
        match *command {
            PathCommand::MoveTo { x, y } => {
                if !current.is_empty() {
                    subpaths.push(std::mem::take(&mut current));
                }
                cursor = (x, y);
                subpath_start = (x, y);
            }
            PathCommand::LineTo { x, y } => {
                current.push(line_to_cubic(cursor, (x, y)));
                cursor = (x, y);
            }
            PathCommand::CubicTo { x1, y1, x2, y2, x, y } => {
                current.push([cursor, (x1, y1), (x2, y2), (x, y)]);
                cursor = (x, y);
            }
            PathCommand::QuadraticTo { x1, y1, x, y } => {
                current.push(quadratic_to_cubic(cursor, (x1, y1), (x, y)));
                cursor = (x, y);
            }
            PathCommand::ArcTo { rx, ry, x_axis_rotation, large_arc, sweep, x, y } => {
                current.extend(arc_to_cubics(
                    cursor,
                    rx,
                    ry,
                    x_axis_rotation,
                    large_arc,
                    sweep,
                    (x, y),
                ));
                cursor = (x, y);
            }
            PathCommand::Close => {
                if cursor != subpath_start {
                    current.push(line_to_cubic(cursor, subpath_start));
                }
                cursor = subpath_start;
            }
        }
    }
    if !current.is_empty() {
        subpaths.push(current);
    }
    subpaths
}

/// Evaluate a cubic Bezier at parameter t in [0, 1]. Mirrors `cubic_bezier` in the shader.
fn eval_cubic(seg: &CubicSegment, t: f32) -> (f32, f32) {
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let t2 = t * t;
    let a = mt2 * mt;
    let b = 3.0 * mt2 * t;
    let c = 3.0 * mt * t2;
    let d = t2 * t;
    (
        seg[0].0 * a + seg[1].0 * b + seg[2].0 * c + seg[3].0 * d,
        seg[0].1 * a + seg[1].1 * b + seg[2].1 * c + seg[3].1 * d,
    )
}

pub struct CurveLayer {
    view_params: ViewParams,
    layer_params: CurveLayerParams,
    /// Flattened cubic segments grouped by sub-path (used for SVG rendering).
    subpaths: Vec<Vec<CubicSegment>>,
}

impl CurveLayer {
    pub fn new(
        view_params: ViewParams,
        layer_params: CurveLayerParams,
    ) -> Self {
        // Error if line_width_unit_mode is "data" when data_unit_mode is "pixels".
        if layer_params.line_width_unit_mode == UnitsMode::Data
            && (layer_params.data_unit_mode_x == UnitsMode::Pixels
                || layer_params.data_unit_mode_y == UnitsMode::Pixels)
        {
            panic!("line_width_unit_mode cannot be 'data' when data_unit_mode is 'pixels'");
        }
        let subpaths = commands_to_subpaths(&layer_params.commands);
        Self {
            view_params,
            layer_params,
            subpaths,
        }
    }

    /// Total number of cubic Bezier segments across all sub-paths.
    fn num_segments(&self) -> usize {
        self.subpaths.iter().map(|s| s.len()).sum()
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for CurveLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        PrepareResult {
            bailed_early: false,
        }
    }
}

#[derive(ShaderType, Debug)]
struct CurveLayerUniforms {
    layer_size: Vec2, // (layer_width, layer_height) in pixels
    camera_view: Mat4,
    data_unit_mode_x: u32, // 0 = pixels, 1 = data units
    data_unit_mode_y: u32, // 0 = pixels, 1 = data units
    line_width: f32,
    line_width_unit_mode: u32, // 0 = pixels, 1 = data units
    aspect_ratio_mode: u32, // 0 = ignore, 1 = contain, 2 = cover
    aspect_ratio_alignment_mode: u32, // 0 = center, 1 = start, 2 = end
    subdivisions: u32, // sub-segments per cubic Bezier segment
    model_matrix: Mat4,
    color: Vec4, // rgba stroke color
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for CurveLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let GpuContext { device, queue } = gpu_context;
        let Self { layer_params, view_params, subpaths } = self;

        let num_segments = self.num_segments();
        let subdivisions = layer_params.subdivisions.max(1);
        // Nothing to draw (empty path, or a path with only MoveTo/Close commands).
        if num_segments == 0 {
            return;
        }

        // Flatten control points into a tightly-packed f32 buffer: 4 vec2 (8 floats)
        // per cubic segment, matching `array<vec2<f32>>` (stride 8) in the shader.
        let mut control_points: Vec<f32> = Vec::with_capacity(num_segments * 8);
        for subpath in subpaths {
            for seg in subpath {
                for (x, y) in seg {
                    control_points.push(*x);
                    control_points.push(*y);
                }
            }
        }
        let control_points_bytes: &[u8] = bytemuck::cast_slice(&control_points);

        let control_points_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Curve Control Points Storage Buffer"),
            size: control_points_bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&control_points_buffer, 0, control_points_bytes);

        // Note: WGSL treats matrices as column-major.
        let camera_view = view_params.camera_view.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        // Use layer-specific bounds if not None, otherwise use the view's margins.
        let bounds = if layer_params.bounds.is_none() {
            &view_params.margins
        } else {
            &layer_params.bounds
        };

        let margin_top = if let Some(m) = &bounds { m.margin_top.unwrap_or(0.0) } else { 0.0 } as f64;
        let margin_right = if let Some(m) = &bounds { m.margin_right.unwrap_or(0.0) } else { 0.0 } as f64;
        let margin_bottom = if let Some(m) = &bounds { m.margin_bottom.unwrap_or(0.0) } else { 0.0 } as f64;
        let margin_left = if let Some(m) = &bounds { m.margin_left.unwrap_or(0.0) } else { 0.0 } as f64;

        let viewport_w = view_params.width as f32;
        let viewport_h = view_params.height as f32;

        let layer_w = viewport_w - (margin_left + margin_right) as f32;
        let layer_h = viewport_h - (margin_top + margin_bottom) as f32;

        let uniform_struct = CurveLayerUniforms {
            layer_size: Vec2::new(layer_w, layer_h),
            camera_view: Mat4::from_cols_array(&camera_view),
            data_unit_mode_x: match layer_params.data_unit_mode_x {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
            },
            data_unit_mode_y: match layer_params.data_unit_mode_y {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
            },
            line_width: layer_params.line_width,
            line_width_unit_mode: match layer_params.line_width_unit_mode {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
            },
            aspect_ratio_mode: match view_params.aspect_ratio_mode {
                AspectRatioMode::Ignore => 0,
                AspectRatioMode::Contain => 1,
                AspectRatioMode::Cover => 2,
            },
            aspect_ratio_alignment_mode: match view_params.aspect_ratio_alignment_mode {
                AspectRatioAlignmentMode::Center => 0,
                AspectRatioAlignmentMode::Start => 1,
                AspectRatioAlignmentMode::End => 2,
            },
            subdivisions,
            model_matrix: Mat4::from_cols_array(&layer_params.model_matrix.unwrap_or([
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ])),
            color: Vec4::from_array(layer_params.color),
        };

        let mut buffer = UniformBuffer::new(Vec::<u8>::new());
        buffer.write(&uniform_struct).unwrap();
        let uniform_bytes = buffer.into_inner();

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Curve Uniform Buffer"),
            size: uniform_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("CurveLayer BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("CurveLayer BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: control_points_buffer.as_entire_binding(),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/curve_layer.wgsl"));

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("CurveLayer PLD"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("CurveLayer RPD"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
            multiview_mask: None,
        });

        // Handle margins by adjusting viewport and scissor rect (see LineLayer).
        pass.set_viewport(
            margin_left as f32,
            margin_top as f32,
            viewport_w - (margin_left + margin_right) as f32,
            viewport_h - (margin_top + margin_bottom) as f32,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(
            margin_left as u32,
            margin_top as u32,
            (viewport_w - (margin_left + margin_right) as f32) as u32,
            (viewport_h - (margin_top + margin_bottom) as f32) as u32,
        );

        pass.set_pipeline(&render_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);

        // One instance per (segment, sub-segment); 4 vertices per instance (quad).
        let instance_count = (num_segments as u32) * subdivisions;
        pass.draw(0..4, 0..instance_count);
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for CurveLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for CurveLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let Self { layer_params, view_params, subpaths } = self;

        let camera_view = view_params.camera_view.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let bounds = if layer_params.bounds.is_none() {
            &view_params.margins
        } else {
            &layer_params.bounds
        };

        let margin_top = if let Some(m) = &bounds { m.margin_top.unwrap_or(0.0) } else { 0.0 } as f64;
        let margin_right = if let Some(m) = &bounds { m.margin_right.unwrap_or(0.0) } else { 0.0 } as f64;
        let margin_bottom = if let Some(m) = &bounds { m.margin_bottom.unwrap_or(0.0) } else { 0.0 } as f64;
        let margin_left = if let Some(m) = &bounds { m.margin_left.unwrap_or(0.0) } else { 0.0 } as f64;

        let viewport_w = view_params.width as f32;
        let viewport_h = view_params.height as f32;

        let layer_w = viewport_w - (margin_left + margin_right) as f32;
        let layer_h = viewport_h - (margin_top + margin_bottom) as f32;

        let model_matrix_raw: [f32; 16] = layer_params.model_matrix.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let subdivisions = layer_params.subdivisions.max(1);

        // Convert a model-space point to a pixel position within the layer area,
        // flipping Y to match the SVG coordinate system (as LineLayer does).
        let to_px = |x: f32, y: f32| -> (f64, f64) {
            let (px, py) = get_point_position(
                x,
                y,
                layer_w,
                layer_h,
                &camera_view,
                layer_params.data_unit_mode_x,
                layer_params.data_unit_mode_y,
                view_params.aspect_ratio_mode,
                view_params.aspect_ratio_alignment_mode,
                Some(&model_matrix_raw),
            );
            (px as f64, (layer_h - py) as f64)
        };

        let [r, g, b, a] = layer_params.color;
        let stroke = TwoColor::Rgba((
            (r * 255.0).round().clamp(0.0, 255.0) as u8,
            (g * 255.0).round().clamp(0.0, 255.0) as u8,
            (b * 255.0).round().clamp(0.0, 255.0) as u8,
            (a * 255.0).round().clamp(0.0, 255.0) as u8,
        ));

        // Emit one stroked TwoPath polyline per sub-path. We flatten each cubic
        // segment into `subdivisions` line segments (matching the GPU tessellation).
        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(subpaths.len());
        for subpath in subpaths {
            if subpath.is_empty() {
                continue;
            }
            let mut points: Vec<(f64, f64)> = Vec::new();
            // Start at the very first control point of the sub-path.
            let first = subpath[0][0];
            points.push(to_px(first.0, first.1));
            for seg in subpath {
                for step in 1..=subdivisions {
                    let t = step as f32 / subdivisions as f32;
                    let (x, y) = eval_cubic(seg, t);
                    points.push(to_px(x, y));
                }
            }
            svg_elements.push(TwoElement::Path(TwoPath {
                points,
                stroke: Some(stroke.clone()),
                fill: None,
                linewidth: layer_params.line_width as f64,
                opacity: 1.0,
            }));
        }

        let svg_elements = vec![TwoElement::Group(TwoGroup {
            elements: svg_elements,
            translate: Some((margin_left, margin_top)),
            layer_id: Some(layer_params.layer_id.clone()),
            clip_rect: Some((0.0, 0.0, layer_w as f64, layer_h as f64)),
            ..Default::default()
        })];

        update_svg(ctx, &svg_elements);
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "CurveLayer",
        create_layer: |value, view_params| {
            let params: CurveLayerParams = serde_json::from_value(value).unwrap();
            Box::new(CurveLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for CurveLayer {}
