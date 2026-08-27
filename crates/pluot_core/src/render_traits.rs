use crate::picking::LayerPickingResult;
use crate::numeric_data::NumericData;
use crate::viewport::{DataCoord, ScreenCoord};
use crate::wgpu;
use crate::two::svg::{init_svg, SvgContext};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult, RenderResult};
use crate::maybe::{MaybeSend, MaybeSync};
use crate::params::{LayerParams, ZarrStoreInfo};
use crate::registry::get_layer_from_registry;
use crate::zarr::StoreMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use zarrs::storage::AsyncReadableStorageTraits;

// TODO: use From and Into to define the integer conversions, rather than manually defining in comments?

/// Specifies how the camera and coordinate system should behave when the plotted region (within the margins) has a non-square aspect ratio.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum AspectRatioMode {
    /*
     - 0: ignore / squeeze: For example,  a 200 x 100 canvas would show values from -1 to 1 in x and y. The -1 to 1 square would be stretched in the X direction since the canvas is wider than it is tall.

     - 1: fit (contain): For example, a 200 x 100 canvas would range from -1 to 1 in the Y direction, and from -1-extra to 1+extra in the X direction. The -1 to 1 square would keep its square aspect ratio and would be fully visible inside the rectangle (with no part of this square clipped). The pixels would be centered.

     - 2: fill (cover): For example, a 200 x 100 canvas would range from -1 to 1 in the X direction, and from -1+extra to 1-extra in the Y direction. The -1 to 1 square would keep its square aspect ratio but would be clipped in the Y direction (at the top and bottom) so that the entire canvas is filled/covered. The pixels would be centered.
     */
     /// Squeeze/stretch the (0, 1) unit square so that no more and no less data is shown. The square aspect ratio of the (0, 1) unit square will NOT be preserved.
     Ignore,
     /// (a.k.a. "fit"): The square aspect ratio of the (0, 1) unit square will be preserved, by showing more data along the longer dimension of the rectangle.
     Contain,
     /// (a.k.a. "fill"): The square aspect ratio of the (0, 1) unit square will be preserved, by showing less data along the shorter dimension of the rectangle.
     Cover,
}

/// Determine what extra data is shown in Contain mode, and what data is hidden in Cover mode.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum AspectRatioAlignmentMode {
    /*
     - 0: center
       - When using "contain" AspectRatioMode with a wide canvas, the unit square will be aligned in the center, with visible excess space on both the left and right sides.
       - When using "cover" AspectRatioMode with a wide canvas, the unit square will extend both above and below the viewport.
       - When using "contain" AspectRatioMode with a tall canvas, the unit square will be aligned in the center, with visible excess space on both the top and bottom sides.
       - When using "cover" AspectRatioMode with a tall canvas, the unit square will extend both to the left and right of the viewport.
       - When using "ignore" AspectRatioMode, no action is needed.

     - 1: start
       - When using "contain" AspectRatioMode with a wide canvas, the unit square will be left-aligned, and there will be visible extra space on the right side.
       - When using "cover" AspectRatioMode with a wide canvas, the unit square will extend beyond the top of the viewport.
       - When using "contain" AspectRatioMode with a tall canvas, the unit square will be bottom-aligned, and there will be visible extra space on the top side.
       - When using "cover" AspectRatioMode with a tall canvas, the unit square will extend beyond the right of the viewport.
       - When using "ignore" AspectRatioMode, no action is needed.

     - 2: end
       - When using "contain" AspectRatioMode with a wide canvas, the unit square will be right-aligned, and there will be visible extra space on the left side.
       - When using "cover" AspectRatioMode with a wide canvas, the unit square will extend beyond the bottom of the viewport.
       - When using "contain" AspectRatioMode with a tall canvas, the unit square will be top-aligned, and there will be visible extra space on the top side.
       - When using "cover" AspectRatioMode with a tall canvas, the unit square will extend beyond the left of the viewport.
       - When using "ignore" AspectRatioMode, no action is needed.
     */
     /// In Contain mode, the unit square will be centered vertically for tall aspect ratios (with extra space on top and bottom) and centered horizontally for wide aspect ratios (with extra space on left and right). In Cover mode, the unit square will extend beyond the left and right (for tall aspect ratios) or will extend beyond the top and bottom (for wide aspect ratios).
     Center,
     /// In Contain mode, the unit square will be bottom-aligned for tall aspect ratios (with extra space on top) and left-aligned for wide aspect ratios (with extra space on right). In Cover mode, the unit square will extend beyond the right (for tall aspect ratios) or will extend beyond the top (for wide aspect ratios).
     Start,
     /// In Contain mode, the unit square will be top-aligned for tall aspect ratios (with extra space on bottom) and right-aligned for wide aspect ratios (with extra space on left). In Cover mode, the unit square will extend beyond the left (for tall aspect ratios) or will extend beyond the bottom (for wide aspect ratios).
     End,
}

