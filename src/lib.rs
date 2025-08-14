mod utils;

use wasm_bindgen::prelude::*;
use wgpu::{TextureDescriptor, TextureUsages, TextureFormat, Extent3d};
use futures_intrusive::channel::shared::oneshot_channel;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static GLOBAL_MAP: OnceLock<Mutex<HashMap<String, Vec<i32>>>> = OnceLock::new();


#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);

}

#[wasm_bindgen]
pub fn register_data(name: &str, arr: js_sys::Int32Array) {
    // Globally register the data with the given name.
    
    // Initialize once
    GLOBAL_MAP.set(Mutex::new(HashMap::new())).unwrap();

    // Insert into the global map
    {
        let mut map = GLOBAL_MAP.get().unwrap().lock().unwrap();
        map.insert(name.to_string(), arr.to_vec());
    }

}



// This function should accept width and height as parameters,
// and return a Uint8Array containing the rendered image data.
#[wasm_bindgen]
pub async fn render(width: u32, height: u32) -> js_sys::Uint8Array {
    // The Instance is the context for all other wgpu objects.
    // This is the first thing you create when using wgpu.
    // Its primary use is to create Adapters and Surfaces.
    // Does not have to be kept alive.
    
    // The InstanceDescriptor has fields for which backends wgpu will choose during instantiation,
    // and which DX12 shader compiler wgpu will use.
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .expect("No suitable GPU adapters found on the system!");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .expect("Failed to create device");

    // Create a texture to render to.
    let texture_desc = TextureDescriptor {
        // Debug label of the texture. This will show up in graphics debuggers for easy identification.
        label: Some("Render Texture"),
        // Size of the texture. All components must be greater than zero.
        // For a regular 1D/2D texture, the unused sizes will be 1.
        // For 2DArray textures, Z is the number of 2D textures in that array.
        size: Extent3d { width, height, depth_or_array_layers: 1 },
        // Mip count of texture. For a texture with no extra mips, this must be 1.
        mip_level_count: 1,
        // Sample count of texture. If this is not 1, texture must have [BindingType::Texture::multisampled] set to true.
        sample_count: 1,
        // Dimensions of the texture.
        dimension: wgpu::TextureDimension::D2,
        // Format of the texture.
        format: TextureFormat::Rgba8UnormSrgb,
        // Allowed usages of the texture. If used in other ways, the operation will panic.
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        // Specifies what view formats will be allowed when calling Texture::create_view on this texture.
        // View formats of the same format as the texture are always allowed.
        // Note: currently, only the srgb-ness is allowed to change. (ex: Rgba8Unorm texture + Rgba8UnormSrgb view)
        view_formats: &[],
    };
    let texture = device.create_texture(&texture_desc);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    // Create a buffer to store the output (RGBA8)
    let bytes_per_pixel: u32 = 4;
    let unpadded_bytes_per_row = width * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT; // 256
    let padded_bytes_per_row = ((unpadded_bytes_per_row + align - 1) / align) * align;
    let output_buffer_size = (padded_bytes_per_row as u64) * (height as u64);

    let output_buffer_desc = wgpu::BufferDescriptor {
        label: Some("Output Buffer"),
        size: output_buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    };
    let output_buffer = device.create_buffer(&output_buffer_desc);

    // Begin render-specific things.
    // Get x and y data from the global map
    let (xs, ys) = {
        let map = GLOBAL_MAP.get().unwrap().lock().unwrap();
        let xs = map.get("x").expect("No 'x' data registered").into_iter()
            .map(|&v| v as f32).collect::<Vec<f32>>();
        let ys = map.get("y").expect("No 'y' data registered").into_iter()
            .map(|&v| v as f32).collect::<Vec<f32>>();
        (xs, ys)
    };
    let n = xs.len();
    assert_eq!(n, ys.len(), "x and y data must have the same length");

    // Pack positions into a contiguous vec2<f32> array for a storage buffer
    let mut positions_bytes: Vec<u8> = Vec::with_capacity(n * 2 * 4);
    let (mut x_min, mut x_max) = (f32::INFINITY, f32::NEG_INFINITY);
    let (mut y_min, mut y_max) = (f32::INFINITY, f32::NEG_INFINITY);
    for i in 0..n {
        let x = xs[i];
        let y = ys[i];
        x_min = x_min.min(x); x_max = x_max.max(x);
        y_min = y_min.min(y); y_max = y_max.max(y);
        positions_bytes.extend_from_slice(&x.to_ne_bytes());
        positions_bytes.extend_from_slice(&y.to_ne_bytes());
    }
    let positions_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Positions Storage Buffer"),
        size: positions_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&positions_buffer, 0, &positions_bytes);

    // Create uniforms matching the WGSL layout
    // struct Uniforms {
    //   x_min, x_max, y_min, y_max : f32,
    //   point_size_px: f32, _pad0: f32,
    //   viewport_size: vec2<f32>,
    //   color: vec4<f32>
    // }
    let point_size_px: f32 = 4.0;
    let _pad0: f32 = 0.0;
    let viewport_w = width as f32;
    let viewport_h = height as f32;
    let color = [1.0_f32, 1.0, 1.0, 1.0];

    let mut uniform_bytes: Vec<u8> = Vec::with_capacity(12 * 4);
    for f in [x_min, x_max, y_min, y_max, point_size_px, _pad0, viewport_w, viewport_h].iter() {
        uniform_bytes.extend_from_slice(&f.to_ne_bytes());
    }
    for c in color { uniform_bytes.extend_from_slice(&c.to_ne_bytes()); }

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Uniform Buffer"),
        size: uniform_bytes.len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

    // Create bind group layout and bind group for positions + uniforms
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Scatter BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Scatter BG"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: positions_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: uniform_buffer.as_entire_binding() },
        ],
    });

    let vs_src = r#"
        struct Uniforms {
            x_min: f32,
            x_max: f32,
            y_min: f32,
            y_max: f32,
            point_size_px: f32,   // diameter in pixels
            _pad0: f32,
            viewport_size: vec2<f32>, // (width, height) in pixels
            color: vec4<f32>,     // rgba color for points
        };

        struct VSOut {
            @builtin(position) position: vec4<f32>,
            @location(0) color: vec4<f32>,
        };

        @group(0) @binding(0)
        var<storage, read> positions: array<vec2<f32>>;

        @group(0) @binding(1)
        var<uniform> u: Uniforms;

        // 4 corners of a unit quad for triangle strip: (-1,-1), (1,-1), (-1,1), (1,1)
        const QUAD: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
            vec2<f32>(-1.0, -1.0),
            vec2<f32>( 1.0, -1.0),
            vec2<f32>(-1.0,  1.0),
            vec2<f32>( 1.0,  1.0)
        );

        // Map a data value v from [min,max] to NDC [-1,1]
        fn to_ndc(v: f32, minv: f32, maxv: f32) -> f32 {
            let t = (v - minv) / max(1e-12, (maxv - minv));
            return t * 2.0 - 1.0;
        }

        @vertex
        fn vs_main(
            @builtin(instance_index) instance: u32,
            @builtin(vertex_index) vid: u32
        ) -> VSOut {
            // Center of this point in data space
            let p = positions[instance];
            // Center in clip/NDC space (y increases up)
            let center_ndc = vec2<f32>(
                to_ndc(p.x, u.x_min, u.x_max),
                to_ndc(p.y, u.y_min, u.y_max)
            );

            // Convert desired pixel radius to NDC
            let radius_px = 0.5 * u.point_size_px;
            // pixels -> NDC: ndc_per_px = 2 / viewport
            let ndc_per_px = 2.0 / u.viewport_size;
            let radius_ndc = vec2<f32>(radius_px * ndc_per_px.x, radius_px * ndc_per_px.y);

            // Pick corner of quad and place around center
            let corner = QUAD[vid & 3u]; // vid % 4
            let offset_ndc = vec2<f32>(corner.x * radius_ndc.x, corner.y * radius_ndc.y);

            var out: VSOut;
            out.position = vec4<f32>(center_ndc + offset_ndc, 0.0, 1.0);
            out.color = u.color;
            return out;
        }

    "#;
    
    let fs_src = r#"
        struct FSOut {
            @location(0) color: vec4<f32>,
        };

        @fragment
        fn fs_main(@location(0) color_in: vec4<f32>) -> FSOut {
            var out: FSOut;
            out.color = color_in;
            return out;
        }
    "#;

    let vs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Vertex Shader"),
        source: wgpu::ShaderSource::Wgsl(vs_src.into()),
    });

    let fs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Fragment Shader"),
        source: wgpu::ShaderSource::Wgsl(fs_src.into()),
    });

    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vs_module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &fs_module,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: texture_desc.format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });
    // End render-specific things.

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Render Encoder"),
    });

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&render_pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..4, 0..(n as u32));

        // End the renderpass.
        drop(render_pass);
    }

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                // Must be 256-byte aligned on WebGPU
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        texture_desc.size,
    );

    let command_buffer = encoder.finish();
    queue.submit([command_buffer]);

    // Map and await completion without blocking the browser thread
    let buffer_slice = output_buffer.slice(..);
    let (sender, receiver) = oneshot_channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
        sender.send(res).ok();
    });
    let _ =device.poll(wgpu::PollType::Poll);
    receiver.receive().await.unwrap().unwrap();

    // Read and depad rows into a tightly packed RGBA buffer
    let data = buffer_slice.get_mapped_range();
    let mut pixels = vec![0u8; (unpadded_bytes_per_row * height) as usize];
    for y in 0..height {
        let src_start = (y as usize) * (padded_bytes_per_row as usize);
        let src_end = src_start + (unpadded_bytes_per_row as usize);
        let dst_start = (y as usize) * (unpadded_bytes_per_row as usize);
        let dst_end = dst_start + (unpadded_bytes_per_row as usize);
        pixels[dst_start..dst_end].copy_from_slice(&data[src_start..src_end]);
    }
    drop(data);
    output_buffer.unmap();

    // Return a Uint8Array of RGBA bytes
    js_sys::Uint8Array::from(pixels.as_slice())
}

#[wasm_bindgen]
pub fn greet() {
    alert("Hello, pluot!");
}
