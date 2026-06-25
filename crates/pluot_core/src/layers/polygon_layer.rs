// PolygonLayer — renders a collection of polygons as stroked outlines, filled
// interiors, or both.
//
// GPU rendering:
//   Fill  — draw_fill_gpu (earcut-triangulated vertices, filled_curve_layer.wgsl)
//   Stroke — draw_stroked_polygon_gpu (one quad per edge, LineLayer approach with
//             stroked_polygon_layer.wgsl). This is much simpler/more efficient than
//             the round-join bezier approach used by CurveLayer, and appropriate for
//             straight polygon edges.
//
// SVG rendering emits one closed TwoPath per polygon.

use earcut::Earcut;
use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::positioning::get_point_position;
use crate::render_traits::{
    AspectRatioAlignmentMode, AspectRatioMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg,
    MarginParams, PickableLayer, PreparedLayer, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult, RenderResult};
use crate::two::shapes::{TwoColor, TwoElement, TwoGroup, TwoPath};
use crate::two::svg::{update_svg, SvgContext};
use crate::wgpu;

use super::filled_curve_layer::draw_fill_gpu;

// ── Params ─────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct PolygonLayerParams {
    pub layer_id: String,
    /// If `None`, the view-level margins are used.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub model_matrix: Option<[f32; 16]>,

    /// One polygon per element; each polygon is a ring of (x, y) model-space vertices.
    /// Rings with fewer than 3 points are silently skipped.
    pub polygons: Arc<Vec<Vec<(f32, f32)>>>,

    /// Whether to stroke the polygon outlines. Defaults to `true`.
    pub stroked: bool,
    /// Whether to fill the polygon interiors. Defaults to `false`.
    pub filled: bool,

    /// RGBA stroke color in [0, 1]. Defaults to opaque black.
    pub stroke_color: [f32; 4],
    /// Stroke width in pixels. Defaults to 1.
    pub stroke_width: f32,
    /// Additional opacity multiplier for the stroke. Defaults to 1.
    pub stroke_opacity: f32,

    /// RGBA fill color in [0, 1]. Defaults to opaque black.
    pub fill_color: [f32; 4],
    /// Additional opacity multiplier for the fill. Defaults to 1.
    pub fill_opacity: f32,
}

impl Default for PolygonLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            model_matrix: None,
            polygons: Arc::new(vec![]),
            stroked: true,
            filled: false,
            stroke_color: [0.0, 0.0, 0.0, 1.0],
            stroke_width: 1.0,
            stroke_opacity: 1.0,
            fill_color: [0.0, 0.0, 0.0, 1.0],
            fill_opacity: 1.0,
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn resolve_margins(params: &PolygonLayerParams, view: &ViewParams) -> (f64, f64, f64, f64) {
    let b = if params.bounds.is_none() { &view.margins } else { &params.bounds };
    let ml = b.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;
    let mt = b.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
    let mr = b.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
    let mb = b.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
    (ml, mt, mr, mb)
}

fn triangulate_ring(ring: &[(f32, f32)], ec: &mut Earcut<f32>, indices: &mut Vec<u32>, out: &mut Vec<(f32, f32)>) {
    if ring.len() < 3 {
        return;
    }
    ec.earcut(ring.iter().map(|&(x, y)| [x, y]), &[] as &[u32], indices);
    for &i in indices.iter() {
        out.push(ring[i as usize]);
    }
}

// ── Polygon stroke draw function (LineLayer approach) ──────────────────────────

#[derive(ShaderType, Debug)]
struct StrokedPolygonUniforms {
    layer_size: Vec2,
    camera_view: Mat4,
    data_unit_mode_x: u32,
    data_unit_mode_y: u32,
    line_width: f32,
    line_width_unit_mode: u32,
    aspect_ratio_mode: u32,
    aspect_ratio_alignment_mode: u32,
    model_matrix: Mat4,
    color: Vec4,
}

