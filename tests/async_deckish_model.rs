#![cfg(all(not(target_arch = "wasm32"), feature = "test_plain_rust"))]

use pluot::deckish::model::{Model, ModelOptions};
use pluot::wgpu;
use pluot::cache::init_gpu_context;

#[tokio::test]
async fn test_render_triangle() {
    let (device, _queue) = init_gpu_context().await;

    let vs = "
        @vertex
        fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> @builtin(position) vec4<f32> {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
    ".to_string();

    let fs = "
        @fragment
        fn fs_main() -> @location(0) vec4<f32> {
            return vec4<f32>(1.0, 0.0, 0.0, 1.0);
        }
    ".to_string();

    let options = ModelOptions {
        vs,
        fs,
        ..Default::default()
    };

    let model = Model::new(device, options);

    // Verify that the model was initialized correctly
    assert_eq!(model.vertex_buffer_count, 0);
    assert!(model.bind_group.is_none());
}
