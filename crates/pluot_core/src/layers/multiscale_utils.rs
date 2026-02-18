use serde::{Deserialize, Serialize};

use crate::layer_traits::{AspectRatioMode, ViewParams};
use crate::log;

/// A single resolution level in the multiscale pyramid.
///
/// Modeled after OME-NGFF: each level represents the same physical region of an image
/// at a different pixel resolution. The `scale` values encode the physical voxel size,
/// so coarser levels have larger scale values.
///
/// All levels should cover the same physical extent: `shape[dim] * scale[dim]` should
/// be approximately equal across levels for each spatial dimension.
///
/// Example for a 3-level pyramid (Y and X spatial dimensions only):
///   - Level 0 (finest):   shape=[4096, 4096], chunk_shape=[256, 256], scale=[0.5, 0.5]
///   - Level 1:            shape=[2048, 2048], chunk_shape=[256, 256], scale=[1.0, 1.0]
///   - Level 2 (coarsest): shape=[1024, 1024], chunk_shape=[256, 256], scale=[2.0, 2.0]
///
/// Physical extent at each level: 4096×0.5 = 2048×1.0 = 1024×2.0 = 2048.0
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResolutionLevel {
    /// Shape of the full image at this resolution: [height, width] in pixels.
    pub shape: [u32; 2],
    /// Chunk/tile shape at this resolution: [chunk_height, chunk_width] in pixels.
    pub chunk_shape: [u32; 2],
    /// Physical voxel size (scale) at this resolution: [scale_y, scale_x].
    /// Per OME-NGFF, this is the pixel size in the axis's physical unit.
    /// Coarser levels have larger scale values.
    pub scale: [f64; 2],
}

/// A visible tile at a given resolution level.
///
/// The coordinate system has (0,0) at the bottom-left. Tile row 0 is the
/// bottom row of the image in physical space (which corresponds to the
/// *last* rows of the image array, since arrays are stored top-to-bottom).
pub struct VisibleTile {
    /// Tile column index (0 = leftmost).
    pub col: i32,
    /// Tile row index in physical space (0 = bottom).
    pub row: i32,
    /// Physical X coordinate of the tile's left edge.
    pub phys_x0: f64,
    /// Physical Y coordinate of the tile's bottom edge.
    pub phys_y0: f64,
    /// Physical X coordinate of the tile's right edge.
    pub phys_x1: f64,
    /// Physical Y coordinate of the tile's top edge.
    pub phys_y1: f64,

    pub tile_x_start: u64, // indexing into the image array for this resolution level
    pub tile_x_end: u64, // indexing into the image array for this resolution level
    pub tile_y_start: u64, // indexing into the image array for this resolution level
    pub tile_y_end: u64, // indexing into the image array for this resolution level
}

/// Extract zoom and translation from the camera_view matrix.
pub fn get_view_transform(view_params: &ViewParams) -> (f32, f32, f32) {
    let camera_view = view_params.camera_view.unwrap_or([
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]);
    let zoom = camera_view[0];
    let translate_x = camera_view[12];
    let translate_y = camera_view[13];
    (zoom, translate_x, translate_y)
}

/// Calculate the visible data range based on camera view.
/// Returns (min_x, max_x, min_y, max_y) in data coordinates.
///
/// The returned range is in whatever coordinate system the camera_view is
/// configured for. When the camera is set up to frame physical coordinates
/// (e.g., micrometers), this returns physical coordinates.
pub fn get_visible_range(view_params: &ViewParams) -> (f64, f64, f64, f64) {
    let (zoom, translate_x, translate_y) = get_view_transform(view_params);

    let aspect_ratio_mode = view_params.aspect_ratio_mode;

    let bounds = &view_params.margins;

    let margin_top = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
    let margin_right = bounds.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
    let margin_bottom = bounds.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
    let margin_left = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;

    let viewport_w = view_params.width as f32;
    let viewport_h = view_params.height as f32;

    let layer_w = viewport_w - (margin_left + margin_right) as f32;
    let layer_h = viewport_h - (margin_top + margin_bottom) as f32;

    let layer_aspect_ratio = layer_w / layer_h;

    let mut x_scale_for_aspect_ratio_mode = 1.0_f32;
    let mut y_scale_for_aspect_ratio_mode = 1.0_f32;
    match aspect_ratio_mode {
        AspectRatioMode::Ignore => {}
        AspectRatioMode::Contain => {
            if layer_aspect_ratio > 1.0 {
                x_scale_for_aspect_ratio_mode = layer_aspect_ratio;
            } else if layer_aspect_ratio < 1.0 {
                y_scale_for_aspect_ratio_mode = layer_aspect_ratio;
            }
        }
        AspectRatioMode::Cover => {
            if layer_aspect_ratio > 1.0 {
                y_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
            } else if layer_aspect_ratio < 1.0 {
                x_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
            }
        }
    }

    let x_adjustment = x_scale_for_aspect_ratio_mode - 1.0;
    let y_adjustment = y_scale_for_aspect_ratio_mode - 1.0;

    let min_x = (((-translate_x - 1.0 - x_adjustment) / zoom) + 1.0) / 2.0;
    let max_x = (((-translate_x + 1.0 + x_adjustment) / zoom) + 1.0) / 2.0;
    let min_y = (((-translate_y - 1.0 - y_adjustment) / zoom) + 1.0) / 2.0;
    let max_y = (((-translate_y + 1.0 + y_adjustment) / zoom) + 1.0) / 2.0;

    (min_x as f64, max_x as f64, min_y as f64, max_y as f64)
}

