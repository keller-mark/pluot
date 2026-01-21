use std::borrow::Cow;
use std::future::Future;

use futures_time::future::FutureExt;
use futures_time::time::Duration;

use crate::log;
use crate::maybe_timeout;
use crate::params::{PlotParams, RenderContext, RenderResult};
use crate::wgpu;

use ome_zarr_metadata::v0_5::RelaxedOmeFields;

use encase::{ArrayLength, ShaderType, StorageBuffer, UniformBuffer};
use glam::{Mat4, Vec2, Vec3, Vec4};

#[derive(ShaderType, Debug)]
struct ChannelUniforms {
    channel_window: Vec2,
    channel_colors: Vec3,
}

#[derive(ShaderType, Debug)]
struct BioimageUniforms {
    camera_view: Mat4,
    viewport_size: Vec2,
    num_channels: ArrayLength,
    // Note: WGSL only allows one runtime-sized array in a struct,
    // and it must be the last field.
    #[shader(size(runtime))]
    channels: Vec<ChannelUniforms>,
}

pub async fn render_bioimage(
    context: &RenderContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
) -> RenderResult {
    // Get x and y data from the Zarr store.
    let store = context.store;

    let PlotParams::Bioimage(bioimage_params) = &context.params.plot_params else {
        panic!("Expected bioimage params");
    };

    // Get the OME-NGFF metadata for the image.
    // See https://github.com/zarrs/ome_zarr_metadata/blob/main/src/v0_5.rs
    let group = zarrs::group::Group::async_open(store.clone(), "/")
        .await
        .expect("Open root group");

    log(&format!(
        "The group metadata is:\n{}\n",
        group.metadata().to_string_pretty()
    ));

    let attrs = group.attributes();
    let ome_fields: RelaxedOmeFields =
        serde_json::from_value(attrs.get("ome").expect("OME").clone()).expect("OME attributes");

    /*log(&format!(
        "The OME fields are:\n{:#?}\n",
        ome_fields
    ));*/

    let multiscales = ome_fields.multiscales
        .expect("Expected the OME-NGFF image to contain a multiscale image. Other OME-NGFF types are not yet supported.");

    // The ome_zarr_metadata crate does not support the "omero" metadata,
    // so we must parse it ourselves.
    let omero = attrs.get("omero");

    let first_multiscale = &multiscales[0];

    // Print the shape of each resolution level.
    for (i, dataset) in first_multiscale.datasets.iter().enumerate() {
        // TODO: support Blosc-compressed arrays, and remove the _nc no-compression suffix here.
        let dataset_array =
            zarrs::array::Array::async_open(store.clone(), &format!("/{}_nc", dataset.path))
                .await
                .expect("Open dataset array");

        log(&format!(
            "Resolution level {}: {:?}",
            dataset.path,
            dataset_array.shape()
        ));
    }

    // Do not assume the dimension order, or that there are Z/C/T dims.
    let z_index = bioimage_params.target_z.unwrap_or(99);
    let c_index = 0;
    let t_index = 0;

    let x_dim_i = first_multiscale
        .axes
        .iter()
        .position(|a| a.name == "x")
        .expect("x axis");
    let y_dim_i = first_multiscale
        .axes
        .iter()
        .position(|a| a.name == "y")
        .expect("y axis");
    let z_dim_i_opt = first_multiscale.axes.iter().position(|a| a.name == "z");
    let c_dim_i_opt = first_multiscale.axes.iter().position(|a| a.name == "c");
    let t_dim_i_opt = first_multiscale.axes.iter().position(|a| a.name == "t");

    // Need to load the highest-resolution level to read its shape metadata.
    let hires_dataset = &first_multiscale
        .datasets
        .first()
        .expect("At least one dataset");
    let hires_array =
        zarrs::array::Array::async_open(store.clone(), &format!("/{}_nc", hires_dataset.path))
            .await
            .expect("Open highest-resolution dataset array");

    let img_hires_w = hires_array.shape()[x_dim_i];
    let img_hires_h = hires_array.shape()[y_dim_i];

    // For now, load the lowest resolution level.
    // If small enough, use as the initial/background image (perhaps only needed in the interactive case, though).
    let lowres_dataset = &first_multiscale
        .datasets
        .last()
        .expect("At least one dataset");
    let lowres_array =
        zarrs::array::Array::async_open(store.clone(), &format!("/{}_nc", lowres_dataset.path))
            .await
            .expect("Open lowest-resolution dataset array");

    let img_lowres_w = lowres_array.shape()[x_dim_i];
    let img_lowres_h = lowres_array.shape()[y_dim_i];

    if let Some(z_dim_i) = z_dim_i_opt {
        let img_num_z = hires_array.shape()[z_dim_i];

        // TODO: do not assume the z axis size is the same for all resolution levels.
        // Reference: https://github.com/scverse/spatialdata/pull/955
        assert!(
            z_index < img_num_z as u32,
            "z_index {z_index} out of bounds for image with {img_num_z} z slices"
        );
    }

    if let Some(c_dim_i) = c_dim_i_opt {
        let img_num_c = hires_array.shape()[c_dim_i];

        // TODO: check that all requested channel_indices are valid.
    }

    let img_aspect_ratio = img_hires_w as f32 / img_hires_h as f32;

    // How do we want the coordinate system to work?
    // If only dealing with a single image, we could operate with the screen quad (-1, 1) corresponding to the full number of pixels.
    // However, it may be easier if instead, we fix the screen quad to correspond to a particular physical size, such as 1 mm square.
    // This may make it easier to align things later, or to implement a scale bar, for instance.
    // It seems that spatialdata-plot puts (0,0) in the top left
    // Reference: https://spatialdata.scverse.org/en/stable/tutorials/notebooks/notebooks/examples/transformations.html
    //
    // See ScaleBarLayer implementation in Viv.
    // Reference: https://github.com/hms-dbmi/viv/blob/08a74203b99f54bc62307c741944ed61e33e810c/packages/layers/src/scale-bar-layer.js#L67
    //
    // Physical size of a pixel, according to OME model: 1 micrometer (um)
    // Reference: https://www.openmicroscopy.org/Schemas/Documentation/Generated/OME-2016-06/ome.html

    // There are 1000 um in 1 mm, so the screen quad would correspond to a 1000 x 1000 image (for a square aspect ratio).
    // Reference: https://github.com/hms-dbmi/viv/blob/08a74203b99f54bc62307c741944ed61e33e810c/packages/layers/src/utils.js#L169

    // The viewport also has an aspect ratio:
    let viewport_w = context.params.width as f32;
    let viewport_h = context.params.height as f32;
    let viewport_aspect_ratio = viewport_w / viewport_h;

    // We probably do not want the resulting visualization to change if the user scales the viewport size.
    // (Although it will necessarily change if the user modifies the viewport aspect ratio.)

    // Therefore, when non-square, the shorter viewport dimension (x/y) should correspond to the 1mm length.
    // This way, a fully square image would fully render in a very narrow viewport.
    // Note: this is all prior to accounting for the camera's view matrix.

    // log(&format!("Image dimensions: {} x {}", img_w, img_h));

    let num_channels = bioimage_params.channel_indices.len();

    // Assert that there are the same number of colors and windows.
    assert_eq!(num_channels, bioimage_params.channel_colors.len());
    assert_eq!(num_channels, bioimage_params.channel_windows.len());

    // TODO: actually use the channel indices.

    // Read the pixel data using a slice that selects the first z, c, and t indices.

    // This array is CZYX.
    // TODO: do not assume 4D and dim order.
    let ch0_arr_slice = zarrs::array::ArraySubset::new_with_start_shape(
        vec![0, z_index as u64, 0, 0],                        // start
        vec![1, 1, img_lowres_h as u64, img_lowres_w as u64], // shape
    )
    .expect("Compatible dimensionality");

    // This array is CZYX.
    // TODO: do not assume 4D and dim order.
    let ch1_arr_slice = zarrs::array::ArraySubset::new_with_start_shape(
        vec![1, z_index as u64, 0, 0],                        // start
        vec![1, 1, img_lowres_h as u64, img_lowres_w as u64], // shape
    )
    .expect("Compatible dimensionality");

    // TODO: support other dtypes.

    // Use futures::join! to run the async retrievals in parallel, similar to Promise.all in JS.
    let futures_try_join_result = futures::try_join!(
        maybe_timeout!(
            lowres_array.async_retrieve_array_subset::<Vec<u16>>(&ch0_arr_slice),
            context.params.timeout
        ),
        maybe_timeout!(
            lowres_array.async_retrieve_array_subset::<Vec<u16>>(&ch1_arr_slice),
            context.params.timeout
        )
    );

    // TODO: load image data as vec of individual chunks (rather than requesting the full slice)
    // to allow for progressive rendering of large images as the chunks load.
    // We want to render the chunks that have loaded prior to the timeout (if there was a timeout specified).
    // First convert the requested slice to the chunk keys?

    let (ch0_result, ch1_result) = match futures_try_join_result {
        Ok((ch0_result, ch1_result)) => {
            // Both channel reads succeeded within the timeout.
            log("Both channel reads succeeded within the timeout.");
            (ch0_result, ch1_result)
        }
        Err(_) => {
            // TODO: still render something in this case
            // (e.g., lower-resolution image or subset of channels)
            log("Channel reads timed out or failed");
            return RenderResult { bailed_early: true };
        }
    };

    // Read the whole array
    let ch0_arr = ch0_result.expect("Read ch0 pixel data");
    let ch1_arr = ch1_result.expect("Read ch1 pixel data");

    // Concatenate the channel data into a single vector.
    // TODO: is this the most efficient way / use the minimal number of copies?
    let mut combined_pixel_data = ch0_arr;
    combined_pixel_data.extend_from_slice(&ch1_arr);

    // Store the ndarray::ArrayD in a WGPU texture.
    // Create a texture to store the image data (R16Uint).

    // TODO: does this need to be padded to 256 bytes per row?
    let bytes_per_pixel: u32 = 2; // R16Uint has 2 bytes per pixel.
    let unpadded_bytes_per_row = img_lowres_w as u32 * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT; // 256
    let padded_bytes_per_row = ((unpadded_bytes_per_row + align - 1) / align) * align;

    let texture_size = wgpu::Extent3d {
        width: img_lowres_w as u32,
        height: img_lowres_h as u32,
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
            rows_per_image: Some(img_lowres_h as u32),
        },
        texture_size,
    );

    // TODO: use the camera_view and derived values up above, to determine which multiscale to load, etc.

    // Note: WebGPU's shading language (WGSL) treats matrices as column-major.
    let camera_view = context.params.camera_view.unwrap_or([
        // Column 0
        1.0, 0.0, 0.0, 0.0, // Column 1
        0.0, 1.0, 0.0, 0.0, // Column 2
        0.0, 0.0, 1.0, 0.0, // Column 3
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
    let scale_factor = (1.0 / zoom).log2();

    log(&format!("scale factor: {}", scale_factor));

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

    // Define the uniforms, matching the WGSL layout (handled by using encase).
    let channel_uniforms: Vec<ChannelUniforms> = bioimage_params
        .channel_windows
        .iter()
        .zip(bioimage_params.channel_colors.iter())
        .map(|(w, c)| ChannelUniforms {
            channel_window: Vec2::new(w.0, w.1),
            channel_colors: Vec3::new(c.0, c.1, c.2),
        })
        .collect();
    let bioimage_uniforms = BioimageUniforms {
        camera_view: Mat4::from_cols_array(&camera_view),
        viewport_size: Vec2::new(viewport_w, viewport_h),
        num_channels: Default::default(),
        channels: channel_uniforms,
    };

    // Runtime-sized arrays cannot be used with the encase UniformBuffer,
    // and require using StorageBuffer instead.
    let mut buffer = StorageBuffer::new(Vec::<u8>::new());
    buffer.write(&bioimage_uniforms).unwrap();
    let uniform_bytes = buffer.into_inner();

    let uniform_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Storage Buffer for Uniforms"),
        size: uniform_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    context
        .queue
        .write_buffer(&uniform_buffer, 0, &uniform_bytes);

    // Create bind group layout and bind group for positions + uniforms
    let bind_group_layout =
        context
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Bioimage BGL"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        // The uniforms buffer.
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
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

    let bind_group = context
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bioimage BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&image_view),
                },
            ],
        });

    let shader = context
        .device
        .create_shader_module(wgpu::include_wgsl!("shaders/bioimage.wgsl"));

    let render_pipeline_layout =
        context
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                immediate_size: 0,
            });

    let render_pipeline = context
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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
            cache: None,
            multiview_mask: None,
        });

    let out_view = context
        .out_tex
        .create_view(&wgpu::TextureViewDescriptor::default());

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &out_view,
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
            multiview_mask: None,
        });

        render_pass.set_pipeline(&render_pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..4, 0..1);

        // End the renderpass.
        drop(render_pass);
    }

    RenderResult {
        bailed_early: false,
    }
}
