// 3D variant of the PointLayer.
// Reference: point_layer.rs (2D) and plots/scatterplot_3d.rs (old 3D implementation).

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use std::collections::HashMap;
use crate::picking::LayerPickingResult;
use crate::render_traits::{ColorMode, DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, MarginParams, PickableLayer, PreparedLayer, ViewParams};
use crate::viewport::{DataCoord, ScreenCoord};
use crate::shader_modules::{common, ShaderBuilder};
use crate::numeric_data::NumericData;
use crate::color_mode::prepare_color_mode;
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::render_types::GpuContext;
use crate::wgpu;
use crate::two::shapes::TwoElement;
use crate::two::svg::{update_svg, SvgContext};
use crate::layers::point_layer::PointShapeMode;


#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Point3dLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub point_radius: f32,
    pub point_shape_mode: PointShapeMode,

    // How to color each point. See [`ColorMode`]: modes carrying `NumericData`
    // (instanced/categorical/quantitative) supply one or more per-element value
    // arrays, which are uploaded to the GPU as textures at draw time.
    pub fill_color: Option<ColorMode>,

    // Per-point X/Y/Z coordinates. Each may be any supported numeric dtype
    // (8–64 bit int/uint, or 32/64-bit float), and may differ. The data is
    // uploaded to the GPU as a texture at its native width wherever possible
    // (see `NumericData::create_data_texture`).
    pub position_x: NumericData,
    pub position_y: NumericData,
    pub position_z: NumericData,
    pub labels_vec: Arc<Vec<i32>>,
}

impl Default for Point3dLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            point_radius: 1.0,
            point_shape_mode: PointShapeMode::Circle,
            fill_color: None,
            position_x: NumericData::Float32(Arc::new(vec![])),
            position_y: NumericData::Float32(Arc::new(vec![])),
            position_z: NumericData::Float32(Arc::new(vec![])),
            labels_vec: Arc::new(vec![]),
        }
    }
}

pub struct Point3dLayer {
    view_params: ViewParams,
    layer_params: Point3dLayerParams,
}

impl Point3dLayer {
    pub fn new(
        view_params: ViewParams,
        layer_params: Point3dLayerParams,
    ) -> Self {
        Self {
            view_params,
            layer_params,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for Point3dLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        return PrepareResult {
            bailed_early: false,
        };
    }
}

#[derive(ShaderType, Debug)]
struct Point3dLayerUniforms {
    layer_size: Vec2, // (layer_width, layer_height) in pixels
    camera_view: Mat4,   // mat4x4<f32>,
    point_radius: f32,  // radius of each point in pixels
    point_shape_mode: u32, // 0 = square, 1 = circle
    fill_color_mode: u32,     // see ColorMode::shader_mode()
    fill_color: Vec4,         // rgba color used by the UniformRgb mode
    fill_color_reverse: u32,  // 1 = reverse the quantitative colormap
    fill_color_domain: Vec2,  // (min, max) normalization domain for quantitative mode
}

// First bind-group binding index used for color-mode value/palette texture(s).
// Bindings 0-3 are the uniforms buffer and the X/Y/Z position textures.
const COLOR_BINDING_START: u32 = 4;

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for Point3dLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let GpuContext { device, queue } = gpu_context;
        let Self { layer_params, view_params } = self;

        let n = layer_params.labels_vec.len();

        // Upload the X, Y, and Z coordinate arrays into single-channel 2D
        // textures, each at its native byte width wherever possible; see
        // `NumericData::create_data_texture`.
        let (x_texture_view, x_dtype) =
            layer_params.position_x.create_data_texture(device, queue, "X Coordinates Texture");
        let (y_texture_view, y_dtype) =
            layer_params.position_y.create_data_texture(device, queue, "Y Coordinates Texture");
        let (z_texture_view, z_dtype) =
            layer_params.position_z.create_data_texture(device, queue, "Z Coordinates Texture");

        // Build the GPU-side color resources for the configured color mode. Modes
        // that carry per-element `NumericData` upload it as one or more textures
        // (bound from COLOR_BINDING_START onward) and contribute the WGSL
        // `get_fill_color` function injected into the shader below.
        let color = prepare_color_mode(device, queue, layer_params.fill_color.as_ref(), COLOR_BINDING_START);

        // Note: WebGPU's shading language (WGSL) treats matrices as column-major.
        let camera_view = view_params.camera_view.unwrap_or([
            // Column 0
            1.0, 0.0, 0.0, 0.0, // Column 1
            0.0, 1.0, 0.0, 0.0, // Column 2
            0.0, 0.0, 1.0, 0.0, // Column 3
            0.0, 0.0, 0.0, 1.0,
        ]);

        // Use layer-specific bounds if not None, otherwise use the view's margins.
        let bounds = if layer_params.bounds.is_none() {
            &view_params.margins
        } else {
            &layer_params.bounds
        };