/// Determines whether size/position values are interpreted in pixel, data, or normalized space.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum UnitsMode {
    /// Pixel units. Agnostic to camera state.
    // 0: pixels (e.g., for fixed pixel-unit sizes).
    Pixels,
    /// Data, or "world", units. Dependent on camera state and aspect ratio modes.
    // 1: data ("world") units (e.g., for physical sizes).
    Data,
    /// Normalized units. Similar to pixel units, but normalized to be between zero and one. Agnostic to camera state and pixel dimensions.
    // 2: normalized: similar to pixel-based but values are between 0 and 1, so they are agnostic to the pixel dimensions of the plot. Similar to Pixels UnitMode, does not depend on the camera state.
    Normalized,
}

/// Named categorical colormap functions.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CategoricalColormap {
    // Reference: https://vega.github.io/vega/docs/schemes/
    Accent,
    Category10,
    Category20,
    Category20b,
    Category20c,
    Observable10,
    Dark2,
    Paired,
    Pastel1,
    Pastel2,
    Set1,
    Set2,
    Set3,
    Tableau10,
    Tableau20,
}

/// Named quantitative colormap functions.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum QuantitativeColormap {
    // Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/colormaps.in.glsl
    Plasma,
    Viridis,
    Greys,
    Magma,
    Jet,
    Bone,
    Copper,
    Density,
    Inferno,
    Cool,
    Hot,
    Spring,
    Summer,
    Autumn,
    Winter,

    // See https://github.com/d3/d3-scale-chromatic/tree/main/src/sequential-single
    Blues,
    Greens,
    Oranges,
    Purples,
    Reds,
}

/// Static (r, g, b) color shared by every instance.
pub type UniformRgbParams = (u8, u8, u8);

/// Explicitly specifies an RGB color per instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstancedRgbParams {
    /// Array of red values (0 to 255). Length must be equal to the number of instances.
    pub r_values: NumericData,
    /// Array of green values (0 to 255). Length must be equal to the number of instances.
    pub g_values: NumericData,
    /// Array of blue values (0 to 255). Length must be equal to the number of instances.
    pub b_values: NumericData,
}

/// Interleaved analog of [`InstancedRgbParams`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstancedRgbInterleavedParams {
    /// Flat array of interleaved RGB values. Length must be equal to the 3 * number of instances.
    pub rgb_values: NumericData,
}

/// Specifies a named categorical colormap and the category per instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CategoricalParams {
    /// An index of a color in the colormap per instance. Length must be equal to the number of instances.
    pub codes: NumericData,
    /// A named categorical colormap function.
    pub colormap: CategoricalColormap,
}

/// Specifies an RGB value per category and the category per instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CategoricalCustomParams {
    /// An index of a color in the colormap per instance. Length must be equal to the number of instances.
    pub values: NumericData,
    /// An array of (r, g, b) values which define the custom categorical colormap.
    pub colormap: Vec<(u8, u8, u8)>,
}

fn default_false() -> bool {
    false
}

/// Specifies a named quantitative colormap and a scalar value per instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantitativeParams {
    /// The scalar values passed as input to the colormap function. Length must be equal to the number of instances.
    pub values: NumericData,
    /// A named quantitative colormap function.
    pub colormap: QuantitativeColormap,
    /// Determines whether the colormap should be reversed (by subtracting `1 - value` before executing the colormap function). By default, false.
    #[serde(default = "default_false")]
    pub reverse: bool,
    /// Optional (min, max) normalization domain, defaulting to (0.0, 1.0).
    ///
    /// The domain is never derived from `values`: normalization happens in the
    /// WGSL colormap function against this domain, so establishing it would cost
    /// a CPU pass over every value. Supply it explicitly whenever `values` is not
    /// already normalized to 0..1.
    #[serde(default)]
    pub domain: Option<(f32, f32)>,
}


