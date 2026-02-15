// Inspired by the DeckGL BitmapLayer
// Reference: https://deck.gl/docs/api-reference/layers/scatterplot-layer

use encase::{ArrayLength, ShaderType, StorageBuffer, UniformBuffer};
use glam::{Mat4, Vec2, Vec3, Vec4};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::layer_traits::{AspectRatioMode, DrawToCanvas, DrawToSvg, MarginParams, PreparedLayer, UnitsMode, ViewParams};
use crate::params::{PrepareResult, RenderResult};
use crate::wgpu;
use crate::cache::{use_memo_vec_f32, use_memo_vec_i32};
use svg::node::element::Group;
use crate::two::shapes::{TwoCircle, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText};
use crate::two::svg::update_svg;
use crate::layers::position_utils::get_point_position;
use crate::log;


/// Typed numeric array supporting multiple dtypes.
/// Serialized as a tagged enum, e.g. `{"Uint16": [1, 2, 3]}`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum NumericData {
    Uint8(Arc<Vec<u8>>),
    Uint16(Arc<Vec<u16>>),
    Uint32(Arc<Vec<u32>>),
    Uint64(Arc<Vec<u64>>),
    Int8(Arc<Vec<i8>>),
    Int16(Arc<Vec<i16>>),
    Int32(Arc<Vec<i32>>),
    Int64(Arc<Vec<i64>>),
    Float32(Arc<Vec<f32>>),
    Float64(Arc<Vec<f64>>),
}

impl NumericData {
    /// Number of elements in the array.
    fn len(&self) -> usize {
        match self {
            NumericData::Uint8(v) => v.len(),
            NumericData::Uint16(v) => v.len(),
            NumericData::Uint32(v) => v.len(),
            NumericData::Uint64(v) => v.len(),
            NumericData::Int8(v) => v.len(),
            NumericData::Int16(v) => v.len(),
            NumericData::Int32(v) => v.len(),
            NumericData::Int64(v) => v.len(),
            NumericData::Float32(v) => v.len(),
            NumericData::Float64(v) => v.len(),
        }
    }

    /// Get element at index as f32.
    fn get_f32(&self, idx: usize) -> f32 {
        match self {
            NumericData::Uint8(v) => v[idx] as f32,
            NumericData::Uint16(v) => v[idx] as f32,
            NumericData::Uint32(v) => v[idx] as f32,
            NumericData::Uint64(v) => v[idx] as f32,
            NumericData::Int8(v) => v[idx] as f32,
            NumericData::Int16(v) => v[idx] as f32,
            NumericData::Int32(v) => v[idx] as f32,
            NumericData::Int64(v) => v[idx] as f32,
            NumericData::Float32(v) => v[idx],
            NumericData::Float64(v) => v[idx] as f32,
        }
    }
}

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

    /// The order of dimensions in the flat `data` array.
    /// Must contain 'X' and 'Y'. May also contain 'Z', 'C', and 'T'.
    /// Examples: "XY", "XYC", "XYCZT", "TCZYX".
    /// Dimensions can be in any order.
    pub dimension_order: String,

    /// The size of each dimension, in the same order as `dimension_order`.
    /// For example, if `dimension_order` is "XYCZT" and the image is
    /// 256x256 with 3 channels, 10 z-slices, and 5 timepoints,
    /// then `shape` would be [256, 256, 3, 10, 5].
    pub shape: Vec<u32>,

    pub channel_settings: Vec<ChannelSettings>,
    /// The z-slice index to render (0-based). Required if 'Z' is in dimension_order.
    pub z_index: Option<u32>,
    /// The timepoint index to render (0-based). Required if 'T' is in dimension_order.
    pub t_index: Option<u32>,

    pub opacity: f32,

    /// Flat array of pixel data in the order specified by `dimension_order`.
    /// Supports multiple numeric dtypes (u8, u16, u32, u64, i8, i16, i32, i64, f32, f64).
    pub data: NumericData,
}

// TODO: defaults for params?


pub struct BitmapLayer {
    view_params: ViewParams,
    layer_params: BitmapLayerParams,
}

