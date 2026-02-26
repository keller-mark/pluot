use std::sync::Arc;

use futures_time::future::FutureExt;
use futures_time::time::Duration;
use pluot_core::maybe_timeout;

use pluot_core::log;
use pluot_core::wgpu;
use pluot_core::zarr::AsyncZarritaStore;
use pluot_core::cache::{get_or_init_store, use_memo_numeric_data};
use pluot_core::render_traits::{
    DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, MarginParams, PreparedLayer, UnitsMode, ViewParams,
};
use pluot_core::two::svg::SvgContext;
use pluot_core::layers::bitmap_layer::{
    BitmapLayer, BitmapLayerParams, ChannelSettings, DimensionOrder, NumericData,
};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use pluot_core::render_types::GpuContext;
use crate::layers::ome_zarr_utils::{OmeDim, OmeDimensionOrder, OmeZarrChannelSetting};
use pluot_core::layers::multiscale_utils::to_y_slice;

/// Parameters for constructing an `OmeZarrBitmapLayer`.
pub struct OmeZarrBitmapLayerParams {
    /// Zarr store name. Falls back to view_params.store_name if None.
    pub store_name: Option<String>,

    /// Path to the zarr array (e.g., "/0/0" for the first dataset at the first level).
    pub array_path: String,
    /// Pre-fetched array metadata from the parent multiscale layer, used to avoid
    /// re-opening the array from storage in `load_tile_data`. If None, the array
    /// is opened from storage via `async_open`.
    pub array_metadata: Option<zarrs::array::ArrayMetadata>,

    // TODO: make this layer easier to use in the absence of a parent multiscale layer
    // by making array_shape, array_chunk_shape, and array_dimension_order optional,
    // and fetching them from the array attrs or parent OME-NGFF attrs as needed.
    /// Full shape of the zarr array at this resolution level.
    pub array_shape: Vec<u64>,
    /// Chunk shape of the zarr array at this resolution level (used for cache key derivation).
    pub array_chunk_shape: Vec<u64>,
    /// Ordered dimension list (e.g., [T, C, Z, Y, X] for "tczyx").
    pub array_dimension_order: OmeDimensionOrder,

    /// Z slice index. Only required if the array has a Z dimension.
    pub target_z: Option<u64>,
    /// T slice index. Only required if the array has a T dimension.
    pub target_t: Option<u64>,

    /// Model matrix encoding the physical voxel size and any affine transforms.
    /// The parent layer should convert per-resolution scale values into this matrix.
    pub model_matrix: [f32; 16],

    // Optional X and Y slice ranges for this tile. If None, the full range of the array is loaded.
    pub slice_x: Option<(u64, u64)>,
    pub slice_y: Option<(u64, u64)>,

    /// Channel settings specifying which channels to render and how.
    /// Each entry's `c_index` determines which slice of the C dimension to load.
    pub channel_settings: Vec<OmeZarrChannelSetting>,

    pub bounds: Option<MarginParams>,
    pub opacity: f32,
    pub layer_id: String,
}

/// A sublayer that loads a single OME-Zarr tile in `prepare()` and delegates
/// rendering to an inner `BitmapLayer`. Tile data is cached via
/// `use_memo_numeric_data` so that repeated renders with the same tile visible
/// do not re-fetch from the zarr store.
pub struct OmeZarrBitmapLayer {
    view_params: ViewParams,
    layer_params: OmeZarrBitmapLayerParams,
    store: Arc<AsyncZarritaStore>,
    store_name: String,

    /// The inner BitmapLayer, constructed during `prepare()`.
    inner: Option<BitmapLayer>,
}

impl OmeZarrBitmapLayer {
    pub fn new(
        view_params: ViewParams,
        layer_params: OmeZarrBitmapLayerParams,
    ) -> Self {
        let store_name = match &layer_params.store_name {
            Some(name) => name.clone(),
            None => match &view_params.store_name {
                Some(name) => name.clone(),
                None => panic!(
                    "store_name must be specified either in layer_params or view_params for Zarr-based layers."
                ),
            },
        };

        let store = get_or_init_store(&store_name);

        Self {
            view_params,
            layer_params,
            store,
            store_name,
            inner: None,
        }
    }

