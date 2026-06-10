//! Test suite for the Web and headless browsers.

// We only run this test on WASM targets AND when the "lacks_gpu" feature is not enabled (e.g., CI).
#![cfg(all(target_arch = "wasm32", not(feature="lacks_gpu")))]

use pluot::{
    render_wasm,
    RawRenderParams, RawLayerParams, RawPlotParams, RawLayeredPlotRenderParams,
};
use std::sync::Arc;
use serde_json::json;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_render_triangle() {
    let width = 32;
    let height = 32;

    let raw_layer_params = json!({
        "layer_type": "RectLayer",
        "layer_params": {
            "layer_id": "rect_layer",
            "bounds": null,
            "data_unit_mode_x": "Pixels",
            "data_unit_mode_y": "Pixels",
            "stroke_width_unit_mode": "Pixels",
            "stroke_width": 2.0,
            "fill_color": null,
            "fill_color_mode": "Categorical",
            "position_x0": [2.0],
            "position_y0": [4.0],
            "position_x1": [16.0],
            "position_y1": [8.0],
            "labels_vec": [4],
        }
    });

    let layer_params: RawLayerParams = serde_json::from_value(raw_layer_params)
        .expect("Deserialize json-based RectLayerParams");

    let params: JsValue = serde_wasm_bindgen::to_value(&RawRenderParams {
        width,
        height,
        camera_view: None,
        plot_id: "my_plot".to_string(),
        store_name: "my_store".to_string(),
        plot_params: RawPlotParams::LayeredPlot(RawLayeredPlotRenderParams {
            layers: vec![
                layer_params,
            ],
        }),
        ..Default::default()
    })
    .expect("Invalid parameters");
    let result = render_wasm(params).await;

    let result_vec = result.to_vec();
    let NUM_EXTRA_BYTES = 1;
    assert_eq!(
        result_vec.len(),
        ((width * height * 4) + NUM_EXTRA_BYTES) as usize
    );

    let is_not_all_zero = result_vec.iter().any(|&x| x != 0);
    assert!(
        is_not_all_zero,
        "The rendered image should not be all black."
    );
}
