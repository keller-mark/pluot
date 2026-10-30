#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{AspectRatioMode, LayerParams, PrecomputedViolinMatrixLayerParams, RenderParams};

fn labels(names: &[&str]) -> Arc<Vec<String>> {
    Arc::new(names.iter().map(|name| name.to_string()).collect())
}

/// Three genes by two groups, with one cell per (group, gene) in group-major order.
/// The last cell has no values, so it draws no violin.
fn violin_matrix(swap_axes: bool) -> LayerParams {
    LayerParams::PrecomputedViolinMatrixLayer(PrecomputedViolinMatrixLayerParams {
        layer_id: "violin_matrix".to_string(),
        swap_axes: Some(swap_axes),
        var_names: labels(&["gene_a", "gene_b", "gene_c"]),
        groups: labels(&["group_1", "group_2"]),
        bin_starts: Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0]),
        bin_counts: Arc::new(vec![
            vec![8, 4, 2, 1, 0],
            vec![0, 2, 6, 2, 0],
            vec![1, 1, 1, 1, 1],
            vec![0, 0, 1, 4, 8],
            vec![3, 6, 3, 0, 0],
            vec![0, 0, 0, 0, 0],
        ]),
        fill_values: Some(Arc::new(vec![Some(0.5), Some(2.0), Some(2.0), Some(3.5), Some(1.0), None])),
        fill_value_title: Some("Median".to_string()),
        var_axis_title: Some("Genes".to_string()),
        group_axis_title: Some("Groups".to_string()),
        ..Default::default()
    })
}

fn params(layers: Vec<LayerParams>) -> RenderParams {
    RenderParams {
        width: 400,
        height: 300,
        margin_left: Some(80.0),
        margin_bottom: Some(60.0),
        margin_top: Some(10.0),
        margin_right: Some(120.0),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        layers,
        ..Default::default()
    }
}

#[tokio::test]
async fn test_precomputed_violin_matrix_layer() {
    render_and_check_both_snapshots(params(vec![violin_matrix(false)]), "test_precomputed_violin_matrix_layer").await;
}

#[tokio::test]
async fn test_precomputed_violin_matrix_layer_swap_axes() {
    render_and_check_both_snapshots(params(vec![violin_matrix(true)]), "test_precomputed_violin_matrix_layer_swap_axes").await;
}
