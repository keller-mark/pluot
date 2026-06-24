// A CurveLayer that renders SVG-like vector paths (lines, cubic/quadratic Bezier
// curves and elliptical arcs) as stroked and/or filled curves.
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
// A path can be stroked, filled, or both (matching SVG `fill`/`stroke`). The stroke
// is rendered exactly as above. The fill is produced by flattening each sub-path into
// a polygon and triangulating it on the CPU (ear clipping); the resulting triangles
// are projected through the same pipeline in the vertex shader. Stroke and fill carry
// independent colors and opacity values.
//
// Both pixel and data units are supported (matching LineLayer); we assume the
// position and size values share the same units.

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// `kurbo` provides the curve math (quadratic/arc -> cubic conversion and Bezier
// evaluation); `pluot_triangulation` provides the polygon fill triangulation.
use kurbo::{Arc as KurboArc, CubicBez, ParamCurve, Point as KurboPoint, QuadBez, SvgArc, Vec2 as KurboVec2};
use pluot_triangulation::point::calc_dedup_edges;
use pluot_triangulation::{is_convex, sweeping_line_triangulation, triangulate_convex_polygon, Point as TriPoint};

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
    pub stroke_width: f32,
    pub stroke_width_unit_mode: UnitsMode,
    pub model_matrix: Option<[f32; 16]>, // Column-major 4x4 matrix

    /// The path to draw, as a sequence of absolute drawing commands.
    pub commands: Arc<Vec<PathCommand>>,
    /// Number of straight sub-segments used to approximate each cubic Bezier
    /// segment. Higher values produce smoother curves at the cost of more
    /// instanced draws.
    pub subdivisions: u32,

    /// Whether to stroke the path outline. Defaults to `true`.
    pub stroked: bool,
    /// Whether to fill the (closed) path interior. Defaults to `false`.
    pub filled: bool,
    /// RGBA stroke color in [0, 1]. Defaults to opaque black.
    pub stroke_color: [f32; 4],
    /// RGBA fill color in [0, 1]. Defaults to opaque black.
    pub fill_color: [f32; 4],
    /// Additional opacity multiplier for the stroke, in [0, 1]. Multiplies the
    /// stroke color's alpha (mirrors SVG `stroke-opacity`). Defaults to 1.0.
    pub stroke_opacity: f32,
    /// Additional opacity multiplier for the fill, in [0, 1]. Multiplies the fill
    /// color's alpha (mirrors SVG `fill-opacity`). Defaults to 1.0.
    pub fill_opacity: f32,
}

impl Default for CurveLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            stroke_width: 1.0,
            stroke_width_unit_mode: UnitsMode::Pixels,
            model_matrix: None,
            commands: Arc::new(vec![]),
            subdivisions: 32,
            stroked: true,
            filled: false,
            stroke_color: [0.0, 0.0, 0.0, 1.0],
            fill_color: [0.0, 0.0, 0.0, 1.0],
            stroke_opacity: 1.0,
            fill_opacity: 1.0,
        }
    }
}

/// Represent a straight line as a (degenerate) cubic Bezier so it can share the same
/// evaluation path on the GPU. With the control points at the endpoints, the traced
/// geometry is exactly the line segment.
fn line_to_cubic(p0: KurboPoint, p1: KurboPoint) -> CubicBez {
    CubicBez::new(p0, p0, p1, p1)
}

