use wasm_bindgen_test::*;
use pluot::render;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_render_triangle() {
    let width = 32;
    let height = 32;
    let plot_type = "triangle";
    let result = render(width, height, plot_type).await;
    
    let result_vec = result.to_vec();
    assert_eq!(result_vec.len(), (width * height * 4) as usize);

    let is_not_all_zero = result_vec.iter().any(|&x| x != 0);
    assert!(is_not_all_zero, "The rendered image should not be all black.");
}
