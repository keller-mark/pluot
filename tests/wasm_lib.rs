//! Test suite for the Web and headless browsers.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;
use wasm_bindgen::prelude::*;
use pluot::{render_wasm, utils::RenderParams};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_render_triangle() {
    let width = 32;
    let height = 32;
    let params: JsValue = serde_wasm_bindgen::to_value(&RenderParams {
        width,
        height,
        zoom: Some(1.0),
        target_x: Some(0.0),
        target_y: Some(0.0),
        camera_view: None,
        plot_id: "my_plot".to_string(),
        plot_type: "triangle".to_string(),
        store_name: "my_store".to_string(),
    })
        .expect("Invalid parameters");
    let result = render_wasm(params).await;

    let result_vec = result.to_vec();
    assert_eq!(result_vec.len(), (width * height * 4) as usize);

    let is_not_all_zero = result_vec.iter().any(|&x| x != 0);
    assert!(is_not_all_zero, "The rendered image should not be all black.");
}
