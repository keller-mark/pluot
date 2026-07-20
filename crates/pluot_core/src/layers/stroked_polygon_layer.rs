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

use crate::positioning::{get_point_position, get_point_size};
use crate::numeric_data::NumericData;
use super::curve_and_polygon_utils::{
    polygon_rings_from_flat, polygon_segments_from_offsets, resolve_margins,
};
use crate::render_traits::{
    AspectRatioAlignmentMode, AspectRatioMode, ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg,
    MarginParams, OpacityMode, PickableLayer, PreparedLayer, SizeMode, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult, RenderResult};
use crate::color_mode::{cpu_fill_color, prepare_stroke_color, quantitative_domain};
use crate::scalar_mode::{cpu_stroke_opacity, cpu_stroke_width, prepare_stroke_opacity_mode, prepare_stroke_width_mode};
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
    /// Whether `stroke_width` is measured in pixels or in data-coordinate units.
    pub stroke_width_unit_mode: UnitsMode,
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

    /// How to color each polygon's outline. See [`ColorMode`]: modes carrying
    /// `NumericData` (instanced/categorical/quantitative) supply one value per
    /// polygon.
    pub stroke_color: Option<ColorMode>,
    /// Stroke width. See [`SizeMode`]: `UniformSize` shares one width across all
    /// polygons, `InstancedSize` supplies one per polygon. Interpreted in the
    /// units given by `stroke_width_unit_mode`. Defaults to 1.
    pub stroke_width: Option<SizeMode>,
    /// Opacity multiplier for the stroke. See [`OpacityMode`]: `UniformOpacity`
    /// shares one value across all polygons, `InstancedOpacity` supplies one per
    /// polygon. Defaults to 1.
    pub stroke_opacity: Option<OpacityMode>,
}

impl Default for StrokedPolygonLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            stroke_width_unit_mode: UnitsMode::Pixels,
            model_matrix: None,
            polygons: NumericData::Float32(Arc::new(vec![])),
            polygon_offsets: NumericData::Uint32(Arc::new(vec![])),
            stroke_color: None,
            stroke_width: Some(SizeMode::UniformSize(1.0)),
            stroke_opacity: Some(OpacityMode::UniformOpacity(1.0)),
        }
    }
}

pub struct StrokedPolygonLayer {
    view_params: ViewParams,
    layer_params: StrokedPolygonLayerParams,
    /// Per-edge metadata: [ring_start, ring_end, local_idx, poly_index] (the
    /// first three are vertex indices into the flat `polygons` coordinate
    /// array; `poly_index` is the 0-based polygon/ring index used to resolve
    /// `stroke_color`).
    segments: Vec<[u32; 4]>,
}

impl StrokedPolygonLayer {
    pub fn new(view_params: ViewParams, layer_params: StrokedPolygonLayerParams) -> Self {
        // TODO: validate the length of the colorMode values when instanced

        // TODO: move this logic to the prepare() function?
        // TODO: only do these computations in the raster drawing case?
        let segments = polygon_segments_from_offsets(&layer_params.polygon_offsets);
        Self { view_params, layer_params, segments }
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
    stroke_width: f32,
    stroke_width_unit_mode: u32,
    aspect_ratio_mode: u32,
    aspect_ratio_alignment_mode: u32,
    model_matrix: Mat4,
    stroke_color_mode: u32,      // see ColorMode::shader_mode()
    stroke_color: Vec4,          // rgba color used by the UniformRgb mode
    stroke_color_reverse: u32,   // 1 = reverse the quantitative colormap
    stroke_color_domain: Vec2,   // (min, max) normalization domain for quantitative mode
    stroke_opacity: f32,
}

// First bind-group binding index used for color-mode value/palette texture(s).
// Bindings 0-2 are the uniforms buffer, the points texture, and the segments buffer.
const COLOR_BINDING_START: u32 = 3;

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for StrokedPolygonLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let Self { view_params, layer_params, segments } = self;

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

        // Build the GPU-side color resources for the configured color mode. Modes
        // that carry per-element `NumericData` upload it as one or more textures
        // (bound from COLOR_BINDING_START onward) and contribute the WGSL
        // `get_stroke_color` function injected into the shader below.
        let color = prepare_stroke_color(device, queue, layer_params.stroke_color.as_ref(), COLOR_BINDING_START);

        // Build the GPU-side width and opacity resources. Like the color mode,
        // the instanced variants upload a per-polygon value texture; those
        // bindings follow the color textures. Both are indexed by poly_index:
        // the width texture is read in the vertex stage, the opacity texture in
        // the fragment stage.
        let width_binding_start = COLOR_BINDING_START + color.textures.len() as u32;
        let width = prepare_stroke_width_mode(device, queue, layer_params.stroke_width.as_ref(), width_binding_start);
        let opacity_binding_start = width_binding_start + width.texture.is_some() as u32;
        let opacity = prepare_stroke_opacity_mode(device, queue, layer_params.stroke_opacity.as_ref(), opacity_binding_start);

