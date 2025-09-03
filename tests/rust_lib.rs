#![cfg(all(not(target_arch = "wasm32"), not(feature = "python")))]

use pluot::{render, RenderParams};

#[tokio::test]
async fn test_render_triangle() {
    let width = 32;
    let height = 32;
    let params = RenderParams::default();
    let result_vec = render(params).await;

    assert_eq!(result_vec.len(), (width * height * 4) as usize);

    let is_not_all_zero = result_vec.iter().any(|&x| x != 0);
    assert!(is_not_all_zero, "The rendered image should not be all black.");
}