    fn dim_index(&self, dim: OmeDim) -> Option<usize> {
        self.layer_params.array_dimension_order.index_of(dim)
    }

    /// Load tile data from the zarr array, using the cache.
    async fn load_tile_data(&self) -> NumericData {
        let store = self.store.clone();
        let array_path = self.layer_params.array_path.clone();
        let array_metadata = self.layer_params.array_metadata.clone();
        let slice_x = self.layer_params.slice_x;
        let slice_y = self.layer_params.slice_y;
        let channel_settings = self.layer_params.channel_settings.clone();
        let c_dim_i = self.dim_index(OmeDim::C);

        let y_dim_i = self.dim_index(OmeDim::Y).expect("array_dimension_order must contain Y");
        let x_dim_i = self.dim_index(OmeDim::X).expect("array_dimension_order must contain X");

        let array_shape = self.layer_params.array_shape.clone();
        let (y_start, y_end) = slice_y.unwrap_or((0, array_shape[y_dim_i]));
        let (x_start, x_end) = slice_x.unwrap_or((0, array_shape[x_dim_i]));

        let z_dim_i = self.dim_index(OmeDim::Z);
        let t_dim_i = self.dim_index(OmeDim::T);
        let target_z = self.layer_params.target_z;
        let target_t = self.layer_params.target_t;

        // Compute tile pixel dimensions from the slice range.
        let tile_h = y_end - y_start;
        let tile_w = x_end - x_start;

        // Build cache keys that uniquely identify this tile's data.
        let mut keys: Vec<String> = vec![
            self.store_name.clone(),
            array_path.clone(),
            format!("slice_x_{:?}", slice_x),
            format!("slice_y_{:?}", slice_y),
            format!("z_{:?}", target_z),
            format!("t_{:?}", target_t),
        ];
        for cs in &channel_settings {
            keys.push(format!("c_{}", cs.c_index));
        }

        let cached = use_memo_numeric_data(async || {
            let num_channels = channel_settings.len();
            let tile_num_elements = num_channels * tile_h as usize * tile_w as usize;

            let array = if let Some(metadata) = array_metadata {
                zarrs::array::Array::new_with_metadata(store.clone(), &array_path, metadata)
                    .unwrap_or_else(|e| {
                        panic!("Failed to create array at {}: {:?}", array_path, e)
                    })
            } else {
                zarrs::array::Array::async_open(store.clone(), &array_path)
                    .await
                    .unwrap_or_else(|e| {
                        panic!("Failed to open array at {}: {:?}", array_path, e)
                    })
            };

            // Build array subsets for each channel.
            let subsets: Vec<zarrs::array::ArraySubset> = channel_settings
                .iter()
                .map(|cs| {
                    let mut start = array_shape.iter().map(|_| 0u64).collect::<Vec<_>>();
                    let mut shape = array_shape.clone();

                    start[y_dim_i] = y_start;
                    shape[y_dim_i] = tile_h;
                    start[x_dim_i] = x_start;
                    shape[x_dim_i] = tile_w;

                    if let Some(z_dim_i) = z_dim_i {
                        let z = target_z.unwrap_or(0);
                        start[z_dim_i] = z;
                        shape[z_dim_i] = 1;
                    }
                    if let Some(t_dim_i) = t_dim_i {
                        let t = target_t.unwrap_or(0);
                        start[t_dim_i] = t;
                        shape[t_dim_i] = 1;
                    }

                    // Override the C dimension for this specific channel.
                    if let Some(c_dim_i) = c_dim_i {
                        start[c_dim_i] = cs.c_index as u64;
                        shape[c_dim_i] = 1;
                    }

                    zarrs::array::ArraySubset::new_with_start_shape(start, shape)
                        .expect("Valid array subset")
                })
                .collect();

            // Detect the array's data type to load in the native dtype.
            use zarrs::plugin::{ExtensionName, ZarrVersion};
            let dtype_name = array
                .data_type()
                .name(ZarrVersion::V3)
                .expect("Array data type must have a V3 name");

            macro_rules! load_tile_data {
                ($rust_ty:ty, $variant:ident) => {{
                    let mut combined: Vec<$rust_ty> = Vec::with_capacity(tile_num_elements);
                    for subset in &subsets {
                        let chunk = array
                            .async_retrieve_array_subset::<Vec<$rust_ty>>(subset)
                            .await
                            .unwrap_or_else(|e| {
                                panic!(
                                    "Failed to load tile data for {} {}: {:?}",
                                    array_path, subset, e
                                )
                            });
                        combined.extend_from_slice(&chunk);
                    }
                    NumericData::$variant(Arc::new(combined))
                }};
            }

            match &*dtype_name {
                "uint8" => load_tile_data!(u8, Uint8),
                "uint16" => load_tile_data!(u16, Uint16),
                "uint32" => load_tile_data!(u32, Uint32),
                "uint64" => load_tile_data!(u64, Uint64),
                "int8" => load_tile_data!(i8, Int8),
                "int16" => load_tile_data!(i16, Int16),
                "int32" => load_tile_data!(i32, Int32),
                "int64" => load_tile_data!(i64, Int64),
                "float32" => load_tile_data!(f32, Float32),
                "float64" => load_tile_data!(f64, Float64),
                _ => panic!("Unsupported zarr data type: {}", dtype_name),
            }
        }, &keys, self.view_params.cache_enabled).await;

        // Unwrap the Arc to get the owned NumericData.
        Arc::unwrap_or_clone(cached)
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for OmeZarrBitmapLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        // Use maybe_timeout to bail early if loading takes too long.
        let data_future = self.load_tile_data();

        let future_result = maybe_timeout!(data_future, self.view_params.timeout)
            .await;

        let data = match future_result {
            Ok(data_result) => data_result,
            Err(_) => {
                // Return early.
                return PrepareResult { bailed_early: true };
            }
        };

        let y_dim_i = self.dim_index(OmeDim::Y).expect("array_dimension_order must contain Y");
        let x_dim_i = self.dim_index(OmeDim::X).expect("array_dimension_order must contain X");

        let (y_start, y_end) = self.layer_params.slice_y.unwrap_or((0, self.layer_params.array_shape[y_dim_i]));
        let (x_start, x_end) = self.layer_params.slice_x.unwrap_or((0, self.layer_params.array_shape[x_dim_i]));

        let num_channels = self.layer_params.channel_settings.len();
        let tile_h = (y_end - y_start) as u32;
        let tile_w = (x_end - x_start) as u32;

        let pixel_offset_x = x_start as u32;
        // Flip array-space Y slice to physical-space (Y=0 at bottom) using to_y_slice.
        let (pixel_offset_y_phys, _) = to_y_slice(
            y_start,
            y_end,
            self.layer_params.array_shape[y_dim_i],
        );
        let pixel_offset_y = pixel_offset_y_phys as u32;

        let channel_settings: Vec<ChannelSettings> = self
            .layer_params
            .channel_settings
            .iter()
            .map(|cs| ChannelSettings {
                window: (cs.window.0, cs.window.1),
                color: (cs.color.0, cs.color.1, cs.color.2),
            })
            .collect();

        let bitmap_params = BitmapLayerParams {
            layer_id: self.layer_params.layer_id.clone(),
            bounds: self.layer_params.bounds.clone(),
            data_unit_mode: UnitsMode::Data,
            pixel_offset: Some((pixel_offset_x, pixel_offset_y)),
            model_matrix: Some(self.layer_params.model_matrix),
            dimension_order: if y_dim_i < x_dim_i {
                DimensionOrder::CYX
            } else {
                DimensionOrder::CXY
            },
            shape: if y_dim_i < x_dim_i {
                vec![num_channels as u32, tile_h, tile_w]
            } else {
                vec![num_channels as u32, tile_w, tile_h]
            },
            channel_settings,
            opacity: self.layer_params.opacity,
            data,
        };

        let mut inner = BitmapLayer::new(self.view_params.clone(), bitmap_params);
        let result = inner.prepare(gpu_context).await;
        self.inner = Some(inner);

        result
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for OmeZarrBitmapLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        if let Some(inner) = &self.inner {
            DrawToRasterGpu::draw(inner, gpu_context, pass).await;
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for OmeZarrBitmapLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for OmeZarrBitmapLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        if let Some(inner) = &self.inner {
            DrawToSvg::draw(inner, ctx).await
        }
    }
}
