use std::sync::Arc;

use serde::{Deserialize, Serialize};
use svg::node::element::Group;

use crate::layers::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use crate::layer_traits::{
    AspectRatioMode, DrawToCanvas, DrawToSvg, MarginParams, PreparedAndDraw, PreparedLayer,
    UnitsMode, ViewParams,
};
use crate::layers::rect_layer::{RectLayer, RectLayerParams};
use crate::wgpu;

// A 2D MultiscaleLayer that accepts an array of resolution levels, where each resolution level has its own shape, chunk shape, and scale factor relative to the base resolution.
// It should render a rectangle corresponding to each chunk/tile at the appropriate resolution level based on the current zoom level and visible data range.
// The current closest layer is the TileLayer, but this does not handle multiple resolution levels or scaling like we want.
// We need to determine when to use each resolution based on:
// - zoom level / scale factor
// - visible data range
// - tile size
// - viewport size and pixel density
// - number of resolution levels available
// - how each resolution level is scaled relative to the base resolution (e.g., 2x, 4x, etc.) and whether it is uniform across levels
//
// We want something that aligns closely to the OME-NGFF multiscales specification:
// - Resolution levels in the multiscales array "MUST be ordered from largest (i.e. highest resolution) to smallest."
// - Each resolution level has a "scale" value per dimension that specifies the pixel/voxel size relative to the axis definition
//   (e.g., { "name": "x", "type": "space", "unit": "micrometer" } indicates that the X dimension has micrometer units)
// - Example of a TCZYX image:
//   - resolution 0: "scale": [1.0, 1.0, 0.5, 0.5, 0.5] // the voxel size for the first scale level (0.5 micrometer)
//   - resolution 1: "scale": [1.0, 1.0, 1.0, 1.0, 1.0] // the voxel size for the second scale level (downscaled by a factor of 2 -> 1 micrometer)
//   - resolution 2: "scale": [1.0, 1.0, 2.0, 2.0, 2.0] // the voxel size for the third scale level (downscaled by a factor of 4 -> 2 micrometer)
// - The image array at each resolution level has a "shape" that specifies the full shape of the array at that resolution level,
//   and a "chunk_shape" that specifies the shape of the tiles/chunks for that resolution level.
//     - If the chunk_shape is small enough to fit within GPU memory constraints,
//       then we can load and render individual tiles as needed based on the visible data range and zoom level.
//     - Otherwise, we just load tiles with some maximum tile_size, and zarr will handle subsetting the chunks for us.
// Reference: https://ngff.openmicroscopy.org/0.5/index.html#trafo-md


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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MultiscaleLayerParams {
    pub layer_id: String,
    /// Resolution levels ordered from highest resolution (finest, level 0)
    /// to lowest resolution (coarsest), per the OME-NGFF spec.
    /// Must contain at least one level.
    pub resolution_levels: Vec<ResolutionLevel>,
}

pub struct MultiscaleLayer {
    view_params: ViewParams,
    layer_params: MultiscaleLayerParams,
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

impl MultiscaleLayer {
    pub fn new(view_params: ViewParams, layer_params: MultiscaleLayerParams) -> Self {
        assert!(
            !layer_params.resolution_levels.is_empty(),
            "MultiscaleLayer requires at least one resolution level"
        );
        Self {
            view_params,
            layer_params,
            sub_layer_instances: Vec::new(),
        }
    }