#[allow(clippy::too_many_arguments)]
fn draw_stroked_polygon_gpu(
    gpu_context: &GpuContext<'_>,
    pass: &mut wgpu::RenderPass,
    src_x: &[f32],
    src_y: &[f32],
    dst_x: &[f32],
    dst_y: &[f32],
    color: Vec4,
    stroke_width: f32,
    layer_w: f32,
    layer_h: f32,
    camera_view: &[f32; 16],
    data_unit_mode_x: u32,
    data_unit_mode_y: u32,
    aspect_ratio_mode: u32,
    aspect_ratio_alignment_mode: u32,
    model_matrix: Mat4,
    margin_left: f64,
    margin_top: f64,
    margin_right: f64,
    margin_bottom: f64,
) {
    let n = src_x.len();
    if n == 0 {
        return;
    }

    let GpuContext { device, queue } = gpu_context;

    let uniform_struct = StrokedPolygonUniforms {
        layer_size: Vec2::new(layer_w, layer_h),
        camera_view: Mat4::from_cols_array(camera_view),
        data_unit_mode_x,
        data_unit_mode_y,
        line_width: stroke_width,
        line_width_unit_mode: 0, // always pixels
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        model_matrix,
        color,
    };

    let mut ub = UniformBuffer::new(Vec::<u8>::new());
    ub.write(&uniform_struct).unwrap();
    let uniform_bytes = ub.into_inner();

    let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("StrokedPolygon Uniform"),
        size: uniform_bytes.len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&uniform_buf, 0, &uniform_bytes);

    let make_storage_buf = |label: &str, data: &[f32]| -> wgpu::Buffer {
        let bytes: &[u8] = bytemuck::cast_slice(data);
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buf, 0, bytes);
        buf
    };

    let src_x_buf = make_storage_buf("StrokedPolygon SrcX", src_x);
    let src_y_buf = make_storage_buf("StrokedPolygon SrcY", src_y);
    let dst_x_buf = make_storage_buf("StrokedPolygon DstX", dst_x);
    let dst_y_buf = make_storage_buf("StrokedPolygon DstY", dst_y);

    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("StrokedPolygon BGL"),
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
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
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
        label: Some("StrokedPolygon BG"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: src_x_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: src_y_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: dst_x_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 4, resource: dst_y_buf.as_entire_binding() },
        ],
    });

    let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/stroked_polygon_layer.wgsl"));

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("StrokedPolygon PLD"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("StrokedPolygon RPD"),
        layout: Some(&pipeline_layout),
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

    pass.set_viewport(margin_left as f32, margin_top as f32, layer_w, layer_h, 0.0, 1.0);
    pass.set_scissor_rect(margin_left as u32, margin_top as u32, layer_w as u32, layer_h as u32);

    pass.set_pipeline(&pipeline);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.draw(0..4, 0..(n as u32));
}

// ── Layer ──────────────────────────────────────────────────────────────────────

pub struct PolygonLayer {
    view_params: ViewParams,
    layer_params: PolygonLayerParams,
    /// Pre-triangulated fill vertices (empty when fill is disabled).
    fill_vertices: Vec<(f32, f32)>,
    /// Flat segment arrays for polygon edge strokes (empty when stroke is disabled).
    stroke_src_x: Vec<f32>,
    stroke_src_y: Vec<f32>,
    stroke_dst_x: Vec<f32>,
    stroke_dst_y: Vec<f32>,
    /// Pre-multiplied stroke/fill colors (opacity baked in).
    stroke_color: Vec4,
    fill_color: Vec4,
}

impl PolygonLayer {
    pub fn new(view_params: ViewParams, layer_params: PolygonLayerParams) -> Self {
        let fill_vertices = if layer_params.filled {
            let mut ec = Earcut::new();
            let mut indices = Vec::new();
            let mut verts: Vec<(f32, f32)> = Vec::new();
            for ring in layer_params.polygons.iter() {
                triangulate_ring(ring, &mut ec, &mut indices, &mut verts);
            }
            verts
        } else {
            vec![]
        };

        let (mut stroke_src_x, mut stroke_src_y, mut stroke_dst_x, mut stroke_dst_y) =
            (vec![], vec![], vec![], vec![]);
        if layer_params.stroked {
            for ring in layer_params.polygons.iter() {
                if ring.len() < 2 { continue; }
                let n = ring.len();
                for i in 0..n {
                    let j = (i + 1) % n;
                    stroke_src_x.push(ring[i].0);
                    stroke_src_y.push(ring[i].1);
                    stroke_dst_x.push(ring[j].0);
                    stroke_dst_y.push(ring[j].1);
                }
            }
        }

        let [r, g, b, a] = layer_params.stroke_color;
        let stroke_color = Vec4::new(r, g, b, a * layer_params.stroke_opacity);
        let [r, g, b, a] = layer_params.fill_color;
        let fill_color = Vec4::new(r, g, b, a * layer_params.fill_opacity);

        Self {
            view_params, layer_params,
            fill_vertices,
            stroke_src_x, stroke_src_y, stroke_dst_x, stroke_dst_y,
            stroke_color, fill_color,
        }
    }
}