/// Specify uniform or instanced colors for rendered elements.
///
/// Serialized as an adjacently-tagged enum, e.g.
/// `{"color_mode": "UniformRgb", "color_params": [255, 0, 0]}`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "color_mode", content = "color_params")]
pub enum ColorMode {
    // 0: static color (e.g., same RGB color for all elements)
    UniformRgb(UniformRgbParams),
    // 1: explicit colors (e.g., for N elements, N individual RGB colors, as 3 N-length vecs)
    InstancedRgb(InstancedRgbParams),
    // 2: explicit colors (e.g., for N elements, N individual RGB colors, as N 3-length vecs (interleaved))
    InstancedRgbInterleaved(InstancedRgbInterleavedParams),
    // 3: instanced categorical color based on K integer class labels, via a known named colormap
    Categorical(CategoricalParams),
    // 4: instanced categorical color based on K integer class labels, via a special "Custom" categorical colormap type accompanied by a list of RGB values per item
    CategoricalCustom(CategoricalCustomParams),
    // 5: quantitative color based on N float values. plus a known named quantiative colormap function.
    Quantitative(QuantitativeParams),
}

/// Static opacity (between 0.0 for fully transparent and 1.0 for fully opaque) shared by every element.
pub type UniformOpacityParams = f32;

/// Per-element opacity, one value (between 0.0 and 1.0) per element.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstancedOpacityParams {
    /// The opacity values per instance. Length must be equal to the number of instances.
    pub values: NumericData,
}

/// Specify uniform or instanced opacity values for rendered elements.
///
/// Serialized as an adjacently-tagged enum, e.g.
/// `{"opacity_mode": "UniformOpacity", "opacity_params": 1.0}`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "opacity_mode", content = "opacity_params")]
pub enum OpacityMode {
    UniformOpacity(UniformOpacityParams),
    InstancedOpacity(InstancedOpacityParams),
}

impl OpacityMode {
    /// Panics if this mode carries per-element [`NumericData`] whose length
    /// doesn't match `expected` (the layer's element count). `UniformOpacity`
    /// carries no per-element data and is always valid.
    pub fn validate_len(&self, expected: usize) {
        if let OpacityMode::InstancedOpacity(params) = self {
            assert_eq!(
                params.values.len(), expected,
                "OpacityMode values has length {} but layer has {expected} elements",
                params.values.len(),
            );
        }
    }
}

/// Static size (e.g., width or radius) shared by every element.
pub type UniformSizeParams = f32;

/// Per-element size (e.g., width or radius), one value per element.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstancedSizeParams {
    /// A size value per instance. Length must be equal to the number of instances.
    pub values: NumericData,
}

/// Specify uniform or instanced size (e.g., width or radius) values for rendered elements.
///
/// Serialized as an adjacently-tagged enum, e.g.
/// `{"size_mode": "UniformSize", "size_params": 1.0}`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "size_mode", content = "size_params")]
pub enum SizeMode {
    UniformSize(UniformSizeParams),
    InstancedSize(InstancedSizeParams),
}

impl SizeMode {
    /// Panics if this mode carries per-element [`NumericData`] whose length
    /// doesn't match `expected` (the layer's element count). `UniformSize`
    /// carries no per-element data and is always valid.
    pub fn validate_len(&self, expected: usize) {
        if let SizeMode::InstancedSize(params) = self {
            assert_eq!(
                params.values.len(), expected,
                "SizeMode values has length {} but layer has {expected} elements",
                params.values.len(),
            );
        }
    }
}

impl ColorMode {
    /// The integer discriminant handed to the shader's `fill_color_mode`
    /// uniform. Must stay in sync with the branch values in `rect_layer.wgsl`.
    pub fn shader_mode(&self) -> u32 {
        match self {
            ColorMode::UniformRgb(_) => 0,
            ColorMode::InstancedRgb(_) => 1,
            ColorMode::InstancedRgbInterleaved(_) => 2,
            ColorMode::Categorical(_) => 3,
            ColorMode::CategoricalCustom(_) => 4,
            ColorMode::Quantitative(_) => 5,
        }
    }

