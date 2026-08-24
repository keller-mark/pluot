// Inspired by the Vitessce/DeckGL BitmaskLayer.
// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/layers/BitmaskLayerBeta.js
//
// A BitmaskLayer accepts the same positioning/data-shape parameters as
// `BitmapLayer` (`dimension_order`, `shape`, `data`, `model_matrix`,
// `pixel_offset`, `bounds`, `data_unit_mode_x`/`y`) -- the "C" dimension holds
// one segmentation "channel" per slice, i.e. a flat array of per-pixel object
// ids (0 meaning "no object"/background) for that channel. Only
// `channel_settings` differs from `BitmapLayer`: instead of an intensity
// window and pseudocolor, each channel is colored independently via a
// [`ColorMode`] -- either a single static color, per-object RGB, a
// named/custom categorical palette indexed by object id (i.e. "set colors"),
// or a quantitative colormap applied to a per-object feature value. Here the
// "index" a `ColorMode` reads is the object id (minus one, since 0 is
// reserved for "no object"), rather than a per-instance index as in e.g.
// `RectLayer`.

use encase::{ArrayLength, ShaderType, StorageBuffer};
use glam::{DMat4, DVec4, Mat4, Vec2, Vec4};
use image::{codecs::png::PngEncoder, ExtendedColorType, ImageBuffer, ImageEncoder, Rgba};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::color_mode::{cpu_fill_color, quantitative_domain};
use crate::colormaps_categorical;
use crate::layers::bitmap_layer::{compute_strides, parse_dimensions, DimensionOrder};
use crate::numeric_data::NumericData;
use crate::picking::LayerPickingResult;
use crate::render_traits::{
    AspectRatioAlignmentMode, AspectRatioMode, ColorMode, DrawToRasterCpu, DrawToRasterGpu,
    DrawToSvg, MarginParams, PickableLayer, PreparedLayer, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult, RenderResult};
use crate::shader_modules::{
    bitmask_channel, colormaps as wgsl_colormaps, common, get_channel_color, ShaderBuilder, TextureDtype,
};
use crate::two::shapes::{TwoElement, TwoGroup, TwoImage, TwoImageRenderingStyle};
use crate::two::svg::{update_svg, SvgContext};
use crate::viewport::{DataCoord, ScreenCoord};
use crate::wgpu;

/// Per-channel settings for [`BitmaskLayer`]. The channel's mask data itself
/// lives in `BitmaskLayerParams::data` (one slice per the "C" dimension of
/// `shape`); this struct only carries how to color that slice, the bitmask
/// counterpart of `BitmapLayer`'s [`crate::layers::bitmap_layer::ChannelSettings`].
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct BitmaskChannelSettings {
    /// How to color each object in this channel.
    // TODO: support independent color and opacity properties for the fill and stroke components.
    pub color: Option<ColorMode>,

    /// Opacity multiplier for this channel (0.0 to 1.0).
    pub opacity: f32,

    /// Whether this channel is drawn at all.
    pub visible: bool,

    /// If true, render filled object regions. If false, render only the
    /// outline of each object (see `stroke_width`).
    pub filled: bool,

    /// Outline thickness, in mask texels, used to detect object boundaries
    /// when `filled` is false. Has no effect when `filled` is true.
    pub stroke_width: f32,
}

impl Default for BitmaskChannelSettings {
    fn default() -> Self {
        Self {
            color: None,
            opacity: 1.0,
            visible: true,
            filled: true,
            stroke_width: 1.0,
        }
    }
}

