// Requires the generated `data/out/pbmc68k.adata.zarr` fixture (see `data/adata.py`).
#![cfg(all(test, not(target_arch = "wasm32"), not(feature = "lacks_gpu")))]

use std::collections::HashMap;
use std::sync::Arc;

use zarrs::filesystem::FilesystemStore;
use zarrs::storage::storage_adapter::sync_to_async::{SyncToAsyncSpawnBlocking, SyncToAsyncStorageAdapter};
use zarrs::storage::AsyncReadableStorageTraits;

use pluot_core::render_traits::{AspectRatioMode, ExtentableLayer, PickableLayer, PreparedLayer, ViewParams};
use pluot_core::render_types::GpuContext;
use pluot_core::viewport::{DataCoord, ScreenCoord};
use pluot_core::zarr::StoreMap;
use pluot_zarr::adata_io::{read_dataframe_index, read_matrix_column_numeric};
use pluot_core::compute::matrix_axis_extents::{reduce_matrix_axis_extents, MatrixAxis as HeatmapAxis};
use pluot_core::numeric_data::NumericData;
use pluot_zarr::adata_io::read_matrix_layout;
use pluot_zarr::heatmap_data::{
    load_matrix_axis_percentile_domains, load_matrix_block, load_matrix_block_axis_extents, load_matrix_block_extent,
    load_matrix_percentile_domain, AxisCriteria, HeatmapNormalization, IdentifierCriteriaParams, LoadedMatrixBlock, MatrixBlock,
    MatrixBlockKey, MatrixCacheMode,
};
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
    check_picks_of(layer, store, "/X", expected, swap_axes).await;
}

