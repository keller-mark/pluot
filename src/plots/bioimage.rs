use core::num;
use std::borrow::Cow;

use vello::wgpu::{self, include_wgsl};
use crate::{utils::RenderContext};
use crate::{log};

use ome_zarr_metadata::v0_5::{RelaxedOmeFields};

pub async fn render_bioimage(context: &RenderContext<'_>, encoder: &mut wgpu::CommandEncoder) {
    // Get x and y data from the Zarr store.
    let store = context.store;

    // Get the OME-NGFF metadata for the image.
    // See https://github.com/zarrs/ome_zarr_metadata/blob/main/src/v0_5.rs
    let group = zarrs::group::Group::async_open(store.clone(), "/")
        .await.expect("Open root group");

    log(&format!(
        "The group metadata is:\n{}\n",
        group.metadata().to_string_pretty()
    ));

    let attrs = group.attributes();
    let ome_fields: RelaxedOmeFields = serde_json::from_value(
       attrs.get("ome").expect("OME").clone()
    ).expect("OME attributes");

    log(&format!(
        "The OME fields are:\n{:#?}\n",
        ome_fields
    ));
    
    let multiscales = ome_fields.multiscales
        .expect("Expected the OME-NGFF image to contain a multiscale image. Other OME-NGFF types are not yet supported.");

    // The ome_zarr_metadata crate does not support the "omero" metadata,
    // so we must parse it ourselves.
    let omero = attrs.get("omero");

    let first_multiscale = &multiscales[0];

    // Print the shape of each resolution level.
    for (i, dataset) in first_multiscale.datasets.iter().enumerate() {
        // TODO: support Blosc-compressed arrays, and remove the _nc no-compression suffix here.
        let dataset_array = zarrs::array::Array::async_open(store.clone(), &format!("/{}_nc", dataset.path))
            .await.expect("Open dataset array");

        log(&format!("Resolution level {}: {:?}", dataset.path, dataset_array.shape()));
    }

    // For now, load the lowest resolution level and render the pixels.
    let lowres_dataset = &first_multiscale.datasets.last().expect("At least one dataset");
    let lowres_array = zarrs::array::Array::async_open(store.clone(), &format!("/{}_nc", lowres_dataset.path))
        .await.expect("Open lowres dataset array");

    // Do not assume the dimension order, or that there are Z/C/T dims.
    let z_index = 99;
    let c_index = 0;
    let t_index = 0;

    let x_dim_i = first_multiscale.axes.iter().position(|a| a.name == "x").expect("x axis");
    let y_dim_i = first_multiscale.axes.iter().position(|a| a.name == "y").expect("y axis");
    let z_dim_i = first_multiscale.axes.iter().position(|a| a.name == "z");
    let c_dim_i = first_multiscale.axes.iter().position(|a| a.name == "c");
    let t_dim_i = first_multiscale.axes.iter().position(|a| a.name == "t");

    let img_w = lowres_array.shape()[x_dim_i];
    let img_h = lowres_array.shape()[y_dim_i];
    log(&format!("Image dimensions: {} x {}", img_w, img_h));

    let num_channels = 2;

    // Read the pixel data using a slice that selects the first z, c, and t indices.
    
    // This array is CZYX.
    // TODO: do not assume 4D and dim order.
    let ch0_arr_slice = zarrs::array_subset::ArraySubset::new_with_start_shape(
        vec![0, z_index, 0, 0], // start
        vec![1, 1, img_h as u64, img_w as u64], // shape
    ).expect("Compatible dimensionality");

    // This array is CZYX.
    // TODO: do not assume 4D and dim order.
    let ch1_arr_slice = zarrs::array_subset::ArraySubset::new_with_start_shape(
        vec![1, z_index, 0, 0], // start
        vec![1, 1, img_h as u64, img_w as u64], // shape
    ).expect("Compatible dimensionality");

    // TODO: support other dtypes.
    let ch0_arr = lowres_array.async_retrieve_array_subset_ndarray::<u16>(&ch0_arr_slice)
        .await.expect("Read pixel data");
    let ch1_arr = lowres_array.async_retrieve_array_subset_ndarray::<u16>(&ch1_arr_slice)
        .await.expect("Read pixel data");

    log(&format!("Read array 0 with shape {:?}", ch0_arr.shape()));
    log(&format!("Read array 1 with shape {:?}", ch1_arr.shape()));

    // Concatenate the channel data into a single vector.
    // TODO: is this the most efficient way / use the minimal number of copies?
    let mut combined_pixel_data = ch0_arr.as_slice().expect("Contiguous array for ch0").to_vec();
    combined_pixel_data.extend_from_slice(ch1_arr.as_slice().expect("Contiguous array for ch1"));


    // Store the ndarray::ArrayD in a WGPU texture.
    // Create a texture to store the image data (R16Uint).

    // TODO: does this need to be padded to 256 bytes per row?
    let bytes_per_pixel: u32 = 2; // R16Uint has 2 bytes per pixel.
    let unpadded_bytes_per_row = img_w as u32 * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT; // 256
    let padded_bytes_per_row = ((unpadded_bytes_per_row + align - 1) / align) * align;

    let texture_size = wgpu::Extent3d {
        width: img_w as u32,
        height: img_h as u32,
        depth_or_array_layers: num_channels as u32,
    };
    let image_texture = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Image Texture"),
        size: texture_size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        // R16Uint is a 16-bit unsigned integer format.
        format: wgpu::TextureFormat::R16Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let image_view = image_texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("Image Texture View"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    
    // Upload the pixel data to the texture.
    context.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &image_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        // The pixel data as a byte slice.
        bytemuck::cast_slice(&combined_pixel_data),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(unpadded_bytes_per_row),
            rows_per_image: Some(img_h as u32),
        },
        texture_size,
    );

    // Create uniforms matching the WGSL layout
    // struct Uniforms {
    //   camera_view: mat4x4<f32>,
    //   point_size_px: f32,
    //   _pad0: f32,
    //   viewport_size: vec2<f32>,
    //   color: vec4<f32>
    // }

    // TODO: use the camera_view and derived values up above, to determine which multiscale to load, etc.

    // Note: WebGPU's shading language (WGSL) treats matrices as column-major.
    let camera_view = context.params.camera_view.unwrap_or([
        // Column 0
        1.0, 0.0, 0.0, 0.0,
        // Column 1
        0.0, 1.0, 0.0, 0.0,
        // Column 2
        0.0, 0.0, 1.0, 0.0,
        // Column 3
        0.0, 0.0, 0.0, 1.0,
    ]);

    let zoom = camera_view[0]; // Assuming uniform scaling in x/y, take the first element (x scaling).
    let translate_x = camera_view[12];
    let translate_y = camera_view[13];
    
    // Convert zoom level to scale factor
    // scale_factor of 0 means zoom = 1.0 (no zoom)
    // scale_factor of 1 means zoom = 0.5 (zoomed out to half)
    // scale_factor of 2 means zoom = 0.25 (zoomed out to a quarter)
    // scale_factor of 3 means zoom = 0.125 (zoomed out to an eighth)

    // scale_factor of -1 means zoom = 2.0 (zoomed in to double)
    // scale_factor of -2 means zoom = 4.0 (zoomed in to quadruple)
    // scale_factor of -3 means zoom = 8.0 (zoomed in to octuple)
    let scale_factor = (1.0/zoom).log2();

    // X translation interpretation:
    // A translate_x value of 1.0 means a point at x=-1.0 (left edge of viewport/screen-quad) is now at the center of the viewport.
    // A translate_x value of 2.0 means a point at x=-1.0 is now at the right edge of the viewport.
    // A translate_x value of -1.0 means a point at x=1.0 (right edge of viewport/screen-quad) is now at the center of the viewport.
    
    // Zoom interpretation:
    // A zoom value of 0.5 means that points are scaled down by half, so a point at x=-1.0 is now at x=-0.5, and a point at x=1.0 is now at x=0.5.
    // A zoom value of 0.25 means that points are scaled down by a quarter, so a point at x=-1.0 is now at x=-0.25, and a point at x=1.0 is now at x=0.25.
    
    // Zoom and translation combined interpretation:
    // A translate_x value of 0.5 when zoom = 0.5 means a point at x=-1.0 is now at the center of the viewport, and a point at x=1.0 is now at the right of the viewport.
    // When zoom = 0.5 AND translate_x = 0.5 AND translate_y = 0.5, all four screen-quad [-1 to 1] corner points are in the top right quadrant of the viewport.
    // When zoom = 0.5 AND translate_x = -0.5 AND translate_y = -0.5, all four screen-quad [-1 to 1] corner points are in the bottom left quadrant of the viewport.
    
    let x_range = 2.0 / zoom; // The range of x values visible in the viewport
    let y_range = 2.0 / zoom; // The range of y values visible in the viewport

    let min_x = (-translate_x - 1.0) / zoom; // translation of (x=-1)
    let max_x = (-translate_x + 1.0) / zoom; // translation of (x=1)
    let min_y = (-translate_y - 1.0) / zoom; // translation of (y=-1)
    let max_y = (-translate_y + 1.0) / zoom; // translation of (y=1)


    // TODO: simplify the uniforms.
    let point_size_px: f32 = context.params.point_radius.unwrap_or(5.0);
    let _pad0: f32 = 0.0;
    let viewport_w = context.params.width as f32;
    let viewport_h = context.params.height as f32;
    let color = [1.0_f32, 0.0, 0.0, 1.0];

    let mut uniform_bytes: Vec<u8> = Vec::with_capacity((16+8) * 4);

    // Log the computed values for debugging.
    // log(&format!("Zoom: {zoom}, x_min: {x_min}, x_max: {x_max}, y_min: {y_min}, y_max: {y_max}"));
    
    for f in camera_view.iter() {
        uniform_bytes.extend_from_slice(&f.to_ne_bytes());
    }
    for f in [point_size_px, _pad0, viewport_w, viewport_h].iter() {
        uniform_bytes.extend_from_slice(&f.to_ne_bytes());
    }
    for c in color { uniform_bytes.extend_from_slice(&c.to_ne_bytes()); }

    let uniform_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Uniform Buffer"),
        size: uniform_bytes.len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    context.queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

    // Create bind group layout and bind group for positions + uniforms
    let bind_group_layout = context.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Bioimage BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                // The uniforms buffer.
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                // The image pixel texture.
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    let bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Bioimage BG"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&image_view) },
        ],
    });

    let vs_module = context.device.create_shader_module(include_wgsl!("shaders/bioimage.vs.wgsl"));
    let fs_module = context.device.create_shader_module(include_wgsl!("shaders/bioimage.fs.wgsl"));

    let render_pipeline_layout = context.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    // TODO: Extract the shared render pipeline and render pass logic. There is a lot of duplication here.
    let render_pipeline = context.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                format: context.texture_desc.format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &context.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Set a white background for the scatterplot.
                    // TODO: make this configurable.
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&render_pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..4, 0..1);

        // End the renderpass.
        drop(render_pass);
    }


}