// Requires the generated `data/out/pbmc68k.adata.zarr` (see `data/adata.py`) and
// `data/out/blobs.sdata.zarr` (see `data/sdata.py`) fixtures.
#![cfg(all(test, not(target_arch = "wasm32"), not(feature = "lacks_gpu")))]

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use zarrs::filesystem::FilesystemStore;
use zarrs::storage::storage_adapter::sync_to_async::{SyncToAsyncSpawnBlocking, SyncToAsyncStorageAdapter};
use zarrs::storage::AsyncReadableStorageTraits;

use pluot_core::numeric_data::NumericValue;
use pluot_core::render_traits::{AspectRatioMode, ExtentableLayer, PickableLayer, PreparedLayer, ViewParams};
use pluot_core::render_types::GpuContext;
use pluot_core::viewport::{DataCoord, ScreenCoord};
use pluot_core::zarr::StoreMap;
use pluot_zarr::adata_io::{read_dataframe_index, read_matrix_column_f32, read_matrix_column_numeric, read_string_array};
use pluot_zarr::heatmap_data::AxisCriteria;
use pluot_zarr::layers::adata_zarr_dotplot_layer::{AdataZarrDotPlotLayer, AdataZarrDotPlotLayerParams, AdataZarrDotPlotPickingInfo};
use pluot_zarr::layers::adata_zarr_heatmap_layer::{AdataZarrHeatmapLayer, AdataZarrHeatmapLayerParams, AdataZarrHeatmapPickingInfo};
use pluot_zarr::layers::adata_zarr_treemap_layer::{AdataZarrTreemapLayer, AdataZarrTreemapLayerParams, AdataZarrTreemapPickingInfo};
use pluot_zarr::zarr_emphasis_criteria::{ZarrCategoricalCriteriaParams, ZarrEmphasisCriteria};
use pluot_zarr::zarr_numeric_data::load_arr_as_numeric_data;

const SIZE: u32 = 200;
const TABLE: &str = "tables/table";

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

fn store(path: &str) -> Arc<dyn AsyncReadableStorageTraits> {
    let store = Arc::new(FilesystemStore::new(path).expect("Create filesystem store"));
    Arc::new(SyncToAsyncStorageAdapter::new(store, TokioSpawnBlocking))
}

fn view_params(store: Arc<dyn AsyncReadableStorageTraits>) -> ViewParams {
    ViewParams {
        width: SIZE,
        height: SIZE,
        aspect_ratio_mode: AspectRatioMode::Ignore,
        store_objects: Some(StoreMap(HashMap::from([("store".to_string(), store)]))),
        ..Default::default()
    }
}

fn band_centers(num_bands: usize) -> Vec<f32> {
    let step = SIZE as f32 / num_bands as f32;
    (0..num_bands).map(|band| (band as f32 + 0.5) * step).collect()
}

async fn with_cpu_and_gpu<F: AsyncFn(Option<&GpuContext<'_>>)>(check: F) {
    check(None).await;
    let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
    check(Some(&GpuContext { device: &device, queue: &queue })).await;
}

const INCLUDED_BULK_LABELS: [i64; 2] = [1, 4];

