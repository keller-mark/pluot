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
        "layer_type": "PointLayer",
        "layer_params": {
            "layer_id": "point_layer",
            "bounds": null,
            "data_unit_mode_x": "Pixels",
            "data_unit_mode_y": "Pixels",
            "point_shape_mode": "Circle",
            "point_radius_unit_mode_x": "Pixels",
            "point_radius_unit_mode_y": "Pixels",
            "point_radius": {"size_mode": "UniformSize", "size_params": 10.0},
            "stroke_width_unit_mode": "Pixels",
            "stroke_width": {"size_mode": "UniformSize", "size_params": 2.0},
            "fill_color": {"color_mode": "UniformRgb", "color_params": [255, 0, 0]},
            "fill_opacity": {"opacity_mode": "UniformOpacity", "opacity_params": 0.5},
            "stroke_color": {"color_mode": "UniformRgb", "color_params": [0, 0, 255]},
            "stroke_opacity": {"opacity_mode": "UniformOpacity", "opacity_params": 1.0},
            "position_x": {"dtype": "Float32", "values": [16.0]},
            "position_y": {"dtype": "Float32", "values": [16.0]},
        }
    });

    let layer_params: RawLayerParams = serde_json::from_value(raw_layer_params)
        .expect("Deserialize json-based PointLayerParams");

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
