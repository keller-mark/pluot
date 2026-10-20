#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    CompressedHeatmapLayerParams, CompressedMatrix, AspectRatioMode, HeatmapAxisDomains, HeatmapColormap,
    HeatmapLayerParams, HeatmapQuantitativeColormapParams, LayerParams, MatrixAxis, NumericData, QuantitativeColormap,
    RenderParams,
};

// Each dense, CSR and CSC encoding of a matrix renders to one shared snapshot.

const DENSE_3X4: [f32; 12] = [
    0.0, 5.0, 0.0, 7.0,
    0.0, 0.0, 0.0, 0.0,
    3.0, 0.0, 8.0, 0.0,
];

fn colormap(axis_domains: Option<HeatmapAxisDomains>) -> Option<HeatmapColormap> {
    Some(HeatmapColormap::Quantitative(HeatmapQuantitativeColormapParams {
        colormap: QuantitativeColormap::Viridis,
        reverse: false,
        domain: Some((0.0, 8.0)),
        axis_domains,
    }))
}

fn fit(num_rows: u32, num_cols: u32, swap_axes: bool) -> [f32; 16] {
    let (width, height) = if swap_axes { (num_rows, num_cols) } else { (num_cols, num_rows) };
    [
        1.0 / width as f32, 0.0, 0.0, 0.0,
        0.0, 1.0 / height as f32, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]
}

fn params(layer: LayerParams) -> RenderParams {
    RenderParams {
        width: 100,
        height: 100,
        aspect_ratio_mode: AspectRatioMode::Ignore,
        layers: vec![layer],
        ..Default::default()
    }
}

fn compressed(dense: &HeatmapLayerParams, matrix: CompressedMatrix) -> CompressedHeatmapLayerParams {
    CompressedHeatmapLayerParams {
        layer_id: dense.layer_id.clone(),
        model_matrix: dense.model_matrix,
        num_rows: dense.num_rows,
        num_cols: dense.num_cols,
        matrix,
        swap_axes: dense.swap_axes,
        colormap: dense.colormap.clone(),
        row_selection: dense.row_selection.clone(),
        col_selection: dense.col_selection.clone(),
        ..Default::default()
    }
}

async fn check_every_encoding(dense: HeatmapLayerParams, csr: CompressedMatrix, csc: CompressedMatrix, name: &str) {
    let (csr, csc) = (compressed(&dense, csr), compressed(&dense, csc));
    render_and_check_both_snapshots(params(LayerParams::HeatmapLayer(dense)), name).await;
    render_and_check_both_snapshots(params(LayerParams::HeatmapCsrLayer(csr)), name).await;
    render_and_check_both_snapshots(params(LayerParams::HeatmapCscLayer(csc)), name).await;
}

#[tokio::test]
async fn test_heatmap_compressed_layers_match_dense() {
    let dense = HeatmapLayerParams {
        layer_id: "heatmap".to_string(),
        num_rows: 3,
        num_cols: 4,
        data: NumericData::Float32(Arc::new(DENSE_3X4.to_vec())),
        model_matrix: Some(fit(3, 4, false)),
        colormap: colormap(None),
        ..Default::default()
    };
    let csr = CompressedMatrix {
        indptr: NumericData::from(vec![0u32, 2, 2, 4]),
        indices: NumericData::from(vec![1i32, 3, 0, 2]),
        data: NumericData::from(vec![5.0f32, 7.0, 3.0, 8.0]),
        minor_subset: None,
    };
    let csc = CompressedMatrix {
        indptr: NumericData::from(vec![0i64, 1, 2, 3, 4]),
        indices: NumericData::from(vec![2u16, 0, 2, 0]),
        data: NumericData::from(vec![3.0f32, 5.0, 8.0, 7.0]),
        minor_subset: None,
    };
    check_every_encoding(dense, csr, csc, "test_heatmap_compressed_layers_match_dense").await;
}

// Rows [2, 0] and columns [3, 1, 0] of `DENSE_3X4`, i.e. [[0, 0, 3], [7, 5, 0]].
#[tokio::test]
async fn test_heatmap_compressed_layers_match_dense_minor_subset_swap_axes_with_selection() {
    let dense = HeatmapLayerParams {
        layer_id: "heatmap".to_string(),
        num_rows: 2,
        num_cols: 3,
        data: NumericData::Float32(Arc::new(vec![0.0, 0.0, 3.0, 7.0, 5.0, 0.0])),
        swap_axes: true,
        model_matrix: Some(fit(2, 3, true)),
        colormap: colormap(Some(HeatmapAxisDomains {
            axis: MatrixAxis::Cols,
            min: NumericData::from(vec![0.0f32, 0.0, 0.0]),
            max: NumericData::from(vec![7.0f32, 5.0, 3.0]),
        })),
        row_selection: Some(NumericData::from(vec![1u8, 0])),
        ..Default::default()
    };
    let csr = CompressedMatrix {
        indptr: NumericData::from(vec![0u32, 1, 3]),
        indices: NumericData::from(vec![0u32, 1, 3]),
        data: NumericData::from(vec![3.0f32, 5.0, 7.0]),
        minor_subset: Some(NumericData::from(vec![3u32, 1, 0])),
    };
    let csc = CompressedMatrix {
        indptr: NumericData::from(vec![0u32, 1, 2, 3]),
        indices: NumericData::from(vec![0u32, 0, 2]),
        data: NumericData::from(vec![7.0f32, 5.0, 3.0]),
        minor_subset: Some(NumericData::from(vec![2u32, 0])),
    };
    check_every_encoding(dense, csr, csc, "test_heatmap_compressed_layers_match_dense_minor_subset_swap_axes_with_selection").await;
}
