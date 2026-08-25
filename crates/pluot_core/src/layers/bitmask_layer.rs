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

    /// Outline thickness, in the units given by
    /// [`BitmaskLayerParams::stroke_width_unit_mode`], used to detect object
    /// boundaries when `filled` is false. Has no effect when `filled` is true.
    ///
    /// The rendered thickness does not depend on the mask's resolution: the
    /// boundary test samples at fractional texel offsets, so a width that works
    /// out to a fraction of a texel still renders as a correspondingly thin
    /// band. (The SVG path up-samples the mask as needed to reproduce this,
    /// quantizing the width to whole layer pixels; see [`DrawToSvg`].)
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

    /// How to interpret every channel's [`BitmaskChannelSettings::stroke_width`]
    /// (i.e. when rendering a channel in outline-only mode): screen pixels,
    /// data-coordinate units, or a fraction (0 to 1) of the layer height.
    /// Analogous to the same field on the stroked polygon/curve layers, and,
    /// as there, widths are measured relative to the Y axis.
    ///
    /// Unlike those layers there is no stroke *geometry* to widen -- which
    /// fragments are stroked is decided from the bitmask data on the fly in
    /// the shader -- so the width is instead resolved into mask texels, the
    /// units the object-boundary test measures in. That resolution divides by
    /// the on-screen (or, for `Data`, the world-space) size of one mask texel:
    /// note this means `model_matrix` is a divisor here rather than a factor
    /// as in the stroked polygon/curve layers, since for a bitmask it maps
    /// mask-texel space into world space rather than data units into world
    /// units.
    ///
    /// Resolving into texels is only a change of units, not of resolution: the
    /// texel count may be fractional and the boundary test samples accordingly,
    /// so a `Pixels` width renders the same number of screen pixels thick
    /// whether the mask is coarse or fine. Only the camera and viewport enter
    /// into it, never the mask's dimensions.
    ///
    /// `Data` is rejected unless `data_unit_mode_y` is also `Data`, since data
    /// units are otherwise meaningless. Only the Y mode is checked because,
    /// as in the stroked polygon/curve layers, widths are measured relative to
    /// the Y axis.
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
            stroke_width_unit_mode: UnitsMode::Pixels,
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

        // 5. A data-unit stroke width has no meaning when the mask is
        // positioned relative to the layer bounds rather than in data space.
        // Mirrors the same check in `LineLayer`/`CurveLayer`, except that only
        // the Y mode is checked: stroke widths here are measured relative to
        // the Y axis, so a data-unit width stays well-defined when X alone is
        // positioned in pixel/normalized units.
        if layer_params.stroke_width_unit_mode == UnitsMode::Data
            && layer_params.data_unit_mode_y != UnitsMode::Data
        {
            panic!("stroke_width_unit_mode cannot be 'data' when data_unit_mode_y is 'pixels' or 'normalized'");
        }

        Self {
            view_params,
            layer_params,
        }
    }
}

/// Screen-pixel (layer-pixel) Y extent of a single mask texel: the on-screen
/// size of a 1x1-texel quad, positioned exactly as the mask quad is. Depends
/// on the model matrix, camera and viewport, but never on the mask's own
/// dimensions.
fn screen_px_per_texel(
    model_matrix: &[f32; 16],
    layer_w: f32,
    layer_h: f32,
    camera_view: &[f32; 16],
    data_unit_mode_x: UnitsMode,
    data_unit_mode_y: UnitsMode,
    aspect_ratio_mode: AspectRatioMode,
    aspect_ratio_alignment_mode: AspectRatioAlignmentMode,
) -> f32 {
    let (_, px_per_texel) = crate::positioning::get_point_size(
        1.0,
        1.0,
        layer_w,
        layer_h,
        camera_view,
        data_unit_mode_x,
        data_unit_mode_y,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        Some(model_matrix.as_slice()),
    );
    px_per_texel.abs()
}