        let uniform_struct = StrokedPolygonLayerUniforms {
            layer_size: Vec2::new(layer_w, layer_h),
            camera_view: Mat4::from_cols_array(&camera_view),
            data_unit_mode_x,
            data_unit_mode_y,
            stroke_width: width.static_value,
            stroke_width_unit_mode: match layer_params.stroke_width_unit_mode {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
            },
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            model_matrix,
            stroke_color_mode: color.mode,
            stroke_color: Vec4::from_array(color.static_color),
            stroke_color_reverse: color.reverse,
            stroke_color_domain: Vec2::from_array(color.domain),
            stroke_opacity: opacity.static_value,
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

        let mut bgl_entries = vec![
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
        ];
        // One fragment-visible texture per color value / palette array.
        for (i, tex) in color.textures.iter().enumerate() {
            bgl_entries.push(wgpu::BindGroupLayoutEntry {
                binding: COLOR_BINDING_START + i as u32,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: tex.sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        // Instanced width texture (read in the vertex stage) and instanced
        // opacity texture (read in the fragment stage), each present only when
        // the corresponding mode is instanced.
        if let Some(tex) = &width.texture {
            bgl_entries.push(wgpu::BindGroupLayoutEntry {
                binding: width_binding_start,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: tex.sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        if let Some(tex) = &opacity.texture {
            bgl_entries.push(wgpu::BindGroupLayoutEntry {
                binding: opacity_binding_start,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: tex.sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("StrokedPolygon BGL"),
            entries: &bgl_entries,
        });

        let mut bg_entries = vec![
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&points_texture_view) },
            wgpu::BindGroupEntry { binding: 2, resource: segments_buf.as_entire_binding() },
        ];
        for (i, tex) in color.textures.iter().enumerate() {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: COLOR_BINDING_START + i as u32,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        if let Some(tex) = &width.texture {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: width_binding_start,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        if let Some(tex) = &opacity.texture {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: opacity_binding_start,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("StrokedPolygon BG"),
            layout: &bgl,
            entries: &bg_entries,
        });

        // Inject the shared WGSL functions at compile time (see `crate::shader_modules`).
        let shader_source = ShaderBuilder::new(include_str!("shaders/stroked_polygon_layer.wgsl"))
            .inject_function("scale", common::SCALE)
            .inject_function("translate", common::TRANSLATE)
            .inject_function("get_aspect_ratio_mat", common::GET_ASPECT_RATIO_MAT)
            .inject_texture_sample_type("points", points_dtype)
            // Color-mode specialization: the flat-index texel helper plus the
            // assembled color module (bindings + `get_stroke_color`).
            .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
            .define("color_module", &color.wgsl)
            // Width- and opacity-mode specialization: each contributes its
            // `get_stroke_width` / `get_stroke_opacity` function (plus a value
            // texture binding when instanced).
            .define("stroke_width_module", &width.wgsl)
            .define("stroke_opacity_module", &opacity.wgsl)
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

        // Quantitative normalization domain, computed once for the whole layer.
        let quant_domain = match layer_params.stroke_color.as_ref() {
            Some(ColorMode::Quantitative(params)) => quantitative_domain(params),
            _ => [0.0, 1.0],
        };

        let rings = polygon_rings_from_flat(&layer_params.polygons, &layer_params.polygon_offsets);
        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(rings.len());
        for (poly_index, ring) in rings.iter().enumerate() {
            if ring.len() < 3 {
                continue;
            }
            let stroke = TwoColor::Rgb(cpu_fill_color(layer_params.stroke_color.as_ref(), poly_index, quant_domain));

            // Per-polygon width / opacity (uniform or instanced), matching the
            // GPU width/opacity modes.
            let width_value = cpu_stroke_width(layer_params.stroke_width.as_ref(), poly_index);
            let stroke_opacity = cpu_stroke_opacity(layer_params.stroke_opacity.as_ref(), poly_index) as f64;

            // Stroke width in pixels. In pixel mode it is used directly; in data
            // mode it is transformed through the same pipeline as positions (with
            // w=0, so translations cancel out), mirroring the GPU shader. Stroke
            // width is measured relative to the Y axis, so use the Y screen extent.
            let stroke_width_px = if layer_params.stroke_width_unit_mode == UnitsMode::Data {
                let (_sx, sy) = get_point_size(
                    width_value,
                    width_value,
                    layer_w,
                    layer_h,
                    &camera_view,
                    layer_params.data_unit_mode_x.clone(),
                    layer_params.data_unit_mode_y.clone(),
                    view_params.aspect_ratio_mode.clone(),
                    view_params.aspect_ratio_alignment_mode.clone(),
                    layer_params.model_matrix.as_ref().map(|m| m.as_slice()),
                );
                sy.abs()
            } else {
                width_value
            };

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
                stroke: Some(stroke),
                fill: None,
                linewidth: stroke_width_px as f64,
                opacity: 1.0,
                fill_opacity: 1.0,
                stroke_opacity,
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
