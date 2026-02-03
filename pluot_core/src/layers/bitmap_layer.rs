// Inspired by the DeckGL BitmapLayer
// Reference: https://deck.gl/docs/api-reference/layers/scatterplot-layer

use encase::{ArrayLength, ShaderType, StorageBuffer, UniformBuffer};
use glam::{Mat4, Vec2, Vec3, Vec4};
use serde::{Deserialize, Serialize};

use crate::layers::core::{AspectRatioMode, DrawToCanvas, DrawToSvg, MarginParams, PreparedLayer, UnitsMode, ViewParams};
use crate::wgpu;
use crate::cache::{use_memo_vec_f32, use_memo_vec_i32};
use svg::node::element::Group;
use crate::two::shapes::{TwoCircle, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText};
use crate::two::svg::update_svg;
use crate::layers::scatterplot_vertex::get_point_position;
use crate::log;



#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChannelSettings {
    pub c_index: u32,
    pub window: (f32, f32),
    pub color: (f32, f32, f32), // RGB colors as floats in [0.0, 1.0]
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BitmapLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode: UnitsMode,

    // How positioning works for the bitmap layer:
    // If data_unit_mode = Pixels, then the image is positioned in pixel space,
    // with the origin at the bottom left of the layer's bounds (i.e., margins).
    // If data_unit_mode = Data, then the image is positioned in data units,
    // with the origin at (0,0) in data space, and pixels extending positively in x and y directions.

    // The model_matrix can be used to apply additional affine transformations
    // to the physical dimensions of the image (XYZ),
    // such as translation, rotation, and scaling.
    // For example, the model_matrix can be used to account for pixels that are not square,
    // or to adjust the pixel size.
    // (e.g., most bioimaging formats store images with 1 pixel = 1 micrometer,
    // but without a model_matrix specified we assume that 1 pixel = 1 meter).
    pub model_matrix: Option<[f32; 16]>, // Column-major 4x4 matrix

    pub img_size_w: u32,
    pub img_size_h: u32,
    pub img_size_c: Option<u32>, // Number of channels in the image.
    pub img_size_z: Option<u32>, // Number of z slices in the image.
    pub img_size_t: Option<u32>, // Number of timepoints in the image.

    pub channel_settings: Vec<ChannelSettings>,
    pub z_index: Option<u32>,
    pub t_index: Option<u32>,

    pub opacity: f32,

    // TODO: channel window and color params

    // TODO(ref): pass in references instead of owned Vecs?
    // Would this cause issues when using serde to create layers based on JSON params?
    // TODO: improve naming here
    // TODO: array of channel vecs for multi-channel images?

    // TODO: accept a dimension order array,
    // a shape array (one size per dimension),
    // and then accept a flat Vec for the image data in its original dimension order.

    pub ch0_vec: Vec<u16>, // TODO: generalize to other numeric dtypes?
}

// TODO: defaults for params?


// Internal representation for BitmapLayer and its "descendant" layers.
pub struct BitmapLayerData {
    pub ch0_arr: Vec<u16>,
}


pub struct BitmapLayer {
    view_params: ViewParams,
    layer_params: BitmapLayerParams,
    // TODO: getters?

    // Data may be None prior to runninng prepare().
    data: Option<BitmapLayerData>,
}