/// Flatten a sequence of drawing commands into per-sub-path lists of cubic Bezier
/// segments. A new sub-path begins at each MoveTo; Close adds a closing segment back
/// to the sub-path start. Quadratic Beziers and elliptical arcs are converted to
/// cubics via `kurbo` (`QuadBez::raise` and `Arc::to_cubic_beziers` respectively).
fn commands_to_subpaths(commands: &[PathCommand]) -> Vec<Vec<CubicBez>> {
    let mut subpaths: Vec<Vec<CubicBez>> = Vec::new();
    let mut current: Vec<CubicBez> = Vec::new();
    let mut cursor = KurboPoint::ZERO;
    let mut subpath_start = KurboPoint::ZERO;

    for command in commands {
        match *command {
            PathCommand::MoveTo { x, y } => {
                if !current.is_empty() {
                    subpaths.push(std::mem::take(&mut current));
                }
                cursor = KurboPoint::new(x as f64, y as f64);
                subpath_start = cursor;
            }
            PathCommand::LineTo { x, y } => {
                let end = KurboPoint::new(x as f64, y as f64);
                current.push(line_to_cubic(cursor, end));
                cursor = end;
            }
            PathCommand::CubicTo { x1, y1, x2, y2, x, y } => {
                let end = KurboPoint::new(x as f64, y as f64);
                current.push(CubicBez::new(
                    cursor,
                    KurboPoint::new(x1 as f64, y1 as f64),
                    KurboPoint::new(x2 as f64, y2 as f64),
                    end,
                ));
                cursor = end;
            }
            PathCommand::QuadraticTo { x1, y1, x, y } => {
                let end = KurboPoint::new(x as f64, y as f64);
                let quad = QuadBez::new(cursor, KurboPoint::new(x1 as f64, y1 as f64), end);
                // kurbo raises the quadratic to an exactly-equivalent cubic.
                current.push(quad.raise());
                cursor = end;
            }
            PathCommand::ArcTo { rx, ry, x_axis_rotation, large_arc, sweep, x, y } => {
                let end = KurboPoint::new(x as f64, y as f64);
                let svg_arc = SvgArc {
                    from: cursor,
                    to: end,
                    radii: KurboVec2::new(rx as f64, ry as f64),
                    x_rotation: (x_axis_rotation as f64).to_radians(),
                    large_arc,
                    sweep,
                };
                match KurboArc::from_svg_arc(&svg_arc) {
                    Some(arc) => {
                        // Tolerance scaled to the arc's size so the cubic approximation
                        // stays accurate regardless of the coordinate unit scale.
                        let tolerance = (svg_arc.radii.hypot() * 1e-3).max(1e-9);
                        let mut p0 = cursor;
                        arc.to_cubic_beziers(tolerance, |p1, p2, p3| {
                            current.push(CubicBez::new(p0, p1, p2, p3));
                            p0 = p3;
                        });
                    }
                    // Degenerate arc (zero radius / zero length): treat as a line.
                    None => current.push(line_to_cubic(cursor, end)),
                }
                cursor = end;
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

/// Flatten a sub-path's cubic segments into a closed polygon ring, matching the
/// per-cubic `subdivisions` tessellation used for stroking. Consecutive duplicate
/// points (and a trailing point coincident with the start) are dropped so the ring
/// has no degenerate edges. Bezier points are evaluated via `kurbo`.
fn subpath_to_ring(subpath: &[CubicBez], subdivisions: u32) -> Vec<(f32, f32)> {
    let mut points: Vec<(f32, f32)> = Vec::new();
    if subpath.is_empty() {
        return points;
    }
    let push_unique = |points: &mut Vec<(f32, f32)>, p: (f32, f32)| {
        match points.last() {
            Some(last) if (last.0 - p.0).abs() < 1e-9 && (last.1 - p.1).abs() < 1e-9 => {}
            _ => points.push(p),
        }
    };
    let start = subpath[0].p0;
    push_unique(&mut points, (start.x as f32, start.y as f32));
    for seg in subpath {
        for step in 1..=subdivisions {
            let t = step as f64 / subdivisions as f64;
            let p = seg.eval(t);
            push_unique(&mut points, (p.x as f32, p.y as f32));
        }
    }
    // Drop a trailing point that closes back onto the start (Close adds one).
    if points.len() > 1 {
        let first = points[0];
        let last = *points.last().unwrap();
        if (first.0 - last.0).abs() < 1e-9 && (first.1 - last.1).abs() < 1e-9 {
            points.pop();
        }
    }
    points
}

/// Twice the signed area of a polygon ring; used only to discard degenerate
/// (zero-area / fully-collinear) rings before triangulation.
fn ring_area_2x(points: &[(f32, f32)]) -> f32 {
    let n = points.len();
    let mut area = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        area += points[i].0 * points[j].1 - points[j].0 * points[i].1;
    }
    area
}

/// Triangulate the filled interior of the path using `pluot_triangulation`, returning
/// a flat list of model-space triangle vertices (3 per triangle). Each sub-path is
/// flattened into a polygon ring and triangulated independently; the rings' fills are
/// then unioned. This matches the SVG fill output, which paints one `<path>` per
/// sub-path (so nested sub-paths overlap rather than cutting holes).
fn compute_fill_vertices(subpaths: &[Vec<CubicBez>], subdivisions: u32) -> Vec<(f32, f32)> {
    let mut verts: Vec<(f32, f32)> = Vec::new();

    for subpath in subpaths {
        let ring = subpath_to_ring(subpath, subdivisions);
        // A fillable ring needs at least 3 vertices and non-zero area; a fully-collinear
        // ring has nothing to fill (and would make the triangulator panic), so skip it.
        if ring.len() < 3 || ring_area_2x(&ring).abs() <= 1e-12 {
            continue;
        }
        let pts: Vec<TriPoint> = ring.iter().map(|&(x, y)| TriPoint::new(x, y)).collect();

        // A convex ring has a trivial fan triangulation; a concave ring goes through the
        // sweep-line algorithm (decompose into monotone polygons, then triangulate).
        let (triangles, points) = if is_convex(&pts) {
            let tris = triangulate_convex_polygon(&pts);
            (tris, pts)
        } else {
            let edges = calc_dedup_edges(std::slice::from_ref(&pts));
            sweeping_line_triangulation(edges)
        };

        for t in &triangles {
            for &idx in &[t.x, t.y, t.z] {
                let p = points[idx];
                verts.push((p.x, p.y));
            }
        }
    }

    verts
}

pub struct CurveLayer {
    view_params: ViewParams,
    layer_params: CurveLayerParams,
    /// Flattened cubic Bezier segments grouped by sub-path (used for stroking and SVG).
    subpaths: Vec<Vec<CubicBez>>,
    /// Triangulated fill geometry: a flat list of model-space triangle vertices
    /// (3 per triangle). Empty unless the layer is filled.
    fill_vertices: Vec<(f32, f32)>,
}

impl CurveLayer {
    pub fn new(
        view_params: ViewParams,
        layer_params: CurveLayerParams,
    ) -> Self {
        // Error if stroke_width_unit_mode is "data" when data_unit_mode is "pixels".
        if layer_params.stroke_width_unit_mode == UnitsMode::Data
            && (layer_params.data_unit_mode_x == UnitsMode::Pixels
                || layer_params.data_unit_mode_y == UnitsMode::Pixels)
        {
            panic!("stroke_width_unit_mode cannot be 'data' when data_unit_mode is 'pixels'");
        }
        let subpaths = commands_to_subpaths(&layer_params.commands);

        // Precompute the fill triangulation only when needed.
        let fill_vertices = if layer_params.filled {
            compute_fill_vertices(&subpaths, layer_params.subdivisions.max(1))
        } else {
            Vec::new()
        };

        Self {
            view_params,
            layer_params,
            subpaths,
            fill_vertices,
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
    stroke_width: f32,
    stroke_width_unit_mode: u32, // 0 = pixels, 1 = data units
    aspect_ratio_mode: u32, // 0 = ignore, 1 = contain, 2 = cover
    aspect_ratio_alignment_mode: u32, // 0 = center, 1 = start, 2 = end
    subdivisions: u32, // sub-segments per cubic Bezier segment
    model_matrix: Mat4,
    stroke_color: Vec4, // rgba stroke color (alpha already includes stroke_opacity)
    fill_color: Vec4, // rgba fill color (alpha already includes fill_opacity)
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for CurveLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let GpuContext { device, queue } = gpu_context;
        let Self { layer_params, view_params, subpaths, fill_vertices } = self;

        let num_segments = self.num_segments();
        let subdivisions = layer_params.subdivisions.max(1);

        // Decide which passes to run. Stroke needs segments; fill needs triangles.
        let do_stroke = layer_params.stroked && num_segments > 0;
        let do_fill = layer_params.filled && !fill_vertices.is_empty();
        // Nothing to draw (empty path, only MoveTo/Close, or both modes disabled).
        if !do_stroke && !do_fill {
            return;
        }

        // Final stroke/fill colors, folding the separate opacity multipliers into alpha.
        let stroke_rgba = {
            let [r, g, b, a] = layer_params.stroke_color;
            [r, g, b, a * layer_params.stroke_opacity]
        };
        let fill_rgba = {
            let [r, g, b, a] = layer_params.fill_color;
            [r, g, b, a * layer_params.fill_opacity]
        };

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
            stroke_width: layer_params.stroke_width,
            stroke_width_unit_mode: match layer_params.stroke_width_unit_mode {
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
            stroke_color: Vec4::from_array(stroke_rgba),
            fill_color: Vec4::from_array(fill_rgba),
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
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/curve_layer.wgsl"));

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("CurveLayer PLD"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        // Both passes share the same layout, blend, and target; only the vertex
        // entry point and primitive topology differ (quads for stroke, triangles
        // for fill).
        let make_pipeline = |entry_point: &str, topology: wgpu::PrimitiveTopology| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("CurveLayer RPD"),
                layout: Some(&render_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(entry_point),
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
                    topology,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                cache: None,
                multiview_mask: None,
            })
        };

        // Build a storage-buffer-backed bind group from a tightly-packed f32 slice
        // (interpreted as `array<vec2<f32>>` in the shader).
        let make_bind_group = |label: &str, data: &[f32]| {
            let bytes: &[u8] = bytemuck::cast_slice(data);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: bytes.len() as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&buffer, 0, bytes);
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: buffer.as_entire_binding(),
                    },
                ],
            })
        };

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

        // Fill first, so the stroke is drawn on top of the fill.
        if do_fill {
            // Flatten triangle vertices into a tightly-packed f32 buffer: 1 vec2 per
            // vertex, 3 vertices per triangle.
            let mut fill_data: Vec<f32> = Vec::with_capacity(fill_vertices.len() * 2);
            for (x, y) in fill_vertices {
                fill_data.push(*x);
                fill_data.push(*y);
            }
            let fill_bind_group = make_bind_group("Curve Fill Vertices", &fill_data);
            let fill_pipeline = make_pipeline("vs_fill", wgpu::PrimitiveTopology::TriangleList);

            pass.set_pipeline(&fill_pipeline);
            pass.set_bind_group(0, &fill_bind_group, &[]);
            pass.draw(0..(fill_vertices.len() as u32), 0..1);
        }

        if do_stroke {
            // Flatten control points into a tightly-packed f32 buffer: 4 vec2 (8 floats)
            // per cubic segment, matching `array<vec2<f32>>` (stride 8) in the shader.
            let mut control_points: Vec<f32> = Vec::with_capacity(num_segments * 8);
            for subpath in subpaths {
                for seg in subpath {
                    for p in [seg.p0, seg.p1, seg.p2, seg.p3] {
                        control_points.push(p.x as f32);
                        control_points.push(p.y as f32);
                    }
                }
            }
            let stroke_bind_group = make_bind_group("Curve Control Points", &control_points);
            let stroke_pipeline = make_pipeline("vs_main", wgpu::PrimitiveTopology::TriangleStrip);

            pass.set_pipeline(&stroke_pipeline);
            pass.set_bind_group(0, &stroke_bind_group, &[]);
            // One instance per (segment, sub-segment); 4 vertices per instance (quad).
            let instance_count = (num_segments as u32) * subdivisions;
            pass.draw(0..4, 0..instance_count);
        }
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
        let Self { layer_params, view_params, subpaths, .. } = self;

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

        // Fold the separate opacity multiplier into each color's alpha (mirrors the
        // GPU path); the element-level SVG `opacity` stays at 1.0.
        let to_rgba = |[r, g, b, a]: [f32; 4], opacity: f32| -> TwoColor {
            TwoColor::Rgba((
                (r * 255.0).round().clamp(0.0, 255.0) as u8,
                (g * 255.0).round().clamp(0.0, 255.0) as u8,
                (b * 255.0).round().clamp(0.0, 255.0) as u8,
                (a * opacity * 255.0).round().clamp(0.0, 255.0) as u8,
            ))
        };
        let stroke = if layer_params.stroked {
            Some(to_rgba(layer_params.stroke_color, layer_params.stroke_opacity))
        } else {
            None
        };
        let fill = if layer_params.filled {
            Some(to_rgba(layer_params.fill_color, layer_params.fill_opacity))
        } else {
            None
        };

        // Emit one TwoPath per sub-path. We flatten each cubic segment into
        // `subdivisions` line segments (matching the GPU tessellation). SVG implicitly
        // closes each sub-path when filling, matching the per-sub-path GPU fill.
        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(subpaths.len());
        for subpath in subpaths {
            if subpath.is_empty() {
                continue;
            }
            let mut points: Vec<(f64, f64)> = Vec::new();
            // Start at the very first control point of the sub-path.
            let first = subpath[0].p0;
            points.push(to_px(first.x as f32, first.y as f32));
            for seg in subpath {
                for step in 1..=subdivisions {
                    let t = step as f64 / subdivisions as f64;
                    let p = seg.eval(t);
                    points.push(to_px(p.x as f32, p.y as f32));
                }
            }
            svg_elements.push(TwoElement::Path(TwoPath {
                points,
                stroke: stroke.clone(),
                fill: fill.clone(),
                linewidth: layer_params.stroke_width as f64,
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
