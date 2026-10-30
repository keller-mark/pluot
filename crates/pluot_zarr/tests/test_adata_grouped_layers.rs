// Requires the generated `data/out/pbmc68k.adata.zarr` fixture (see `data/adata.py`).
#![cfg(all(test, not(target_arch = "wasm32"), not(feature = "lacks_gpu")))]

use std::collections::HashMap;
use std::sync::Arc;

use zarrs::filesystem::FilesystemStore;
use zarrs::storage::storage_adapter::sync_to_async::{SyncToAsyncSpawnBlocking, SyncToAsyncStorageAdapter};
use zarrs::storage::AsyncReadableStorageTraits;

use pluot_core::numeric_data::NumericValue;
use pluot_core::render_traits::{AspectRatioMode, PickableLayer, PreparedLayer, ViewParams};
use pluot_core::render_types::GpuContext;
use pluot_core::viewport::ScreenCoord;
use pluot_core::zarr::StoreMap;
use pluot_zarr::adata_io::{read_dataframe_index, read_matrix_column_f32, read_string_array};
use pluot_zarr::heatmap_data::{AxisCriteria, ExpressionCriteriaParams, IdentifierCriteriaParams};
use pluot_zarr::layers::adata_zarr_dotplot_layer::{AdataZarrDotPlotLayer, AdataZarrDotPlotLayerParams, AdataZarrDotPlotPickingInfo};
use pluot_zarr::layers::adata_zarr_treemap_layer::{AdataZarrTreemapLayer, AdataZarrTreemapLayerParams, AdataZarrTreemapPickingInfo};
use pluot_zarr::layers::adata_zarr_violin_layer::{AdataZarrViolinLayer, AdataZarrViolinLayerParams, AdataZarrViolinPickingInfo};
use pluot_zarr::layers::adata_zarr_violin_matrix_layer::{
    AdataZarrViolinMatrixLayer, AdataZarrViolinMatrixLayerParams, AdataZarrViolinMatrixPickingInfo,
};
use pluot_zarr::zarr_emphasis_criteria::{ZarrCategoricalCriteriaParams, ZarrEmphasisCriteria, ZarrQuantitativeCriteriaParams};
use pluot_zarr::zarr_numeric_data::load_arr_as_numeric_data;

const SIZE: u32 = 200;

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
        width: SIZE,
        height: SIZE,
        aspect_ratio_mode: AspectRatioMode::Ignore,
        store_objects: Some(StoreMap(HashMap::from([("pbmc".to_string(), store)]))),
        ..Default::default()
    }
}

const INCLUDED_CODES: [i64; 2] = [1, 4];
const MIN_N_GENES: f32 = 1300.0;

fn obs_filtering() -> Option<AxisCriteria> {
    Some(AxisCriteria::Criteria(vec![ZarrEmphasisCriteria::Categorical(ZarrCategoricalCriteriaParams {
        codes_key: "/obs/bulk_labels/codes".to_string(),
        included_codes: INCLUDED_CODES.to_vec(),
    })]))
}

fn obs_selection() -> Option<AxisCriteria> {
    Some(AxisCriteria::Criteria(vec![ZarrEmphasisCriteria::Quantitative(ZarrQuantitativeCriteriaParams {
        values_key: "/obs/n_genes".to_string(),
        min: Some(MIN_N_GENES),
        max: None,
        min_exclusive: None,
        max_exclusive: None,
    })]))
}

fn identifiers(genes: &[String]) -> Option<AxisCriteria> {
    Some(AxisCriteria::Identifiers(IdentifierCriteriaParams { column: None, identifiers: genes.to_vec() }))
}

/// The filter-included categories (in stored order), and the selected rows of each.
async fn selected_rows_by_category(store: Arc<dyn AsyncReadableStorageTraits>) -> (Vec<String>, HashMap<String, Vec<usize>>) {
    let categories = read_string_array(store.clone(), "/obs/bulk_labels/categories").await.unwrap();
    let codes = load_arr_as_numeric_data(store.clone(), "/obs/bulk_labels/codes").await.unwrap();
    let n_genes = load_arr_as_numeric_data(store, "/obs/n_genes").await.unwrap();
    let included: Vec<String> = INCLUDED_CODES.iter().map(|&code| categories[code as usize].clone()).collect();
    let mut rows: HashMap<String, Vec<usize>> = included.iter().map(|category| (category.clone(), Vec::new())).collect();
    let mut num_unselected = 0;
    for row in 0..codes.len() {
        let code = codes.get_f32(row) as i64;
        if !INCLUDED_CODES.contains(&code) {
            continue;
        }
        if n_genes.get_f32(row) >= MIN_N_GENES {
            rows.get_mut(&categories[code as usize]).unwrap().push(row);
        } else {
            num_unselected += 1;
        }
    }
    assert!(num_unselected > 0 && rows.values().all(|rows| !rows.is_empty()), "the selection narrows every category without emptying it");
    (included, rows)
}