    /// Panics if this mode carries per-element [`NumericData`] whose length
    /// doesn't match `expected` (the layer's element count). `UniformRgb`
    /// carries no per-element data and is always valid.
    pub fn validate_len(&self, expected: usize) {
        let check = |name: &str, len: usize| {
            assert_eq!(
                len, expected,
                "ColorMode {name} has length {len} but layer has {expected} elements",
            );
        };
        match self {
            ColorMode::UniformRgb(_) => {}
            ColorMode::InstancedRgb(params) => {
                check("r_values", params.r_values.len());
                check("g_values", params.g_values.len());
                check("b_values", params.b_values.len());
            }
            ColorMode::InstancedRgbInterleaved(params) => {
                let expected_len = expected * 3;
                assert_eq!(
                    params.rgb_values.len(), expected_len,
                    "ColorMode rgb_values has length {} but layer has {expected} elements (expected {expected_len})",
                    params.rgb_values.len(),
                );
            }
            ColorMode::Categorical(params) => {
                check("codes", params.codes.len());
            }
            ColorMode::CategoricalCustom(params) => {
                check("values", params.values.len());
            }
            ColorMode::Quantitative(params) => {
                check("values", params.values.len());
            }
        }
    }
}

/// Filtering or selection criteria for a layer's data items.
///
/// Serialized as an adjacently-tagged enum, e.g.
/// `{"criteria_mode": "Categorical", "criteria_params": {...}}`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "criteria_mode", content = "criteria_params")]
pub enum EmphasisCriteria {
    // A categorical column (categories+codes format with NumericData, similar to ColorMode),
    // along with a set of included categories
    Categorical(CategoricalCriteriaParams),

    // A quantitative column (NumericData),
    // along with min and/or max (included values within this range) - omitted min/max implicitly means -inf/+inf.
    Quantitative(QuantitativeCriteriaParams),
}

impl EmphasisCriteria {
    /// Panics if the per-element [`NumericData`] this criteria is defined
    /// over has a length that doesn't match `expected` (the layer's element
    /// count).
    pub fn validate_len(&self, expected: usize) {
        match self {
            EmphasisCriteria::Categorical(params) => {
                assert_eq!(
                    params.codes.len(), expected,
                    "EmphasisCriteria codes has length {} but layer has {expected} elements",
                    params.codes.len(),
                );
            }
            EmphasisCriteria::Quantitative(params) => {
                assert_eq!(
                    params.values.len(), expected,
                    "EmphasisCriteria values has length {} but layer has {expected} elements",
                    params.values.len(),
                );
            }
        }
    }
}

/// A categorical column in categories+codes format (one category code per
/// item, similar to [`ColorMode::Categorical`]), along with the set of
/// category codes that are included.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CategoricalCriteriaParams {
    /// A category code per item. Length must be equal to the number of instances.
    pub codes: NumericData,
    /// The category codes to include. An explicit empty list means nothing is included.
    pub included_codes: Vec<i64>,
}

/// A quantitative column (one value per item), along with the included
/// range. Omitting `min` or `max` implicitly means -infinity/+infinity in
/// that direction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantitativeCriteriaParams {
    /// A value per item. Length must be equal to the number of instances.
    pub values: NumericData,
    /// Inclusive lower bound of included values. Omitted implies -infinity.
    pub min: Option<f32>,
    /// Inclusive upper bound of included values. Omitted implies +infinity.
    pub max: Option<f32>,
}

/// Specify the font style: normal, italique, or oblique.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

/// Specify the font weight: normal or bold.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum FontWeight {
    Normal,
    Bold,
}


/// Define plot margins: left, right, top, bottom.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarginParams {
    pub margin_left: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_bottom: Option<f32>,
}