/// Resolve a channel's `stroke_width` -- expressed in screen pixels, data
/// (world) units, or a fraction of the layer height -- into the mask texels
/// the object-boundary test steps by.
///
/// CPU mirror of `bitmask_stroke_width_texels` in
/// `wgsl_functions/bitmask/channel_stroke_width.wgsl`; see
/// [`BitmaskLayerParams::stroke_width_unit_mode`] for the rationale.
fn stroke_width_texels(
    stroke_width: f32,
    stroke_width_unit_mode: UnitsMode,
    model_matrix: &[f32; 16],
    layer_w: f32,
    layer_h: f32,
    camera_view: &[f32; 16],
    data_unit_mode_x: UnitsMode,
    data_unit_mode_y: UnitsMode,
    aspect_ratio_mode: AspectRatioMode,
    aspect_ratio_alignment_mode: AspectRatioAlignmentMode,
) -> f32 {
    // World-space Y extent of a single mask texel. w = 0, so the model_matrix's
    // translation cancels out (this is a size, not a position).
    let world_per_texel = (Mat4::from_cols_array(model_matrix) * Vec4::new(1.0, 1.0, 0.0, 0.0))
        .y
        .abs();

    if stroke_width_unit_mode == UnitsMode::Data {
        // Data-unit width: camera-independent, because the mask itself scales
        // with the camera.
        return if world_per_texel == 0.0 { 0.0 } else { stroke_width / world_per_texel };
    }

    let px_per_texel = screen_px_per_texel(
        model_matrix,
        layer_w,
        layer_h,
        camera_view,
        data_unit_mode_x,
        data_unit_mode_y,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
    );

    // Normalized-unit width: a fraction (0 to 1) of the layer height, which a
    // pixel-unit width already is in absolute terms.
    let stroke_width_px = if stroke_width_unit_mode == UnitsMode::Normalized {
        stroke_width * layer_h
    } else {
        stroke_width
    };
    if px_per_texel == 0.0 { 0.0 } else { stroke_width_px / px_per_texel }
}

/// Upper bound on the number of cells in the SVG path's rasterization grid
/// (4096 x 4096), bounding both the intermediate RGBA buffer and the base64
/// PNG embedded in the SVG. Only reached at extreme magnifications, where the
/// outline is drawn as close to the requested width as the cap allows (see
/// [`plan_svg_raster_grid`]).
const MAX_SVG_RASTER_CELLS: u64 = 4096 * 4096;

/// How closely the rasterized outline must match its target width, in layer
/// pixels. Half a pixel is as close as the target itself is defined, since the
/// target is rounded to whole layer pixels.
const STROKE_WIDTH_TOLERANCE_PX: f32 = 0.5;

/// The grid the SVG path rasterizes the mask into, plus the per-channel
/// outline widths measured in the units its boundary test steps by.
struct SvgRasterGrid {
    /// Grid cells per mask texel along each axis: 1 means one cell per texel
    /// (the mask's own resolution), `n > 1` means the mask is up-sampled `n`x.
    /// Always an integer, so cell boundaries include every texel boundary and
    /// the fill is reproduced exactly rather than resampled.
    up: u32,

    /// Per-channel outline offset in mask texels, i.e. the `stroke_width`
    /// argument of the CPU counterpart of `bitmask_is_edge`. Zero for channels
    /// that draw no outline (filled, hidden, or a non-positive width).
    stroke_offsets: Vec<f32>,
}

/// Thickness, in layer pixels, that an outline of `target_texels` actually
/// rasterizes to on a grid of `up` cells per texel.
///
/// Grid cell boundaries fall on texel boundaries, so the cells whose centers
/// lie within `target_texels` of an object boundary -- the ones the boundary
/// test turns on -- are exactly the first `round(target_texels * up)` of them
/// (and at least one, per the `0.5 / up` floor in [`plan_svg_raster_grid`]).
fn rasterized_stroke_width_px(target_texels: f32, px_per_texel: f32, up: u32) -> f32 {
    let up = up as f32;
    ((target_texels * up).round().max(1.0) / up) * px_per_texel
}

