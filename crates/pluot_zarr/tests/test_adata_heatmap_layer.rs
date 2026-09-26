// Requires the generated `data/out/pbmc68k.adata.zarr` fixture (see `data/adata.py`).
#![cfg(all(test, not(target_arch = "wasm32"), not(feature = "lacks_gpu")))]

use std::collections::HashMap;
use std::sync::Arc;

use zarrs::filesystem::FilesystemStore;
use zarrs::storage::storage_adapter::sync_to_async::{SyncToAsyncSpawnBlocking, SyncToAsyncStorageAdapter};
use zarrs::storage::AsyncReadableStorageTraits;

use pluot_core::render_traits::{AspectRatioMode, PickableLayer, PreparedLayer, ViewParams};
use pluot_core::render_types::GpuContext;
use pluot_core::viewport::{DataCoord, ScreenCoord};
use pluot_core::zarr::StoreMap;
use pluot_zarr::adata_io::{read_dataframe_index, read_dense_column_numeric};
use pluot_zarr::heatmap_data::{AxisCriteria, IdentifierCriteriaParams};
use pluot_zarr::layers::adata_zarr_heatmap_layer::{AdataZarrHeatmapLayer, AdataZarrHeatmapLayerParams};
use pluot_zarr::zarr_emphasis_criteria::{ZarrCategoricalCriteriaParams, ZarrEmphasisCriteria, ZarrQuantitativeCriteriaParams};
use pluot_zarr::zarr_numeric_data::load_arr_as_numeric_data;

struct TokioSpawnBlocking;

impl SyncToAsyncSpawnBlocking for TokioSpawnBlocking {
    async fn spawn_blocking<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        tokio::task::spawn_blocking(f).await.unwrap()
    }
}

fn pbmc_store() -> Arc<dyn AsyncReadableStorageTraits> {
    let store = Arc::new(FilesystemStore::new("../../data/out/pbmc68k.adata.zarr").expect("Create filesystem store"));
    Arc::new(SyncToAsyncStorageAdapter::new(store, TokioSpawnBlocking))
}

fn view_params(store: Arc<dyn AsyncReadableStorageTraits>) -> ViewParams {
    ViewParams {
        width: 200,
        height: 200,
        aspect_ratio_mode: AspectRatioMode::Ignore,
        store_objects: Some(StoreMap(HashMap::from([("pbmc".to_string(), store)]))),
        ..Default::default()
    }
}

struct Expected {
    obs_indices: Vec<usize>,
    var_names: Vec<String>,
}

async fn layer_params_and_expected(store: Arc<dyn AsyncReadableStorageTraits>) -> (AdataZarrHeatmapLayerParams, Expected) {
    let var_index = read_dataframe_index(store.clone(), "/var").await.unwrap();
    let var_names: Vec<String> = vec![var_index[10].clone(), var_index[3].clone(), var_index[700].clone()];

    let codes = load_arr_as_numeric_data(store.clone(), "/obs/bulk_labels/codes").await.unwrap();
    let obs_indices: Vec<usize> = (0..codes.len()).filter(|&i| codes.get_f32(i) as i64 == 2).collect();

    let params = AdataZarrHeatmapLayerParams {
        layer_id: "heatmap".to_string(),
        store_name: Some("pbmc".to_string()),
        obs_filtering: Some(AxisCriteria::Criteria(vec![ZarrEmphasisCriteria::Categorical(ZarrCategoricalCriteriaParams {
            codes_key: "/obs/bulk_labels/codes".to_string(),
            included_codes: vec![2],
        })])),
        obs_selection: Some(AxisCriteria::Criteria(vec![ZarrEmphasisCriteria::Quantitative(ZarrQuantitativeCriteriaParams {
            values_key: "/obs/n_genes".to_string(),
            min: Some(1000.0),
            max: None,
            min_exclusive: None,
            max_exclusive: None,
        })])),
        var_filtering: Some(AxisCriteria::Identifiers(IdentifierCriteriaParams { column: None, identifiers: var_names.clone() })),
        ..Default::default()
    };
    (params, Expected { obs_indices, var_names })
}

/// Picks the center of every `stride`-th cell and checks it against the matrix read directly.
async fn check_picks(layer: &AdataZarrHeatmapLayer, store: Arc<dyn AsyncReadableStorageTraits>, expected: &Expected, swap_axes: bool) {
    let obs_names = read_dataframe_index(store.clone(), "/obs").await.unwrap();
    let var_index = read_dataframe_index(store.clone(), "/var").await.unwrap();
    let (num_rows, num_cols) = (expected.obs_indices.len(), expected.var_names.len());
    for row in (0..num_rows).step_by(7) {
        for col in 0..num_cols {
            let (display_x, display_y_from_top, display_w, display_h) =
                if swap_axes { (row, col, num_rows, num_cols) } else { (col, row, num_cols, num_rows) };
            let data_coord = DataCoord::TwoD {
                x: (display_x as f32 + 0.5) / display_w as f32,
                y: 1.0 - (display_y_from_top as f32 + 0.5) / display_h as f32,
            };
            let info = layer.pick(ScreenCoord { x: 0.0, y: 0.0 }, Some(data_coord)).expect("a cell is picked").info;

            let obs_index = expected.obs_indices[row];
            let var_index_of_col = var_index.iter().position(|name| *name == expected.var_names[col]).unwrap();
            let column = read_dense_column_numeric(store.clone(), "/X", var_index_of_col as u64).await.unwrap();
            assert_eq!(info["obs_index"], obs_index.to_string());
            assert_eq!(info["obs_name"], obs_names[obs_index]);
            assert_eq!(info["var_name"], expected.var_names[col]);
            assert_eq!(info["value"], column.format_element(obs_index));
        }
    }
}

#[tokio::test]
async fn filtered_cells_are_picked_with_their_matrix_values() {
    let store = pbmc_store();
    let (params, expected) = layer_params_and_expected(store.clone()).await;
    assert!(!expected.obs_indices.is_empty());

    let mut layer = AdataZarrHeatmapLayer::new(view_params(store.clone()), params);
    assert!(!layer.prepare(None).await.bailed_early);
    check_picks(&layer, store, &expected, false).await;
}

#[tokio::test]
async fn swapped_axes_are_picked_with_their_matrix_values() {
    let store = pbmc_store();
    let (params, expected) = layer_params_and_expected(store.clone()).await;
    let params = AdataZarrHeatmapLayerParams { swap_axes: true, ..params };

    let mut layer = AdataZarrHeatmapLayer::new(view_params(store.clone()), params);
    assert!(!layer.prepare(None).await.bailed_early);
    check_picks(&layer, store, &expected, true).await;
}

#[tokio::test]
async fn gpu_filtering_matches_cpu_filtering() {
    let store = pbmc_store();
    let (params, expected) = layer_params_and_expected(store.clone()).await;
    let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
    let gpu_context = GpuContext { device: &device, queue: &queue };

    let mut layer = AdataZarrHeatmapLayer::new(view_params(store.clone()), params);
    assert!(!layer.prepare(Some(&gpu_context)).await.bailed_early);
    check_picks(&layer, store, &expected, false).await;
}