/// Layer params struct for [`BitmaskLayer`].
///
/// Mirrors `BitmapLayerParams` field-for-field, except `channel_settings`
/// (bitmask coloring rather than an intensity window / pseudocolor).
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct BitmaskLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,

    // TODO: support data, pixels, or normalized unit mode when rendering in stroked mode.
    // This behavior should be analogous to the stroked_polygon_layer and stroked_curve_layer,
    // the only difference is that for the bitmasks, we use the bitmask data array to determine which fragments should be stroked on the fly in the shader.
    pub stroke_width_unit_mode: UnitsMode,

    // (x_offset, y_offset) in pixels, applied before model_matrix, to enable
    // this layer to be used to render an individual "tile" of a larger image
    // layer, where tiles correspond to the way the original array is
    // chunked/tiled on disk.
    pub pixel_offset: Option<(u32, u32)>,

    // The model_matrix can be used to apply additional affine transformations
    // to the physical dimensions of the mask (XYZ), such as translation,
    // rotation, and scaling.
    pub model_matrix: Option<[f32; 16]>, // Column-major 4x4 matrix

    /// The order of dimensions in the flat `data` array.
    pub dimension_order: DimensionOrder,

    /// The size of each dimension, in the same order as `dimension_order`.
    /// For example, if `dimension_order` is "CYX" and the mask is 256x256
    /// with 3 segmentation channels, then `shape` would be [3, 256, 256].
    pub shape: Vec<u32>,

    /// One entry per channel (i.e. one per the "C" dimension of `shape`),
    /// specifying how to color that channel's segmentation mask.
    pub channel_settings: Vec<BitmaskChannelSettings>,

    /// Overall opacity multiplier applied to the layer as a whole, on top of
    /// each channel's own opacity.
    pub opacity: f32,

    /// Flat array of per-pixel, per-channel object ids, in the order
    /// specified by `dimension_order`. A value of 0 means "no object"
    /// (background) at that pixel; object ids are otherwise expected to be
    /// 1-based (object `k` is id `k + 1`), matching the `ColorMode` index each
    /// channel resolves colors against. Supports multiple numeric dtypes (u8,
    /// u16, u32, u64, i8, i16, i32, i64, f32, f64).
    pub data: NumericData,
}

impl Default for BitmaskLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            pixel_offset: None,
            model_matrix: None,
            dimension_order: DimensionOrder::CYX,
            shape: vec![],
            channel_settings: vec![],
            opacity: 1.0,
            data: NumericData::Uint8(Arc::new(vec![])),
        }
    }
}

pub struct BitmaskLayer {
    view_params: ViewParams,
    layer_params: BitmaskLayerParams,
}

impl BitmaskLayer {
    pub fn new(view_params: ViewParams, layer_params: BitmaskLayerParams) -> Self {
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

        Self {
            view_params,
            layer_params,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for BitmaskLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        // No-op: self.layer_params is already fully populated in the constructor.
        return PrepareResult {
            bailed_early: false,
        };
    }
}

#[derive(ShaderType, Debug)]
struct BitmaskChannelUniforms {
    color_mode: u32,        // see ColorMode::shader_mode()
    static_color: Vec4,     // rgba color used by the UniformRgb mode
    color_reverse: u32,     // 1 = reverse the quantitative colormap
    color_domain: Vec2,     // (min, max) normalization domain for quantitative mode
    opacity: f32,
    filled: u32,            // 1 = draw filled object regions, 0 = draw outlines only
    stroke_width: f32,      // outline thickness, in mask texels (used when filled == 0)
    visible: u32,
}

#[derive(ShaderType, Debug)]
struct BitmaskLayerUniforms {
    layer_size: Vec2, // (layer_width, layer_height) in pixels
    camera_view: Mat4,
    data_unit_mode_x: u32, // 0 = pixels, 1 = data units, 2 = normalized
    data_unit_mode_y: u32, // 0 = pixels, 1 = data units, 2 = normalized
    aspect_ratio_mode: u32, // 0 = ignore, 1 = contain, 2 = cover
    aspect_ratio_alignment_mode: u32, // 0 = center, 1 = start, 2 = end

    img_size: Vec2, // (img_width, img_height) in pixels
    pixel_offset: Vec2, // (x_offset, y_offset) in pixels

    model_matrix: Mat4,

    opacity: f32, // overall layer opacity multiplier

    // Strides for each dimension (in units of f32 elements), allowing the
    // shader to index into the flat data buffer regardless of the dimension
    // ordering (e.g., CYX vs YXC). Mirrors `BitmapLayerUniforms`.
    x_stride: u32,
    y_stride: u32,
    c_stride: u32,