        let margin_top = if let Some(margin_params) = &bounds {
            margin_params.margin_top.unwrap_or(0.0)
        } else { 0.0 } as f64;
        let margin_right = if let Some(margin_params) = &bounds {
            margin_params.margin_right.unwrap_or(0.0)
        } else { 0.0 } as f64;
        let margin_bottom = if let Some(margin_params) = &bounds {
            margin_params.margin_bottom.unwrap_or(0.0)
        } else { 0.0 } as f64;
        let margin_left = if let Some(margin_params) = &bounds {
            margin_params.margin_left.unwrap_or(0.0)
        } else { 0.0 } as f64;

        let viewport_w = view_params.width as f32;
        let viewport_h = view_params.height as f32;

        let layer_w = viewport_w - (margin_left + margin_right) as f32;
        let layer_h = viewport_h - (margin_top + margin_bottom) as f32;

        // Construct the uniform struct using Encase.
        let uniform_struct = Point3dLayerUniforms {
            layer_size: Vec2::new(layer_w, layer_h),
            camera_view: Mat4::from_cols_array(&camera_view),
            point_radius: layer_params.point_radius,
            point_shape_mode: match layer_params.point_shape_mode {
                PointShapeMode::Square => 0,
                PointShapeMode::Circle => 1,
            },
            fill_color_mode: color.mode,
            fill_color: Vec4::from_array(color.static_color),
            fill_color_reverse: color.reverse,
            fill_color_domain: Vec2::from_array(color.domain),
        };

        let mut buffer = UniformBuffer::new(Vec::<u8>::new());
        buffer.write(&uniform_struct).unwrap();
        let uniform_bytes = buffer.into_inner();

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: uniform_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

        // Create bind group layout and bind group for positions + uniforms.
        // Bindings 0-3 are fixed (uniforms + the three position textures); the
        // color-mode textures follow at binding `COLOR_BINDING_START` onward,
        // matching the WGSL declarations injected below.
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
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: x_dtype.binding_sample_type(),
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: y_dtype.binding_sample_type(),
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: z_dtype.binding_sample_type(),
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
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
        let bind_group_layout = device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Point3dLayer BGL"),
                entries: &bgl_entries,
            });

        let mut bg_entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&x_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&y_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&z_texture_view),
            },
        ];
        for (i, tex) in color.textures.iter().enumerate() {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: COLOR_BINDING_START + i as u32,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        let bind_group = device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Point3dLayer BG"),
                layout: &bind_group_layout,
                entries: &bg_entries,
            });

        let shader_source = ShaderBuilder::new(include_str!("shaders/point_3d_layer.wgsl"))
            .inject_texture_sample_type("x_coords", x_dtype)
            .inject_texture_sample_type("y_coords", y_dtype)
            .inject_texture_sample_type("z_coords", z_dtype)
            // Color-mode specialization: the flat-index texel helper plus the
            // assembled color module (bindings + `get_fill_color`).
            .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
            .define("color_module", &color.wgsl)
            .build();
        let shader = device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("point_3d_layer.wgsl"),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });

        let render_pipeline_layout = device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            });

        let render_pipeline = device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Render Pipeline"),
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

        // Handle margins by adjusting viewport and scissor rect.
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

        pass.draw(0..4, 0..(n as u32));
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for Point3dLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for Point3dLayer {
    async fn draw(&self, _ctx: &mut SvgContext) {
        // SVG rendering not supported for 3D layers.
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "Point3dLayer",
        create_layer: |value, view_params| {
            let params: Point3dLayerParams = serde_json::from_value(value).unwrap();
            Box::new(Point3dLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for Point3dLayer {
    fn pick(&self, _screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let DataCoord::ThreeD { x: cx, y: cy, z: cz } = data_coord? else {
            return None;
        };

        let n = self.layer_params.labels_vec.len();
        if n == 0 {
            return None;
        }

        let mut min_dist_sq = f32::MAX;
        let mut closest_idx = 0usize;

        for i in 0..n {
            let dx = self.layer_params.position_x.get_f32(i) - cx;
            let dy = self.layer_params.position_y.get_f32(i) - cy;
            let dz = self.layer_params.position_z.get_f32(i) - cz;
            let dist_sq = dx * dx + dy * dy + dz * dz;
            if dist_sq < min_dist_sq {
                min_dist_sq = dist_sq;
                closest_idx = i;
            }
        }

        let mut info = HashMap::new();
        info.insert("index".to_string(), closest_idx.to_string());
        info.insert("label".to_string(), self.layer_params.labels_vec[closest_idx].to_string());
        info.insert("x".to_string(), self.layer_params.position_x.format_element(closest_idx));
        info.insert("y".to_string(), self.layer_params.position_y.format_element(closest_idx));
        info.insert("z".to_string(), self.layer_params.position_z.format_element(closest_idx));

        Some(LayerPickingResult {
            layer_id: self.layer_params.layer_id.clone(),
            info,
        })
    }
}
