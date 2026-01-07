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

#[tokio::test]
async fn test_model_with_texture_and_draw() {
    let (device, queue) = init_gpu_context().await;

    let vs = "
        @vertex
        fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> @builtin(position) vec4<f32> {
            let x = f32(i32(in_vertex_index) - 1);
            let y = f32(i32(in_vertex_index & 1u) * 2 - 1);
            return vec4<f32>(x, y, 0.0, 1.0);
        }
    ".to_string();

    let fs = "
        @group(0) @binding(0) var my_texture: texture_2d<f32>;
        @group(0) @binding(1) var my_sampler: sampler;

        @fragment
        fn fs_main() -> @location(0) vec4<f32> {
            return textureSample(my_texture, my_sampler, vec2<f32>(0.5, 0.5));
        }
    ".to_string();

    // Create uniform descriptors for texture and sampler
    let uniforms = vec![
        pluot::deckish::model::UniformDescriptor {
            shader_stage: wgpu::ShaderStages::FRAGMENT,
            binding_type: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
        },
        pluot::deckish::model::UniformDescriptor {
            shader_stage: wgpu::ShaderStages::FRAGMENT,
            binding_type: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        },
    ];

    let options = ModelOptions {
        vs,
        fs,
        uniforms,
        ..Default::default()
    };

    let mut model = Model::new(device.clone(), options);

    // Verify initial state
    assert!(model.bind_group.is_none(), "Bind group should be None before setting uniforms");
    assert_eq!(model.options.uniforms.len(), 2, "Should have exactly 2 uniform descriptors");

    // Create a simple 1x1 texture
    let texture_size = wgpu::Extent3d {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Test Texture"),
        size: texture_size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    // Create a sampler
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Test Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });

    // Set the texture and sampler uniforms
    model.set_uniform_texture(0, texture_view);
    model.set_uniform_sampler(1, sampler);

    // Verify that the bind group was created after setting all uniforms
    assert!(model.bind_group.is_some(), "Bind group should be created after setting all uniforms");

    // Create a render target texture for drawing
    let render_target_size = wgpu::Extent3d {
        width: 256,
        height: 256,
        depth_or_array_layers: 1,
    };

    let render_target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Render Target"),
        size: render_target_size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    let render_target_view = render_target.create_view(&wgpu::TextureViewDescriptor::default());

    // Create a command encoder and render pass
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Test Encoder"),
    });

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Test Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &render_target_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        // Call the draw method
        model.draw(&mut render_pass);
    }

    // Submit the commands
    queue.submit(std::iter::once(encoder.finish()));

    // The test passes if draw() completes without panicking
    // In a real scenario, you would verify the rendered output, but for a unit test
    // verifying that the draw call succeeds is sufficient
}
