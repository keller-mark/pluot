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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum AspectRatioMode {
    /*
     - 0: ignore / squeeze: For example,  a 200 x 100 canvas would show values from -1 to 1 in x and y. The -1 to 1 square would be stretched in the X direction since the canvas is wider than it is tall.

     - 1: fit (contain): For example, a 200 x 100 canvas would range from -1 to 1 in the Y direction, and from -1-extra to 1+extra in the X direction. The -1 to 1 square would keep its square aspect ratio and would be fully visible inside the rectangle (with no part of this square clipped). The pixels would be centered.

     - 2: fill (cover): For example, a 200 x 100 canvas would range from -1 to 1 in the X direction, and from -1+extra to 1-extra in the Y direction. The -1 to 1 square would keep its square aspect ratio but would be clipped in the Y direction (at the top and bottom) so that the entire canvas is filled/covered. The pixels would be centered.
     */
     Ignore,
     Contain,
     Cover,
}

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
     Center,
     Start,
     End,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum UnitsMode {
    // 0: pixels (e.g., for fixed pixel-unit sizes).
    Pixels,
    // 1: data ("world") units (e.g., for physical sizes).
    Data,
    // 2: normalized: similar to pixel-based but values are between 0 and 1, so they are agnostic to the pixel dimensions of the plot. Similar to Pixels UnitMode, does not depend on the camera state.
    Normalized,
}

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
}

/// Static (r, g, b) color shared by every element.
pub type UniformRgbParams = (u8, u8, u8);

/// Per-element RGB stored as three parallel arrays (one per channel). Each
/// value is interpreted on a 0–255 scale (matching the `(u8, u8, u8)` used by
/// the static modes) and normalized to 0–1 before shading.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstancedRgbParams {
    pub r_values: NumericData,
    pub g_values: NumericData,
    pub b_values: NumericData,
}

/// Per-element RGB stored as a single interleaved array: element `i` occupies
/// indices `3*i`, `3*i + 1`, `3*i + 2`. Values use the same 0–255 scale as
/// [`InstancedRgbParams`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstancedRgbInterleavedParams {
    pub rgb_values: NumericData,
}

/// Categorical color: per-element integer class labels sampled against a named
/// categorical palette. The label wraps around (modulo) the palette length.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CategoricalParams {
    pub codes: NumericData,
    pub colormap: CategoricalColormap,
}

/// Categorical color against a caller-supplied palette (rather than a named
/// scheme). Otherwise identical to [`CategoricalParams`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CategoricalCustomParams {
    pub values: NumericData,
    pub colormap: Vec<(u8, u8, u8)>,
}

fn default_false() -> bool {
    false
}

/// Quantitative color: per-element scalar values mapped through a named
/// continuous colormap. Values are normalized into 0–1 using `domain` (or the
/// data's own min/max when `domain` is `None`) before sampling; `reverse` flips
/// the colormap direction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantitativeParams {
    pub values: NumericData,
    pub colormap: QuantitativeColormap,
    #[serde(default = "default_false")]
    pub reverse: bool,
    // Optional (min, max) normalization domain. When `None`, the domain is
    // derived from the data's own minimum and maximum.
    #[serde(default)]
    pub domain: Option<(f32, f32)>,
}


/// How the fill color of each element in a layer is determined.
///
/// Serialized as an adjacently-tagged enum, e.g.
/// `{"color_mode": "UniformRgb", "color_params": [255, 0, 0]}`. Variants that
/// carry [`NumericData`] describe per-element color, and the layer uploads that
/// data to the GPU as one or more textures (see `RectLayer`).
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

/// Static opacity (0.0–1.0) shared by every element.
pub type UniformOpacityParams = f32;

/// Per-element opacity, one value (0.0–1.0) per element.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstancedOpacityParams {
    pub values: NumericData,
}

/// How the opacity of each element in a layer is determined.
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
    pub values: NumericData,
}

/// How the size (width or radius) of each element in a layer is determined.
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum FontWeight {
    Normal,
    Bold,
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarginParams {
    pub margin_left: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_bottom: Option<f32>,
}

// Struct to store anything at the view level (i.e., not layer-specific)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ViewParams {
    pub view_id: String, // Just reuse the plot_id when there is a single view.
    pub width: u32,
    pub height: u32,

    pub aspect_ratio_mode: AspectRatioMode,
    pub aspect_ratio_alignment_mode: AspectRatioAlignmentMode,

    // Device pixel ratio to support retina displays.
    // Default to 1.0 for standard displays.
    // Retina screens will have a value of 2.0 or higher.
    pub device_pixel_ratio: f32,

    pub camera_view: Option<[f32; 16]>,

    // Timeout in ms before bailing out of awaiting a data request.
    pub timeout: Option<u32>,

    pub wait_for_store_gets: bool,

    // Allow disabling memoization/cacheing. Useful for testing/debugging.
    pub cache_enabled: bool,

    // Margins for plots that need them (e.g. scatterplot axes).
    pub margins: Option<MarginParams>,

    /// Zarr store metadata keyed by name, forwarded from
    /// [`crate::params::RenderParams::stores`]. Zarr-based layers resolve the
    /// store they read from against these keys (see [`resolve_store_name`]).
    pub stores: Option<HashMap<String, ZarrStoreInfo>>,

    /// Concrete Zarr store objects keyed by name, threaded down from
    /// [`crate::render::render`]'s `stores` argument so that layer constructors
    /// can read from them directly rather than looking a store up in (or
    /// inserting one into) the global store registry. When present, the keys
    /// mirror those of [`ViewParams::stores`]. When `None`, layers fall back to
    /// [`crate::cache::get_or_init_store`]. Resolved per layer via
    /// [`ViewParams::get_store`].
    ///
    /// Not serialized: store objects are runtime handles, not plot parameters.
    #[serde(skip)]
    pub store_objects: Option<StoreMap>,

    // Note: Views should have margins, but these should be translated to "bounds" for layers.
    // This is because we may want to render certain layers in the margins
    // (e.g., text/line layers for axes/titles/etc).
}

impl ViewParams {
    /// Resolve the concrete Zarr store a layer should read from.
    ///
    /// When explicit store objects were threaded in (via
    /// [`crate::render::render`]'s `stores` argument and [`ViewParams::store_objects`]),
    /// the matching one is returned. Otherwise this falls back to the global
    /// store registry ([`crate::cache::get_or_init_store`]), which constructs an
    /// [`crate::zarr::AsyncZarritaStore`] dispatching to the globally registered
    /// bound functions.
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
/// [`ViewParams::stores`] map (forwarded from
/// [`crate::params::RenderParams::stores`]); it identifies the store that the
/// language bindings registered so that the `zarr_`-prefixed bound functions
/// can resolve `(store_name, key)` lookups.
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


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait PreparedLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult;
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DrawToSvg {
    async fn draw(&self, ctx: &mut SvgContext);
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DrawToRasterGpu: MaybeSend + MaybeSync {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass);
}

// Stub trait for CPU-based raster rendering (software rasterizer).
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DrawToRasterCpu: MaybeSend + MaybeSync {
    async fn draw(&self, cpu_context: &CpuContext<'_>, pass: &mut CpuRenderPass);
}

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