impl BitmapLayer {
    pub fn new(
        view_params: ViewParams,
        layer_params: BitmapLayerParams,
    ) -> Self {
        let data = Some(BitmapLayerData {
            // TODO: can cloning be avoided here?
            ch0_arr: layer_params.ch0_vec.clone(),
        });
        Self {
            view_params,
            layer_params,
            data,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for BitmapLayer {
    async fn prepare(&mut self) {

        // TODO: include the layer type in the memoization dependencies?
        // But what if we want multiple layers to be able to reuse the same cached data?
        // Then we should also avoid including the layer_id...

        // TODO: execute getters and cache the results.

        // For now, it is a no-op, since self.data is set in the constructor.
    }
}

#[derive(ShaderType, Debug)]
struct ChannelUniforms {
    window: Vec2,
    color: Vec3,
}

#[derive(ShaderType, Debug)]
struct BitmapLayerUniforms {
    layer_size: Vec2, // (layer_width, layer_height) in pixels
    camera_view: Mat4,   // mat4x4<f32>,
    data_unit_mode: u32, // 0 = pixels, 1 = data units
    aspect_ratio_mode: u32, // 0 = ignore, 1 = contain, 2 = cover
    aspect_ratio_alignment_mode: u32, // 0 = center, 1 = start, 2 = end
    
    img_size: Vec2, // (img_width, img_height) in pixels // TODO: use u32?
    // TODO: pass model_matrix here

    opacity: f32,
    num_channels: ArrayLength,
    // Note: WGSL only allows one runtime-sized array in a struct,
    // and it must be the last field.
    #[shader(size(runtime))]
    channels: Vec<ChannelUniforms>,
}


// We extract this function for reuse in derived scatterplot layers (e.g., ZarrBitmapLayer).
// TODO: is this the best way to share this logic?
// See https://www.youtube.com/watch?v=Phk0C-kLlho
// See https://github.com/linebender/xilem/blob/main/xilem_core/src/views/any_view.rs

// TODO: just pass view_params and layer_params here? But layer_params contains data too, which for some layers is not provided via constructor params...

pub async fn base_draw_bitmap_layer(
    device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass<'_>,
    data: &BitmapLayerData,
    view_params: &ViewParams,
    layer_bounds: &Option<MarginParams>,
    data_unit_mode: &UnitsMode,
    img_size_w: u32,
    img_size_h: u32,
    opacity: f32,
    channel_settings: &[ChannelSettings],
) {
    // Store the ndarray::ArrayD in a WGPU texture.
    // Create a texture to store the image data (R16Uint).

    let num_channels = 1; // TODO: extend to multi-channel images

    let combined_pixel_data: &[u8] = bytemuck::cast_slice(&data.ch0_arr);

    // TODO: does this need to be padded to 256 bytes per row?
    let bytes_per_pixel: u32 = 2; // R16Uint has 2 bytes per pixel.
    let unpadded_bytes_per_row = img_size_w as u32 * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT; // 256
    let padded_bytes_per_row = ((unpadded_bytes_per_row + align - 1) / align) * align;

    let texture_size = wgpu::Extent3d {
        width: img_size_w as u32,
        height: img_size_h as u32,
        depth_or_array_layers: num_channels as u32,
    };
    let image_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Image Texture"),
        size: texture_size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        // R16Uint is a 16-bit unsigned integer format.
        format: wgpu::TextureFormat::R16Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let image_view = image_texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("Image Texture View"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });

    // Upload the pixel data to the texture.
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &image_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        // The pixel data as a byte slice.
        bytemuck::cast_slice(&combined_pixel_data),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(unpadded_bytes_per_row),
            rows_per_image: Some(img_size_h as u32),
        },
        texture_size,
    );

    // Note: WebGPU's shading language (WGSL) treats matrices as column-major.
    let camera_view = view_params.camera_view.unwrap_or([
        // Column 0
        1.0, 0.0, 0.0, 0.0, // Column 1
        0.0, 1.0, 0.0, 0.0, // Column 2
        0.0, 0.0, 1.0, 0.0, // Column 3
        0.0, 0.0, 0.0, 1.0,
    ]);

    let zoom = camera_view[0]; // Assuming uniform scaling in x/y, take the first element (x scaling).
    let translate_x = camera_view[12];
    let translate_y = camera_view[13];

    // Convert zoom level to scale factor
    // scale_factor of 0 means zoom = 1.0 (no zoom)
    // scale_factor of 1 means zoom = 0.5 (zoomed out to half)
    // scale_factor of 2 means zoom = 0.25 (zoomed out to a quarter)
    // scale_factor of 3 means zoom = 0.125 (zoomed out to an eighth)

    // scale_factor of -1 means zoom = 2.0 (zoomed in to double)
    // scale_factor of -2 means zoom = 4.0 (zoomed in to quadruple)
    // scale_factor of -3 means zoom = 8.0 (zoomed in to octuple)
    let scale_factor = (1.0 / zoom).log2();

    log(&format!("scale factor: {}", scale_factor));

    // X translation interpretation:
    // A translate_x value of 1.0 means a point at x=-1.0 (left edge of viewport/screen-quad) is now at the center of the viewport.
    // A translate_x value of 2.0 means a point at x=-1.0 is now at the right edge of the viewport.
    // A translate_x value of -1.0 means a point at x=1.0 (right edge of viewport/screen-quad) is now at the center of the viewport.

    // Zoom interpretation:
    // A zoom value of 0.5 means that points are scaled down by half, so a point at x=-1.0 is now at x=-0.5, and a point at x=1.0 is now at x=0.5.
    // A zoom value of 0.25 means that points are scaled down by a quarter, so a point at x=-1.0 is now at x=-0.25, and a point at x=1.0 is now at x=0.25.

    // Zoom and translation combined interpretation:
    // A translate_x value of 0.5 when zoom = 0.5 means a point at x=-1.0 is now at the center of the viewport, and a point at x=1.0 is now at the right of the viewport.
    // When zoom = 0.5 AND translate_x = 0.5 AND translate_y = 0.5, all four screen-quad [-1 to 1] corner points are in the top right quadrant of the viewport.
    // When zoom = 0.5 AND translate_x = -0.5 AND translate_y = -0.5, all four screen-quad [-1 to 1] corner points are in the bottom left quadrant of the viewport.

    let x_range = 2.0 / zoom; // The range of x values visible in the viewport
    let y_range = 2.0 / zoom; // The range of y values visible in the viewport

    let min_x = (-translate_x - 1.0) / zoom; // translation of (x=-1)
    let max_x = (-translate_x + 1.0) / zoom; // translation of (x=1)
    let min_y = (-translate_y - 1.0) / zoom; // translation of (y=-1)
    let max_y = (-translate_y + 1.0) / zoom; // translation of (y=1)

