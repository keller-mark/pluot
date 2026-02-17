use std::sync::Arc;

use svg::node::element::Group;

use pluot_core::wgpu;
use pluot_core::zarr::AsyncZarritaStore;
use pluot_core::cache::{get_or_init_store, use_memo_numeric_data};
use pluot_core::layer_traits::{
    DrawToCanvas, DrawToSvg, MarginParams, PreparedLayer, UnitsMode, ViewParams,
};
use pluot_core::layers::bitmap_layer::{
    BitmapLayer, BitmapLayerParams, ChannelSettings, DimensionOrder, NumericData,
};
use pluot_core::params::PrepareResult;

use crate::layers::ome_zarr_utils::{OmeZarrChannelSetting, to_y_slice};

/// Parameters for constructing an `OmeZarrBitmapLayer`.
pub struct OmeZarrBitmapLayerParams {
    /// Zarr store name. Falls back to view_params.store_name if None.
    pub store_name: Option<String>,

    /// Path to the zarr array (e.g., "/0/0" for the first dataset at the first level).
    pub array_path: String,
    /// Full shape of the zarr array at this resolution level.
    pub array_shape: Vec<u64>,
    /// Chunk shape of the zarr array at this resolution level (used for cache key derivation).
    pub array_chunk_shape: Vec<u64>,
    /// Dimension order string (e.g., "tczyx"). Each character is one of t, c, z, y, x.
    pub array_dimension_order: String,

    /// Z slice index. Only required if the array has a Z dimension.
    pub target_z: Option<u64>,
    /// T slice index. Only required if the array has a T dimension.
    pub target_t: Option<u64>,

    /// Model matrix encoding the physical voxel size and any affine transforms.
    /// The parent layer should convert per-resolution scale values into this matrix.
    pub model_matrix: [f32; 16],

    /// Start of the array subset to load (one value per dimension).
    /// For the C dimension, this value is ignored — channels are loaded
    /// individually based on `channel_settings`.
    pub start_slice: Vec<u64>,
    /// Stop (exclusive) of the array subset to load (one value per dimension).
    /// For the C dimension, this value is ignored.
    pub stop_slice: Vec<u64>,

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

    /// Find the index of a dimension character in the dimension order string.
    fn dim_index(&self, dim_char: char) -> Option<usize> {
        self.layer_params.array_dimension_order.chars().position(|c| c == dim_char)
    }

    /// Load tile data from the zarr array, using the cache.
    async fn load_tile_data(&self) -> NumericData {
        let store = self.store.clone();
        let array_path = self.layer_params.array_path.clone();
        let start_slice = self.layer_params.start_slice.clone();
        let stop_slice = self.layer_params.stop_slice.clone();
        let channel_settings = self.layer_params.channel_settings.clone();
        let c_dim_i = self.dim_index('c');

        let y_dim_i = self.dim_index('y').expect("array_dimension_order must contain 'y'");
        let x_dim_i = self.dim_index('x').expect("array_dimension_order must contain 'x'");

        // Compute tile pixel dimensions from the slice range.
        let tile_h = stop_slice[y_dim_i] - start_slice[y_dim_i];
        let tile_w = stop_slice[x_dim_i] - start_slice[x_dim_i];

        // Build cache keys that uniquely identify this tile's data.
        let mut keys: Vec<String> = vec![
            self.store_name.clone(),
            array_path.clone(),
            format!("start_{:?}", start_slice),
            format!("stop_{:?}", stop_slice),
        ];
        for cs in &channel_settings {
            keys.push(format!("c_{}", cs.c_index));
        }

        let cached = use_memo_numeric_data(async || {
            let num_channels = channel_settings.len();
            let tile_num_elements = num_channels * tile_h as usize * tile_w as usize;

            // TODO: use Array::new_with_metadata here instead of async_open,
            // if we already have the metadata from the parent.

            let array = zarrs::array::Array::async_open(store.clone(), &array_path)
                .await
                .unwrap_or_else(|e| {
                    panic!("Failed to open array at {}: {:?}", array_path, e)
                });

            // Build array subsets for each channel.
            let subsets: Vec<zarrs::array::ArraySubset> = channel_settings
                .iter()
                .map(|cs| {
                    let ndim = start_slice.len();
                    let mut start = start_slice.clone();
                    let mut shape: Vec<u64> = stop_slice.iter().zip(start_slice.iter())
                        .map(|(stop, start)| stop - start)
                        .collect();

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
    async fn prepare(&mut self) -> PrepareResult {
        // TODO: use maybe_timeout here, and bail early if loading takes too long.
        let data = self.load_tile_data().await;

        let y_dim_i = self.dim_index('y').expect("array_dimension_order must contain 'y'");
        let x_dim_i = self.dim_index('x').expect("array_dimension_order must contain 'x'");

        let num_channels = self.layer_params.channel_settings.len();
        let tile_h = (self.layer_params.stop_slice[y_dim_i] - self.layer_params.start_slice[y_dim_i]) as u32;
        let tile_w = (self.layer_params.stop_slice[x_dim_i] - self.layer_params.start_slice[x_dim_i]) as u32;

        let pixel_offset_x = self.layer_params.start_slice[x_dim_i] as u32;
        // Flip array-space Y slice to physical-space (Y=0 at bottom) using to_y_slice.
        let (pixel_offset_y_phys, _) = to_y_slice(
            self.layer_params.start_slice[y_dim_i],
            self.layer_params.stop_slice[y_dim_i],
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
        let result = inner.prepare().await;
        self.inner = Some(inner);

        result
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToCanvas for OmeZarrBitmapLayer {
    async fn draw(&self, device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass) {
        if let Some(inner) = &self.inner {
            DrawToCanvas::draw(inner, device, queue, pass).await;
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for OmeZarrBitmapLayer {
    async fn draw(&self, group: &Group) -> Group {
        if let Some(inner) = &self.inner {
            DrawToSvg::draw(inner, group).await
        } else {
            group.clone()
        }
    }
}
