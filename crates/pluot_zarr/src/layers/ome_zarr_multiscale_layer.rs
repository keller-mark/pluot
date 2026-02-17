use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use svg::node::element::Group;

use pluot_core::log;
use pluot_core::wgpu;
use pluot_core::zarr::AsyncZarritaStore;
use pluot_core::cache::get_or_init_store;
use pluot_core::layer_traits::{
    DrawToCanvas, DrawToSvg, MarginParams, PreparedLayer, ViewParams,
};
use pluot_core::layers::multiscale_utils::{
    ResolutionLevel, VisibleTile, get_visible_tiles, select_resolution_level,
};
use pluot_core::params::PrepareResult;

use ome_zarr_metadata::v0_5::{RelaxedOmeFields, CoordinateTransform, CoordinateTransformScale};

use crate::layers::ome_zarr_bitmap_layer::{OmeZarrBitmapLayer, OmeZarrBitmapLayerParams};
use crate::layers::ome_zarr_utils::OmeZarrChannelSetting;
use crate::layers::ome_zarr_utils::{PhysicalRect, rects_overlap, bounding_box};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmeZarrMultiscaleLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    /// Zarr store name. Falls back to view_params.store_name if None.
    pub store_name: Option<String>,
    /// Path to the zarr group containing the multiscale metadata. Defaults to "/".
    pub group_path: Option<String>,
    /// Which multiscale entry to use (index into the multiscales array). Defaults to 0.
    pub multiscale_index: Option<usize>,
    /// Z slice index. Defaults to 0.
    pub target_z: Option<u32>,
    /// T slice index. Defaults to 0.
    pub target_t: Option<u32>,
    /// Channel settings specifying which channels to render and how.
    pub channel_settings: Vec<OmeZarrChannelSetting>,
    pub opacity: f32,
}

thread_local! {
    static USE_MEMO_CACHE_MULTISCALE_METADATA: RefCell<Option<HashMap<Vec<String>, Arc<OmeZarrMultiscaleMetadata>>>> = const { RefCell::new(None) };
}

async fn use_memo_multiscale_metadata(
    initializer: impl AsyncFnOnce() -> OmeZarrMultiscaleMetadata,
    keys: &[String],
    cache_enabled: bool,
) -> Arc<OmeZarrMultiscaleMetadata> {
    if !cache_enabled {
        log("Cache disabled for OME-Zarr metadata, skipping cache lookup and initialization.");
        return Arc::new(initializer().await);
    }

    let data_exists = USE_MEMO_CACHE_MULTISCALE_METADATA.with(|map| {
        map.borrow()
            .as_ref()
            .and_then(|m| m.get(keys).cloned())
    });

    if let Some(data) = data_exists {
        log("Cache hit for OME-Zarr metadata");
        return data;
    }

    log("Cache miss for OME-Zarr metadata, running initializer.");

    let data = Arc::new(initializer().await);

    USE_MEMO_CACHE_MULTISCALE_METADATA.with(|map| {
        let mut map_ref = map.borrow_mut();

        if map_ref.is_none() {
            *map_ref = Some(HashMap::new());
        }

        map_ref.as_mut().unwrap().insert(keys.to_vec(), data.clone());
    });

    data
}

/// Cached metadata parsed from the OME-Zarr group.
struct OmeZarrMultiscaleMetadata {
    /// Resolution levels derived from OME-Zarr datasets (finest first).
    resolution_levels: Vec<ResolutionLevel>,
    /// Zarr array path for each resolution level.
    dataset_paths: Vec<String>,
    /// Full array shape at each resolution level.
    full_shapes: Vec<Vec<u64>>,
    /// Chunk shape at each resolution level (full ndim).
    chunk_shapes: Vec<Vec<u64>>,
    /// Dimension order string (e.g., "tczyx").
    dimension_order: String,
    /// Axis index for X dimension.
    x_dim_i: usize,
    /// Axis index for Y dimension.
    y_dim_i: usize,
    /// Axis index for Z dimension (if present).
    z_dim_i: Option<usize>,
    /// Axis index for C (channel) dimension (if present).
    c_dim_i: Option<usize>,
    /// Axis index for T (time) dimension (if present).
    t_dim_i: Option<usize>,
}