/// Shared rendering parameters at the view level (i.e., not layer-specific).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ViewParams {
    /// The view ID.
    pub view_id: String, // Just reuse the plot_id when there is a single view.
    pub width: u32,
    pub height: u32,

    pub aspect_ratio_mode: AspectRatioMode,
    pub aspect_ratio_alignment_mode: AspectRatioAlignmentMode,

    // Device pixel ratio to support retina displays.
    // Default to 1.0 for standard displays.
    // Retina screens will have a value of 2.0 or higher.
    pub device_pixel_ratio: f32,

    /// The camera state, as a 4x4 matrix.
    pub camera_view: Option<[f32; 16]>,

    /// Timeout (in ms) before bailing out of awaiting a data request.
    pub timeout: Option<u32>,

    /// If false, bail early (e.g., upon encountering a pending Promise) rather than waiting for data requests to resolve.
    pub wait_for_store_gets: bool,

    /// Allow disabling memoization/cacheing.
    // Useful for testing/debugging.
    pub cache_enabled: bool,

    /// Specify margins for plots that need them (e.g. scatterplot axes).
    ///
    /// Subtract margins from view-level width/height to obtain layer-level width/height.
    pub margins: Option<MarginParams>,

    /// Mapping from zarr store name to its metadata.
    ///
    /// Keeping track of store metadata in parallel with the store instances in
    /// [`store_objects`] enables serializing store info,
    /// facilitating the render-to-script functionality.
    pub stores: Option<HashMap<String, ZarrStoreInfo>>,

    /// Mapping from zarr store name to its zarrs store instance.
    ///
    /// Not serialized.
    #[serde(skip)]
    pub store_objects: Option<StoreMap>,
}

impl ViewParams {
    /// Given a Zarr store name, obtain the corresponding store instance from the [`ViewParams::store_objects`] hashmap.
    pub fn get_store(&self, store_name: &str) -> Arc<dyn AsyncReadableStorageTraits> {
        if let Some(store_objects) = &self.store_objects {
            if let Some(store) = store_objects.0.get(store_name) {
                return store.clone();
            }
        }
        crate::cache::get_or_init_store(store_name, self.wait_for_store_gets)
    }
}

impl Default for ViewParams {
    fn default() -> Self {
        Self {
            view_id: "default_view".to_string(),
            width: 100,
            height: 100,
            aspect_ratio_mode: AspectRatioMode::Contain,
            aspect_ratio_alignment_mode: AspectRatioAlignmentMode::Center,
            device_pixel_ratio: 1.0,
            camera_view: None,
            timeout: None,
            wait_for_store_gets: true,
            cache_enabled: true,
            margins: None,
            stores: None,
            store_objects: None,
        }
    }
}

/// Resolve which top-level store a Zarr-based layer reads from.
///
/// The layer may specify a `store_name` directly (via its `layer_params`).
/// The resolved name must be present in the keys of the top-level
/// [`ViewParams::stores`] map.
///
/// As an ergonomic shortcut, when the layer omits `store_name` and exactly one
/// store is defined at the top level, that single store is used.
///
/// # Panics
///
/// Panics when no `store_name` can be resolved, or when the resolved
/// `store_name` is not one of the keys of the top-level `stores` map.
pub fn resolve_store_name(
    layer_store_name: &Option<String>,
    view_params: &ViewParams,
) -> String {
    let stores = view_params.stores.as_ref();
    match layer_store_name {
        Some(name) => {
            if let Some(stores) = stores {
                if !stores.contains_key(name) {
                    let keys: Vec<&String> = stores.keys().collect();
                    panic!(
                        "Zarr layer store_name {name:?} is not present in the top-level \
                         `stores` map (available store names: {keys:?})."
                    );
                }
            }
            name.clone()
        }
        None => match stores {
            // Ergonomic shortcut: a single top-level store needs no explicit name.
            Some(stores) if stores.len() == 1 => stores.keys().next().unwrap().clone(),
            Some(stores) if stores.is_empty() => panic!(
                "A Zarr layer requires a `store_name`, but the top-level `stores` map is empty."
            ),
            Some(_) => panic!(
                "A Zarr layer must specify a `store_name` when multiple stores are defined \
                 in the top-level `stores` map."
            ),
            None => panic!(
                "A Zarr layer requires a `store_name` present in the top-level `stores` map, \
                 but no `stores` were provided."
            ),
        },
    }
}


/// Prepare a layer for drawing. Load data, cache expensive results, instantiate sublayers, etc.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait PreparedLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult;
}

/// Render a layer to a vector output.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DrawToSvg {
    async fn draw(&self, ctx: &mut SvgContext);
}


/// Render a layer to a raster output via the GPU (i.e., via [`wgpu`]).
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DrawToRasterGpu: MaybeSend + MaybeSync {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass);
}