    num_channels: ArrayLength,
    // Note: WGSL only allows one runtime-sized array in a struct, and it must
    // be the last field.
    #[shader(size(runtime))]
    channels: Vec<BitmaskChannelUniforms>,
}

/// A color-mode value/palette texture bound for a single channel, paired with
/// the sample type its bind-group layout entry must declare.
struct ChannelColorTexture {
    view: wgpu::TextureView,
    sample_type: wgpu::TextureSampleType,
}

/// Everything needed to render one channel's [`ColorMode`] on the GPU.
///
/// Mirrors [`crate::color_mode::prepare_color_mode`], but specialized to emit
/// a uniquely-named `get_channel_color_{ch}` function (and uniquely-named
/// texture bindings) per channel, since a `BitmaskLayer` may have several
/// channels active in the same shader simultaneously — unlike layers with a
/// single fill color, WGSL has no per-instance function dispatch, so each
/// channel needs its own function/binding names rather than sharing one
/// `get_fill_color`.
struct PreparedChannelColor {
    mode: u32,
    static_color: [f32; 4],
    reverse: u32,
    domain: [f32; 2],
    textures: Vec<ChannelColorTexture>,
    wgsl: String,
    /// The quantitative colormap function this channel needs, if any: (wgsl
    /// source, function name). Callers should inject each distinct one only
    /// once across all channels (see `draw`), since two channels may share
    /// the same named colormap.
    colormap_fn: Option<(&'static str, &'static str)>,
}

/// Upload a palette as a 1-row `Rgba32Float` texture, one texel per color.
/// WGSL textures cannot have zero width, so an empty palette falls back to a
/// single opaque-black texel. Mirrors `crate::color_mode::create_palette_texture`.
fn create_channel_palette_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    colors: &[[f32; 4]],
    label: &str,
) -> wgpu::TextureView {
    let fallback = [[0.0f32, 0.0, 0.0, 1.0]];
    let colors: &[[f32; 4]] = if colors.is_empty() { &fallback } else { colors };
    let width = colors.len() as u32;

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        bytemuck::cast_slice(colors),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 16), // 4 channels * 4 bytes (f32)
            rows_per_image: Some(1),
        },
        wgpu::Extent3d { width, height: 1, depth_or_array_layers: 1 },
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Prepare the GPU resources and WGSL for one channel's [`ColorMode`],
/// assigning texture bindings sequentially starting at `first_binding` and
/// naming everything with the `ch` suffix so multiple channels can coexist in
/// one shader module. See [`PreparedChannelColor`].
fn prepare_channel_color(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    color: Option<&ColorMode>,
    first_binding: u32,
    ch: usize,
) -> PreparedChannelColor {
    let mut static_color = [0.0f32, 0.0, 0.0, 1.0];
    let mut reverse = 0u32;
    let mut domain = [0.0f32, 1.0];
    let mut textures: Vec<ChannelColorTexture> = Vec::new();
    let mut colormap_fn = None;

    let wgsl = match color {
        None => ShaderBuilder::new(get_channel_color::UNIFORM_RGB).define("ch", &ch.to_string()).build(),
        Some(ColorMode::UniformRgb((r, g, b))) => {
            static_color = [*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, 1.0];
            ShaderBuilder::new(get_channel_color::UNIFORM_RGB).define("ch", &ch.to_string()).build()
        }
        Some(ColorMode::InstancedRgb(params)) => {
            let (r_view, r_dtype) =
                params.r_values.create_data_texture(device, queue, "channel color r Texture");
            let (g_view, g_dtype) =
                params.g_values.create_data_texture(device, queue, "channel color g Texture");
            let (b_view, b_dtype) =
                params.b_values.create_data_texture(device, queue, "channel color b Texture");
            let wgsl = ShaderBuilder::new(get_channel_color::INSTANCED_RGB)
                .define("ch", &ch.to_string())
                .define_bidx("r", first_binding)
                .define_bidx("g", first_binding + 1)
                .define_bidx("b", first_binding + 2)
                .inject_texture_sample_type("r", r_dtype)
                .inject_texture_sample_type("g", g_dtype)
                .inject_texture_sample_type("b", b_dtype)
                .build();
            textures.push(ChannelColorTexture { view: r_view, sample_type: r_dtype.binding_sample_type() });
            textures.push(ChannelColorTexture { view: g_view, sample_type: g_dtype.binding_sample_type() });
            textures.push(ChannelColorTexture { view: b_view, sample_type: b_dtype.binding_sample_type() });
            wgsl
        }
        Some(ColorMode::InstancedRgbInterleaved(params)) => {
            let (view, dtype) =
                params.rgb_values.create_data_texture(device, queue, "channel color rgb Texture");
            let wgsl = ShaderBuilder::new(get_channel_color::INSTANCED_RGB_INTERLEAVED)
                .define("ch", &ch.to_string())
                .define_bidx("rgb", first_binding)
                .inject_texture_sample_type("rgb", dtype)
                .build();
            textures.push(ChannelColorTexture { view, sample_type: dtype.binding_sample_type() });
            wgsl
        }
        Some(ColorMode::Categorical(params)) => {
            let (view, dtype) =
                params.codes.create_data_texture(device, queue, "channel color labels Texture");
            let palette: Vec<[f32; 4]> = colormaps_categorical::palette(params.colormap).to_vec();
            let palette_view =
                create_channel_palette_texture(device, queue, &palette, "channel color palette Texture");
            let wgsl = ShaderBuilder::new(get_channel_color::CATEGORICAL)
                .define("ch", &ch.to_string())
                .define_bidx("labels", first_binding)
                .define_bidx("palette", first_binding + 1)
                .inject_texture_sample_type("labels", dtype)
                .build();
            textures.push(ChannelColorTexture { view, sample_type: dtype.binding_sample_type() });
            textures.push(ChannelColorTexture {
                view: palette_view,
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
            });
            wgsl
        }
        Some(ColorMode::CategoricalCustom(params)) => {
            let (view, dtype) =
                params.values.create_data_texture(device, queue, "channel color labels Texture");
            let palette: Vec<[f32; 4]> = params
                .colormap
                .iter()
                .map(|(r, g, b)| [*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, 1.0])
                .collect();
            let palette_view =
                create_channel_palette_texture(device, queue, &palette, "channel color palette Texture");
            let wgsl = ShaderBuilder::new(get_channel_color::CATEGORICAL)
                .define("ch", &ch.to_string())
                .define_bidx("labels", first_binding)
                .define_bidx("palette", first_binding + 1)
                .inject_texture_sample_type("labels", dtype)
                .build();
            textures.push(ChannelColorTexture { view, sample_type: dtype.binding_sample_type() });
            textures.push(ChannelColorTexture {
                view: palette_view,
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
            });
            wgsl
        }
        Some(ColorMode::Quantitative(params)) => {
            let (view, dtype) =
                params.values.create_data_texture(device, queue, "channel color values Texture");
            reverse = if params.reverse { 1 } else { 0 };
            domain = quantitative_domain(params);
            let (cmap_src, cmap_name) = wgsl_colormaps::wgsl_source_and_name(params.colormap);
            colormap_fn = Some((cmap_src, cmap_name));
            let wgsl = ShaderBuilder::new(get_channel_color::QUANTITATIVE)
                .define("ch", &ch.to_string())
                .define_bidx("values", first_binding)
                .inject_texture_sample_type("values", dtype)
                .define("colormap_fn_name", cmap_name)
                .build();
            textures.push(ChannelColorTexture { view, sample_type: dtype.binding_sample_type() });
            wgsl
        }
    };

    PreparedChannelColor {
        mode: color.map_or(0, ColorMode::shader_mode),
        static_color,
        reverse,
        domain,
        textures,
        wgsl,
        colormap_fn,
    }
}

