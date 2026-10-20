use std::sync::Arc;

use glam::Vec2;
use pluot_core::layers::heatmap_layer::{rows_per_texture, HeatmapColormap, HeatmapLayerParams};
use pluot_core::numeric_data::NumericData;

fn params_2x3(swap_axes: bool) -> HeatmapLayerParams {
    HeatmapLayerParams {
        num_rows: 2,
        num_cols: 3,
        data: NumericData::Float32(Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0])),
        swap_axes,
        ..Default::default()
    }
}

#[test]
fn row_blocks_stack_top_to_bottom() {
    let params = HeatmapLayerParams { num_rows: 5, ..params_2x3(false) };
    assert_eq!(params.common().row_block_quad(0, 2), (Vec2::new(0.0, 3.0), Vec2::new(3.0, 2.0)));
    assert_eq!(params.common().row_block_quad(2, 3), (Vec2::new(0.0, 0.0), Vec2::new(3.0, 3.0)));
}

#[test]
fn swapped_row_blocks_run_left_to_right() {
    let params = HeatmapLayerParams { num_rows: 5, ..params_2x3(true) };
    assert_eq!(params.common().row_block_quad(2, 3), (Vec2::new(2.0, 0.0), Vec2::new(3.0, 3.0)));
}

#[test]
fn rows_per_texture_keeps_rows_whole() {
    assert_eq!(rows_per_texture(3, 10), 3);
    assert_eq!(rows_per_texture(20, 10), 1);
}

#[test]
fn categorical_cpu_color_wraps_codes() {
    let colormap = HeatmapColormap::CategoricalCustom(vec![(255, 0, 0), (0, 255, 0)]);
    assert_eq!(colormap.cpu_color(1.0, 0, 0), [0, 255, 0]);
    assert_eq!(colormap.cpu_color(-1.0, 0, 0), [0, 255, 0]);
    assert_eq!(colormap.cpu_color(2.0, 0, 0), [255, 0, 0]);
}
