use crate::wgpu;
use crate::wgpu::util::DeviceExt; // This import enables usage of device.create_buffer_init
use std::{num::NonZeroU64, str::FromStr};

use futures::FutureExt;
use futures_intrusive::channel::shared::oneshot_channel;

use crate::cache::use_memo_vec_f32;

// Reference: https://github.com/gfx-rs/wgpu/blob/trunk/examples/standalone/01_hello_compute/src/main.rs
pub async fn compute_example(device: wgpu::Device, queue: wgpu::Queue) -> f32 {
    let arguments: Vec<f32> = vec![99.0, 100.0, 101.0, 102.0, 103.0, 104.0];

    // Create a shader module from our shader code. This will parse and validate the shader.
    //
    // `include_wgsl` is a macro provided by wgpu like `include_str` which constructs a ShaderModuleDescriptor.
    // If you want to load shaders differently, you can construct the ShaderModuleDescriptor manually.
    let shader_src = r#"
        // Input to the shader. The length of the array is determined by what buffer is bound.
        //
        // Out of bounds accesses
        @group(0) @binding(0)
        var<storage, read> input: array<f32>;
        // Output of the shader.
        @group(0) @binding(1)
        var<storage, read_write> output: array<f32>;

        // Ideal workgroup size depends on the hardware, the workload, and other factors. However, it should
        // _generally_ be a multiple of 64. Common sizes are 64x1x1, 256x1x1; or 8x8x1, 16x16x1 for 2D workloads.
        @compute @workgroup_size(64)
        fn doubleMe(@builtin(global_invocation_id) global_id: vec3<u32>) {
            // While compute invocations are 3d, we're only using one dimension.
            let index = global_id.x;

            // Because we're using a workgroup size of 64, if the input size isn't a multiple of 64,
            // we will have some "extra" invocations. This is fine, but we should tell them to stop
            // to avoid out-of-bounds accesses.
            let array_length = arrayLength(&input);
            if (global_id.x >= array_length) {
                return;
            }

            // Do the multiply by two and write to the output.
            output[global_id.x] = input[global_id.x] * 2.0;
        }
    "#;

    let module = device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Compute shader example"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });


    // Create a buffer with the data we want to process on the GPU.
    //
    // `create_buffer_init` is a utility provided by `wgpu::util::DeviceExt` which simplifies creating
    // a buffer with some initial data.
    //
    // We use the `bytemuck` crate to cast the slice of f32 to a &[u8] to be uploaded to the GPU.
    let input_data_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&arguments),
        usage: wgpu::BufferUsages::STORAGE,
    });

    // Now we create a buffer to store the output data.
    let output_data_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: input_data_buffer.size(),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    // Finally we create a buffer which can be read by the CPU. This buffer is how we will read
    // the data. We need to use a separate buffer because we need to have a usage of `MAP_READ`,
    // and that usage can only be used with `COPY_DST`.
    let download_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: input_data_buffer.size(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    // A bind group layout describes the types of resources that a bind group can contain. Think
    // of this like a C-style header declaration, ensuring both the pipeline and bind group agree
    // on the types of resources.
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            // Input buffer
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    // This is the size of a single element in the buffer.
                    min_binding_size: Some(NonZeroU64::new(4).unwrap()),
                    has_dynamic_offset: false,
                },
                count: None,
            },
            // Output buffer
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    // This is the size of a single element in the buffer.
                    min_binding_size: Some(NonZeroU64::new(4).unwrap()),
                    has_dynamic_offset: false,
                },
                count: None,
            },
        ],
    });

    // The bind group contains the actual resources to bind to the pipeline.
    //
    // Even when the buffers are individually dropped, wgpu will keep the bind group and buffers
    // alive until the bind group itself is dropped.
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input_data_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output_data_buffer.as_entire_binding(),
            },
        ],
    });

    // The pipeline layout describes the bind groups that a pipeline expects
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&bind_group_layout],
        immediate_size: 0,
    });

    // The pipeline is the ready-to-go program state for the GPU. It contains the shader modules,
    // the interfaces (bind group layouts) and the shader entry point.
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("doubleMe"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    // The command encoder allows us to record commands that we will later submit to the GPU.
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    // A compute pass is a single series of compute operations. While we are recording a compute
    // pass, we cannot record to the encoder.
    let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: None,
        timestamp_writes: None,
    });

    // Set the pipeline that we want to use
    compute_pass.set_pipeline(&pipeline);
    // Set the bind group that we want to use
    compute_pass.set_bind_group(0, &bind_group, &[]);

    // Now we dispatch a series of workgroups. Each workgroup is a 3D grid of individual programs.
    //
    // We defined the workgroup size in the shader as 64x1x1. So in order to process all of our
    // inputs, we ceiling divide the number of inputs by 64. If the user passes 32 inputs, we will
    // dispatch 1 workgroups. If the user passes 65 inputs, we will dispatch 2 workgroups, etc.
    let workgroup_count = arguments.len().div_ceil(64);
    compute_pass.dispatch_workgroups(workgroup_count as u32, 1, 1);

    // Now we drop the compute pass, giving us access to the encoder again.
    drop(compute_pass);

    // We add a copy operation to the encoder. This will copy the data from the output buffer on the
    // GPU to the download buffer on the CPU.
    encoder.copy_buffer_to_buffer(
        &output_data_buffer,
        0,
        &download_buffer,
        0,
        output_data_buffer.size(),
    );

    // We finish the encoder, giving us a fully recorded command buffer.
    let command_buffer = encoder.finish();

    // At this point nothing has actually been executed on the gpu. We have recorded a series of
    // commands that we want to execute, but they haven't been sent to the gpu yet.
    //
    // Submitting to the queue sends the command buffer to the gpu. The gpu will then execute the
    // commands in the command buffer in order.
    queue.submit([command_buffer]);

    // We now map the download buffer so we can read it. Mapping tells wgpu that we want to read/write
    // to the buffer directly by the CPU and it should not permit any more GPU operations on the buffer.
    //
    // Mapping requires that the GPU be finished using the buffer before it resolves, so mapping has a callback
    // to tell you when the mapping is complete.
    let buffer_slice = download_buffer.slice(..);


    #[cfg(target_arch = "wasm32")]
    {
        let (sender, receiver) = oneshot_channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
            if res.is_err() {
                panic!("Failed to map texture for reading");
            }
            sender.send(res).ok();
        });

        let _ = device.poll(wgpu::PollType::Poll);
        receiver.receive().await.unwrap().unwrap();
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            if result.is_err() {
                panic!("Failed to map texture for reading");
            }
            //let _ = tx.send(result);
        });

        let _ = device.poll(wgpu::PollType::wait_indefinitely());

    }

    // We can now read the data from the buffer.
    let data = buffer_slice.get_mapped_range();
    // Convert the data back to a slice of f32.
    let result: &[f32] = bytemuck::cast_slice(&data);
    return result[0] as f32;
}

pub async fn compute_example_with_memo(device: wgpu::Device, queue: wgpu::Queue) -> f32 {
    let x_f32_future_deps = vec!["compute_example".to_string()];
    let x_f32_future = use_memo_vec_f32(async || {
        // Simulate some async work that produces a Vec<f32>.
        let mut x_f32_inner: Vec<f32> = vec![1.0, 2.0, 3.0];
        let compute_result = compute_example(device, queue).await; // Call the compute example to ensure it runs and we can see the memoization in action.
        x_f32_inner[0] = compute_result; // Update the first element with the result from the compute example.
        x_f32_inner
    }, &x_f32_future_deps, true);

    return x_f32_future.await[0];
}
