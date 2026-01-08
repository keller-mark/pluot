#![cfg(all(not(target_arch = "wasm32"), feature = "test_plain_rust"))]

// Run with
// cargo test --features test_plain_rust

use pluot::{render, RenderParams};

#[tokio::test]
async fn test_render_triangle() {
    let params = RenderParams::default();
    let result_vec = render(params).await;

    let NUM_EXTRA_BYTES = 1;

    assert_eq!(
        result_vec.len(),
        ((100 * 100 * 4) + NUM_EXTRA_BYTES) as usize
    );

    let is_not_all_zero = result_vec.iter().any(|&x| x != 0);
    assert!(
        is_not_all_zero,
        "The rendered image should not be all black."
    );
}