impl BitmapLayer {
    pub fn new(
        view_params: ViewParams,
        layer_params: BitmapLayerParams,
    ) -> Self {
        // Validate that dimension_order string is valid.
        let dim_order = &layer_params.dimension_order;
        let valid_dims: &[char] = &['X', 'Y', 'Z', 'C', 'T'];

        // May not contain any invalid characters.
        for ch in dim_order.chars() {
            if !valid_dims.contains(&ch) {
                panic!("Invalid character '{}' in dimension_order \"{}\". Valid characters are: X, Y, Z, C, T.", ch, dim_order);
            }
        }

        // Must contain 'X' and 'Y'.
        if !dim_order.contains('X') || !dim_order.contains('Y') {
            panic!("dimension_order \"{}\" must contain both 'X' and 'Y'.", dim_order);
        }

        // May not contain duplicate dimensions.
        let mut seen = std::collections::HashSet::new();
        for ch in dim_order.chars() {
            if !seen.insert(ch) {
                panic!("Duplicate dimension '{}' in dimension_order \"{}\".", ch, dim_order);
            }
        }

        // Validate that the colors in ChannelSettings are in the range [0.0, 1.0].
        for (i, channel) in layer_params.channel_settings.iter().enumerate() {
            let (r, g, b) = channel.color;
            if !(0.0..=1.0).contains(&r) || !(0.0..=1.0).contains(&g) || !(0.0..=1.0).contains(&b) {
                panic!(
                    "Channel {} color ({}, {}, {}) has components outside the range [0.0, 1.0].",
                    i, r, g, b
                );
            }
        }
        Self {
            view_params,
            layer_params,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for BitmapLayer {
    async fn prepare(&mut self) -> PrepareResult {

        // TODO: cache the sliced texture data here for the specified z/t/channel settings.

        // For now, it is a no-op, since self.data is set in the constructor.

        return PrepareResult {
            bailed_early: false,
        };
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


/// Parse dimension_order and shape into a map from dimension char to (index_in_order, size).
fn parse_dimensions(dimension_order: &str, shape: &[u32]) -> std::collections::HashMap<char, (usize, u32)> {
    assert_eq!(
        dimension_order.len(),
        shape.len(),
        "dimension_order length ({}) must match shape length ({})",
        dimension_order.len(),
        shape.len()
    );
    dimension_order
        .chars()
        .enumerate()
        .zip(shape.iter())
        .map(|((i, ch), &sz)| (ch, (i, sz)))
        .collect()
}

/// Compute the stride for each dimension given the shape and dimension order.
/// Strides are in units of elements (not bytes).
/// The last dimension in the order is the fastest-varying (stride=1).
fn compute_strides(shape: &[u32]) -> Vec<usize> {
    let ndim = shape.len();
    let mut strides = vec![1usize; ndim];
    for i in (0..ndim.saturating_sub(1)).rev() {
        strides[i] = strides[i + 1] * shape[i + 1] as usize;
    }
    strides
}

/// Extract a 2D XY slice from the flat nD data array for a given channel, z-index, and t-index.
/// Converts to f32 regardless of the source dtype.
/// Returns a Vec<f32> of length (img_w * img_h) in row-major order (Y outer, X inner).
fn extract_xy_slice(
    data: &NumericData,
    dimension_order: &str,
    shape: &[u32],
    c_index: Option<u32>,
    z_index: Option<u32>,
    t_index: Option<u32>,
) -> Vec<f32> {
    let dims = parse_dimensions(dimension_order, shape);
    let strides = compute_strides(shape);

    let (x_dim_idx, img_w) = dims.get(&'X').expect("dimension_order must contain 'X'");
    let (y_dim_idx, img_h) = dims.get(&'Y').expect("dimension_order must contain 'Y'");
    let img_w = *img_w as usize;
    let img_h = *img_h as usize;

    // Compute the base offset from fixed dimensions (C, Z, T).
    let mut base_offset: usize = 0;
    if let Some(&(dim_idx, _sz)) = dims.get(&'C') {
        let c = c_index.unwrap_or(0) as usize;
        base_offset += c * strides[dim_idx];
    }
    if let Some(&(dim_idx, _sz)) = dims.get(&'Z') {
        let z = z_index.unwrap_or(0) as usize;
        base_offset += z * strides[dim_idx];
    }
    if let Some(&(dim_idx, _sz)) = dims.get(&'T') {
        let t = t_index.unwrap_or(0) as usize;
        base_offset += t * strides[dim_idx];
    }

    let x_stride = strides[*x_dim_idx];
    let y_stride = strides[*y_dim_idx];

    let mut slice = Vec::with_capacity(img_w * img_h);
    for y in 0..img_h {
        for x in 0..img_w {
            let idx = base_offset + y * y_stride + x * x_stride;
            slice.push(data.get_f32(idx));
        }
    }
    slice
}

pub async fn base_draw_bitmap_layer(
    device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass<'_>,
    view_params: &ViewParams,
    layer_params: &BitmapLayerParams,
) {
    let dims = parse_dimensions(&layer_params.dimension_order, &layer_params.shape);
    let (_, img_w) = dims.get(&'X').expect("dimension_order must contain 'X'");
    let (_, img_h) = dims.get(&'Y').expect("dimension_order must contain 'Y'");
    let img_w = *img_w;
    let img_h = *img_h;

    let num_channels = layer_params.channel_settings.len() as u32;

    // Extract a 2D XY slice for each channel and concatenate into a single buffer
    // for upload as a 2D texture array (one layer per channel).
    // Values are converted to f32 regardless of the source dtype.
    let mut combined_pixel_data: Vec<f32> = Vec::with_capacity(
        (img_w * img_h * num_channels) as usize,
    );
    for channel_setting in &layer_params.channel_settings {
        let has_c_dim = dims.contains_key(&'C');
        let c_index = if has_c_dim { Some(channel_setting.c_index) } else { None };
        let slice = extract_xy_slice(
            &layer_params.data,
            &layer_params.dimension_order,
            &layer_params.shape,
            c_index,
            layer_params.z_index,
            layer_params.t_index,
        );
        combined_pixel_data.extend_from_slice(&slice);
    }

    let bytes_per_pixel: u32 = 4; // R32Float has 4 bytes per pixel.
    let unpadded_bytes_per_row = img_w * bytes_per_pixel;

    let texture_size = wgpu::Extent3d {
        width: img_w,
        height: img_h,
        depth_or_array_layers: num_channels,
    };
    let image_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Image Texture"),
        size: texture_size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        // TODO: make format dynamic, dependent on the numeric dtype of the `data` array.
        // Then, use WESL for to dynamically specify the dtype used for the corresponding `texture_2d_array` in the shader.
        format: wgpu::TextureFormat::R32Float,
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
        bytemuck::cast_slice(&combined_pixel_data),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(unpadded_bytes_per_row),
            rows_per_image: Some(img_h),
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

    // Define the uniforms, matching the WGSL layout (handled by using encase).
    let channel_uniforms: Vec<ChannelUniforms> = layer_params.channel_settings
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
        data_unit_mode: match layer_params.data_unit_mode {
            UnitsMode::Pixels => 0,
            UnitsMode::Data => 1,
        },
        aspect_ratio_mode: match view_params.aspect_ratio_mode {
            AspectRatioMode::Ignore => 0,
            AspectRatioMode::Contain => 1,
            AspectRatioMode::Cover => 2,
        },
        aspect_ratio_alignment_mode: 0, // center. TODO
        img_size: Vec2::new(img_w as f32, img_h as f32),
        opacity: layer_params.opacity,
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
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
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
        base_draw_bitmap_layer(
            device, queue, pass,
            &self.view_params,
            &self.layer_params,
        ).await;
    }
}

pub fn base_draw_bitmap_layer_svg(
    view_params: &ViewParams,
    layer_params: &BitmapLayerParams,
) -> Vec<TwoElement> {
    return vec![]; // TODO
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for BitmapLayer {
    async fn draw(&self, group: &Group) -> Group {
        let svg_elements = base_draw_bitmap_layer_svg(
            &self.view_params,
            &self.layer_params,
        );

        // TODO: refactor to avoid the cloning here?
        let updated_group = update_svg(group.clone(), &svg_elements);

        return updated_group.clone();

    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "BitmapLayer",
        create_layer: |value, view_params| {
            let params: BitmapLayerParams = serde_json::from_value(value).unwrap();
            Box::new(BitmapLayer::new(view_params.clone(), params))
        },
    }
}
