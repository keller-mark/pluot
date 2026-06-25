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

// ── Params ─────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct StrokedPolygonLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub model_matrix: Option<[f32; 16]>,

    /// One polygon per element; each is a ring of (x, y) model-space vertices.
    /// Rings with fewer than 2 points are silently skipped.
    pub polygons: Arc<Vec<Vec<(f32, f32)>>>,

    /// RGBA stroke color in [0, 1]. Defaults to opaque black.
    pub stroke_color: [f32; 4],
    /// Stroke width in pixels. Defaults to 1.
    pub stroke_width: f32,
    /// Additional opacity multiplier for the stroke. Defaults to 1.
    pub stroke_opacity: f32,
}

impl Default for StrokedPolygonLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            model_matrix: None,
            polygons: Arc::new(vec![]),
            stroke_color: [0.0, 0.0, 0.0, 1.0],
            stroke_width: 1.0,
            stroke_opacity: 1.0,
        }
    }
}

// ── Uniforms ───────────────────────────────────────────────────────────────────

#[derive(ShaderType, Debug)]
struct StrokedPolygonLayerUniforms {
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

// ── Layer ──────────────────────────────────────────────────────────────────────

pub struct StrokedPolygonLayer {
    view_params: ViewParams,
    layer_params: StrokedPolygonLayerParams,
    stroke_src_x: Vec<f32>,
    stroke_src_y: Vec<f32>,
    stroke_dst_x: Vec<f32>,
    stroke_dst_y: Vec<f32>,
    stroke_color: Vec4,
}

impl StrokedPolygonLayer {
    pub fn new(view_params: ViewParams, layer_params: StrokedPolygonLayerParams) -> Self {
        let mut stroke_src_x = vec![];
        let mut stroke_src_y = vec![];
        let mut stroke_dst_x = vec![];
        let mut stroke_dst_y = vec![];

        for ring in layer_params.polygons.iter() {
            if ring.len() < 2 {
                continue;
            }
            let n = ring.len();
            for i in 0..n {
                let j = (i + 1) % n;
                stroke_src_x.push(ring[i].0);
                stroke_src_y.push(ring[i].1);
                stroke_dst_x.push(ring[j].0);
                stroke_dst_y.push(ring[j].1);
            }
        }

        let [r, g, b, a] = layer_params.stroke_color;
        let stroke_color = Vec4::new(r, g, b, a * layer_params.stroke_opacity);

        Self { view_params, layer_params, stroke_src_x, stroke_src_y, stroke_dst_x, stroke_dst_y, stroke_color }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn resolve_margins(params: &StrokedPolygonLayerParams, view: &ViewParams) -> (f64, f64, f64, f64) {
    let b = if params.bounds.is_none() { &view.margins } else { &params.bounds };
    let ml = b.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;
    let mt = b.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
    let mr = b.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
    let mb = b.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
    (ml, mt, mr, mb)
}

// ── Trait impls ────────────────────────────────────────────────────────────────

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for StrokedPolygonLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        PrepareResult { bailed_early: false }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for StrokedPolygonLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let Self { view_params, layer_params, stroke_src_x, stroke_src_y, stroke_dst_x, stroke_dst_y, stroke_color } = self;

        let n = stroke_src_x.len();
        if n == 0 {
            return;
        }

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
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]));

        let GpuContext { device, queue } = gpu_context;

        let uniform_struct = StrokedPolygonLayerUniforms {
            layer_size: Vec2::new(layer_w, layer_h),
            camera_view: Mat4::from_cols_array(&camera_view),
            data_unit_mode_x,
            data_unit_mode_y,
            line_width: layer_params.stroke_width,
            line_width_unit_mode: 0, // always pixels
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            model_matrix,
            color: *stroke_color,
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

        let src_x_buf = make_storage_buf("StrokedPolygon SrcX", stroke_src_x);
        let src_y_buf = make_storage_buf("StrokedPolygon SrcY", stroke_src_y);
        let dst_x_buf = make_storage_buf("StrokedPolygon DstX", stroke_dst_x);
        let dst_y_buf = make_storage_buf("StrokedPolygon DstY", stroke_dst_y);

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
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for StrokedPolygonLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for StrokedPolygonLayer {
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

        let [r, g, b, a] = layer_params.stroke_color;
        let stroke = TwoColor::Rgba((
            (r * 255.0).round().clamp(0.0, 255.0) as u8,
            (g * 255.0).round().clamp(0.0, 255.0) as u8,
            (b * 255.0).round().clamp(0.0, 255.0) as u8,
            (a * layer_params.stroke_opacity * 255.0).round().clamp(0.0, 255.0) as u8,
        ));

        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(layer_params.polygons.len());
        for ring in layer_params.polygons.iter() {
            if ring.len() < 3 {
                continue;
            }
            let mut points: Vec<(f64, f64)> = ring.iter().map(|&(x, y)| to_px(x, y)).collect();
            points.push(points[0]);
            svg_elements.push(TwoElement::Path(TwoPath {
                points,
                stroke: Some(stroke.clone()),
                fill: None,
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
        layer_type_name: "StrokedPolygonLayer",
        create_layer: |value, view_params| {
            let params: StrokedPolygonLayerParams = serde_json::from_value(value).unwrap();
            Box::new(StrokedPolygonLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for StrokedPolygonLayer {}