/// Stub trait for CPU-based raster rendering (software rasterizer).
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DrawToRasterCpu: MaybeSend + MaybeSync {
    async fn draw(&self, cpu_context: &CpuContext<'_>, pass: &mut CpuRenderPass);
}

/// Identify which data point(s) are located at (or nearby) the given screen coordinate.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait PickableLayer {
    // TODO: should this be async?
    fn pick(&self, screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        // Default implementation: not pickable, return empty result.
        None
    }
}


// Stub trait for CPU-based compute operations.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait ComputeCpu: MaybeSend + MaybeSync {
    // TODO: what should this return?
    async fn compute(&self, cpu_context: &CpuContext<'_>);
}

// Stub trait for GPU-based compute operations via wgpu compute shaders.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait ComputeGpu: MaybeSend + MaybeSync {
    // TODO: what should this return?
    async fn compute(&self, gpu_context: &GpuContext<'_>);
}

pub trait PreparedAndDrawToSvg: PreparedLayer + DrawToSvg + MaybeSend + MaybeSync {}
impl<T: PreparedLayer + DrawToSvg + MaybeSend + MaybeSync> PreparedAndDrawToSvg for T {}

pub trait PreparedAndDrawToRasterGpu: PreparedLayer + DrawToRasterGpu + MaybeSend + MaybeSync {}
impl<T: PreparedLayer + DrawToRasterGpu + MaybeSend + MaybeSync> PreparedAndDrawToRasterGpu for T {}

pub trait PreparedAndDrawToRasterCpu: PreparedLayer + DrawToRasterCpu + MaybeSend + MaybeSync {}
impl<T: PreparedLayer + DrawToRasterCpu + MaybeSend + MaybeSync> PreparedAndDrawToRasterCpu for T {}

// Trait for layers that can prepare and render to all output formats.
pub trait PreparedAndDraw: PreparedLayer + DrawToSvg + DrawToRasterGpu + DrawToRasterCpu + PickableLayer + MaybeSend + MaybeSync {}
impl<T: PreparedLayer + DrawToSvg + DrawToRasterGpu + DrawToRasterCpu + PickableLayer + MaybeSend + MaybeSync> PreparedAndDraw for T {}



pub fn get_layer(layer_params: &LayerParams, view_params: &ViewParams) -> Box<dyn PreparedAndDraw> {
    get_layer_from_registry(&layer_params.layer_type, layer_params.layer_params.clone(), view_params)
}


pub fn get_layers(layers: &[LayerParams], view_params: &ViewParams) -> Vec<Box<dyn PreparedAndDraw>> {
    layers.iter().map(|layer_params| {
        get_layer(layer_params, view_params)
    }).collect()
}

pub async fn draw_layers_to_vector(
    view_params: &ViewParams,
    layers: &mut Vec<Box<dyn PreparedAndDraw>>,
    _gpu_context: Option<&GpuContext<'_>>,
) -> (SvgContext, RenderResult) {
    let mut ctx = init_svg(view_params.width as f64, view_params.height as f64);

    for layer in layers.iter_mut() {
        DrawToSvg::draw(layer.as_ref(), &mut ctx).await;
    }

    let bailed_early = false; // TODO: aggregate from prepare_results when timeout support is added.
    (ctx, RenderResult { bailed_early })
}

pub async fn draw_layers_to_raster(
    view_params: &ViewParams,
    layers: &mut Vec<Box<dyn PreparedAndDraw>>,
    gpu_context: &GpuContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
    out_tex: &wgpu::Texture,
) -> RenderResult {
    // For pyo3 usage, we need to use iterator types that are Send to avoid the following error
    // when iterating over vectors of layers:
    // "has type `std::slice::Iter<'_, Box<dyn PreparedAndDrawToCanvas>>` which is not `Send`"
    let layer_refs: Vec<_> = layers.iter_mut().collect();

    let out_view = out_tex.create_view(&wgpu::TextureViewDescriptor::default());

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Layered Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &out_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // TODO: make background color configurable.
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        for layer in layer_refs {
            // TODO: when/where to pass view_params to each layer? during draw call? before draw call?
            // Should we instead assume the layer already has the necessary info from view_params?
            DrawToRasterGpu::draw(layer.as_ref(), gpu_context, &mut render_pass).await;
        }

        drop(render_pass);
    }

    let bailed_early = false; // TODO: aggregate from prepare_results when timeout support is added.
    RenderResult { bailed_early }
}