async fn var_names(store: Arc<dyn AsyncReadableStorageTraits>, indices: &[usize]) -> Vec<String> {
    let var_index = read_dataframe_index(store, "/var").await.unwrap();
    indices.iter().map(|&index| var_index[index].clone()).collect()
}

async fn expression(store: Arc<dyn AsyncReadableStorageTraits>, gene: &str) -> Vec<f32> {
    let var_index = read_dataframe_index(store.clone(), "/var").await.unwrap();
    let col_index = var_index.iter().position(|name| name == gene).unwrap();
    read_matrix_column_f32(store, "/X", col_index as u64).await.unwrap()
}

/// The center of each of `num_bands` equal bands along `SIZE` pixels.
fn band_centers(num_bands: usize) -> Vec<f32> {
    let step = SIZE as f32 / num_bands as f32;
    (0..num_bands).map(|band| (band as f32 + 0.5) * step).collect()
}

async fn check_dotplot_summarizes_the_selected_obs_of_the_filtered_categories_and_vars(gpu_context: Option<&GpuContext<'_>>) {
    let store = pbmc_store();
    let requested = var_names(store.clone(), &[10, 3, 700]).await;
    let (categories, selected_rows) = selected_rows_by_category(store.clone()).await;

    let mut layer = AdataZarrDotPlotLayer::new(view_params(store.clone()), AdataZarrDotPlotLayerParams {
        store_name: Some("pbmc".to_string()),
        groupby: "bulk_labels".to_string(),
        var_names: requested.clone(),
        obs_filtering: obs_filtering(),
        obs_selection: obs_selection(),
        var_filtering: identifiers(&[requested[2].clone(), requested[0].clone()]),
        var_selection: identifiers(&[requested[0].clone()]),
        ..Default::default()
    });
    assert!(!layer.prepare(gpu_context).await.bailed_early);

    let genes = [requested[0].clone(), requested[2].clone()];
    let mut picked_categories = Vec::new();
    for (gene, x) in genes.iter().zip(band_centers(genes.len())) {
        let values = expression(store.clone(), gene).await;
        for y in band_centers(categories.len()) {
            let info: AdataZarrDotPlotPickingInfo = layer.pick(ScreenCoord { x, y }, None).expect("a dot is picked").into_info().unwrap();
            assert_eq!(&info.var_name, gene);
            let rows = &selected_rows[&info.obs_value];
            let mean = rows.iter().map(|&row| values[row]).sum::<f32>() / rows.len() as f32;
            let fraction_expressing = rows.iter().filter(|&&row| values[row] > 0.0).count() as f32 / rows.len() as f32;
            let NumericValue::Float32(picked_mean) = info.mean_expression else { panic!("expected a Float32 mean") };
            assert!((picked_mean - mean).abs() < 1e-4, "{gene} {}: {picked_mean} != {mean}", info.obs_value);
            assert_eq!(info.fraction_expressing, NumericValue::Float32(fraction_expressing));
            picked_categories.push(info.obs_value);
        }
    }
    picked_categories.sort();
    picked_categories.dedup();
    let mut expected_categories = categories.clone();
    expected_categories.sort();
    assert_eq!(picked_categories, expected_categories);
}

async fn check_violin_summarizes_the_selected_obs_of_the_filtered_categories(gpu_context: Option<&GpuContext<'_>>) {
    let store = pbmc_store();
    let gene = var_names(store.clone(), &[10]).await.remove(0);
    let (categories, selected_rows) = selected_rows_by_category(store.clone()).await;

    let mut layer = AdataZarrViolinLayer::new(view_params(store.clone()), AdataZarrViolinLayerParams {
        store_name: Some("pbmc".to_string()),
        groupby: "bulk_labels".to_string(),
        var_name: gene,
        obs_filtering: obs_filtering(),
        obs_selection: obs_selection(),
        ..Default::default()
    });
    assert!(!layer.prepare(gpu_context).await.bailed_early);

    for (category, x) in categories.iter().zip(band_centers(categories.len())) {
        let info: AdataZarrViolinPickingInfo = layer.pick(ScreenCoord { x, y: SIZE as f32 / 2.0 }, None).expect("a violin is picked").into_info().unwrap();
        assert_eq!(&info.obs_value, category);
        assert_eq!(info.num_values as usize, selected_rows[category].len());
    }
}

