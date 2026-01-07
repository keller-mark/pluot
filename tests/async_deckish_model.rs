#![cfg(all(not(target_arch = "wasm32"), feature = "test_plain_rust"))]

// Run with
// cargo test --features test_plain_rust

use pluot::deckish::model::{Model, ModelOptions};
use pluot::wgpu;
use pluot::cache::init_gpu_context;

#[tokio::test]
async fn test_model_creation() {
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

#[tokio::test]
async fn test_model_with_uniform_buffer() {
    let (device, _queue) = init_gpu_context().await;

    let vs = "
        struct Uniforms {
            mvp: mat4x4<f32>,
        }
        @group(0) @binding(0) var<uniform> uniforms: Uniforms;

        @vertex
        fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> @builtin(position) vec4<f32> {
            let pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
            return uniforms.mvp * pos;
        }
    ".to_string();

    let fs = "
        @fragment
        fn fs_main() -> @location(0) vec4<f32> {
            return vec4<f32>(1.0, 0.0, 0.0, 1.0);
        }
    ".to_string();

    // Create a uniform descriptor
    let uniforms = vec![
        pluot::deckish::model::UniformDescriptor {
            shader_stage: wgpu::ShaderStages::VERTEX,
            binding_type: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
        }
    ];

    let options = ModelOptions {
        vs,
        fs,
        uniforms,
        ..Default::default()
    };

    let mut model = Model::new(device.clone(), options);

    // Verify initial state before setting uniform buffer
    assert!(model.bind_group.is_none(), "Bind group should be None before setting uniform buffer");
    assert_eq!(model.vertex_buffer_count, 0, "Vertex buffer count should be 0");
    assert_eq!(model.options.uniforms.len(), 1, "Should have exactly 1 uniform descriptor");
    assert_eq!(model.options.primitive_topology, wgpu::PrimitiveTopology::TriangleList, "Default primitive topology should be TriangleList");
    assert_eq!(model.options.texture_format, wgpu::TextureFormat::Bgra8Unorm, "Default texture format should be Bgra8Unorm");

    // Verify the uniform descriptor properties
    assert_eq!(model.options.uniforms[0].shader_stage, wgpu::ShaderStages::VERTEX, "Uniform should be for vertex stage");
    if let wgpu::BindingType::Buffer { ty, has_dynamic_offset, .. } = model.options.uniforms[0].binding_type {
        assert_eq!(ty, wgpu::BufferBindingType::Uniform, "Binding type should be Uniform");
        assert!(!has_dynamic_offset, "Should not have dynamic offset");
    } else {
        panic!("Binding type should be Buffer");
    }

    // Create a uniform buffer
    let buffer_size = 64; // Size for a mat4x4<f32>
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Uniform Buffer"),
        size: buffer_size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // Verify buffer properties
    assert_eq!(uniform_buffer.size(), buffer_size, "Buffer size should match requested size");
    assert_eq!(uniform_buffer.usage(), wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, "Buffer usage should match");

    // Set the uniform buffer
    model.set_uniform_buffer(0, uniform_buffer, 0, buffer_size);

    // Verify that the bind group was created after setting the uniform buffer
    assert!(model.bind_group.is_some(), "Bind group should be created after setting uniform buffer");

    // Verify that the model state remains consistent after setting uniform buffer
    assert_eq!(model.vertex_buffer_count, 0, "Vertex buffer count should remain 0");
    assert_eq!(model.options.uniforms.len(), 1, "Uniform count should remain 1");
}