/// Mask data texture is always bound at binding 1 (right after the binding-0
/// uniforms buffer), mirroring `BitmapLayer`'s single shared `img_data`
/// texture. Per-channel color-mode textures (if any) start immediately after.
const MASK_DATA_BINDING: u32 = 1;
const FIRST_CHANNEL_COLOR_BINDING: u32 = MASK_DATA_BINDING + 1;

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for BitmaskLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let GpuContext { device, queue } = gpu_context;
        let Self { layer_params, view_params } = self;

        let dims = parse_dimensions(&layer_params.dimension_order, &layer_params.shape);
        let (x_dim_idx, img_w) = *dims.get(&'X').expect("dimension_order must contain 'X'");
        let (y_dim_idx, img_h) = *dims.get(&'Y').expect("dimension_order must contain 'Y'");
        let (c_dim_idx, _) = *dims.get(&'C').expect("dimension_order must contain 'C'");

        // Compute strides so the shader can index into the flat data buffer
        // regardless of the dimension ordering (e.g., CYX vs YXC).
        let strides = compute_strides(&layer_params.shape);
        let x_stride = strides[x_dim_idx] as u32;
        let y_stride = strides[y_dim_idx] as u32;
        let c_stride = strides[c_dim_idx] as u32;

        // Upload the flat mask data (every channel) into a single-channel
        // (red-only) 2D texture, same as `BitmapLayer::img_data`.
        let (mask_texture_view, mask_dtype) =
            layer_params.data.create_data_texture(device, queue, "Bitmask Data Texture");

        let n_channels = layer_params.channel_settings.len();

        // Prepare each channel's color mode, assigning its texture bindings
        // (if any) sequentially after the shared mask texture. Quantitative
        // colormap functions are deduplicated by name, since two channels may
        // share the same named colormap.
        let mut next_binding = FIRST_CHANNEL_COLOR_BINDING;
        let mut colormap_fns: BTreeMap<&'static str, &'static str> = BTreeMap::new();
        let mut channel_colors: Vec<PreparedChannelColor> = Vec::with_capacity(n_channels);
        let mut channel_color_first_bindings: Vec<u32> = Vec::with_capacity(n_channels);
        for (i, ch) in layer_params.channel_settings.iter().enumerate() {
            let first_binding = next_binding;
            channel_color_first_bindings.push(first_binding);
            let prepared = prepare_channel_color(device, queue, ch.color.as_ref(), first_binding, i);
            next_binding += prepared.textures.len() as u32;
            if let Some((src, name)) = prepared.colormap_fn {
                colormap_fns.insert(name, src);
            }
            channel_colors.push(prepared);
        }

        // Note: WebGPU's shading language (WGSL) treats matrices as column-major.
        let camera_view = view_params.camera_view.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

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

        let channels_uniforms: Vec<BitmaskChannelUniforms> = layer_params
            .channel_settings
            .iter()
            .zip(channel_colors.iter())
            .map(|(ch, prepared)| BitmaskChannelUniforms {
                color_mode: prepared.mode,
                static_color: Vec4::from_array(prepared.static_color),
                color_reverse: prepared.reverse,
                color_domain: Vec2::from_array(prepared.domain),
                opacity: ch.opacity,
                filled: if ch.filled { 1 } else { 0 },
                stroke_width: ch.stroke_width,
                visible: if ch.visible { 1 } else { 0 },
            })
            .collect();

        let uniform_struct = BitmaskLayerUniforms {
            layer_size: Vec2::new(layer_w, layer_h),
            camera_view: Mat4::from_cols_array(&camera_view),
            data_unit_mode_x: match layer_params.data_unit_mode_x {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
                UnitsMode::Normalized => 2,
            },
            data_unit_mode_y: match layer_params.data_unit_mode_y {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
                UnitsMode::Normalized => 2,
            },
            aspect_ratio_mode: match view_params.aspect_ratio_mode {
                AspectRatioMode::Ignore => 0,
                AspectRatioMode::Contain => 1,
                AspectRatioMode::Cover => 2,
            },
            aspect_ratio_alignment_mode: match view_params.aspect_ratio_alignment_mode {
                AspectRatioAlignmentMode::Center => 0,
                AspectRatioAlignmentMode::Start => 1,
                AspectRatioAlignmentMode::End => 2,
            },
            img_size: Vec2::new(img_w as f32, img_h as f32),
            pixel_offset: Vec2::new(
                layer_params.pixel_offset.map_or(0.0, |(x, _)| x as f32),
                layer_params.pixel_offset.map_or(0.0, |(_, y)| y as f32),
            ),
            model_matrix: Mat4::from_cols_array(&layer_params.model_matrix.unwrap_or([
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ])),
            opacity: layer_params.opacity,
            x_stride,
            y_stride,
            c_stride,
            num_channels: Default::default(),
            channels: channels_uniforms,
        };

        // Runtime-sized arrays cannot be used with the encase UniformBuffer,
        // and require using StorageBuffer instead.
        let mut buffer = StorageBuffer::new(Vec::<u8>::new());
        buffer.write(&uniform_struct).unwrap();
        let uniform_bytes = buffer.into_inner();

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("BitmaskLayer Storage Buffer for Uniforms"),
            size: uniform_bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

        // Build the bind group layout: uniforms, then the shared mask data
        // texture, then each channel's color-mode textures (if any).
        let mut bgl_entries = vec![
            wgpu::BindGroupLayoutEntry {
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
                binding: MASK_DATA_BINDING,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: mask_dtype.binding_sample_type(),
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ];
        for (i, prepared) in channel_colors.iter().enumerate() {
            let first_binding = channel_color_first_bindings[i];
            for (j, tex) in prepared.textures.iter().enumerate() {
                bgl_entries.push(wgpu::BindGroupLayoutEntry {
                    binding: first_binding + j as u32,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: tex.sample_type,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                });
            }
        }
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("BitmaskLayer BGL"),
            entries: &bgl_entries,
        });

        let mut bg_entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: MASK_DATA_BINDING,
                resource: wgpu::BindingResource::TextureView(&mask_texture_view),
            },
        ];
        for (i, prepared) in channel_colors.iter().enumerate() {
            let first_binding = channel_color_first_bindings[i];
            for (j, tex) in prepared.textures.iter().enumerate() {
                bg_entries.push(wgpu::BindGroupEntry {
                    binding: first_binding + j as u32,
                    resource: wgpu::BindingResource::TextureView(&tex.view),
                });
            }
        }
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("BitmaskLayer BG"),
            layout: &bind_group_layout,
            entries: &bg_entries,
        });

        // Assemble the dynamic parts of the fragment shader: the deduplicated
        // quantitative colormap function(s), one `get_channel_color_N` per
        // channel, and the small `switch` dispatching to the right one by
        // channel index (the sampling/edge-test/blend loop itself is *not*
        // generated -- it's a real `for` loop over `u.num_channels` in
        // bitmask_layer.wgsl, see `bitmask_channel::CHANNEL_SAMPLE`/
        // `CHANNEL_IS_EDGE`).
        let colormap_functions: String =
            colormap_fns.values().copied().collect::<Vec<_>>().join("\n");

        let channel_color_functions: String =
            channel_colors.iter().map(|c| c.wgsl.clone()).collect::<Vec<_>>().join("\n");

        let switch_cases: String = (0..n_channels)
            .map(|i| format!("case {i}u: {{ return get_channel_color_{i}(label_index); }}"))
            .collect::<Vec<_>>()
            .join("\n        ");
        let channel_color_dispatch = ShaderBuilder::new(bitmask_channel::CHANNEL_COLOR_DISPATCH)
            .define("switch_cases", &switch_cases)
            .build();

        let shader_source = ShaderBuilder::new(include_str!("shaders/bitmask_layer.wgsl"))
            .inject_function("scale", common::SCALE)
            .inject_function("translate", common::TRANSLATE)
            .inject_function("get_aspect_ratio_mat", common::GET_ASPECT_RATIO_MAT)
            .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
            .inject_texture_sample_type("mask_data", mask_dtype)
            .inject_function("bitmask_sample", bitmask_channel::CHANNEL_SAMPLE)
            .inject_function("bitmask_is_edge", bitmask_channel::CHANNEL_IS_EDGE)
            .define("colormap_functions", &colormap_functions)
            .define("channel_color_functions", &channel_color_functions)
            .define("channel_color_dispatch", &channel_color_dispatch)
            .build();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bitmask_layer.wgsl"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("BitmaskLayer PLD"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("BitmaskLayer RPD"),
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

        // Handle margins by adjusting viewport and scissor rect (see BitmapLayer).
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
        pass.draw(0..4, 0..1);
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for BitmaskLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Encode raw RGBA pixels as a PNG byte stream using the `image` crate.
fn encode_png_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let img: ImageBuffer<Rgba<u8>, _> =
        ImageBuffer::from_raw(width, height, rgba.to_vec()).expect("valid dimensions");
    let mut buf = Vec::new();
    PngEncoder::new(&mut buf)
        .write_image(img.as_raw(), width, height, ExtendedColorType::Rgba8)
        .expect("PNG encode");
    buf
}