    // Use layer-specific bounds if not None, otherwise use the view's margins
    // (which may also be None).
    let bounds = if layer_bounds.is_none() {
        &view_params.margins
    } else {
        layer_bounds
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

    // Define the uniforms, matching the WGSL layout (handled by using encase).
    let channel_uniforms: Vec<ChannelUniforms> = channel_settings
        .iter()
        .map(|channel_setting| {
            ChannelUniforms {
                window: Vec2::new(channel_setting.window.0, channel_setting.window.1),
                color: Vec3::new(channel_setting.color.0, channel_setting.color.1, channel_setting.color.2),
            }
        }).collect();

    // Construct the uniform struct using Encase.
    let uniform_struct = BitmapLayerUniforms {
        layer_size: Vec2::new(layer_w, layer_h),
        camera_view: Mat4::from_cols_array(&camera_view),
        data_unit_mode: match data_unit_mode {
            UnitsMode::Pixels => 0,
            UnitsMode::Data => 1,
        },
        aspect_ratio_mode: match view_params.aspect_ratio_mode {
            AspectRatioMode::Ignore => 0,
            AspectRatioMode::Contain => 1,
            AspectRatioMode::Cover => 2,
        },
        aspect_ratio_alignment_mode: 0, // center. TODO
        img_size: Vec2::new(img_size_w as f32, img_size_h as f32),
        opacity,
        num_channels: Default::default(),
        channels: channel_uniforms,
    };

    // Runtime-sized arrays cannot be used with the encase UniformBuffer,
    // and require using StorageBuffer instead.
    let mut buffer = StorageBuffer::new(Vec::<u8>::new());
    buffer.write(&uniform_struct).unwrap();
    let uniform_bytes = buffer.into_inner();

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Storage Buffer for Uniforms"),
        size: uniform_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);


    // Create bind group layout and bind group for positions + uniforms
    let bind_group_layout = device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bioimage BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    // The uniforms buffer.
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    // The image pixel texture.
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
    
    let bind_group = device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bioimage BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&image_view),
                },
            ],
        });

    let shader = device
        .create_shader_module(wgpu::include_wgsl!("shaders/bitmap_layer.wgsl"));

    let render_pipeline_layout = device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

    // TODO: Extract the shared render pipeline logic. There is a lot of duplication here.
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
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    //blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
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

    // Can everything before pass.set_pipeline be cached? Probably not the queue.write calls...

    // Handle margins by adjusting viewport and scissor rect.
    // This allows us to avoid accounting for margins in the shaders, simplifying them.
    // (Shaders can simply assume the full viewport size is the plot area.)
    // Note: these settings will affect all subsequent draw calls in this render pass,
    // so ensure that other layers are setting their own viewport/scissor_rect appropriately.

    // Set viewport so that the (-1 to 1) NDC coordinates map to the desired plot area within the canvas.
    pass.set_viewport(
        margin_left as f32,
        margin_top as f32,
        viewport_w - (margin_left + margin_right) as f32,
        viewport_h - (margin_top + margin_bottom) as f32,
        0.0, // min_depth
        1.0, // max_depth
    );

    // Set scissor rect so that fragments rendered into the margins are clipped.
    // "Sets the scissor rectangle used during the rasterization stage. After transformation into viewport coordinates."
    // "The function of the scissor rectangle resembles set_viewport(), but it does not affect the coordinate system, only which fragments are discarded."
    pass.set_scissor_rect(
        margin_left as u32,
        margin_top as u32,
        (viewport_w - (margin_left + margin_right) as f32) as u32,
        (viewport_h - (margin_top + margin_bottom) as f32) as u32,
    );

    pass.set_pipeline(&render_pipeline);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.draw(0..4, 0..1);
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToCanvas for BitmapLayer {
    async fn draw(&self, device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass) {
        let data = self.data.as_ref().expect("Data was not prepared. Call prepare() first.");
        base_draw_bitmap_layer(
            device, queue, pass,
            data,
            &self.view_params,
            &self.layer_params.bounds,
            &self.layer_params.data_unit_mode,
            self.layer_params.img_size_w,
            self.layer_params.img_size_h,
            self.layer_params.opacity,
            &self.layer_params.channel_settings,
        ).await;
    }
}

pub fn base_draw_bitmap_layer_svg(
    data: &BitmapLayerData,
    view_params: &ViewParams,
    layer_bounds: &Option<MarginParams>,
    data_unit_mode: &UnitsMode,
    layer_id: &str,
) -> Vec<TwoElement> {
    return vec![]; // TODO
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for BitmapLayer {
    async fn draw(&self, group: &Group) -> Group {
        let data = self.data.as_ref().expect("Data was not prepared. Call prepare() first.");

        let view_params = &self.view_params;
        let bounds = &self.layer_params.bounds;

        let svg_elements = base_draw_bitmap_layer_svg(
            data,
            view_params,
            bounds,
            &self.layer_params.data_unit_mode,
            &self.layer_params.layer_id,
        );
        
        // TODO: refactor to avoid the cloning here?
        let updated_group = update_svg(group.clone(), &svg_elements);

        return updated_group.clone();
        
    }
}