/// Compute the effective layer size in CSS pixels (accounting for margins).
pub fn get_layer_size(view_params: &ViewParams) -> (f64, f64) {
    let bounds = &view_params.margins;

    let margin_top = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
    let margin_right = bounds.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
    let margin_bottom = bounds.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
    let margin_left = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;

    let layer_w = view_params.width as f64 - margin_left - margin_right;
    let layer_h = view_params.height as f64 - margin_top - margin_bottom;

    (layer_w, layer_h)
}

/// Select the best resolution level for the current viewport state.
///
/// Strategy: pick the coarsest level whose voxel size is no larger than one
/// screen pixel (in physical units, accounting for device pixel ratio). This
/// avoids loading unnecessarily fine data while keeping the image sharp.
///
/// Iterates from the coarsest level (last) to the finest (first) and returns
/// the first level whose voxel size ≤ the screen pixel size in both
/// dimensions. Falls back to level 0 if even the finest level is too coarse
/// (i.e., the user is zoomed in past native resolution).
pub fn select_resolution_level(view_params: &ViewParams, levels: &[ResolutionLevel]) -> usize {
    if levels.len() == 1 {
        return 0;
    }

    let (min_x, max_x, min_y, max_y) = get_visible_range(view_params);
    let (layer_w, layer_h) = get_layer_size(view_params);
    let dpr = view_params.device_pixel_ratio as f64;

    // The visible range is already in physical coordinates (same coordinate
    // system as scale values), so we can directly compute the physical size
    // of one screen pixel.
    let screen_pixel_phys_x = (max_x - min_x) / (layer_w * dpr);
    let screen_pixel_phys_y = (max_y - min_y) / (layer_h * dpr);

    // Use the smaller screen pixel size (the more demanding dimension)
    // to ensure sharpness in both directions.
    let screen_pixel_phys = screen_pixel_phys_x.min(screen_pixel_phys_y);

    // Iterate from coarsest to finest. Return the first (coarsest) level
    // whose voxel size is ≤ the screen pixel size.
    for i in (0..levels.len()).rev() {
        let voxel_size = levels[i].scale[0].max(levels[i].scale[1]);
        if voxel_size <= screen_pixel_phys {
            return i;
        }
    }

    // Zoomed in past native resolution — use the finest level.
    0
}

pub fn to_y_slice(start: u64, end: u64, height: u64) -> (u64, u64) {
    // OME-Zarr uses a coordinate system where (0, 0) is the top-left corner, and Y increases downwards.
    // We want to convert to a coordinate system where (0, 0) is the bottom-left corner, and Y increases upwards.
    // So we need to flip the Y coordinates.
    let y_start = height - end;
    let y_end = height - start;
    (y_start, y_end)
}