// ---------------------------------------------------------------------------
// OmeZarrMultiscaleLayer — metadata + sublayer orchestration only
// ---------------------------------------------------------------------------

/// A sublayer group for a single resolution level.
struct LevelSublayers {
    level_idx: usize,
    sublayers: Vec<OmeZarrBitmapLayer>,
    /// Physical rectangle for each sublayer (parallel to `sublayers`).
    tile_rects: Vec<PhysicalRect>,
    prepare_results: Vec<PrepareResult>,
}

pub struct OmeZarrMultiscaleLayer {
    view_params: ViewParams,
    layer_params: OmeZarrMultiscaleLayerParams,
    store: Arc<AsyncZarritaStore>,
    store_name: String,
    /// Cached metadata, loaded on first prepare() call.
    metadata: Option<Arc<OmeZarrMultiscaleMetadata>>,
    /// Sublayers grouped by resolution level, ordered coarsest-first.
    level_sublayers: Vec<LevelSublayers>,
}

impl OmeZarrMultiscaleLayer {
    pub fn new(view_params: ViewParams, layer_params: OmeZarrMultiscaleLayerParams) -> Self {
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
            metadata: None,
            level_sublayers: Vec::new(),
        }
    }

    /// Load and parse OME-Zarr multiscale metadata from the zarr group, using the cache.
    async fn load_metadata(&self) -> Arc<OmeZarrMultiscaleMetadata> {
        let store = self.store.clone();
        let group_path = self
            .layer_params
            .group_path
            .clone()
            .unwrap_or_else(|| "/".to_string());
        let multiscale_index = self.layer_params.multiscale_index.unwrap_or(0);
        let cache_enabled = self.view_params.cache_enabled;

        let keys = vec![
            self.store_name.clone(),
            group_path.clone(),
            format!("multiscale_{}", multiscale_index),
        ];

        use_memo_multiscale_metadata(async || {
            let group = zarrs::group::Group::async_open(store.clone(), &group_path)
                .await
                .expect("Failed to open zarr group for OME-Zarr metadata");

            let attrs = group.attributes();
            let ome_fields: RelaxedOmeFields =
                serde_json::from_value(attrs.get("ome").expect("OME attribute missing").clone())
                    .expect("Failed to parse OME attributes");

            let multiscales = ome_fields
                .multiscales
                .expect("Expected OME-NGFF multiscales metadata");

            let multiscale = &multiscales[multiscale_index];

            // Find axis indices for x, y, and optionally z, c, t.
            let x_dim_i = multiscale
                .axes
                .iter()
                .position(|a| a.name == "x")
                .expect("OME-Zarr must have an x axis");
            let y_dim_i = multiscale
                .axes
                .iter()
                .position(|a| a.name == "y")
                .expect("OME-Zarr must have a y axis");
            let z_dim_i = multiscale.axes.iter().position(|a| a.name == "z");
            let c_dim_i = multiscale.axes.iter().position(|a| a.name == "c");
            let t_dim_i = multiscale.axes.iter().position(|a| a.name == "t");

            // Build dimension order string from axes.
            // TODO: use OmeDimensionOrder struct.
            let dimension_order: String = multiscale.axes.iter()
                .map(|a| a.name.chars().next().unwrap_or('?'))
                .collect();

            let mut resolution_levels = Vec::new();
            let mut dataset_paths = Vec::new();
            let mut full_shapes = Vec::new();
            let mut chunk_shapes = Vec::new();

            for dataset in &multiscale.datasets {
                let array_path = if group_path == "/" {
                    format!("/{}", dataset.path)
                } else {
                    format!("{}/{}", group_path, dataset.path)
                };

                // Open the array to read its shape and chunk grid shape.
                let array = zarrs::array::Array::async_open(store.clone(), &array_path)
                    .await
                    .unwrap_or_else(|e| panic!("Failed to open array at {}: {:?}", array_path, e));

                let shape = array.shape().to_vec();

                let img_h = shape[y_dim_i];
                let img_w = shape[x_dim_i];

                // Get the chunk size (in pixels) by querying the shape of the
                // first chunk. chunk_grid_shape() returns the *number* of
                // chunks, not the chunk size, so we must use chunk_shape().
                let ndim = shape.len();
                let origin = vec![0u64; ndim];
                let chunk_shape_vec = array.chunk_shape(&origin)
                    .expect("Failed to get chunk shape for origin chunk");
                let chunk_h = chunk_shape_vec[y_dim_i].get();
                let chunk_w = chunk_shape_vec[x_dim_i].get();

                // Extract scale values from coordinate transformations.
                let mut scale_x: f64 = 1.0;
                let mut scale_y: f64 = 1.0;
                for transform in &dataset.coordinate_transformations {
                    if let CoordinateTransform::Scale(CoordinateTransformScale::List { scale }) = transform {
                        scale_x = scale[x_dim_i] as f64;
                        scale_y = scale[y_dim_i] as f64;
                    }
                }

                // Build full chunk shape vector (all dimensions).
                let full_chunk_shape: Vec<u64> = chunk_shape_vec.iter().map(|s| s.get()).collect();

                resolution_levels.push(ResolutionLevel {
                    shape: [img_h as u32, img_w as u32],
                    chunk_shape: [chunk_h as u32, chunk_w as u32],
                    scale: [scale_y, scale_x],
                });
                dataset_paths.push(array_path);
                full_shapes.push(shape);
                chunk_shapes.push(full_chunk_shape);
            }

            OmeZarrMultiscaleMetadata {
                resolution_levels,
                dataset_paths,
                full_shapes,
                chunk_shapes,
                dimension_order,
                // TODO: use OmeDimensionOrder struct, rather than passing individual axis indices.
                x_dim_i,
                y_dim_i,
                z_dim_i,
                c_dim_i,
                t_dim_i,
            }
        }, &keys, cache_enabled).await
    }

    /// Build OmeZarrBitmapLayer sublayers for visible tiles at levels from coarsest to target_level.
    /// This method only constructs sublayer structs — no tile data is loaded here.
    fn build_sublayers(
        &self,
        metadata: &OmeZarrMultiscaleMetadata,
    ) -> Vec<LevelSublayers> {
        let target_level = select_resolution_level(
            &self.view_params,
            &metadata.resolution_levels,
        );

        let num_levels = metadata.resolution_levels.len();

        let target_z = self.layer_params.target_z.map(|v| v as u64);
        let target_t = self.layer_params.target_t.map(|v| v as u64);

        let mut all_level_sublayers = Vec::new();

        // TODO: when timeout is None, we should start at the target resolution level already;
        // In the case of timeout==None, we do not need to worry about loading coarser levels than we need,
        // as we can just wait for the target level to load without worrying about a timeout.
        // (i.e., we only need to load coarser levels when we have a timeout and want to show something while waiting for the target level to load.)

        // Iterate from coarsest to target level (inclusive).
        // Levels are ordered finest-first (index 0 = finest), so coarsest is last.
        let coarsest_idx = num_levels - 1;
        // We iterate from coarsest down to target (which is finer).
        for level_idx in (target_level..=coarsest_idx).rev() {
            let level = &metadata.resolution_levels[level_idx];
            let tiles = get_visible_tiles(&self.view_params, level);

            if tiles.is_empty() {
                continue;
            }

            let dataset_path = &metadata.dataset_paths[level_idx];
            let full_shape = &metadata.full_shapes[level_idx];
            let chunk_shape = &metadata.chunk_shapes[level_idx];

            // Convert per-resolution scale to a model_matrix.
            let scale_x = level.scale[1] as f32;
            let scale_y = level.scale[0] as f32;
            let model_matrix: [f32; 16] = [
                scale_x, 0.0, 0.0, 0.0,
                0.0, scale_y, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ];

            let mut sublayers = Vec::new();
            let mut tile_rects = Vec::new();

            for tile in &tiles {
                let tile_h = tile.tile_pixels_h as u64;
                let tile_w = tile.tile_pixels_w as u64;

                // Convert physical-space row (0 = bottom) to array-space row
                // (0 = top). The array is stored top-to-bottom, so:
                let array_row = tile.num_tile_rows - 1 - tile.row;
                let tile_y_start = array_row as u64 * level.chunk_shape[0] as u64;
                let tile_x_start = tile.col as u64 * level.chunk_shape[1] as u64;

                // Build start_slice and stop_slice for the full ndim array.
                let ndim = full_shape.len();

                log(&format!("Building sublayer for tile at level {}, row {}, col {}: tile_y_start={}, tile_x_start={}, tile_h={}, tile_w={}", level_idx, tile.row, tile.col, tile_y_start, tile_x_start, tile_h, tile_w));


                let mut start_slice = vec![0u64; ndim];
                let mut stop_slice = vec![1u64; ndim];

                start_slice[metadata.y_dim_i] = tile_y_start;
                stop_slice[metadata.y_dim_i] = tile_y_start + tile_h;
                start_slice[metadata.x_dim_i] = tile_x_start;
                stop_slice[metadata.x_dim_i] = tile_x_start + tile_w;

                if let Some(z_dim_i) = metadata.z_dim_i {
                    let z = target_z.unwrap_or(0);
                    start_slice[z_dim_i] = z;
                    stop_slice[z_dim_i] = z + 1;
                }
                if let Some(t_dim_i) = metadata.t_dim_i {
                    let t = target_t.unwrap_or(0);
                    start_slice[t_dim_i] = t;
                    stop_slice[t_dim_i] = t + 1;
                }
                // C dimension: set to 0..1 as placeholder; OmeZarrBitmapLayer
                // overrides per-channel based on channel_settings c_index.
                if let Some(c_dim_i) = metadata.c_dim_i {
                    start_slice[c_dim_i] = 0;
                    stop_slice[c_dim_i] = 1;
                }

                sublayers.push(OmeZarrBitmapLayer::new(
                    self.view_params.clone(),
                    OmeZarrBitmapLayerParams {
                        store_name: Some(self.store_name.clone()),
                        array_path: dataset_path.clone(),
                        array_shape: full_shape.clone(),
                        array_chunk_shape: chunk_shape.clone(),
                        array_dimension_order: metadata.dimension_order.clone(),
                        target_z,
                        target_t,
                        model_matrix,
                        start_slice,
                        stop_slice,
                        channel_settings: self.layer_params.channel_settings.clone(),
                        layer_id: format!(
                            "{}_level{}_tile_{}_{}",
                            self.layer_params.layer_id, level_idx, tile.row, tile.col
                        ),
                        bounds: self.layer_params.bounds.clone(),
                        opacity: self.layer_params.opacity,
                    },
                ));

                tile_rects.push(PhysicalRect {
                    x0: tile.phys_x0,
                    y0: tile.phys_y0,
                    x1: tile.phys_x0 + tile.tile_pixels_w * level.scale[1],
                    y1: tile.phys_y0 + tile.tile_pixels_h * level.scale[0],
                });
            }

            all_level_sublayers.push(LevelSublayers {
                level_idx,
                sublayers,
                tile_rects,
                prepare_results: Vec::new(),
            });
        }

        all_level_sublayers
    }

    /// Check if a coarse tile's physical region is fully occluded by ready finer tiles.
    ///
    /// Iterates through finer level groups (starting at `from_group_idx`) and checks
    /// whether any single finer level has all its overlapping tiles ready, fully covering
    /// the coarse tile's physical extent.
    fn is_tile_occluded(&self, coarse_rect: &PhysicalRect, from_group_idx: usize) -> bool {
        for finer_group in &self.level_sublayers[from_group_idx..] {
            // Skip this finer level if any of its tiles bailed early.
            let all_ready = finer_group
                .prepare_results
                .iter()
                .all(|r| !r.bailed_early);
            if !all_ready {
                continue;
            }

            // Collect the physical rects of ready finer tiles that overlap
            // the coarse tile's region.
            let overlapping_rects: Vec<&PhysicalRect> = finer_group
                .tile_rects
                .iter()
                .enumerate()
                .filter(|(i, rect)| {
                    !finer_group.prepare_results[*i].bailed_early
                        && rects_overlap(coarse_rect, rect)
                })
                .map(|(_, rect)| rect)
                .collect();

            if overlapping_rects.is_empty() {
                continue;
            }

            // Check if the overlapping finer tiles fully cover the coarse rect.
            // Since tiles within a level are axis-aligned and form a regular grid
            // (no gaps between them), we just need to verify that the bounding box
            // of overlapping finer tiles contains the coarse rect.
            let union = bounding_box(&overlapping_rects);
            if union.contains(coarse_rect) {
                return true;
            }
        }
        false
    }
}



