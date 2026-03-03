// Inspired by the DeckGL BitmapLayer
// Reference: https://deck.gl/docs/api-reference/layers/scatterplot-layer

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use encase::{ArrayLength, ShaderType, StorageBuffer, UniformBuffer};
use glam::{Mat4, Vec2, Vec3, Vec4};
use image::{codecs::bmp::BmpEncoder, ExtendedColorType, ImageBuffer, ImageEncoder, Rgba};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;

use crate::render_traits::{AspectRatioMode, DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, MarginParams, PickableLayer, PreparedLayer, UnitsMode, ViewParams};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::render_types::GpuContext;
use crate::wgpu;
use crate::cache::{use_memo_vec_f32, use_memo_vec_i32};
use crate::two::shapes::{TwoElement, TwoImage, TwoImageRenderingStyle};
use crate::two::svg::{update_svg, SvgContext};
use crate::positioning::get_point_position;
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

    /// Convert the entire data array to f32 in one go.
    /// For Float32 data, this borrows the existing slice via bytemuck (zero-copy).
    /// For other dtypes, values are batch-converted to f32 via iterators.
    fn as_f32(&self) -> Cow<'_, [f32]> {
        match self {
            NumericData::Float32(v) => Cow::Borrowed(v.as_slice()),
            NumericData::Uint8(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Uint16(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Uint32(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Uint64(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Int8(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Int16(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Int32(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Int64(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Float64(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChannelSettings {
    // pub c_index: u32, // Rather than using c_index, we can just assume that the channel settings are provided in-order.
    pub window: (f32, f32),
    pub color: (f32, f32, f32), // RGB colors as floats in [0.0, 1.0]
}


#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DimensionOrder {
    // The data array must be 3D, with dimensions in the specified order.
    // For example, if dimension_order is "XYC", then the data array must be in (X, Y, C) order,
    // and the shape would be [img_w, img_h, num_channels].
    // For 2D images, the parent can simply specify dimension_order = "CXY" with num_channels of 1,
    // as the contiguous data array will be the same regardless of whether the order is CXY vs XY.
    // This also allows avoiding handling the lack-of-C-dimension as a special case everywhere,
    // and forces the parent to provide channel settings.
    // For 4D and 5D images with T and Z dimensions, the parent layer would be expected to
    // slice the data into 3D XY(C) slices for the specified z_index and t_index before passing to the shader.
    // Similarly, if the original data array contains more channels than being visualized,
    // the parent layer is responsible to slice them and provide them in the order that corresponds with
    // the provided channel_settings c_index values.
    CXY,
    CYX,
    XCY,
    XYC,
    YCX,
    YXC,
}

impl DimensionOrder {
    /// Returns the string representation of the dimension order (e.g., "CYX").
    pub fn as_str(&self) -> &'static str {
        match self {
            DimensionOrder::CXY => "CXY",
            DimensionOrder::CYX => "CYX",
            DimensionOrder::XCY => "XCY",
            DimensionOrder::XYC => "XYC",
            DimensionOrder::YCX => "YCX",
            DimensionOrder::YXC => "YXC",
        }
    }

    /// Returns the number of dimensions.
    pub fn num_dims(&self) -> usize {
        self.as_str().len()
    }

    /// Returns the position of the channel dimension in the shape array, if present.
    pub fn channel_dim_index(&self) -> usize {
        self.as_str().chars()
            .position(|c| c == 'C')
            .expect("Dimension order must include a channel dimension")
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BitmapLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode: UnitsMode,

    // (x_offset, y_offset) in pixels, applied before model_matrix,
    // to enable this layer to be used to render an individual "tile" of a larger image layer,
    // where tiles correspond to the way the original image array is chunked/tiled on disk.
    pub pixel_offset: Option<(u32, u32)>,

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
    pub dimension_order: DimensionOrder,

    /// The size of each dimension, in the same order as `dimension_order`.
    /// For example, if `dimension_order` is "XYCZT" and the image is
    /// 256x256 with 3 channels, 10 z-slices, and 5 timepoints,
    /// then `shape` would be [256, 256, 3, 10, 5].
    // TODO: use a 3-element tuple here instead?
    pub shape: Vec<u32>,

    // TODO: allow to specify photometric_interpretation here, to easily support RGB images?
    // Alternatively, delegate to some parent layer.

    pub channel_settings: Vec<ChannelSettings>,

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
        // Validate that dimension_order, shape, channel_settings, and data length are consistent with each other.
        let expected_num_dims = layer_params.dimension_order.num_dims();

        // 1. shape length must match the number of dimensions in dimension_order.
        if layer_params.shape.len() != expected_num_dims {
            panic!(
                "shape length ({}) must match the number of dimensions in dimension_order {:?} ({})",
                layer_params.shape.len(),
                layer_params.dimension_order,
                expected_num_dims,
            );
        }

        // 2. The product of all shape dimensions must equal the data length.
        let expected_data_len: usize = layer_params.shape.iter().map(|&s| s as usize).product();
        if layer_params.data.len() != expected_data_len {
            panic!(
                "data length ({}) must equal the product of shape dimensions {:?} (= {})",
                layer_params.data.len(),
                layer_params.shape,
                expected_data_len,
            );
        }

        // 3. channel_settings must not be empty.
        if layer_params.channel_settings.is_empty() {
            panic!("channel_settings must contain at least one channel");
        }

        // 4. Validate the number of provided channel_settings against the size of the C dimension.
        let c_dim_idx = layer_params.dimension_order.channel_dim_index();
        let c_size = layer_params.shape[c_dim_idx];
        let num_channel_settings = layer_params.channel_settings.len() as u32;
        if num_channel_settings != c_size {
            panic!(
                "channel_settings length {} did not match C dimension size ({})",
                num_channel_settings,
                c_size,
            );
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
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {

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
    pixel_offset: Vec2, // (x_offset, y_offset) in pixels

    model_matrix: Mat4, // mat4x4<f32> for affine transformations of the image.

    opacity: f32,

    // Strides for each dimension (in units of f32 elements),
    // allowing the shader to index into the flat data buffer
    // regardless of the dimension ordering (e.g., CYX vs YXC).
    x_stride: u32,
    y_stride: u32,
    c_stride: u32,

    num_channels: ArrayLength,
    // Note: WGSL only allows one runtime-sized array in a struct,
    // and it must be the last field.
    #[shader(size(runtime))]
    channels: Vec<ChannelUniforms>,
}


/// Parse dimension_order and shape into a map from dimension char to (index_in_order, size).
fn parse_dimensions(dimension_order: &DimensionOrder, shape: &[u32]) -> std::collections::HashMap<char, (usize, u32)> {
    let dim_str = dimension_order.as_str();
    assert_eq!(
        dim_str.len(),
        shape.len(),
        "dimension_order {:?} length ({}) must match shape length ({})",
        dimension_order,
        dim_str.len(),
        shape.len()
    );
    dim_str
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




pub async fn base_draw_bitmap_layer(
    gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass<'_>,
    view_params: &ViewParams,
    layer_params: &BitmapLayerParams,
) {
    let GpuContext { device, queue } = gpu_context;
    let dims = parse_dimensions(&layer_params.dimension_order, &layer_params.shape);
    let (x_dim_idx, img_w) = dims.get(&'X').expect("dimension_order must contain 'X'");
    let (y_dim_idx, img_h) = dims.get(&'Y').expect("dimension_order must contain 'Y'");
    let (c_dim_idx, _) = dims.get(&'C').expect("dimension_order must contain 'C'");
    let img_w = *img_w;
    let img_h = *img_h;

    let num_channels = layer_params.channel_settings.len() as u32;

    // Compute strides so the shader can index into the flat data buffer
    // regardless of the dimension ordering (e.g., CYX vs YXC).
    let strides = compute_strides(&layer_params.shape);
    let x_stride = strides[*x_dim_idx] as u32;
    let y_stride = strides[*y_dim_idx] as u32;
    let c_stride = strides[*c_dim_idx] as u32;

    // Convert the entire data array to f32 in one go using bytemuck (zero-copy for Float32).
    // The flat memory layout is preserved; the shader uses strides to handle dimension ordering.
    let data_f32 = layer_params.data.as_f32();
    let data_bytes: &[u8] = bytemuck::cast_slice(&data_f32);

    // Upload the flat f32 data as a storage buffer (not a texture),
    // so the shader can index into it using strides.
    // TODO: switch back to using a texture if performance becomes an issue;
    // textures supposedly have optimizations for 2D spatial access patterns,
    // but we may need to do more CPU transposing for that to be effective.
    let image_data_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Image Data Storage Buffer"),
        size: data_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&image_data_buffer, 0, data_bytes);

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

    // log(&format!("scale factor: {}", scale_factor));

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
        pixel_offset: Vec2::new(
            layer_params.pixel_offset.map_or(0.0, |(x, _)| x as f32),
            layer_params.pixel_offset.map_or(0.0, |(_, y)| y as f32),
        ),
        model_matrix: Mat4::from_cols_array(&layer_params.model_matrix.unwrap_or([
            // Column 0
            1.0, 0.0, 0.0, 0.0, // Column 1
            0.0, 1.0, 0.0, 0.0, // Column 2
            0.0, 0.0, 1.0, 0.0, // Column 3
            0.0, 0.0, 0.0, 1.0,
        ])),
        opacity: layer_params.opacity,
        x_stride,
        y_stride,
        c_stride,
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
                    // The image pixel data storage buffer.
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
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
                    resource: image_data_buffer.as_entire_binding(),
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
impl DrawToRasterGpu for BitmapLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        base_draw_bitmap_layer(
            gpu_context, pass,
            &self.view_params,
            &self.layer_params,
        ).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for BitmapLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

/// Encode raw RGBA pixels as a BMP byte stream using the `image` crate.
/// TODO: it seems this is not supported in all environments (e.g., macOS Preview),
/// so we may want to switch to a different encoding approach.
fn encode_bmp_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let img: ImageBuffer<Rgba<u8>, _> =
        ImageBuffer::from_raw(width, height, rgba.to_vec()).expect("valid dimensions");
    let mut buf = Vec::new();
    BmpEncoder::new(&mut buf)
        .write_image(img.as_raw(), width, height, ExtendedColorType::Rgba8)
        .expect("BMP encode");
    buf
}

/// Encode bytes to a base64 string using the `base64` crate.
fn base64_encode(data: &[u8]) -> String {
    BASE64_STANDARD.encode(data)
}

pub fn base_draw_bitmap_layer_svg(
    view_params: &ViewParams,
    layer_params: &BitmapLayerParams,
) -> Vec<TwoElement> {
    let dims = parse_dimensions(&layer_params.dimension_order, &layer_params.shape);
    let (x_dim_idx, img_w) = dims[&'X'];
    let (y_dim_idx, img_h) = dims[&'Y'];
    let (c_dim_idx, _)     = dims[&'C'];

    let strides = compute_strides(&layer_params.shape);
    let x_stride = strides[x_dim_idx];
    let y_stride = strides[y_dim_idx];
    let c_stride = strides[c_dim_idx];

    let mut rgba = vec![0u8; (img_w * img_h * 4) as usize];

    for y in 0..img_h as usize {
        for x in 0..img_w as usize {
            // Accumulate blended RGB across channels (additive).
            let mut r = 0.0f32;
            let mut g = 0.0f32;
            let mut b = 0.0f32;

            for (c, ch) in layer_params.channel_settings.iter().enumerate() {
                let idx = y * y_stride + x * x_stride + c * c_stride;
                let raw = layer_params.data.get_f32(idx);

                // Apply window [lo, hi] → normalize to [0, 1], clamp.
                let (lo, hi) = ch.window;
                let t = ((raw - lo) / (hi - lo)).clamp(0.0, 1.0);

                r += t * ch.color.0;
                g += t * ch.color.1;
                b += t * ch.color.2;
            }

            let pixel_idx = (y * img_w as usize + x) * 4;
            rgba[pixel_idx]     = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
            rgba[pixel_idx + 1] = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
            rgba[pixel_idx + 2] = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
            rgba[pixel_idx + 3] = 255 as u8;
        }
    }

    let bmp = encode_bmp_rgba(img_w, img_h, &rgba);
    let href = format!("data:image/bmp;base64,{}", base64_encode(&bmp));

    let margin_top = layer_params.bounds.as_ref()
        .or(view_params.margins.as_ref())
        .and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
    let margin_left = layer_params.bounds.as_ref()
        .or(view_params.margins.as_ref())
        .and_then(|m| m.margin_left).unwrap_or(0.0) as f64;
    let margin_right = layer_params.bounds.as_ref()
        .or(view_params.margins.as_ref())
        .and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
    let margin_bottom = layer_params.bounds.as_ref()
        .or(view_params.margins.as_ref())
        .and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;

    let draw_w = view_params.width as f64 - margin_left - margin_right;
    let draw_h = view_params.height as f64 - margin_top - margin_bottom;

    vec![TwoElement::Image(TwoImage {
        // TODO: fix the positioning so that it matches the logic in the shader.
        // It will need to use position_utils, but will also be more complicated.
        x: margin_left,
        y: margin_top,
        width: draw_w,
        height: draw_h,
        href,
        opacity: layer_params.opacity as f64,
        image_rendering_style: Some(TwoImageRenderingStyle::Pixelated),
    })]
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for BitmapLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        // Draw to an SVG <image/> element.
        // Do this as naively as possible; for example with a nested for loop over the pixels, computing the color and position of each,
        // and finally converting to an image HREF.

        let svg_elements = base_draw_bitmap_layer_svg(
            &self.view_params,
            &self.layer_params,
        );
        update_svg(ctx, &svg_elements);
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

impl PickableLayer for BitmapLayer {}
