#![cfg(not(target_arch = "wasm32"))]

use pluot::{render, RenderParams};

#[tokio::test]
async fn test_render_triangle() {
    let width = 32;
    let height = 32;
    let params = RenderParams {
        width,
        height,
        zoom: Some(1.0),
        target_x: Some(0.0),
        target_y: Some(0.0),
        camera_view: None,
        plot_id: "my_plot".to_string(),
        plot_type: "triangle".to_string(),
        store_name: "my_store".to_string(),
    };
    let result_vec = render(params).await;

    assert_eq!(result_vec.len(), (width * height * 4) as usize);

    let is_not_all_zero = result_vec.iter().any(|&x| x != 0);
    assert!(is_not_all_zero, "The rendered image should not be all black.");
}