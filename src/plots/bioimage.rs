use std::borrow::Cow;

use vello::wgpu;
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
    let z_index = 0;
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

    // Read the pixel data using a slice that selects the first z, c, and t indices.
    
    // This array is CZYX.
    // TODO: do not assume 4D and dim order.
    let arr_subset = zarrs::array_subset::ArraySubset::new_with_start_shape(
        vec![0, 0, 0, 0], // start
        vec![2, 1, img_h as u64, img_w as u64], // shape
    ).expect("Compatible dimensionality");

    // TODO: support other dtypes.
    let arr = lowres_array.async_retrieve_array_subset_ndarray::<u16>(&arr_subset)
        .await.expect("Read pixel data");

    log(&format!("Read array with shape {:?} and dtype i16", arr.shape()));


    // Determine the visible region and the resolution level to use based on the camera view.

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

}