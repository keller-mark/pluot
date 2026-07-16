// TriangulatedLayer is intended to be used as a sub-layer
// by FilledPolygonLayer and FilledCurveLayer.
// These "parent" layers can internally compute triangulations
// before passing the pre-triangulated vertices to a
// TriangulatedLayer sub-layer.

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
use crate::numeric_data::NumericData;
use crate::shader_modules::{common, ShaderBuilder};
use crate::two::shapes::{TwoColor, TwoElement, TwoGroup, TwoPath};
use crate::two::svg::{update_svg, SvgContext};
use crate::wgpu;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct TriangulatedLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub model_matrix: Option<[f32; 16]>,

    /// Pre-triangulated vertices as a flat, interleaved 1D array of model-space
    /// coordinates `[x0, y0, x1, y1, …]`. Its length is `2 * num_vertices`, and
    /// each consecutive run of 3 vertices (6 values) forms one triangle. Any
    /// supported numeric dtype is accepted (see [`NumericData`]); the data is
    /// uploaded to the GPU as a texture at its native width where possible.
    // TODO: also support non-interleaved vertices?
    pub vertices: NumericData,

    /// RGB fill color as `[r, g, b]` bytes in `[0, 255]`. Defaults to opaque black.
    pub fill_color: [u8; 3],
    /// Opacity multiplier for the fill. Defaults to 1.
    pub fill_opacity: f32,
}

impl Default for TriangulatedLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            model_matrix: None,
            vertices: NumericData::Float32(Arc::new(vec![])),
            fill_color: [0, 0, 0],
            fill_opacity: 1.0,
        }
    }
}

pub struct TriangulatedLayer {
    view_params: ViewParams,
    layer_params: TriangulatedLayerParams,
    fill_color: Vec4,
}

impl TriangulatedLayer {
    pub fn new(view_params: ViewParams, layer_params: TriangulatedLayerParams) -> Self {
        let [r, g, b] = layer_params.fill_color;
        let fill_color = Vec4::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, layer_params.fill_opacity);
        Self { view_params, layer_params, fill_color }
    }
}

fn resolve_margins(params: &TriangulatedLayerParams, view: &ViewParams) -> (f64, f64, f64, f64) {
    let b = if params.bounds.is_none() { &view.margins } else { &params.bounds };
    let ml = b.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;
    let mt = b.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
    let mr = b.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
    let mb = b.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
    (ml, mt, mr, mb)
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for TriangulatedLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        PrepareResult { bailed_early: false }
    }
}

#[derive(ShaderType, Debug)]
struct TriangulatedLayerUniforms {
    layer_size: Vec2,
    camera_view: Mat4,
    data_unit_mode_x: u32,
    data_unit_mode_y: u32,
    aspect_ratio_mode: u32,
    aspect_ratio_alignment_mode: u32,
    model_matrix: Mat4,
    fill_color: Vec4,
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for TriangulatedLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let Self { view_params, layer_params, fill_color } = self;

        if layer_params.vertices.is_empty() {
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

        // Upload the flat interleaved [x0, y0, x1, y1, …] coordinate array into a
        // single-channel 2D texture at its native byte width where possible. The
        // shader recomputes each vertex's two flat indices (2*i, 2*i+1) and maps
        // them back to 2D texel coordinates. See `NumericData::create_data_texture`.
        let (vertices_texture_view, vertices_dtype) =
            layer_params.vertices.create_data_texture(device, queue, "Triangulated Vertices Texture");

        let uniform_struct = TriangulatedLayerUniforms {
            layer_size: Vec2::new(layer_w, layer_h),
            camera_view: Mat4::from_cols_array(&camera_view),
            data_unit_mode_x,
            data_unit_mode_y,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            model_matrix,
            fill_color: *fill_color,
        };

        let mut buf = UniformBuffer::new(Vec::<u8>::new());
        buf.write(&uniform_struct).unwrap();
        let uniform_bytes = buf.into_inner();

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Triangulated Uniform"),
            size: uniform_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Triangulated BGL"),
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
                    // The interleaved vertex coordinates texture. Its sample type
                    // must match the dtype-specific texture format chosen above.
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Texture {
                        sample_type: vertices_dtype.binding_sample_type(),
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        // Inject the shared WGSL functions at compile time (see `crate::shader_modules`).
        let shader_source = ShaderBuilder::new(include_str!("shaders/triangulated_layer.wgsl"))
            .inject_function("scale", common::SCALE)
            .inject_function("translate", common::TRANSLATE)
            .inject_function("get_aspect_ratio_mat", common::GET_ASPECT_RATIO_MAT)
            .inject_texture_sample_type("vertices_dtype", vertices_dtype)
            .build();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("triangulated_layer.wgsl"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Triangulated PLD"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Triangulated RPD"),
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
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
            multiview_mask: None,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Triangulated BG"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&vertices_texture_view),
                },
            ],
        });

        pass.set_viewport(margin_left as f32, margin_top as f32, layer_w, layer_h, 0.0, 1.0);
        pass.set_scissor_rect(margin_left as u32, margin_top as u32, layer_w as u32, layer_h as u32);

        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        // Two interleaved coordinate values per vertex.
        let num_vertices = (layer_params.vertices.len() / 2) as u32;
        pass.draw(0..num_vertices, 0..1);
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for TriangulatedLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for TriangulatedLayer {
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

        let [r, g, b] = layer_params.fill_color;
        let fill = TwoColor::Rgb((r, g, b));

        let verts = &layer_params.vertices;
        // Flat interleaved [x, y, …]: 2 values per vertex, 3 vertices per triangle.
        let num_triangles = verts.len() / 6;
        let vertex = |v: usize| (verts.get_f32(v * 2), verts.get_f32(v * 2 + 1));
        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(num_triangles);

        for i in 0..num_triangles {
            let (v0x, v0y) = vertex(i * 3);
            let (v1x, v1y) = vertex(i * 3 + 1);
            let (v2x, v2y) = vertex(i * 3 + 2);
            let p0 = to_px(v0x, v0y);
            let p1 = to_px(v1x, v1y);
            let p2 = to_px(v2x, v2y);
            let d = format!("M {} {} L {} {} L {} {} Z", p0.0, p0.1, p1.0, p1.1, p2.0, p2.1);
            svg_elements.push(TwoElement::Path(TwoPath {
                d,
                stroke: None,
                fill: Some(fill.clone()),
                linewidth: 0.0,
                opacity: 1.0,
                fill_opacity: layer_params.fill_opacity as f64,
                stroke_opacity: 1.0,
                stroke_linejoin: None,
                stroke_linecap: None,
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

impl PickableLayer for TriangulatedLayer {}