/// Compute all visible tiles at a given resolution level for the current viewport.
///
/// The coordinate system has (0,0) at the bottom-left. Physical Y increases
/// upward. Tile row 0 is the bottom of the image.
///
/// Because image arrays are stored top-to-bottom, the bottom physical row
/// corresponds to the last rows of the array. The `row` field on each
/// `VisibleTile` counts from the bottom in physical space; callers that need
/// array indices should convert via `array_row = num_tile_rows - 1 - row`.
///
/// Tile positions are in physical coordinates:
///   - A tile at column `col` starts at x = col * chunk_width * scale_x
///   - A tile at row `row` starts at y = row * chunk_height * scale_y
///   - Its width/height is chunk_shape * scale (or smaller for partial edge tiles)
pub fn get_visible_tiles(view_params: &ViewParams, level: &ResolutionLevel) -> Vec<VisibleTile> {
    // Compute the visible extent with respect to the coordinate system.
    let (min_x, max_x, min_y, max_y) = get_visible_range(view_params);

    log(&format!(
        "Visible range: min_x={}, max_x={}, min_y={}, max_y={}",
        min_x, max_x, min_y, max_y
    ));

    // Map the physical extent to pixel indices.
    let min_x_pixel = ((min_x / level.scale[1]).floor() as i32).max(0);
    let max_x_pixel = ((max_x / level.scale[1]).ceil() as i32).min(level.shape[1] as i32);
    // Note min_y_pixel here is below max_y_pixel (we have not yet flipped).
    let min_y_pixel_below = ((min_y / level.scale[0]).floor() as i32).max(0);
    let max_y_pixel_above = ((max_y / level.scale[0]).ceil() as i32).min(level.shape[0] as i32);

    // Flip the Y pixel indices to match the array coordinate system (0 = top).
    // let (min_y_pixel_above, max_y_pixel_below) = to_y_slice(min_y_pixel_below as u64, max_y_pixel_above as u64, level.shape[0] as u64);

    // Convert the pixel indices to tile indices, accounting for irregular edge tiles.
    // NOTE: It is possible for the final chunk along each axis to be a partial tile.
    // When accounting for this, we must keep in mind that pixel (0, 0) is at the top left,
    // but our coordinate system has physical row 0 at the bottom.
    let bottom_tile_height = level.shape[0] as f64 % level.chunk_shape[0] as f64;
    let right_tile_width = level.shape[1] as f64 % level.chunk_shape[1] as f64;

    // Total number of tile columns and rows at this resolution level.
    let num_tile_cols = (level.shape[1] as f64 / level.chunk_shape[1] as f64).ceil() as i32;
    let num_tile_rows = (level.shape[0] as f64 / level.chunk_shape[0] as f64).ceil() as i32;

    let min_x_tile_i = ((min_x_pixel as f64 / level.chunk_shape[1] as f64).floor() as i32).max(0);
    let max_x_tile_i = ((max_x_pixel as f64 / level.chunk_shape[1] as f64).ceil() as i32).min(num_tile_cols);

    let min_y_tile_i = ((min_y_pixel_below as f64 / level.chunk_shape[0] as f64).floor() as i32).max(0);
    let max_y_tile_i = ((max_y_pixel_above as f64 / level.chunk_shape[0] as f64).ceil() as i32).min(num_tile_rows);

    let mut tiles = Vec::new();

    let phys_height = level.shape[0] as f64 * level.scale[0];

    // Print all above calculations for debugging.
    log(&format!(
        "Pixel info: min_x_pixel={}, max_x_pixel={}, min_y_pixel_below={}, max_y_pixel_above={}",
        min_x_pixel, max_x_pixel, min_y_pixel_below, max_y_pixel_above
    ));
    log(&format!(
        "Tile indices: min_x_tile_i={}, max_x_tile_i={}, min_y_tile_i={}, max_y_tile_i={}",
        min_x_tile_i, max_x_tile_i, min_y_tile_i, max_y_tile_i
    ));

    // For the purposes of this loop, we treat row 0 at the bottom of the coordinate system.
    for y_tile_i in min_y_tile_i..max_y_tile_i { // Note: max_y_tile_i is the bottom tile, min_y_tile_i is the top tile, so we iterate from max to min.
        for x_tile_i in min_x_tile_i..max_x_tile_i {
            // These start/end are in array pixel coordinates (0 = top), not physical coordinates.
            let tile_x_start = x_tile_i as u64 * level.chunk_shape[1] as u64;
            let tile_x_end = (tile_x_start + level.chunk_shape[1] as u64).min(level.shape[1] as u64);

            let tile_y_start_top = y_tile_i as u64 * level.chunk_shape[0] as u64;
            let tile_y_end_bottom = (tile_y_start_top + level.chunk_shape[0] as u64).min(level.shape[0] as u64);

            let phys_x0 = tile_x_start as f64 * level.scale[1];
            let phys_x1 = tile_x_end as f64 * level.scale[1];

            let phys_y0 = phys_height - (tile_y_end_bottom as f64 * level.scale[0]);
            let phys_y1 = phys_height - (tile_y_start_top as f64 * level.scale[0]);

            let (tile_y_start, tile_y_end) = to_y_slice(tile_y_start_top, tile_y_end_bottom, level.shape[0] as u64);

            log(&format!("Tile: col={}, row={}, phys_x0={}, phys_y0={}, phys_x1={}, phys_y1={}, tile_pixels_w={}, tile_pixels_h={}",
                x_tile_i, y_tile_i, phys_x0, phys_y0, phys_x1, phys_y1, tile_x_end - tile_x_start, tile_y_end - tile_y_start));

            tiles.push(VisibleTile {
                col: x_tile_i,
                row: y_tile_i,
                phys_x0,
                phys_y0,
                phys_x1,
                phys_y1,
                tile_x_start,
                tile_x_end,
                tile_y_start,
                tile_y_end,
            });
        }
    }

    tiles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer_traits::MarginParams;

    /// Helper to create a ViewParams with sensible defaults for testing.
    fn make_view_params(
        width: u32,
        height: u32,
        camera_view: Option<[f32; 16]>,
    ) -> ViewParams {
        ViewParams {
            width,
            height,
            camera_view,
            ..ViewParams::default()
        }
    }

    /// Helper to build a column-major 4x4 camera matrix from zoom and translation.
    fn camera_matrix(zoom: f32, tx: f32, ty: f32) -> [f32; 16] {
        [
            zoom, 0.0, 0.0, 0.0,
            0.0, zoom, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            tx, ty, 0.0, 1.0,
        ]
    }

    /// Standard 3-level pyramid used across many tests.
    /// Physical extent: 4096*0.5 = 2048*1.0 = 1024*2.0 = 2048.0
    fn three_level_pyramid() -> Vec<ResolutionLevel> {
        vec![
            ResolutionLevel { shape: [4096, 4096], chunk_shape: [256, 256], scale: [0.5, 0.5] },
            ResolutionLevel { shape: [2048, 2048], chunk_shape: [256, 256], scale: [1.0, 1.0] },
            ResolutionLevel { shape: [1024, 1024], chunk_shape: [256, 256], scale: [2.0, 2.0] },
        ]
    }

    /*
    #[test]
    fn test_debug_plant_tiles() {
        // Width, height, camera_view: 800, 800, Some([3.4370332e-5, 0.0, 0.0, 0.0, 0.0, 3.4370332e-5, 0.0, 0.0, 0.0, 0.0, 0.005, 0.0, -15.321514, -16.47842, 0.0, 1.0])
        // Visible range: min_x=208341.71875, max_x=237436.578125, min_y=225171.734375, max_y=254266.59375
        // scale factor: 14.828477
        // Level 0: chunk_shape=(1024,1024) scale=(70.55555725097656,70.55555725097656)
        // Level 1: chunk_shape=(1024,1024) scale=(141.11111450195313,141.11111450195313)
        // Level 2: chunk_shape=(1024,1024) scale=(282.22222900390625,282.22222900390625)
        let mut vp = make_view_params(800, 800, Some([3.4370332e-5, 0.0, 0.0, 0.0, 0.0, 3.4370332e-5, 0.0, 0.0, 0.0, 0.0, 0.005, 0.0, -15.321514, -16.47842, 0.0, 1.0]));
        vp.margins = Some(MarginParams {
            margin_left: Some(100.0),
            margin_right: Some(100.0),
            margin_top: Some(100.0),
            margin_bottom: Some(100.0),
        });

        let (zoom, tx, ty) = get_view_transform(&vp);
        assert_eq!(zoom, 3.4370332e-5);
        assert_eq!(tx, -15.321514);
        assert_eq!(ty, -16.47842);


        let (w, h) = get_layer_size(&vp);
        assert_eq!(w, 600.0); // 800 - 100 - 100
        assert_eq!(h, 600.0);  // 800 - 100 - 100

        let levels = vec![
            ResolutionLevel { shape: [5464,8192], chunk_shape: [1024,1024], scale: [70.55555725097656,70.55555725097656] },
            ResolutionLevel { shape: [2732,4096], chunk_shape: [1024,1024], scale: [141.11111450195313,141.11111450195313] },
            ResolutionLevel { shape: [1366,2048], chunk_shape: [1024,1024], scale: [282.22222900390625,282.22222900390625] },
        ];

        assert_eq!(select_resolution_level(&vp, &levels), 0);

        let level = levels[0].clone();

        let visible_tiles = get_visible_tiles(&vp, &level);
        for tile in visible_tiles {
            println!(
                "Tile: col={}, row={}, phys_x0={}, phys_y0={}, phys_x1={}, phys_y1={}, tile_pixels_w={}, tile_pixels_h={}",
                tile.col, tile.row, tile.phys_x0, tile.phys_y0, tile.phys_x1, tile.phys_y1, tile.tile_pixels_w, tile.tile_pixels_h,
            );
            // Printed:
            // Tile: col=2, row=3, phys_x0=144497.78125, phys_y0=216746.671875, phys_x1=216746.671875, phys_y1=288995.5625, tile_pixels_w=1024, tile_pixels_h=1024
            // Tile: col=3, row=3, phys_x0=216746.671875, phys_y0=216746.671875, phys_x1=288995.5625, phys_y1=288995.5625, tile_pixels_w=1024, tile_pixels_h=1024

            let tile_h = tile.tile_pixels_h as u64;
            let tile_w = tile.tile_pixels_w as u64;

            // Convert physical-space row (0 = bottom) to array-space row
            // (0 = top). The array is stored top-to-bottom, so:
            let array_row = tile.num_tile_rows - 1 - tile.row;
            let tile_y_start = array_row as u64 * level.chunk_shape[0] as u64;
            let tile_x_start = tile.col as u64 * level.chunk_shape[1] as u64;

            println!("Tile slice: x=[{}, {}], y=[{}, {}]", tile_x_start, tile_x_start + tile_w, tile_y_start, tile_y_start + tile_h);

            // Printed:
            // Tile: col=2, row=3, phys_x0=144497.78125, phys_y0=216746.671875, phys_x1=216746.671875, phys_y1=288995.5625, tile_pixels_w=1024, tile_pixels_h=1024
            // Tile slice: x=[2048, 3072], y=[2048, 3072]
            // Tile: col=3, row=3, phys_x0=216746.671875, phys_y0=216746.671875, phys_x1=288995.5625, phys_y1=288995.5625, tile_pixels_w=1024, tile_pixels_h=1024
            // Tile slice: x=[3072, 4096], y=[2048, 3072]

            // TODO: calculate the ground truth slices for these axis extents.
            // Verify that the calculated slices match the expected slices based on the level's chunk shape and scale.



            /*
            let mut start_slice = vec![0u64; ndim];
            let mut stop_slice = vec![1u64; ndim];

            start_slice[metadata.y_dim_i] = tile_y_start;
            stop_slice[metadata.y_dim_i] = tile_y_start + tile_h;
            start_slice[metadata.x_dim_i] = tile_x_start;
            stop_slice[metadata.x_dim_i] = tile_x_start + tile_w;
            */

        }




    }
     */

    // ========================================================================
    // get_view_transform
    // ========================================================================

    #[test]
    fn test_get_view_transform_identity() {
        let vp = make_view_params(100, 100, None);
        let (zoom, tx, ty) = get_view_transform(&vp);
        assert_eq!(zoom, 1.0);
        assert_eq!(tx, 0.0);
        assert_eq!(ty, 0.0);
    }

    #[test]
    fn test_get_view_transform_zoomed_and_translated() {
        let vp = make_view_params(100, 100, Some(camera_matrix(2.0, 0.5, -0.3)));
        let (zoom, tx, ty) = get_view_transform(&vp);
        assert_eq!(zoom, 2.0);
        assert_eq!(tx, 0.5);
        assert_eq!(ty, -0.3);
    }

    // ========================================================================
    // get_layer_size
    // ========================================================================

    #[test]
    fn test_get_layer_size_no_margins() {
        let vp = make_view_params(200, 100, None);
        let (w, h) = get_layer_size(&vp);
        assert_eq!(w, 200.0);
        assert_eq!(h, 100.0);
    }

    #[test]
    fn test_get_layer_size_with_margins() {
        let mut vp = make_view_params(200, 100, None);
        vp.margins = Some(MarginParams {
            margin_left: Some(10.0),
            margin_right: Some(20.0),
            margin_top: Some(5.0),
            margin_bottom: Some(15.0),
        });
        let (w, h) = get_layer_size(&vp);
        assert_eq!(w, 170.0); // 200 - 10 - 20
        assert_eq!(h, 80.0);  // 100 - 5 - 15
    }

    // ========================================================================
    // get_visible_range
    // ========================================================================

    #[test]
    fn test_get_visible_range_identity_camera_square() {
        // Identity camera on a square viewport with Ignore aspect ratio mode.
        let mut vp = make_view_params(100, 100, None);
        vp.aspect_ratio_mode = AspectRatioMode::Ignore;
        let (min_x, max_x, min_y, max_y) = get_visible_range(&vp);
        // With identity camera, the visible range maps NDC [-1,1] to [0,1].
        assert!((min_x - 0.0).abs() < 1e-6);
        assert!((max_x - 1.0).abs() < 1e-6);
        assert!((min_y - 0.0).abs() < 1e-6);
        assert!((max_y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_get_visible_range_zoomed_in_2x() {
        // Zoomed in 2x: visible range should be [0.25, 0.75] in both axes.
        let mut vp = make_view_params(100, 100, Some(camera_matrix(2.0, 0.0, 0.0)));
        vp.aspect_ratio_mode = AspectRatioMode::Ignore;
        let (min_x, max_x, min_y, max_y) = get_visible_range(&vp);
        assert!((min_x - 0.25).abs() < 1e-6);
        assert!((max_x - 0.75).abs() < 1e-6);
        assert!((min_y - 0.25).abs() < 1e-6);
        assert!((max_y - 0.75).abs() < 1e-6);
    }

    #[test]
    fn test_get_visible_range_zoomed_out_2x() {
        // Zoomed out 0.5x: visible range should be [-0.5, 1.5] in both axes.
        let mut vp = make_view_params(100, 100, Some(camera_matrix(0.5, 0.0, 0.0)));
        vp.aspect_ratio_mode = AspectRatioMode::Ignore;
        let (min_x, max_x, min_y, max_y) = get_visible_range(&vp);
        assert!((min_x - (-0.5)).abs() < 1e-6);
        assert!((max_x - 1.5).abs() < 1e-6);
        assert!((min_y - (-0.5)).abs() < 1e-6);
        assert!((max_y - 1.5).abs() < 1e-6);
    }

    #[test]
    fn test_get_visible_range_with_contain_wide_viewport() {
        // Wide viewport (200x100) with Contain mode.
        // The (0,1) data square should be contained in the viewport,
        // so the visible x range extends beyond (0,1) symmetrically.
        let mut vp = make_view_params(200, 100, None);
        vp.aspect_ratio_mode = AspectRatioMode::Contain;
        let (min_x, max_x, min_y, max_y) = get_visible_range(&vp);
        // Y range stays [0,1]; X range expands to accommodate the wider viewport.
        assert!((min_y - 0.0).abs() < 1e-6);
        assert!((max_y - 1.0).abs() < 1e-6);
        assert!(min_x < 0.0, "min_x should be negative for wide contain");
        assert!(max_x > 1.0, "max_x should exceed 1 for wide contain");
        // The X range should be symmetric around 0.5.
        let x_center = (min_x + max_x) / 2.0;
        assert!((x_center - 0.5).abs() < 1e-6);
    }

    // ========================================================================
    // select_resolution_level
    // ========================================================================

    #[test]
    fn test_select_resolution_level_single_level() {
        let levels = vec![
            ResolutionLevel { shape: [1024, 1024], chunk_shape: [256, 256], scale: [1.0, 1.0] },
        ];
        let vp = make_view_params(100, 100, None);
        assert_eq!(select_resolution_level(&vp, &levels), 0);
    }

    #[test]
    fn test_select_resolution_level_zoomed_out_picks_coarsest() {
        // Zoomed out so far that even the coarsest level has sub-pixel voxels.
        let levels = three_level_pyramid();
        // At zoom=0.005: visible range width = 2/0.005 = 400.
        // screen_pixel_phys = 400 / 100 = 4.0.
        // Level 2: voxel 2.0 <= 4.0 => pick level 2 (coarsest).
        let vp = make_view_params(100, 100, Some(camera_matrix(0.005, 0.0, 0.0)));
        let selected = select_resolution_level(&vp, &levels);
        assert_eq!(selected, 2, "Should select coarsest level when zoomed far out");
    }

    #[test]
    fn test_select_resolution_level_zoomed_in_picks_finest() {
        // Zoomed in so much that we've exceeded native resolution.
        let levels = three_level_pyramid();
        // Zoom in 100x: each screen pixel covers a tiny physical area.
        let vp = make_view_params(100, 100, Some(camera_matrix(100.0, 0.0, 0.0)));
        let selected = select_resolution_level(&vp, &levels);
        assert_eq!(selected, 0, "Should select finest level when zoomed far in");
    }

    #[test]
    fn test_select_resolution_level_at_native_picks_finest() {
        // The visible range is (0, 1) and we have a 1024px viewport.
        // The finest level has scale 0.5, meaning each voxel covers 0.5 physical units.
        // screen_pixel_phys = 1.0 / 1024.0 ≈ 0.000977
        // voxel_size for level 0 = 0.5, which is >> screen pixel size.
        // So we'll still pick the finest because we're zoomed in past native.
        let levels = three_level_pyramid();
        let vp = make_view_params(1024, 1024, None);
        let selected = select_resolution_level(&vp, &levels);
        assert_eq!(selected, 0);
    }

    #[test]
    fn test_select_resolution_level_medium_zoom() {
        // Set up so the middle level is appropriate.
        // Physical extent is 2048. Level 1 has scale [1.0, 1.0].
        // We need screen_pixel_phys >= 1.0 but < 2.0.
        // With identity camera, visible range is (0,1) on a 100x100 viewport.
        // screen_pixel_phys = 1.0 / 100 = 0.01, which is < 0.5, so finest is selected.
        //
        // We need to zoom out enough that the coarsest voxels (2.0) fit but the middle (1.0) does not exceed.
        // Let's try zoom=0.01. visible range: (-49.5, 50.5). Range = 100.
        // screen_pixel_phys = 100.0 / 100 = 1.0.
        // Level 2: voxel 2.0 > 1.0 => skip. Level 1: voxel 1.0 <= 1.0 => pick level 1.
        let levels = three_level_pyramid();
        let vp = make_view_params(100, 100, Some(camera_matrix(0.01, 0.0, 0.0)));
        let selected = select_resolution_level(&vp, &levels);
        assert_eq!(selected, 1, "Should select middle level at appropriate zoom");
    }

    #[test]
    fn test_select_resolution_level_respects_dpr() {
        // Higher DPR means more demanding (smaller physical pixel size), which
        // should push towards finer levels.
        let levels = three_level_pyramid();
        // At zoom=0.005, screen_pixel_phys = 200/100 = 2.0 with dpr=1.
        // Level 2: voxel 2.0 <= 2.0 => pick level 2.
        let mut vp = make_view_params(100, 100, Some(camera_matrix(0.005, 0.0, 0.0)));
        let selected_1x = select_resolution_level(&vp, &levels);
        assert_eq!(selected_1x, 2);

        // With dpr=2, screen_pixel_phys = 200/(100*2) = 1.0.
        // Level 2: voxel 2.0 > 1.0 => skip. Level 1: voxel 1.0 <= 1.0 => pick level 1.
        vp.device_pixel_ratio = 2.0;
        let selected_2x = select_resolution_level(&vp, &levels);
        assert_eq!(selected_2x, 1, "Higher DPR should select a finer level");
    }

    // ========================================================================
    // get_visible_tiles
    // ========================================================================

    #[test]
    fn test_get_visible_tiles_identity_camera() {
        // Identity camera, square viewport. Visible range is (0,1).
        // Level with shape=[1024,1024], chunk_shape=[256,256], scale=[1.0,1.0].
        // Physical extent: 1024 * 1.0 = 1024.
        // Tile phys size: 256 * 1.0 = 256.
        // Number of tile cols/rows: 1024/256 = 4.
        // Visible range (0,1) in normalized coords.
        // tile_col_start = floor(0/256) = 0, tile_col_end = ceil(1/256) = 1.
        // So only tile (0,0) is visible (the range 0..1 is tiny compared to 0..1024).
        let level = ResolutionLevel {
            shape: [1024, 1024],
            chunk_shape: [256, 256],
            scale: [1.0, 1.0],
        };
        let vp = make_view_params(100, 100, None);
        let tiles = get_visible_tiles(&vp, &level);
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].col, 0);
        assert_eq!(tiles[0].row, 0);
        assert_eq!(tiles[0].phys_x0, 0.0);
        assert_eq!(tiles[0].phys_y0, 0.0);
    }

    #[test]
    fn test_get_visible_tiles_full_image_visible() {
        // Set up so the full image is visible.
        // Image: 512x512, chunk 256x256, scale 1.0 => 4 tiles total, physical extent 512.
        // We need visible range to cover [0, 512].
        // With identity camera, visible range is [0,1].
        // We need to zoom out so range covers 0..512.
        // visible_range = (-translate - 1) / zoom to (-translate + 1) / zoom, mapped (x+1)/2.
        // Range width = 2/zoom = 1/zoom * 2 => need 1/zoom = 512 => zoom ~ 1/512.
        // Actually: range = [0, 1/zoom] approximately when centered. Let's just use a very small zoom.
        // min_x = ((-0 -1)/zoom + 1)/2 = (-1/zoom + 1)/2. For zoom=0.001: (-1000+1)/2 = -499.5
        // max_x = ((-0 +1)/zoom + 1)/2 = (1/zoom + 1)/2. For zoom=0.001: (1000+1)/2 = 500.5
        // So range [-499.5, 500.5] covers [0, 512].
        let level = ResolutionLevel {
            shape: [512, 512],
            chunk_shape: [256, 256],
            scale: [1.0, 1.0],
        };
        let vp = make_view_params(100, 100, Some(camera_matrix(0.001, 0.0, 0.0)));
        let tiles = get_visible_tiles(&vp, &level);
        // 512/256 = 2 cols x 2 rows = 4 tiles
        assert_eq!(tiles.len(), 4);

        // Verify tile positions.
        let cols: Vec<i32> = tiles.iter().map(|t| t.col).collect();
        let rows: Vec<i32> = tiles.iter().map(|t| t.row).collect();
        assert!(cols.contains(&0));
        assert!(cols.contains(&1));
        assert!(rows.contains(&0));
        assert!(rows.contains(&1));
    }

    #[test]
    fn test_get_visible_tiles_partial_edge_tile() {
        // Image 300x300 with chunk 256x256, scale 1.0.
        // Tile grid: ceil(300/256) = 2 cols x 2 rows.
        // Edge tiles should be partial: 300 - 256 = 44 pixels.
        //
        // With bottom-left origin, the partial row in Y is at the bottom
        // (physical row 0 → array_row 1, which has 44 px remaining).
        // The top row (physical row 1 → array_row 0) has a full 256 px.
        let level = ResolutionLevel {
            shape: [300, 300],
            chunk_shape: [256, 256],
            scale: [1.0, 1.0],
        };
        // Zoom out to see all tiles.
        let vp = make_view_params(100, 100, Some(camera_matrix(0.001, 0.0, 0.0)));
        let tiles = get_visible_tiles(&vp, &level);
        assert_eq!(tiles.len(), 4);

    }

    #[test]
    fn test_get_visible_tiles_with_scale() {
        // Level with scale [2.0, 2.0]: each pixel covers 2 physical units.
        // shape=[512,512], chunk=[256,256], scale=[2.0,2.0].
        // Physical extent: 512*2 = 1024.
        // Tile phys size: 256*2 = 512.
        // With identity camera, visible range is (0,1) — only a tiny sliver.
        let level = ResolutionLevel {
            shape: [512, 512],
            chunk_shape: [256, 256],
            scale: [2.0, 2.0],
        };
        let vp = make_view_params(100, 100, None);
        let tiles = get_visible_tiles(&vp, &level);
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].phys_x0, 0.0);
        assert_eq!(tiles[0].phys_y0, 0.0);
    }

    #[test]
    fn test_get_visible_tiles_no_tiles_visible() {
        // Camera panned completely away from the image.
        // Translate such that visible range is entirely negative.
        // With zoom=1, tx=3.0: min_x = ((-3-1)/1 + 1)/2 = (-4+1)/2 = -1.5
        //                       max_x = ((-3+1)/1 + 1)/2 = (-2+1)/2 = -0.5
        // Both negative => no tiles (image starts at x=0).
        let level = ResolutionLevel {
            shape: [1024, 1024],
            chunk_shape: [256, 256],
            scale: [1.0, 1.0],
        };
        let vp = make_view_params(100, 100, Some(camera_matrix(1.0, 3.0, 3.0)));
        let tiles = get_visible_tiles(&vp, &level);
        assert_eq!(tiles.len(), 0, "No tiles should be visible when panned away");
    }

    #[test]
    fn test_get_visible_tiles_tile_ordering() {
        // Verify tiles are returned in row-major order (bottom row first, then next row up).
        // Row 0 = bottom of the image in physical space.
        let level = ResolutionLevel {
            shape: [512, 512],
            chunk_shape: [256, 256],
            scale: [1.0, 1.0],
        };
        let vp = make_view_params(100, 100, Some(camera_matrix(0.001, 0.0, 0.0)));
        let tiles = get_visible_tiles(&vp, &level);
        assert_eq!(tiles.len(), 4);
        // Bottom row first: (row=0,col=0), (row=0,col=1), then top row: (row=1,col=0), (row=1,col=1)
        assert_eq!((tiles[0].row, tiles[0].col), (0, 0));
        assert_eq!((tiles[1].row, tiles[1].col), (0, 1));
        assert_eq!((tiles[2].row, tiles[2].col), (1, 0));
        assert_eq!((tiles[3].row, tiles[3].col), (1, 1));
    }
}
