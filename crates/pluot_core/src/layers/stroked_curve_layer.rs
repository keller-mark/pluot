// StrokedCurveLayer accepts path commands as input.
// This layer is intended to be used as a sub-layer of CurveLayer.
// In the vector drawing case, rendering is performed by simply defining an SVG path element.
// In the raster drawing case, we use the approach from rreusser/webgpu-instanced-lines
// to render bezier curves and arcs via WebGPU. Note that this approach has overhead,
// and is therefore not currently meant to render a ton of individual curves.

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use kurbo::{CubicBez, ParamCurve};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::positioning::get_point_position;
use crate::render_traits::{
    AspectRatioAlignmentMode, AspectRatioMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg,
    MarginParams, PickableLayer, PreparedLayer, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult};
use crate::two::shapes::{TwoColor, TwoElement, TwoGroup, TwoPath};
use crate::two::svg::{update_svg, SvgContext};
use crate::wgpu;

use super::curve_and_polygon_utils::{flatten_subpath, resolve_margins};
use super::filled_curve_layer::{commands_to_subpaths, PathCommand};

// Must match VERTS_PER_INSTANCE_F = 38 in stroked_curve_layer.wgsl.
// With JOIN_RESOLUTION=8: (8*2 + 3) * 2 = 38.
const VERTS_PER_INSTANCE: u32 = 38;

#[derive(ShaderType, Debug)]
struct StrokedCurveLayerUniforms {
    layer_size: Vec2,
    camera_view: Mat4,
    data_unit_mode_x: u32,
    data_unit_mode_y: u32,
    stroke_width: f32,
    aspect_ratio_mode: u32,
    aspect_ratio_alignment_mode: u32,
    model_matrix: Mat4,
    stroke_color: Vec4,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct StrokedCurveLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub stroke_width: f32,
    pub model_matrix: Option<[f32; 16]>,
    pub commands: Arc<Vec<PathCommand>>,
    pub subdivisions: u32,
    pub stroke_color: [f32; 3],
    pub stroke_opacity: f32,
}

impl Default for StrokedCurveLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            stroke_width: 1.0,
            model_matrix: None,
            commands: Arc::new(vec![]),
            subdivisions: 32,
            stroke_color: [0.0, 0.0, 0.0],
            stroke_opacity: 1.0,
        }
    }
}

pub struct StrokedCurveLayer {
    view_params: ViewParams,
    layer_params: StrokedCurveLayerParams,
    subpaths: Vec<Vec<CubicBez>>,
    polylines: Vec<Vec<(f32, f32)>>,
    stroke_color: Vec4,
}

impl StrokedCurveLayer {
    pub fn new(view_params: ViewParams, layer_params: StrokedCurveLayerParams) -> Self {
        let subpaths = commands_to_subpaths(&layer_params.commands);
        let subdivisions = layer_params.subdivisions.max(1);
        let polylines = subpaths.iter().map(|s| flatten_subpath(s, subdivisions)).collect();
        let [r, g, b] = layer_params.stroke_color;
        let stroke_color = Vec4::new(r, g, b, layer_params.stroke_opacity);
        Self { view_params, layer_params, subpaths, polylines, stroke_color }
    }
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for StrokedCurveLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        PrepareResult { bailed_early: false }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for StrokedCurveLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let Self { view_params, layer_params, polylines, stroke_color, .. } = self;

        let has_work = polylines.iter().any(|p| p.len() >= 2);
        if !has_work {
            return;
        }

        let (margin_left, margin_top, margin_right, margin_bottom) =
            resolve_margins(&layer_params.bounds, &view_params.margins);

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

        let GpuContext { device, queue } = gpu_context;

        let mut pts_data: Vec<f32> = Vec::new();
        let mut seg_data: Vec<u32> = Vec::new();
        let mut total_segments: u32 = 0;