async fn check_violin_matrix_summarizes_the_selected_obs_of_the_filtered_categories_and_vars(gpu_context: Option<&GpuContext<'_>>) {
    let store = pbmc_store();
    let requested = var_names(store.clone(), &[10, 3, 700]).await;
    let (categories, selected_rows) = selected_rows_by_category(store.clone()).await;

    let mut layer = AdataZarrViolinMatrixLayer::new(view_params(store.clone()), AdataZarrViolinMatrixLayerParams {
        store_name: Some("pbmc".to_string()),
        groupby: "bulk_labels".to_string(),
        var_names: requested.clone(),
        obs_filtering: obs_filtering(),
        obs_selection: obs_selection(),
        var_filtering: identifiers(&[requested[2].clone(), requested[1].clone()]),
        var_selection: identifiers(&[requested[1].clone()]),
        ..Default::default()
    });
    assert!(!layer.prepare(gpu_context).await.bailed_early);

    let genes = [requested[1].clone(), requested[2].clone()];
    let mut picked_categories = Vec::new();
    for (gene, x) in genes.iter().zip(band_centers(genes.len())) {
        for y in band_centers(categories.len()) {
            let info: AdataZarrViolinMatrixPickingInfo = layer.pick(ScreenCoord { x, y }, None).expect("a cell is picked").into_info().unwrap();
            assert_eq!(&info.var_name, gene);
            assert_eq!(info.num_values as usize, selected_rows[&info.obs_value].len());
            picked_categories.push(info.obs_value);
        }
    }
    picked_categories.sort();
    picked_categories.dedup();
    let mut expected_categories = categories.clone();
    expected_categories.sort();
    assert_eq!(picked_categories, expected_categories);
}

async fn check_treemap_counts_the_selected_obs_of_the_filtered_categories(gpu_context: Option<&GpuContext<'_>>) {
    let store = pbmc_store();
    let gene = var_names(store.clone(), &[10]).await.remove(0);
    let values = expression(store.clone(), &gene).await;
    let categories = read_string_array(store.clone(), "/obs/bulk_labels/categories").await.unwrap();
    let codes = load_arr_as_numeric_data(store.clone(), "/obs/bulk_labels/codes").await.unwrap();

    let mut layer = AdataZarrTreemapLayer::new(view_params(store.clone()), AdataZarrTreemapLayerParams {
        store_name: Some("pbmc".to_string()),
        groupby: vec!["bulk_labels".to_string()],
        obs_filtering: obs_filtering(),
        obs_selection: Some(AxisCriteria::Expression(ExpressionCriteriaParams {
            var_name: gene,
            var_column: None,
            layer: None,
            min: Some(0.0),
            max: None,
            min_exclusive: Some(true),
            max_exclusive: None,
        })),
        ..Default::default()
    });
    assert!(!layer.prepare(gpu_context).await.bailed_early);

    let mut expected: HashMap<String, (u32, u32)> = HashMap::new();
    for row in 0..codes.len() {
        let code = codes.get_f32(row) as i64;
        if INCLUDED_CODES.contains(&code) {
            let (count, selected_count) = expected.entry(categories[code as usize].clone()).or_default();
            *count += 1;
            *selected_count += (values[row] > 0.0) as u32;
        }
    }
    assert!(expected.values().all(|&(count, selected_count)| 0 < selected_count && selected_count < count), "the selection narrows every category without emptying it");

    let mut picked: HashMap<String, (u32, u32)> = HashMap::new();
    for x in band_centers(20) {
        for y in band_centers(20) {
            if let Some(result) = layer.pick(ScreenCoord { x, y }, None) {
                let info: AdataZarrTreemapPickingInfo = result.into_info().unwrap();
                picked.insert(info.obs_values[0].clone(), (info.count, info.selected_count.expect("a selected count")));
            }
        }
    }
    assert_eq!(picked, expected);
}

async fn with_cpu_and_gpu<F: AsyncFn(Option<&GpuContext<'_>>)>(check: F) {
    check(None).await;
    let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
    check(Some(&GpuContext { device: &device, queue: &queue })).await;
}

#[tokio::test]
async fn dotplot_summarizes_the_selected_obs_of_the_filtered_categories_and_vars() {
    with_cpu_and_gpu(check_dotplot_summarizes_the_selected_obs_of_the_filtered_categories_and_vars).await;
}

#[tokio::test]
async fn violin_summarizes_the_selected_obs_of_the_filtered_categories() {
    with_cpu_and_gpu(check_violin_summarizes_the_selected_obs_of_the_filtered_categories).await;
}

#[tokio::test]
async fn violin_matrix_summarizes_the_selected_obs_of_the_filtered_categories_and_vars() {
    with_cpu_and_gpu(check_violin_matrix_summarizes_the_selected_obs_of_the_filtered_categories_and_vars).await;
}

#[tokio::test]
async fn treemap_counts_the_selected_obs_of_the_filtered_categories() {
    with_cpu_and_gpu(check_treemap_counts_the_selected_obs_of_the_filtered_categories).await;
}
