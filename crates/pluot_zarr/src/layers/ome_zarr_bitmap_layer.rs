use std::sync::Arc;

use serde::{Deserialize, Serialize};
use svg::node::element::Group;

use pluot_core::log;
use pluot_core::wgpu;
use pluot_core::zarr::AsyncZarritaStore;
use pluot_core::cache::{get_or_init_store, use_memo_numeric_data};
use pluot_core::layer_traits::{
    DrawToCanvas, DrawToSvg, MarginParams, PreparedLayer, UnitsMode, ViewParams,
};
use pluot_core::layers::bitmap_layer::{
    BitmapLayer, BitmapLayerParams, ChannelSettings, DimensionOrder, NumericData,
    base_draw_bitmap_layer, base_draw_bitmap_layer_svg,
};
use pluot_core::layers::multiscale_utils::{
    ResolutionLevel, VisibleTile, get_visible_tiles, select_resolution_level,
};
use pluot_core::params::PrepareResult;
use pluot_core::two::svg::update_svg;

use ome_zarr_metadata::v0_5::{RelaxedOmeFields, CoordinateTransform, CoordinateTransformScale};

use crate::layers::ome_zarr_utils::OmeZarrChannelSetting;

/// Parameters for constructing an `OmeZarrBitmapLayer`.
pub struct OmeZarrBitmapLayerParams {
    /// Zarr store name. Falls back to view_params.store_name if None.
    pub store_name: Option<String>,

    // Tile identity
    pub level_idx: usize,
    pub row: i32,
    pub col: i32,
    pub tile_pixels_h: u64,
    pub tile_pixels_w: u64,
    pub tile_y_start: u64,
    pub tile_x_start: u64,

    // Metadata from the parent (subset needed for loading)
    pub dataset_path: String,
    pub full_shape: Vec<u64>,
    pub x_dim_i: usize,
    pub y_dim_i: usize,
    pub z_dim_i: Option<usize>,
    pub c_dim_i: Option<usize>,
    pub t_dim_i: Option<usize>,
    pub scale: [f64; 2], // [scale_y, scale_x]

    // Rendering params
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub channel_settings: Vec<ChannelSettings>,
    pub ome_channel_settings: Vec<OmeZarrChannelSetting>,
    pub target_z: u64,
    pub target_t: u64,
    pub opacity: f32,
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

    /// Load tile data from the zarr array, using the cache.
    async fn load_tile_data(&self) -> NumericData {
        let store = self.store.clone();
        let dataset_path = self.layer_params.dataset_path.clone();
        let full_shape = self.layer_params.full_shape.clone();
        let x_dim_i = self.layer_params.x_dim_i;
        let y_dim_i = self.layer_params.y_dim_i;
        let z_dim_i = self.layer_params.z_dim_i;
        let c_dim_i = self.layer_params.c_dim_i;
        let t_dim_i = self.layer_params.t_dim_i;
        let tile_y_start = self.layer_params.tile_y_start;
        let tile_x_start = self.layer_params.tile_x_start;
        let tile_h = self.layer_params.tile_pixels_h;
        let tile_w = self.layer_params.tile_pixels_w;
        let target_z = self.layer_params.target_z;
        let target_t = self.layer_params.target_t;
        let ome_channel_settings = self.layer_params.ome_channel_settings.clone();
        let level_idx = self.layer_params.level_idx;
        let row = self.layer_params.row;
        let col = self.layer_params.col;

        // Build cache keys that uniquely identify this tile's data.
        let mut keys: Vec<String> = vec![
            self.store_name.clone(),
            dataset_path.clone(),
            format!("level_{}", level_idx),
            format!("row_{}", row),
            format!("col_{}", col),
            format!("z_{}", target_z),
            format!("t_{}", target_t),
        ];
        for cs in &ome_channel_settings {
            keys.push(format!("c_{}", cs.c_index));
        }

        let cached = use_memo_numeric_data(async || {
            let num_channels = ome_channel_settings.len();
            let tile_num_elements = num_channels * tile_h as usize * tile_w as usize;

            let array = zarrs::array::Array::async_open(store.clone(), &dataset_path)
                .await
                .unwrap_or_else(|e| {
                    panic!("Failed to open array at {}: {:?}", dataset_path, e)
                });

            // Build array subsets for each channel.
            let subsets: Vec<zarrs::array::ArraySubset> = ome_channel_settings
                .iter()
                .map(|cs| {
                    let ndim = full_shape.len();
                    let mut start = vec![0u64; ndim];
                    let mut shape = vec![1u64; ndim];

                    start[y_dim_i] = tile_y_start;
                    shape[y_dim_i] = tile_h;
                    start[x_dim_i] = tile_x_start;
                    shape[x_dim_i] = tile_w;

                    if let Some(c_dim_i) = c_dim_i {
                        start[c_dim_i] = cs.c_index as u64;
                        shape[c_dim_i] = 1;
                    }
                    if let Some(z_dim_i) = z_dim_i {
                        start[z_dim_i] = target_z;
                        shape[z_dim_i] = 1;
                    }
                    if let Some(t_dim_i) = t_dim_i {
                        start[t_dim_i] = target_t;
                        shape[t_dim_i] = 1;
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
                                    "Failed to load tile data at level {} ({},{}): {:?}",
                                    level_idx, row, col, e
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
        let data = self.load_tile_data().await;

        let num_channels = self.layer_params.channel_settings.len();
        let scale_x = self.layer_params.scale[1] as f32;
        let scale_y = self.layer_params.scale[0] as f32;

        let model_matrix: [f32; 16] = [
            scale_x, 0.0, 0.0, 0.0,
            0.0, scale_y, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];

        let bitmap_params = BitmapLayerParams {
            layer_id: self.layer_params.layer_id.clone(),
            bounds: self.layer_params.bounds.clone(),
            data_unit_mode: UnitsMode::Data,
            pixel_offset: Some((self.layer_params.tile_x_start as u32, self.layer_params.tile_y_start as u32)),
            model_matrix: Some(model_matrix),
            dimension_order: if self.layer_params.y_dim_i < self.layer_params.x_dim_i {
                DimensionOrder::CYX
            } else {
                DimensionOrder::CXY
            },
            shape: if self.layer_params.y_dim_i < self.layer_params.x_dim_i {
                vec![num_channels as u32, self.layer_params.tile_pixels_h as u32, self.layer_params.tile_pixels_w as u32]
            } else {
                vec![num_channels as u32, self.layer_params.tile_pixels_w as u32, self.layer_params.tile_pixels_h as u32]
            },
            channel_settings: self.layer_params.channel_settings.clone(),
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