        for polyline in polylines {
            if polyline.len() < 2 {
                continue;
            }
            let poly_start = (pts_data.len() / 2) as u32;
            for (x, y) in polyline {
                pts_data.push(*x);
                pts_data.push(*y);
            }
            let poly_end = poly_start + polyline.len() as u32 - 1;
            for local_b in 0..(polyline.len() as u32 - 1) {
                seg_data.push(poly_start);
                seg_data.push(poly_end);
                seg_data.push(local_b);
                total_segments += 1;
            }
        }

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("StrokedCurve BGL"),
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
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/stroked_curve_layer.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("StrokedCurve PLD"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("StrokedCurve RPD"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_stroke"),
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

        let uniform_struct = StrokedCurveLayerUniforms {
            layer_size: Vec2::new(layer_w, layer_h),
            camera_view: Mat4::from_cols_array(&camera_view),
            data_unit_mode_x,
            data_unit_mode_y,
            stroke_width: layer_params.stroke_width,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            model_matrix,
            stroke_color: *stroke_color,
        };
        let mut ub = UniformBuffer::new(Vec::<u8>::new());
        ub.write(&uniform_struct).unwrap();
        let uniform_bytes = ub.into_inner();

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("StrokedCurve Uniform"),
            size: uniform_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniform_buf, 0, &uniform_bytes);

        let pts_bytes: &[u8] = bytemuck::cast_slice(&pts_data);
        let pts_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("StrokedCurve Points"),
            size: pts_bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&pts_buf, 0, pts_bytes);

        let seg_bytes: &[u8] = bytemuck::cast_slice(&seg_data);
        let seg_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("StrokedCurve Segments"),
            size: seg_bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&seg_buf, 0, seg_bytes);

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("StrokedCurve BG"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: pts_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: seg_buf.as_entire_binding(),
                },
            ],
        });

        pass.set_viewport(margin_left as f32, margin_top as f32, layer_w, layer_h, 0.0, 1.0);
        pass.set_scissor_rect(margin_left as u32, margin_top as u32, layer_w as u32, layer_h as u32);

        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..VERTS_PER_INSTANCE, 0..total_segments);
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for StrokedCurveLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for StrokedCurveLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let Self { layer_params, view_params, subpaths, .. } = self;

        let camera_view = view_params.camera_view.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let (margin_left, margin_top, margin_right, margin_bottom) =
            resolve_margins(&layer_params.bounds, &view_params.margins);

        let layer_w = view_params.width as f32 - (margin_left + margin_right) as f32;
        let layer_h = view_params.height as f32 - (margin_top + margin_bottom) as f32;
        let subdivisions = layer_params.subdivisions.max(1) as f64;

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

        let [r, g, b] = layer_params.stroke_color;
        let stroke = TwoColor::Rgb((
            (r * 255.0).round().clamp(0.0, 255.0) as u8,
            (g * 255.0).round().clamp(0.0, 255.0) as u8,
            (b * 255.0).round().clamp(0.0, 255.0) as u8,
        ));

        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(subpaths.len());
        for subpath in subpaths {
            if subpath.is_empty() {
                continue;
            }
            let mut points: Vec<(f64, f64)> = Vec::new();
            let first = subpath[0].p0;
            points.push(to_px(first.x as f32, first.y as f32));
            for seg in subpath {
                for step in 1..=(subdivisions as u32) {
                    let t = step as f64 / subdivisions;
                    let p = seg.eval(t);
                    points.push(to_px(p.x as f32, p.y as f32));
                }
            }
            svg_elements.push(TwoElement::Path(TwoPath {
                points,
                stroke: Some(stroke.clone()),
                fill: None,
                linewidth: layer_params.stroke_width as f64,
                opacity: 1.0,
                fill_opacity: 1.0,
                stroke_opacity: layer_params.stroke_opacity as f64,
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
        layer_type_name: "StrokedCurveLayer",
        create_layer: |value, view_params| {
            let params: StrokedCurveLayerParams = serde_json::from_value(value).unwrap();
            Box::new(StrokedCurveLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for StrokedCurveLayer {}
