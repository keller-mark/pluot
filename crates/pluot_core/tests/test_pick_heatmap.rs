use std::sync::Arc;

use pluot_core::layers::heatmap_layer::{HeatmapLayer, HeatmapLayerParams, HeatmapPickingInfo};
use pluot_core::render_traits::PickableLayer;
use pluot_core::viewport::{DataCoord, ScreenCoord};
use pluot_core::{NumericData, NumericValue, ViewParams};

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
fn pick_returns_the_cell_under_the_cursor() {
    let view_params = ViewParams { width: 100, height: 100, ..Default::default() };
    let layer = HeatmapLayer::new(view_params, params_2x3(false));
    let result = layer.pick(ScreenCoord { x: 0.0, y: 0.0 }, Some(DataCoord::TwoD { x: 2.5, y: 1.5 })).unwrap();
    assert_eq!(result.into_info(), Some(HeatmapPickingInfo { row: 0, col: 2, value: NumericValue::Float32(2.0) }));

    let swapped = HeatmapLayer::new(ViewParams { width: 100, height: 100, ..Default::default() }, params_2x3(true));
    let result = swapped.pick(ScreenCoord { x: 0.0, y: 0.0 }, Some(DataCoord::TwoD { x: 1.5, y: 0.5 })).unwrap();
    let info: HeatmapPickingInfo = result.into_info().unwrap();
    assert_eq!((info.row, info.col), (1, 2));
}