// ── Trait impls ────────────────────────────────────────────────────────────────

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for PolygonLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        PrepareResult { bailed_early: false }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for PolygonLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let Self { view_params, layer_params, fill_vertices, stroke_src_x, stroke_src_y, stroke_dst_x, stroke_dst_y, stroke_color, fill_color } = self;

        let (margin_left, margin_top, margin_right, margin_bottom) =
            resolve_margins(layer_params, view_params);

        let camera_view = view_params.camera_view.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let layer_w = view_params.width as f32 - (margin_left + margin_right) as f32;
        let layer_h = view_params.height as f32 - (margin_top + margin_bottom) as f32;

        let data_unit_mode_x = match layer_params.data_unit_mode_x { UnitsMode::Pixels => 0, UnitsMode::Data => 1 };
        let data_unit_mode_y = match layer_params.data_unit_mode_y { UnitsMode::Pixels => 0, UnitsMode::Data => 1 };
        let aspect_ratio_mode = match view_params.aspect_ratio_mode { AspectRatioMode::Ignore => 0, AspectRatioMode::Contain => 1, AspectRatioMode::Cover => 2 };
        let aspect_ratio_alignment_mode = match view_params.aspect_ratio_alignment_mode { AspectRatioAlignmentMode::Center => 0, AspectRatioAlignmentMode::Start => 1, AspectRatioAlignmentMode::End => 2 };
        let model_matrix = Mat4::from_cols_array(&layer_params.model_matrix.unwrap_or([
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]));

        // Fill first so stroke renders on top.
        draw_fill_gpu(
            gpu_context, pass, fill_vertices, *fill_color,
            layer_w, layer_h, &camera_view,
            data_unit_mode_x, data_unit_mode_y,
            aspect_ratio_mode, aspect_ratio_alignment_mode,
            model_matrix, margin_left, margin_top, margin_right, margin_bottom,
        );
        draw_stroked_polygon_gpu(
            gpu_context, pass,
            stroke_src_x, stroke_src_y, stroke_dst_x, stroke_dst_y,
            *stroke_color, layer_params.stroke_width,
            layer_w, layer_h, &camera_view,
            data_unit_mode_x, data_unit_mode_y,
            aspect_ratio_mode, aspect_ratio_alignment_mode,
            model_matrix, margin_left, margin_top, margin_right, margin_bottom,
        );
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for PolygonLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for PolygonLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let Self { layer_params, view_params, .. } = self;

        let camera_view = view_params.camera_view.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let (margin_left, margin_top, margin_right, margin_bottom) =
            resolve_margins(layer_params, view_params);

        let viewport_w = view_params.width as f32;
        let viewport_h = view_params.height as f32;
        let layer_w = viewport_w - (margin_left + margin_right) as f32;
        let layer_h = viewport_h - (margin_top + margin_bottom) as f32;

        let to_px = |x: f32, y: f32| -> (f64, f64) {
            let (px, py) = get_point_position(
                x, y,
                layer_w, layer_h,
                &camera_view,
                layer_params.data_unit_mode_x.clone(),
                layer_params.data_unit_mode_y.clone(),
                view_params.aspect_ratio_mode.clone(),
                view_params.aspect_ratio_alignment_mode.clone(),
                layer_params.model_matrix.as_ref().map(|m| m.as_slice()),
            );
            (px as f64, (layer_h - py) as f64)
        };

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

        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(layer_params.polygons.len());
        for ring in layer_params.polygons.iter() {
            if ring.len() < 3 {
                continue;
            }
            let mut points: Vec<(f64, f64)> = ring.iter().map(|&(x, y)| to_px(x, y)).collect();
            points.push(points[0]);
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
        layer_type_name: "PolygonLayer",
        create_layer: |value, view_params| {
            let params: PolygonLayerParams = serde_json::from_value(value).unwrap();
            Box::new(PolygonLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for PolygonLayer {}