/// Decide what resolution to rasterize the mask at, so that every outline-only
/// channel's stroke comes out the same thickness the GPU path would give it.
///
/// The GPU decides per *screen pixel* whether it is within `stroke_width` of an
/// object boundary, so its outlines are limited only by the screen's
/// resolution. Rasterizing at the mask's own resolution instead quantizes an
/// outline to whole texels, which at anything above 1:1 magnification is far
/// too coarse -- a 1px outline on a mask magnified 20x would come out 20px
/// thick. So: measure each channel's stroke in layer pixels (`px_per_texel`
/// having already folded in the unit mode, layer dimensions and camera), round
/// it to whole pixels, and then rasterize at
///
/// * the mask's own dimensions, up-sampling nothing, if that already draws
///   every channel's outline to within half a layer pixel of its target -- the
///   case whenever the mask is at least as fine as the screen (where
///   `ceil(px_per_texel)` is 1 regardless), and whenever the targets land on
///   whole texels anyway (a mask magnified `n`x with an `n`-pixel outline);
/// * otherwise `ceil(px_per_texel)` cells per texel, so that one cell is at
///   most one layer pixel and any whole-pixel width is representable. This is
///   the finest grid that resolves anything the eventual raster does not, and
///   puts the rasterization on (near enough) the GPU's own fragment grid.
///
/// A coarser grid than that second case sometimes also lands within half a
/// pixel, but once up-sampling is happening at all the finer grid is the more
/// faithful choice: half a pixel is a large error relative to a thin outline
/// (drawing a 1px stroke 1.5px thick clears the tolerance but is 50% too
/// thick), and the extra cells cost little next to getting the width right.
/// [`MAX_SVG_RASTER_CELLS`] is the only reason the second case may fall short.
///
/// Note the grid is isotropic in texel space, while `px_per_texel` measures Y
/// (stroke widths are Y-relative here, as in the stroked polygon/curve layers).
/// A mask magnified more along X than along Y therefore quantizes its
/// vertical outline edges more coarsely -- but so does the GPU path's own
/// texel-space boundary test, which is likewise isotropic in texel space.
fn plan_svg_raster_grid(
    channel_settings: &[BitmaskChannelSettings],
    stroke_widths_texels: &[f32],
    px_per_texel: f32,
    img_w: u32,
    img_h: u32,
) -> SvgRasterGrid {
    let px_per_texel_valid = px_per_texel.is_finite() && px_per_texel > 0.0;

    // Per-channel target width in texels: the resolved width measured in layer
    // pixels, rounded to the nearest whole pixel -- but never below one, since
    // (as on the GPU) a band cannot be thinner than the rasterizer's own
    // resolution -- and converted back into texels.
    let targets: Vec<f32> = channel_settings
        .iter()
        .zip(stroke_widths_texels.iter())
        .map(|(ch, &texels)| {
            if ch.filled || !ch.visible || texels <= 0.0 || !texels.is_finite() {
                // Draws no outline, so imposes no constraint on the grid.
                0.0
            } else if px_per_texel_valid {
                (texels * px_per_texel).round().max(1.0) / px_per_texel
            } else {
                // Degenerate on-screen size (a collapsed camera or model
                // matrix): nothing meaningful to round against, so keep the
                // width as-is.
                texels
            }
        })
        .collect();

    let up = choose_upsample_factor(&targets, px_per_texel_valid, px_per_texel, img_w, img_h);

    // A cell's center is at least `0.5 / up` texels from any texel boundary,
    // so that floor keeps an outline at least one cell thick -- the same
    // "never thinner than one pixel of the output grid" behavior the GPU path
    // gets from the rasterizer. It only binds when `up` fell short of
    // `px_per_texel` because of `MAX_SVG_RASTER_CELLS`.
    let floor_texels = 0.5 / up as f32;
    let stroke_offsets = targets
        .iter()
        .map(|&t| if t > 0.0 { t.max(floor_texels) } else { 0.0 })
        .collect();

    SvgRasterGrid { up, stroke_offsets }
}