async fn check_dotplot_groups_by_each_combination_of_nested_categories(gpu_context: Option<&GpuContext<'_>>) {
    let store = store("../../data/out/pbmc68k.adata.zarr");
    let var_index = read_dataframe_index(store.clone(), "/var").await.unwrap();
    let gene = var_index[10].clone();
    let values = read_matrix_column_f32(store.clone(), "/X", 10).await.unwrap();
    let columns = ["bulk_labels", "phase"];
    let mut categories = Vec::new();
    let mut codes = Vec::new();
    for column in columns {
        categories.push(read_string_array(store.clone(), &format!("/obs/{column}/categories")).await.unwrap());
        codes.push(load_arr_as_numeric_data(store.clone(), &format!("/obs/{column}/codes")).await.unwrap());
    }
    let mut expected_rows: HashMap<Vec<String>, Vec<usize>> = HashMap::new();
    for row in 0..codes[0].len() {
        let row_codes: Vec<i64> = codes.iter().map(|codes| codes.get_f32(row) as i64).collect();
        if INCLUDED_BULK_LABELS.contains(&row_codes[0]) && row_codes.iter().all(|&code| code >= 0) {
            let combination = row_codes.iter().zip(&categories).map(|(&code, categories)| categories[code as usize].clone()).collect();
            expected_rows.entry(combination).or_default().push(row);
        }
    }
    assert!(expected_rows.len() > INCLUDED_BULK_LABELS.len(), "the finer column splits the coarser one's categories");

    let mut layer = AdataZarrDotPlotLayer::new(view_params(store.clone()), AdataZarrDotPlotLayerParams {
        store_name: Some("store".to_string()),
        groupby: columns.iter().map(|column| format!("obs/{column}")).collect(),
        var_names: vec![gene.clone()],
        var_index_path: Some("var/index".to_string()),
        obs_filtering: Some(AxisCriteria::Criteria(vec![ZarrEmphasisCriteria::Categorical(ZarrCategoricalCriteriaParams {
            codes_key: "/obs/bulk_labels/codes".to_string(),
            included_codes: INCLUDED_BULK_LABELS.to_vec(),
        })])),
        ..Default::default()
    });
    assert!(!layer.prepare(gpu_context).await.bailed_early);

    let mut picked = BTreeSet::new();
    for y in band_centers(expected_rows.len()) {
        let info: AdataZarrDotPlotPickingInfo = layer.pick(ScreenCoord { x: SIZE as f32 / 2.0, y }, None).expect("a dot is picked").into_info().unwrap();
        let rows = &expected_rows[&info.obs_values];
        let mean = rows.iter().map(|&row| values[row]).sum::<f32>() / rows.len() as f32;
        let NumericValue::Float32(picked_mean) = info.mean_expression else { panic!("expected a Float32 mean") };
        assert!((picked_mean - mean).abs() < 1e-4, "{:?}: {picked_mean} != {mean}", info.obs_values);
        picked.insert(info.obs_values);
    }
    assert_eq!(picked, expected_rows.into_keys().collect());
}

#[tokio::test]
async fn dotplot_groups_by_each_combination_of_nested_categories() {
    with_cpu_and_gpu(check_dotplot_groups_by_each_combination_of_nested_categories).await;
}

fn blobs_store() -> Arc<dyn AsyncReadableStorageTraits> {
    store("../../data/out/blobs.sdata.zarr")
}

#[tokio::test]
async fn heatmap_reads_a_spatialdata_table() {
    let store = blobs_store();
    let obs_names = read_dataframe_index(store.clone(), &format!("/{TABLE}/obs")).await.unwrap();
    let var_names = read_dataframe_index(store.clone(), &format!("/{TABLE}/var")).await.unwrap();

    let mut layer = AdataZarrHeatmapLayer::new(view_params(store.clone()), AdataZarrHeatmapLayerParams {
        store_name: Some("store".to_string()),
        layer_path: format!("{TABLE}/X"),
        groupby: vec![format!("{TABLE}/obs/region")],
        ..Default::default()
    });
    assert!(!layer.prepare(None).await.bailed_early);
    let extent = layer.extent().unwrap();
    assert_eq!((extent.x, extent.y), ((0.0, var_names.len() as f32), (0.0, obs_names.len() as f32)));

    let column = read_matrix_column_numeric(store, &format!("/{TABLE}/X"), 1).await.unwrap();
    let data_coord = DataCoord::TwoD { x: 1.5, y: obs_names.len() as f32 - 0.5 };
    let info: AdataZarrHeatmapPickingInfo = layer.pick(ScreenCoord { x: 0.0, y: 0.0 }, Some(data_coord)).expect("a cell is picked").into_info().unwrap();
    assert_eq!((info.obs_name.as_str(), info.var_name.as_str()), (obs_names[0].as_str(), var_names[1].as_str()));
    assert_eq!(info.value, column.value(0));
}

#[tokio::test]
async fn treemap_counts_the_obs_of_a_spatialdata_table() {
    let store = blobs_store();
    let obs_names = read_dataframe_index(store.clone(), &format!("/{TABLE}/obs")).await.unwrap();
    let regions = read_string_array(store.clone(), &format!("/{TABLE}/obs/region/categories")).await.unwrap();

    let mut layer = AdataZarrTreemapLayer::new(view_params(store), AdataZarrTreemapLayerParams {
        store_name: Some("store".to_string()),
        groupby: vec![format!("{TABLE}/obs/region")],
        ..Default::default()
    });
    assert!(!layer.prepare(None).await.bailed_early);
    let info: AdataZarrTreemapPickingInfo = layer.pick(ScreenCoord { x: SIZE as f32 / 2.0, y: SIZE as f32 / 2.0 }, None).unwrap().into_info().unwrap();
    assert_eq!((info.obs_values, info.count), (regions, obs_names.len() as u32));
}
