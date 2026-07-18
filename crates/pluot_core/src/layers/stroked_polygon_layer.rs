// StrokedPolygonLayer accepts polygon vertices as input.
// This layer is intended to be used as a sub-layer of PolygonLayer.
// In the vector drawing case, rendering is performed by simply defining an SVG path element.
// In the raster drawing case, we use the approach from the DeckGL PathLayer
// to ensure clean line joins at polygon vertices via WebGPU.
// Note that this approach has slightly more overhead than simply rendering lines alone,
// (e.g., compared to delegating to a LineLayer sub-layer).

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::positioning::get_point_position;
use crate::numeric_data::NumericData;
use super::curve_and_polygon_utils::{
    polygon_rings_from_flat, polygon_segments_from_offsets, resolve_margins,
};
use crate::render_traits::{
    AspectRatioAlignmentMode, AspectRatioMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg,
    MarginParams, PickableLayer, PreparedLayer, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult, RenderResult};
use crate::shader_modules::{common, ShaderBuilder};
use crate::two::shapes::{TwoColor, TwoElement, TwoGroup, TwoPath};
use crate::two::svg::{update_svg, SvgContext};
use crate::wgpu;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct StrokedPolygonLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub model_matrix: Option<[f32; 16]>,

    /// All polygon vertices as a flat, interleaved 1D array of model-space
    /// coordinates `[x0, y0, x1, y1, …]`, with each polygon's ring concatenated
    /// after the previous one. Any supported numeric dtype is accepted.
    pub polygons: NumericData,
    /// Arrow-style vertex offsets with `num_polygons + 1` entries: polygon `p`
    /// occupies vertex indices `polygon_offsets[p]..polygon_offsets[p + 1]`.
    /// Any supported numeric dtype is accepted. Rings with fewer than 2 vertices
    /// are silently skipped.
    pub polygon_offsets: NumericData,

    /// RGB stroke color as `[r, g, b]` bytes in `[0, 255]`. Defaults to opaque black.
    pub stroke_color: [u8; 3],
    /// Stroke width in pixels. Defaults to 1.
    pub stroke_width: f32,

    // TODO: implement a stroke_width_unit_mode to support both pixel and data unit mode.

    /// Opacity multiplier for the stroke. Defaults to 1.
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
            polygons: NumericData::Float32(Arc::new(vec![])),
            polygon_offsets: NumericData::Uint32(Arc::new(vec![])),
            stroke_color: [0, 0, 0],
            stroke_width: 1.0,
            stroke_opacity: 1.0,
        }
    }
}

pub struct StrokedPolygonLayer {
    view_params: ViewParams,
    layer_params: StrokedPolygonLayerParams,
    /// Per-edge metadata: [ring_start, ring_end, local_idx] (vertex indices into
    /// the flat `polygons` coordinate array).
    segments: Vec<[u32; 3]>,
    stroke_color: Vec4,
}

impl StrokedPolygonLayer {
    pub fn new(view_params: ViewParams, layer_params: StrokedPolygonLayerParams) -> Self {
        // TODO: move this logic to the prepare() function?
        // TODO: only do these computations in the raster drawing case?
        let segments = polygon_segments_from_offsets(&layer_params.polygon_offsets);
        let [r, g, b] = layer_params.stroke_color;
        let stroke_color = Vec4::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, layer_params.stroke_opacity);
        Self { view_params, layer_params, segments, stroke_color }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for StrokedPolygonLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        PrepareResult { bailed_early: false }
    }
}

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

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for StrokedPolygonLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let Self { view_params, layer_params, segments, stroke_color } = self;

        let n = segments.len();
        if n == 0 {
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

        // Upload the flat interleaved [x0, y0, x1, y1, …] vertex coordinates into a
        // single-channel 2D texture at its native byte width where possible. The
        // shader recomputes each vertex's two flat indices (2*i, 2*i+1) and maps
        // them back to 2D texel coordinates. See `NumericData::create_data_texture`.
        let (points_texture_view, points_dtype) =
            layer_params.polygons.create_data_texture(device, queue, "StrokedPolygon Points Texture");

        // Flat [ring_start, ring_end, local_idx, ...].
        let segments_flat: Vec<u32> = segments.iter().flat_map(|s| s.iter().copied()).collect();
        let segments_bytes: &[u8] = bytemuck::cast_slice(&segments_flat);
        let segments_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("StrokedPolygon Segments"),
            size: segments_bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&segments_buf, 0, segments_bytes);

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
                    // The interleaved vertex coordinates texture. Its sample type
                    // must match the dtype-specific texture format chosen above.
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Texture {
                        sample_type: points_dtype.binding_sample_type(),
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("StrokedPolygon BG"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&points_texture_view) },
                wgpu::BindGroupEntry { binding: 2, resource: segments_buf.as_entire_binding() },
            ],
        });

        // Inject the shared WGSL functions at compile time (see `crate::shader_modules`).
        let shader_source = ShaderBuilder::new(include_str!("shaders/stroked_polygon_layer.wgsl"))
            .inject_function("scale", common::SCALE)
            .inject_function("translate", common::TRANSLATE)
            .inject_function("get_aspect_ratio_mat", common::GET_ASPECT_RATIO_MAT)
            .inject_texture_sample_type("points", points_dtype)
            .build();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stroked_polygon_layer.wgsl"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

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
            resolve_margins(&layer_params.bounds, &view_params.margins);

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

        let [r, g, b] = layer_params.stroke_color;
        let stroke = TwoColor::Rgb((r, g, b));

        let rings = polygon_rings_from_flat(&layer_params.polygons, &layer_params.polygon_offsets);
        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(rings.len());
        for ring in rings.iter() {
            if ring.len() < 3 {
                continue;
            }
            let mut d = String::new();
            for (i, &(x, y)) in ring.iter().enumerate() {
                let (px, py) = to_px(x, y);
                if i == 0 {
                    d.push_str(&format!("M {} {}", px, py));
                } else {
                    d.push_str(&format!(" L {} {}", px, py));
                }
            }
            d.push_str(" Z");
            svg_elements.push(TwoElement::Path(TwoPath {
                d,
                stroke: Some(stroke.clone()),
                fill: None,
                linewidth: layer_params.stroke_width as f64,
                opacity: 1.0,
                fill_opacity: 1.0,
                stroke_opacity: layer_params.stroke_opacity as f64,
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

impl PickableLayer for StrokedPolygonLayer {}