/// The up-sampling factor for [`plan_svg_raster_grid`]: 1 if the mask's own
/// resolution draws every target width to within [`STROKE_WIDTH_TOLERANCE_PX`],
/// otherwise one cell per layer pixel, capped by [`MAX_SVG_RASTER_CELLS`].
fn choose_upsample_factor(
    targets: &[f32],
    px_per_texel_valid: bool,
    px_per_texel: f32,
    img_w: u32,
    img_h: u32,
) -> u32 {
    // Nothing to resolve: no outline-only channel is drawn (up-sampling would
    // only nearest-neighbor-magnify a fill that the SVG viewer magnifies just
    // as well), or there is no meaningful on-screen size to match.
    if !px_per_texel_valid || img_w == 0 || img_h == 0 || !targets.iter().any(|&t| t > 0.0) {
        return 1;
    }

    let worst_error_at = |up: u32| -> f32 {
        targets
            .iter()
            .filter(|&&t| t > 0.0)
            .map(|&t| (rasterized_stroke_width_px(t, px_per_texel, up) - t * px_per_texel).abs())
            .fold(0.0f32, f32::max)
    };
    if worst_error_at(1) <= STROKE_WIDTH_TOLERANCE_PX + 1e-4 {
        return 1;
    }

    // One cell per layer pixel. Float-to-int casts saturate rather than wrap,
    // so an extreme magnification just clamps to `max_up`.
    let finest_useful_up = px_per_texel.ceil().max(1.0) as u32;
    let texels = (img_w as u64) * (img_h as u64);
    let max_up = ((MAX_SVG_RASTER_CELLS / texels.max(1)) as f64).sqrt().floor().max(1.0) as u32;
    finest_useful_up.min(max_up)
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

    // How to interpret each channel's stroke_width.
    stroke_width_unit_mode: u32, // 0 = pixels, 1 = data units, 2 = normalized

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
            stroke_width_unit_mode: match layer_params.stroke_width_unit_mode {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
                UnitsMode::Normalized => 2,
            },
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
            .inject_function("bitmask_stroke_width_texels", bitmask_channel::CHANNEL_STROKE_WIDTH)
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

        // On-screen (layer-pixel) Y extent of one mask texel: the conversion
        // between the mask-texel space the rasterization below works in and
        // the layer pixels the stroke widths are matched against.
        let px_per_texel = screen_px_per_texel(
            &model_matrix_raw,
            layer_w,
            layer_h,
            &camera_view,
            layer_params.data_unit_mode_x,
            layer_params.data_unit_mode_y,
            view_params.aspect_ratio_mode,
            view_params.aspect_ratio_alignment_mode,
        );

        // Per-channel outline thickness, resolved out of `stroke_width_unit_mode`
        // into the mask texels the boundary test below steps by (the CPU
        // counterpart of `bitmask_stroke_width_texels` in the fragment shader).
        let stroke_widths_texels: Vec<f32> = layer_params
            .channel_settings
            .iter()
            .map(|ch| {
                stroke_width_texels(
                    ch.stroke_width,
                    layer_params.stroke_width_unit_mode,
                    &model_matrix_raw,
                    layer_w,
                    layer_h,
                    &camera_view,
                    layer_params.data_unit_mode_x,
                    layer_params.data_unit_mode_y,
                    view_params.aspect_ratio_mode,
                    view_params.aspect_ratio_alignment_mode,
                )
            })
            .collect();

        // Resolution to rasterize at: the mask's own dimensions, up-sampled
        // by an integer factor when an outline-only channel needs a finer grid
        // than one cell per texel to come out the thickness the GPU path would
        // give it. See `plan_svg_raster_grid`.
        let SvgRasterGrid { up, stroke_offsets } = plan_svg_raster_grid(
            &layer_params.channel_settings,
            &stroke_widths_texels,
            px_per_texel,
            img_w,
            img_h,
        );

        // Naive per-pixel CPU rasterization, mirroring the GPU fragment
        // shader's per-channel sampling/edge-detection/blending logic (see
        // the channel loop in `bitmask_layer.wgsl`'s `fs_main`), reusing
        // `cpu_fill_color` (the same per-object color resolution the GPU
        // path uses, keyed here by object index rather than per-instance
        // index).
        //
        // Each grid cell plays the role a fragment does on the GPU: its center,
        // as a continuous position in mask-texel space, gives both the texel
        // whose object id it takes (the containing one) and the origin the
        // boundary test offsets from -- so, exactly as in `bitmask_is_edge`, a
        // band edge may fall part-way through a texel and the offsets stay
        // fractional. The grid's own resolution is the only quantization left,
        // and it is chosen so that one cell is at most one layer pixel, the
        // same granularity as the GPU's fragment grid.
        //
        // TODO: currently, bitmask texels/pixels that fall outside the layer bounds are rendered, and clipped via the clipping rectangle.
        // Instead, determine which bitmask data falls outside the bounds, and skip rasterization of this data altogether, to improve performance.
        let raster_w = img_w * up;
        let raster_h = img_h * up;
        let up_f = up as f32;
        let max_texel_x = img_w as f32 - 1.0;
        let max_texel_y = img_h as f32 - 1.0;
        let mut rgba = vec![0u8; (raster_w as usize) * (raster_h as usize) * 4];

        for oy in 0..raster_h {
            // Cell center in mask-texel space (row 0 at the top, matching the
            // data array), and the texel containing it.
            let cy = (oy as f32 + 0.5) / up_f;
            let y = (oy / up) as usize;

            for ox in 0..raster_w {
                let cx = (ox as f32 + 0.5) / up_f;
                let x = (ox / up) as usize;

                let mut out_rgb = [0.0f32; 3];
                let mut out_a = 0.0f32;
                let mut any_on = false;

                for (ci, channel) in layer_params.channel_settings.iter().enumerate() {
                    if !channel.visible {
                        continue;
                    }
                    let idx = y * y_stride + x * x_stride + ci * c_stride;
                    let raw_label = layer_params.data.get_f32(idx) as i64;
                    if raw_label == 0 {
                        continue;
                    }

                    let is_on = if channel.filled {
                        true
                    } else {
                        // Object-boundary test, mirroring `bitmask_is_edge`:
                        // sample 8 directions at `off` texels from this cell's
                        // continuous position, clamping to the mask's bounds.
                        let off = stroke_offsets[ci];
                        let deltas = [
                            (off, 0.0), (-off, 0.0),
                            (0.0, off), (0.0, -off),
                            (off, off), (-off, off),
                            (off, -off), (-off, -off),
                        ];
                        let mut is_edge = false;
                        for (dx, dy) in deltas {
                            let nx = (cx + dx).floor().clamp(0.0, max_texel_x) as usize;
                            let ny = (cy + dy).floor().clamp(0.0, max_texel_y) as usize;
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

                let pixel_idx = ((oy as usize) * (raster_w as usize) + (ox as usize)) * 4;
                if any_on {
                    rgba[pixel_idx] = (out_rgb[0].clamp(0.0, 1.0) * 255.0).round() as u8;
                    rgba[pixel_idx + 1] = (out_rgb[1].clamp(0.0, 1.0) * 255.0).round() as u8;
                    rgba[pixel_idx + 2] = (out_rgb[2].clamp(0.0, 1.0) * 255.0).round() as u8;
                    rgba[pixel_idx + 3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
        }

        let png = encode_png_rgba(raster_w, raster_h, &rgba);
        let href = format!("data:image/png;base64,{}", base64_encode(&png));

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

        // The raster covers exactly the same rect either way -- `up` only
        // changes how many cells that rect is divided into, not where it sits.
        let image_element = if final_w == raster_w as f64 && final_h == raster_h as f64 {
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
                    width: raster_w as f64,
                    height: raster_h as f64,
                    href,
                    opacity: 1.0,
                    image_rendering_style: Some(TwoImageRenderingStyle::Pixelated),
                })],
                translate: Some((final_x, final_y)),
                scale: Some((final_w / raster_w as f64, final_h / raster_h as f64)),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn outline(stroke_width: f32) -> BitmaskChannelSettings {
        BitmaskChannelSettings { filled: false, stroke_width, ..Default::default() }
    }

    fn filled() -> BitmaskChannelSettings {
        BitmaskChannelSettings { filled: true, ..Default::default() }
    }

    /// Thickness, in layer pixels, that `plan_svg_raster_grid`'s chosen grid
    /// actually draws channel `ci`'s outline at.
    fn drawn_px(grid: &SvgRasterGrid, ci: usize, px_per_texel: f32) -> f32 {
        rasterized_stroke_width_px(grid.stroke_offsets[ci], px_per_texel, grid.up)
    }

    #[test]
    fn filled_channels_never_up_sample() {
        // Up-sampling a fill would only nearest-neighbor-magnify it, so even a
        // heavily magnified mask stays at its own resolution.
        let grid = plan_svg_raster_grid(&[filled(), filled()], &[0.0, 0.0], 200.0, 4, 4);
        assert_eq!(grid.up, 1);
        assert_eq!(grid.stroke_offsets, vec![0.0, 0.0]);
    }

    #[test]
    fn hidden_and_zero_width_outlines_never_up_sample() {
        let hidden = BitmaskChannelSettings { visible: false, ..outline(1.0) };
        let grid = plan_svg_raster_grid(&[hidden, outline(0.0)], &[0.005, 0.0], 200.0, 4, 4);
        assert_eq!(grid.up, 1);
        assert_eq!(grid.stroke_offsets, vec![0.0, 0.0]);
    }

    #[test]
    fn whole_texel_outline_never_up_samples() {
        // A one-texel outline on a mask magnified 200x is already exactly
        // representable at the mask's own resolution.
        let grid = plan_svg_raster_grid(&[outline(1.0)], &[1.0], 200.0, 4, 4);
        assert_eq!(grid.up, 1);
        assert_eq!(drawn_px(&grid, 0, 200.0), 200.0);
    }

    #[test]
    fn sub_texel_outline_up_samples_to_layer_pixels() {
        // A 1px outline on a mask magnified 200x: one texel per grid cell
        // would draw it 200x too thick, so the grid goes to one cell per
        // layer pixel and draws it exactly 1px thick.
        let grid = plan_svg_raster_grid(&[outline(1.0)], &[1.0 / 200.0], 200.0, 4, 4);
        assert_eq!(grid.up, 200);
        assert!((drawn_px(&grid, 0, 200.0) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn fractional_texel_outline_up_samples_to_layer_pixels() {
        // 2.5 texels at 4 px/texel = 10px, which whole texels can only render
        // as 8px or 12px, so the grid goes to one cell per layer pixel.
        let grid = plan_svg_raster_grid(&[outline(10.0)], &[2.5], 4.0, 16, 16);
        assert_eq!(grid.up, 4);
        assert!((drawn_px(&grid, 0, 4.0) - 10.0).abs() < 1e-3);
    }

    #[test]
    fn near_whole_texel_outline_stays_at_the_mask_resolution() {
        // 1 texel at 12.5 px/texel rounds to a 13px target, which whole texels
        // render as 12.5px -- within half a pixel, so nothing is up-sampled.
        let grid = plan_svg_raster_grid(&[outline(1.0)], &[13.0 / 12.5], 12.5, 4, 4);
        assert_eq!(grid.up, 1);
        assert_eq!(drawn_px(&grid, 0, 12.5), 12.5);
    }

    #[test]
    fn masks_finer_than_the_screen_rasterize_at_their_own_resolution() {
        // 4 texels per layer pixel: the data is already finer than the target
        // stroke needs, so nothing is up-sampled.
        let grid = plan_svg_raster_grid(&[outline(1.0)], &[4.0], 0.25, 512, 512);
        assert_eq!(grid.up, 1);
        assert!((drawn_px(&grid, 0, 0.25) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn multiple_outline_channels_share_one_grid() {
        // 1px and 200px outlines at 200 px/texel: the grid must be fine enough
        // for the thinner one, and still draws the thicker one exactly.
        let grid = plan_svg_raster_grid(
            &[outline(1.0), outline(200.0)],
            &[1.0 / 200.0, 1.0],
            200.0,
            4,
            4,
        );
        assert_eq!(grid.up, 200);
        assert!((drawn_px(&grid, 0, 200.0) - 1.0).abs() < 1e-3);
        assert!((drawn_px(&grid, 1, 200.0) - 200.0).abs() < 1e-3);
    }

    #[test]
    fn up_sampling_is_capped_for_large_masks() {
        // A 4096x4096 mask is already at the cap, so a 1px outline on it
        // magnified 8x cannot be drawn thinner than one texel -- but it is
        // still drawn, one cell thick, rather than vanishing or panicking.
        let grid = plan_svg_raster_grid(&[outline(1.0)], &[0.125], 8.0, 4096, 4096);
        assert_eq!(grid.up, 1);
        assert_eq!(grid.stroke_offsets[0], 0.5);
        assert_eq!(drawn_px(&grid, 0, 8.0), 8.0);
    }

    #[test]
    fn degenerate_on_screen_size_keeps_the_resolved_width() {
        for px_per_texel in [0.0, f32::NAN, f32::INFINITY] {
            let grid = plan_svg_raster_grid(&[outline(1.0)], &[2.0], px_per_texel, 4, 4);
            assert_eq!(grid.up, 1);
            assert_eq!(grid.stroke_offsets[0], 2.0);
        }
    }

    #[test]
    fn empty_masks_do_not_divide_by_zero() {
        let grid = plan_svg_raster_grid(&[outline(1.0)], &[0.005], 200.0, 0, 0);
        assert_eq!(grid.up, 1);
    }
}