async fn check_picks_of(
    layer: &AdataZarrHeatmapLayer,
    store: Arc<dyn AsyncReadableStorageTraits>,
    matrix_path: &str,
    expected: &Expected,
    swap_axes: bool,
) {
    let obs_names = read_dataframe_index(store.clone(), "/obs").await.unwrap();
    let var_index = read_dataframe_index(store.clone(), "/var").await.unwrap();
    let (num_rows, num_cols) = (expected.obs_indices.len(), expected.var_names.len());
    for row in (0..num_rows).step_by(7) {
        for col in 0..num_cols {
            let (display_x, display_y_from_top, display_h) =
                if swap_axes { (row, col, num_cols) } else { (col, row, num_rows) };
            let data_coord = DataCoord::TwoD {
                x: display_x as f32 + 0.5,
                y: (display_h - display_y_from_top) as f32 - 0.5,
            };
            let info = layer.pick(ScreenCoord { x: 0.0, y: 0.0 }, Some(data_coord)).expect("a cell is picked").info;

            let obs_index = expected.obs_indices[row];
            let var_index_of_col = var_index.iter().position(|name| *name == expected.var_names[col]).unwrap();
            let column = read_matrix_column_numeric(store.clone(), matrix_path, var_index_of_col as u64).await.unwrap();
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

#[tokio::test]
async fn var_columns_cache_mode_matches_row_blocks() {
    let store = pbmc_store();
    let (params, expected) = layer_params_and_expected(store.clone()).await;
    let params = AdataZarrHeatmapLayerParams { cache_mode: Some(MatrixCacheMode::VarColumns), ..params };

    let mut layer = AdataZarrHeatmapLayer::new(view_params(store.clone()), params);
    assert!(!layer.prepare(None).await.bailed_early);
    check_picks(&layer, store, &expected, false).await;
}

#[tokio::test]
async fn every_normalization_prepares_on_cpu_and_gpu() {
    let store = pbmc_store();
    let (params, expected) = layer_params_and_expected(store.clone()).await;
    let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
    let gpu_context = GpuContext { device: &device, queue: &queue };

    for normalization in [HeatmapNormalization::Matrix, HeatmapNormalization::PerObs, HeatmapNormalization::PerVar] {
        for (lower, upper) in [(None, None), (Some(2.0), Some(98.0))] {
            for gpu in [None, Some(&gpu_context)] {
                let params = AdataZarrHeatmapLayerParams {
                    normalization: Some(normalization),
                    normalization_lower_percentile: lower,
                    normalization_upper_percentile: upper,
                    ..params.clone()
                };
                let mut layer = AdataZarrHeatmapLayer::new(view_params(store.clone()), params);
                assert!(!layer.prepare(gpu).await.bailed_early, "{normalization:?} {lower:?} {upper:?}");
                check_picks(&layer, store.clone(), &expected, false).await;
            }
        }
    }
}

#[tokio::test]
async fn extent_is_the_filtered_cell_counts() {
    let store = pbmc_store();
    let (params, expected) = layer_params_and_expected(store.clone()).await;
    let (num_obs, num_var) = (expected.obs_indices.len() as f32, expected.var_names.len() as f32);

    let mut layer = AdataZarrHeatmapLayer::new(view_params(store.clone()), params.clone());
    assert!(layer.extent().is_none(), "no extent before prepare");
    layer.prepare(None).await;
    let extent = layer.extent().unwrap();
    assert_eq!((extent.x, extent.y, extent.z), ((0.0, num_var), (0.0, num_obs), None));

    let mut swapped = AdataZarrHeatmapLayer::new(view_params(store), AdataZarrHeatmapLayerParams { swap_axes: true, ..params });
    swapped.prepare(None).await;
    let extent = swapped.extent().unwrap();
    assert_eq!((extent.x, extent.y), ((0.0, num_obs), (0.0, num_var)));
}

// `layers/csr` and `layers/csc` hold the same thresholded copy of X, mostly zeros.
#[tokio::test]
async fn sparse_matrices_are_picked_with_their_matrix_values() {
    let store = pbmc_store();
    let (params, expected) = layer_params_and_expected(store.clone()).await;
    let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
    let gpu_context = GpuContext { device: &device, queue: &queue };

    for layer_name in ["csr", "csc"] {
        for cache_mode in [MatrixCacheMode::RowBlocks, MatrixCacheMode::VarColumns] {
            for swap_axes in [false, true] {
                for gpu in [None, Some(&gpu_context)] {
                    let params = AdataZarrHeatmapLayerParams {
                        layer: layer_name.to_string(),
                        cache_mode: Some(cache_mode),
                        swap_axes,
                        ..params.clone()
                    };
                    let mut layer = AdataZarrHeatmapLayer::new(view_params(store.clone()), params);
                    assert!(!layer.prepare(gpu).await.bailed_early, "{layer_name} {cache_mode:?}");
                    check_picks_of(&layer, store.clone(), &format!("/layers/{layer_name}"), &expected, swap_axes).await;
                }
            }
        }
    }
}

#[tokio::test]
async fn every_normalization_of_a_sparse_matrix_prepares_on_cpu_and_gpu() {
    let store = pbmc_store();
    let (params, expected) = layer_params_and_expected(store.clone()).await;
    let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
    let gpu_context = GpuContext { device: &device, queue: &queue };

    for layer_name in ["csr", "csc"] {
        for normalization in [HeatmapNormalization::Matrix, HeatmapNormalization::PerObs, HeatmapNormalization::PerVar] {
            for gpu in [None, Some(&gpu_context)] {
                let params = AdataZarrHeatmapLayerParams {
                    layer: layer_name.to_string(),
                    normalization: Some(normalization),
                    normalization_upper_percentile: Some(99.0),
                    ..params.clone()
                };
                let mut layer = AdataZarrHeatmapLayer::new(view_params(store.clone()), params);
                assert!(!layer.prepare(gpu).await.bailed_early, "{layer_name} {normalization:?}");
                check_picks_of(&layer, store.clone(), &format!("/layers/{layer_name}"), &expected, false).await;
            }
        }
    }
}

#[tokio::test]
async fn sparse_block_extents_match_the_dense_cells() {
    let store = pbmc_store();
    let (_, expected) = layer_params_and_expected(store.clone()).await;
    let var_index = read_dataframe_index(store.clone(), "/var").await.unwrap();
    let rows: Vec<u32> = expected.obs_indices.iter().map(|&i| i as u32).collect();
    let cols: Vec<u32> = expected.var_names.iter().map(|name| var_index.iter().position(|v| v == name).unwrap() as u32).collect();
    let (num_rows, num_cols) = (rows.len(), cols.len());
    let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
    let gpu_context = GpuContext { device: &device, queue: &queue };

    let matrix_path = "/layers/csr";
    let mut dense_cells = Vec::with_capacity(num_rows * num_cols);
    let columns = futures::future::join_all(cols.iter().map(|&col| read_matrix_column_numeric(store.clone(), matrix_path, col as u64))).await;
    let columns: Vec<_> = columns.into_iter().map(Result::unwrap).collect();
    for &row in &rows {
        dense_cells.extend(columns.iter().map(|column| column.get_f32(row as usize)));
    }
    let dense = NumericData::from(dense_cells);
    let dense_extent = dense.as_f32().iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| (lo.min(v), hi.max(v)));

    for matrix_path in ["/layers/csr", "/layers/csc"] {
        let layout = read_matrix_layout(store.clone(), matrix_path).await;
        for cache_mode in [MatrixCacheMode::RowBlocks, MatrixCacheMode::VarColumns] {
            let key = MatrixBlockKey {
                store_name: "pbmc",
                matrix_path,
                cache_mode,
                obs_filtering: None,
                var_filtering: None,
                rows_per_block: num_rows,
                block_index: 0,
            };
            let block = load_matrix_block(store.clone(), &key, layout, &rows, &cols, false).await.unwrap();
            assert!(matches!(block, MatrixBlock::Compressed(..)), "{matrix_path} {cache_mode:?} stays sparse");
            for gpu in [None, Some(&gpu_context)] {
                let extent = load_matrix_block_extent(gpu, &key, &block, num_rows, num_cols, false).await;
                assert_eq!(extent, dense_extent, "{matrix_path} {cache_mode:?}");
                for axis in [HeatmapAxis::Rows, HeatmapAxis::Cols] {
                    let sparse = load_matrix_block_axis_extents(gpu, &key, &block, num_rows, num_cols, axis, false).await;
                    let reference = reduce_matrix_axis_extents(None, &dense, num_rows, num_cols, axis).await;
                    assert_eq!(sparse, reference, "{matrix_path} {cache_mode:?} {axis:?} gpu={}", gpu.is_some());
                }
            }
        }
    }
}

/// Asserts that `estimate` is within a final bin width of the `percentile` of `values`.
fn assert_near_percentile(values: &[f32], percentile: f32, estimate: f32, num_bins: usize, context: &str) {
    let mut sorted: Vec<f32> = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    let n = sorted.len();
    let rank = (percentile / 100.0 * n as f32).floor() as usize;
    let (lo, hi) = (sorted[rank.saturating_sub(1)], sorted[rank.min(n - 1)]);
    let tolerance = (sorted[n - 1] - sorted[0]) / (num_bins * num_bins) as f32 + 1e-4;
    assert!(lo - tolerance <= estimate && estimate <= hi + tolerance, "{context}: p{percentile} {estimate} not within [{lo}, {hi}]");
}

#[tokio::test]
async fn percentile_domains_match_the_sorted_cells() {
    let store = pbmc_store();
    let (_, expected) = layer_params_and_expected(store.clone()).await;
    let var_index = read_dataframe_index(store.clone(), "/var").await.unwrap();
    let rows: Vec<u32> = expected.obs_indices.iter().map(|&i| i as u32).collect();
    let cols: Vec<u32> = expected.var_names.iter().map(|name| var_index.iter().position(|v| v == name).unwrap() as u32).collect();
    let (num_rows, num_cols) = (rows.len(), cols.len());
    let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
    let gpu_context = GpuContext { device: &device, queue: &queue };
    let percentiles = (5.0, 95.0);

    for matrix_path in ["/X", "/layers/csr", "/layers/csc"] {
        let layout = read_matrix_layout(store.clone(), matrix_path).await;
        let key = MatrixBlockKey {
            store_name: "pbmc",
            matrix_path,
            cache_mode: MatrixCacheMode::RowBlocks,
            obs_filtering: None,
            var_filtering: None,
            rows_per_block: num_rows,
            block_index: 0,
        };
        let block = load_matrix_block(store.clone(), &key, layout, &rows, &cols, false).await.unwrap();
        let columns = futures::future::join_all(cols.iter().map(|&col| read_matrix_column_numeric(store.clone(), matrix_path, col as u64))).await;
        let columns: Vec<_> = columns.into_iter().map(Result::unwrap).collect();
        let cell = |row: usize, col: usize| columns[col].get_f32(rows[row] as usize);
        let cells: Vec<f32> = (0..num_rows).flat_map(|row| (0..num_cols).map(move |col| (row, col))).map(|(row, col)| cell(row, col)).collect();

        for gpu in [None, Some(&gpu_context)] {
            let context = format!("{matrix_path} gpu={}", gpu.is_some());
            let extent = load_matrix_block_extent(gpu, &key, &block, num_rows, num_cols, false).await;
            let blocks = [LoadedMatrixBlock { key: MatrixBlockKey { ..key }, block: &block, num_rows }];

            let (lower, upper) = load_matrix_percentile_domain(gpu, &blocks, num_cols, extent, percentiles, false).await;
            assert_near_percentile(&cells, percentiles.0, lower, 256, &context);
            assert_near_percentile(&cells, percentiles.1, upper, 256, &context);

            for axis in [HeatmapAxis::Rows, HeatmapAxis::Cols] {
                let (mins, maxs) = load_matrix_block_axis_extents(gpu, &key, &block, num_rows, num_cols, axis, false).await;
                let (lowers, uppers) =
                    load_matrix_axis_percentile_domains(gpu, &blocks, num_cols, axis, (&mins, &maxs), percentiles, false).await;
                let (num_segments, segment_len) = if axis == HeatmapAxis::Rows { (num_rows, num_cols) } else { (num_cols, num_rows) };
                for segment in 0..num_segments {
                    let values: Vec<f32> = (0..segment_len)
                        .map(|j| if axis == HeatmapAxis::Rows { cell(segment, j) } else { cell(j, segment) })
                        .collect();
                    let context = format!("{context} {axis:?} {segment}");
                    assert_near_percentile(&values, percentiles.0, lowers[segment], segment_len.min(256), &context);
                    assert_near_percentile(&values, percentiles.1, uppers[segment], segment_len.min(256), &context);
                }
            }
        }
    }
}