/// Encode bytes to a base64 string using the `base64` crate.
fn base64_encode(data: &[u8]) -> String {
    BASE64_STANDARD.encode(data)
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for BitmaskLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let Self { layer_params, view_params } = self;

        let dims = parse_dimensions(&layer_params.dimension_order, &layer_params.shape);
        let (x_dim_idx, img_w) = dims[&'X'];
        let (y_dim_idx, img_h) = dims[&'Y'];
        let (c_dim_idx, _) = dims[&'C'];

        let strides = compute_strides(&layer_params.shape);
        let x_stride = strides[x_dim_idx];
        let y_stride = strides[y_dim_idx];
        let c_stride = strides[c_dim_idx];

        // Naive per-pixel CPU rasterization, mirroring the GPU fragment
        // shader's per-channel sampling/edge-detection/blending logic (see
        // the channel loop in `bitmask_layer.wgsl`'s `fs_main`), reusing
        // `cpu_fill_color` (the same per-object color resolution the GPU
        // path uses, keyed here by object index rather than per-instance
        // index).
        let mut rgba = vec![0u8; (img_w * img_h * 4) as usize];

        for y in 0..img_h as i64 {
            for x in 0..img_w as i64 {
                let mut out_rgb = [0.0f32; 3];
                let mut out_a = 0.0f32;
                let mut any_on = false;

                for (ci, channel) in layer_params.channel_settings.iter().enumerate() {
                    if !channel.visible {
                        continue;
                    }
                    let idx = (y as usize) * y_stride + (x as usize) * x_stride + ci * c_stride;
                    let raw_label = layer_params.data.get_f32(idx) as i64;
                    if raw_label == 0 {
                        continue;
                    }

                    let is_on = if channel.filled {
                        true
                    } else {
                        let off = (channel.stroke_width.max(1.0)) as i64;
                        let deltas = [
                            (off, 0), (-off, 0),
                            (0, off), (0, -off),
                            (off, off), (-off, off),
                            (off, -off), (-off, -off),
                        ];
                        let mut is_edge = false;
                        for (dx, dy) in deltas {
                            let nx = (x + dx).clamp(0, img_w as i64 - 1) as usize;
                            let ny = (y + dy).clamp(0, img_h as i64 - 1) as usize;
                            let nidx = ny * y_stride + nx * x_stride + ci * c_stride;
                            let nval = layer_params.data.get_f32(nidx) as i64;
                            if nval != raw_label {
                                is_edge = true;
                                break;
                            }
                        }
                        is_edge
                    };
                    if !is_on {
                        continue;
                    }

                    let label_index = (raw_label - 1) as usize;
                    let quant_domain = match channel.color.as_ref() {
                        Some(ColorMode::Quantitative(p)) => quantitative_domain(p),
                        _ => [0.0, 1.0],
                    };
                    let (r, g, b) = cpu_fill_color(channel.color.as_ref(), label_index, quant_domain);
                    let src_rgb = [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0];
                    let a = channel.opacity;
                    out_rgb = mix3(out_rgb, src_rgb, a);
                    out_a = out_a.max(a);
                    any_on = true;
                }

                out_a *= layer_params.opacity;

                let pixel_idx = ((y as u32 * img_w + x as u32) * 4) as usize;
                if any_on {
                    rgba[pixel_idx] = (out_rgb[0].clamp(0.0, 1.0) * 255.0).round() as u8;
                    rgba[pixel_idx + 1] = (out_rgb[1].clamp(0.0, 1.0) * 255.0).round() as u8;
                    rgba[pixel_idx + 2] = (out_rgb[2].clamp(0.0, 1.0) * 255.0).round() as u8;
                    rgba[pixel_idx + 3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
        }

        let png = encode_png_rgba(img_w, img_h, &rgba);
        let href = format!("data:image/png;base64,{}", base64_encode(&png));

        let camera_view = view_params.camera_view.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

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

        let model_matrix_raw: [f32; 16] = layer_params.model_matrix.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let (offset_x, offset_y) = layer_params.pixel_offset.unwrap_or((0, 0));

        let (px, py) = crate::positioning::get_point_position(
            offset_x as f32,
            offset_y as f32,
            layer_w,
            layer_h,
            &camera_view,
            layer_params.data_unit_mode_x,
            layer_params.data_unit_mode_y,
            view_params.aspect_ratio_mode,
            view_params.aspect_ratio_alignment_mode,
            Some(&model_matrix_raw),
        );

        let (sw, sh) = crate::positioning::get_point_size(
            img_w as f32,
            img_h as f32,
            layer_w,
            layer_h,
            &camera_view,
            layer_params.data_unit_mode_x,
            layer_params.data_unit_mode_y,
            view_params.aspect_ratio_mode,
            view_params.aspect_ratio_alignment_mode,
            Some(&model_matrix_raw),
        );

        let final_x = px as f64;
        let final_y = (layer_h - py - sh) as f64;
        let final_w = sw as f64;
        let final_h = sh as f64;

        let image_element = if final_w == img_w as f64 && final_h == img_h as f64 {
            TwoElement::Image(TwoImage {
                x: final_x,
                y: final_y,
                width: final_w,
                height: final_h,
                href,
                opacity: 1.0,
                image_rendering_style: Some(TwoImageRenderingStyle::Pixelated),
            })
        } else {
            TwoElement::Group(TwoGroup {
                elements: vec![TwoElement::Image(TwoImage {
                    x: 0.0,
                    y: 0.0,
                    width: img_w as f64,
                    height: img_h as f64,
                    href,
                    opacity: 1.0,
                    image_rendering_style: Some(TwoImageRenderingStyle::Pixelated),
                })],
                translate: Some((final_x, final_y)),
                scale: Some((final_w / img_w as f64, final_h / img_h as f64)),
                ..Default::default()
            })
        };

        let svg_elements = vec![TwoElement::Group(TwoGroup {
            elements: vec![image_element],
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
        layer_type_name: "BitmaskLayer",
        create_layer: |value, view_params| {
            let params: BitmaskLayerParams = serde_json::from_value(value).unwrap();
            Box::new(BitmaskLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for BitmaskLayer {
    /// Pick the object id under the given data coordinate, for each channel.
    ///
    /// Returns the array indices of the picked pixel ("x", "y", top-down array
    /// convention, local to this layer's data) and the object id of each
    /// channel at that pixel ("channel_{i}"; 0 means "no object").
    fn pick(&self, _screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let DataCoord::TwoD { x: cx, y: cy } = data_coord? else {
            return None;
        };

        // Pixel/normalized-units positioning places the mask relative to the
        // layer bounds rather than in data space, so a data-space containment
        // test does not apply.
        if self.layer_params.data_unit_mode_x != UnitsMode::Data
            || self.layer_params.data_unit_mode_y != UnitsMode::Data
        {
            return None;
        }

        let dims = parse_dimensions(&self.layer_params.dimension_order, &self.layer_params.shape);
        let (x_dim_idx, img_w) = *dims.get(&'X')?;
        let (y_dim_idx, img_h) = *dims.get(&'Y')?;
        let (c_dim_idx, _) = *dims.get(&'C')?;
        if img_w == 0 || img_h == 0 {
            return None;
        }

        // Map the world coordinate into the mask's (Y-up) pixel space by
        // inverting the model_matrix; the vertex shader computes
        // world = model_matrix * (pixel + pixel_offset).
        let m = self.layer_params.model_matrix.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);
        let mut m64 = [0.0f64; 16];
        for (i, v) in m.iter().enumerate() {
            m64[i] = *v as f64;
        }
        let mat = DMat4::from_cols_array(&m64);
        if mat.determinant() == 0.0 {
            return None;
        }
        let p = mat.inverse() * DVec4::new(cx as f64, cy as f64, 0.0, 1.0);

        let (off_x, off_y) = self.layer_params.pixel_offset.unwrap_or((0, 0));
        let lx = p.x - off_x as f64;
        let ly = p.y - off_y as f64;
        if lx < 0.0 || lx > img_w as f64 || ly < 0.0 || ly > img_h as f64 {
            return None;
        }

        let texel_x = (lx.floor() as i64).clamp(0, img_w as i64 - 1) as usize;
        let texel_y = ((img_h as f64 - ly).floor() as i64).clamp(0, img_h as i64 - 1) as usize;

        let strides = compute_strides(&self.layer_params.shape);
        let x_stride = strides[x_dim_idx];
        let y_stride = strides[y_dim_idx];
        let c_stride = strides[c_dim_idx];

        let mut info = HashMap::new();
        info.insert("x".to_string(), texel_x.to_string());
        info.insert("y".to_string(), texel_y.to_string());
        for i in 0..self.layer_params.channel_settings.len() {
            let idx = texel_y * y_stride + texel_x * x_stride + i * c_stride;
            if idx < self.layer_params.data.len() {
                info.insert(format!("channel_{i}"), self.layer_params.data.format_element(idx));
            }
        }

        Some(LayerPickingResult {
            layer_id: self.layer_params.layer_id.clone(),
            info,
        })
    }
}