    /// Extract zoom and translation from the camera_view matrix.
    fn get_view_transform(&self) -> (f32, f32, f32) {
        let camera_view = self.view_params.camera_view.unwrap_or([
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
    // TODO: factor this out into a shared utility (also used by TileLayer).
    fn get_visible_range(&self) -> (f64, f64, f64, f64) {
        let (zoom, translate_x, translate_y) = self.get_view_transform();

        let aspect_ratio_mode = self.view_params.aspect_ratio_mode;

        let bounds = &self.view_params.margins;

        let margin_top = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
        let margin_right = bounds.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
        let margin_bottom = bounds.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
        let margin_left = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;

        let viewport_w = self.view_params.width as f32;
        let viewport_h = self.view_params.height as f32;

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
    fn get_layer_size(&self) -> (f64, f64) {
        let bounds = &self.view_params.margins;

        let margin_top = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
        let margin_right = bounds.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
        let margin_bottom = bounds.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
        let margin_left = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;

        let layer_w = self.view_params.width as f64 - margin_left - margin_right;
        let layer_h = self.view_params.height as f64 - margin_top - margin_bottom;

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
    fn select_resolution_level(&self) -> usize {
        let levels = &self.layer_params.resolution_levels;
        if levels.len() == 1 {
            return 0;
        }

        let (min_x, max_x, min_y, max_y) = self.get_visible_range();
        let (layer_w, layer_h) = self.get_layer_size();
        let dpr = self.view_params.device_pixel_ratio as f64;

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

    /// Build RectLayer sublayers for each visible tile at the selected resolution level.
    /// TODO: check the RenderResult value returned by each sublayer's prepare() and draw the next-coarsest level
    /// whose RenderResult is RenderResult::Ready, to provide a fallback while loading higher-res tiles.
    /// We will need to ensure that drawing of sublayers occurs from coarser to finer, so that finer tiles are drawn on top of coarser tiles.
    /// This will ensure that we don't have visual holes while loading finer tiles, and that we get a sharper image as soon as finer tiles are ready.
    ///
    /// Tile positions are in physical coordinates:
    ///   - A tile at column `col` starts at x = col * chunk_width * scale_x
    ///   - Its width is chunk_width * scale_x (or smaller for partial edge tiles)
    fn build_sublayers(&self) -> Vec<Box<dyn PreparedAndDraw>> {
        let levels = &self.layer_params.resolution_levels;
        let level_idx = self.select_resolution_level();
        let level = &levels[level_idx];

        let (min_x, max_x, min_y, max_y) = self.get_visible_range();

        // Physical size of one full tile at this resolution level.
        let tile_phys_w = level.chunk_shape[1] as f64 * level.scale[1];
        let tile_phys_h = level.chunk_shape[0] as f64 * level.scale[0];

        // Total number of tile columns and rows at this resolution level.
        let num_tile_cols =
            (level.shape[1] as f64 / level.chunk_shape[1] as f64).ceil() as i32;
        let num_tile_rows =
            (level.shape[0] as f64 / level.chunk_shape[0] as f64).ceil() as i32;

        // Determine the range of tile indices that overlap the visible area.
        let tile_col_start = ((min_x / tile_phys_w).floor() as i32).max(0);
        let tile_col_end = ((max_x / tile_phys_w).ceil() as i32).min(num_tile_cols);
        let tile_row_start = ((min_y / tile_phys_h).floor() as i32).max(0);
        let tile_row_end = ((max_y / tile_phys_h).ceil() as i32).min(num_tile_rows);

        let mut sublayers: Vec<Box<dyn PreparedAndDraw>> = Vec::new();

        let mut x0_vec: Vec<f32> = Vec::new();
        let mut y0_vec: Vec<f32> = Vec::new();
        let mut x1_vec: Vec<f32> = Vec::new();
        let mut y1_vec: Vec<f32> = Vec::new();
        let mut labels_vec: Vec<i32> = Vec::new();

        for row in tile_row_start..tile_row_end {
            for col in tile_col_start..tile_col_end {
                let phys_x0 = col as f64 * tile_phys_w;
                let phys_y0 = row as f64 * tile_phys_h;

                // Clamp to the physical extent of the image at this level.
                // The last tile in a row/column may be a partial tile if the
                // image shape is not evenly divisible by the chunk shape.
                let pixels_remaining_x = level.shape[1] as f64 - (col as f64 * level.chunk_shape[1] as f64);
                let pixels_remaining_y = level.shape[0] as f64 - (row as f64 * level.chunk_shape[0] as f64);
                let tile_pixels_w = (level.chunk_shape[1] as f64).min(pixels_remaining_x);
                let tile_pixels_h = (level.chunk_shape[0] as f64).min(pixels_remaining_y);

                let phys_x1 = phys_x0 + tile_pixels_w * level.scale[1];
                let phys_y1 = phys_y0 + tile_pixels_h * level.scale[0];

                x0_vec.push(phys_x0 as f32);
                y0_vec.push(phys_y0 as f32);
                x1_vec.push(phys_x1 as f32);
                y1_vec.push(phys_y1 as f32);

                // Checkerboard label for visual debugging.
                // Encode the resolution level in the label so different levels
                // produce visually distinct patterns.
                labels_vec.push(((row + col + level_idx as i32) % 2) as i32);
            }
        }

        if !x0_vec.is_empty() {
            let rect_params = RectLayerParams {
                layer_id: format!(
                    "{}_tiles_level{}",
                    self.layer_params.layer_id, level_idx
                ),
                bounds: self.view_params.margins.clone(),
                data_unit_mode: UnitsMode::Data,
                stroke_width: 1.0,
                stroke_width_unit_mode: UnitsMode::Pixels,
                position_x0: Arc::new(x0_vec),
                position_y0: Arc::new(y0_vec),
                position_x1: Arc::new(x1_vec),
                position_y1: Arc::new(y1_vec),
                labels_vec: Arc::new(labels_vec),
            };
            sublayers.push(Box::new(RectLayer::new(
                self.view_params.clone(),
                rect_params,
            )));
        }

        sublayers
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for MultiscaleLayer {
    async fn prepare(&mut self) {
        // TODO: Build sublayers should return a list of all tiles in the viewport, from the coarsest resolution
        // to the current resolution. We will run all of their prepare() methods,
        // but we will only draw those whose RenderResult is Ready, so that we can show coarser tiles while finer tiles are loading.
        // We will need to filter out the coarser sublayers if all of the contained finer sublayers are ready,
        // while keeping coarser sublayers if any of the contained finer sublayers are still loading, to avoid visual holes.
        self.sub_layer_instances = self.build_sublayers();

        for sub_layer in self.sub_layer_instances.iter_mut() {
            sub_layer.prepare().await;
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToCanvas for MultiscaleLayer {
    async fn draw(&self, device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass) {
        base_draw_composite_layer(&self.sub_layer_instances, device, queue, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for MultiscaleLayer {
    async fn draw(&self, group: &Group) -> Group {
        base_draw_composite_layer_svg(&self.sub_layer_instances, group).await
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "MultiscaleLayer",
        create_layer: |value, view_params| {
            let params: MultiscaleLayerParams = serde_json::from_value(value).unwrap();
            Box::new(MultiscaleLayer::new(view_params.clone(), params))
        },
    }
}