#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for OmeZarrMultiscaleLayer {
    async fn prepare(&mut self) -> PrepareResult {
        // Load metadata (cached via use_memo_multiscale_metadata).
        let metadata = self.load_metadata().await;
        self.metadata = Some(metadata.clone());
        let metadata = metadata.as_ref();

        // Build sublayers for all visible tiles at each level from coarsest to target.
        // No tile data is loaded here — only sublayer structs are constructed.
        self.level_sublayers = self.build_sublayers(metadata);

        log(&format!("Preparing {} sublayers", self.level_sublayers.len()));

        // Prepare all sublayers (each loads its own tile data with caching).
        let mut any_bailed = false;
        for level_group in &mut self.level_sublayers {
            let mut results = Vec::new();
            for sublayer in &mut level_group.sublayers {
                let result = sublayer.prepare().await;
                if result.bailed_early {
                    any_bailed = true;
                }
                results.push(result);
            }
            level_group.prepare_results = results;
        }

        PrepareResult {
            bailed_early: any_bailed,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToCanvas for OmeZarrMultiscaleLayer {
    async fn draw(&self, device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass) {
        // level_sublayers is ordered coarsest-first.
        // Draw levels from coarsest to finest, but skip coarser tiles that are
        // fully occluded by ready finer-level tiles.
        let num_groups = self.level_sublayers.len();

        for (group_i, level_group) in self.level_sublayers.iter().enumerate() {
            let all_ready = level_group
                .prepare_results
                .iter()
                .all(|r| !r.bailed_early);

            if !all_ready {
                continue;
            }

            // For non-finest groups, check if each coarse tile is fully covered
            // by ready tiles at any single finer level.
            let is_finest = group_i == num_groups - 1;

            for (tile_i, sublayer) in level_group.sublayers.iter().enumerate() {
                let should_draw = if is_finest {
                    true
                } else {
                    let coarse_rect = &level_group.tile_rects[tile_i];
                    // Check finer groups (higher indices = finer levels).
                    // If any finer group fully covers this coarse tile with
                    // ready tiles, skip drawing it.
                    !self.is_tile_occluded(coarse_rect, group_i + 1)
                };

                if should_draw {
                    DrawToCanvas::draw(sublayer, device.clone(), queue.clone(), pass)
                        .await;
                }
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for OmeZarrMultiscaleLayer {
    async fn draw(&self, group: &Group) -> Group {
        // SVG rendering is not yet supported for bitmap-based layers.
        // Return the group unchanged.
        group.clone()
    }
}
