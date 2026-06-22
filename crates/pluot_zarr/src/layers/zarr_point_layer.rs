use std::sync::Arc;
use serde::{Deserialize, Serialize};
use pluot_core::{maybe_timeout, FutureExt, Duration};

use pluot_core::log;
use pluot_core::wgpu;
use pluot_core::zarr::AsyncZarritaStore;
use pluot_core::cache::{get_or_init_store, use_memo_vec_f32, use_memo_vec_i32};
use pluot_core::zarr::is_timed_out_zarrs_error;
use pluot_core::two::svg::{update_svg, SvgContext};
use pluot_core::render_traits::{DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, PickableLayer, PreparedLayer, ViewParams, AspectRatioMode, UnitsMode, MarginParams};
use pluot_core::layers::point_layer::{PointLayer, PointShapeMode, PointLayerParams};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use pluot_core::render_types::GpuContext;
use pluot_core::LayerPickingResult;
use pluot_core::viewport::DataCoord;
use pluot_core::viewport::ScreenCoord;
use pluot_core::viewport::{get_bounds, camera_matrix_to_zoom_and_translation};



#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct ZarrPointLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,

    pub point_radius_unit_mode_x: UnitsMode,
    pub point_radius_unit_mode_y: UnitsMode,
    pub point_shape_mode: PointShapeMode,
    pub model_matrix: Option<[f32; 16]>, // Column-major 4x4 matrix

    pub point_radius: Option<f32>, // None means automatically-determine
    pub point_opacity: Option<f32>, // None means automatically-determine

    // Data keys
    pub store_name: Option<String>,
    pub x_key: String,
    pub y_key: String,
    pub color_key: Option<String>,
}

impl Default for ZarrPointLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            point_radius: Some(1.0),
            point_radius_unit_mode_x: UnitsMode::Pixels,
            point_radius_unit_mode_y: UnitsMode::Pixels,
            point_shape_mode: PointShapeMode::Circle,
            model_matrix: None,
            point_opacity: Some(1.0),
            store_name: None,
            x_key: "".to_string(),
            y_key: "".to_string(),
            color_key: None,
        }
    }
}

pub struct ZarrPointLayerData {
    pub x_arr: Arc<Vec<f32>>,
    pub y_arr: Arc<Vec<f32>>,
    pub labels_arr: Arc<Vec<i32>>,
}

pub struct ZarrPointLayer {
    view_params: ViewParams,
    layer_params: ZarrPointLayerParams,
    // TODO: do we want the store or just the store_name here?
    store: Arc<AsyncZarritaStore>,
    store_name: String,

    /// The inner BarPlotLayer, constructed during `prepare()`.
    inner: Option<PointLayer>,
}

impl ZarrPointLayer {
    pub fn new(
        view_params: ViewParams,
        layer_params: ZarrPointLayerParams,
    ) -> Self {
        // Error if point_radius_unit_mode is "data" when data_unit_mode is "pixels".
        if layer_params.point_radius_unit_mode_x == UnitsMode::Data && layer_params.data_unit_mode_x == UnitsMode::Pixels {
            panic!("point_radius_unit_mode cannot be 'data' when data_unit_mode is 'pixels'");
        }
        if layer_params.point_radius_unit_mode_y == UnitsMode::Data && layer_params.data_unit_mode_y == UnitsMode::Pixels {
            panic!("point_radius_unit_mode cannot be 'data' when data_unit_mode is 'pixels'");
        }
        // If store_name is None, use the store name from view_params.
        let store_name = match &layer_params.store_name {
            Some(layer_store_name) => layer_store_name.clone(),
            None => {
                match &view_params.store_name {
                    Some(view_store_name) => view_store_name.clone(),
                    None => panic!("store_name must be specified either in layer_params or view_params for Zarr-based layers."),
                }
            }
        };

        let store = get_or_init_store(&store_name, view_params.wait_for_store_gets);
        Self {
            view_params,
            layer_params,
            store,
            store_name,
            inner: None,
        }
    }
}

// Port of the dynamic point-scale / density-opacity model from regl-scatterplot:
// https://github.com/flekschas/regl-scatterplot/blob/master/src/index.js
const MIN_POINT_SIZE_IN_PX: f32 = 1.0;
const DEFAULT_POINT_SIZE_IN_PX: f32 = 6.0;
const OPACITY_BY_DENSITY_FILL: f32 = 0.15;
// Lowest opacity that still renders something in 8-bit (u8) alpha output
const MIN_DENSITY_OPACITY: f32 = 1.01 / 255.0;

/// Port of `getAsinhPointScale` (the default `pointScaleMode = 'asinh'`).
fn get_asinh_point_scale(scaling: f32, base_point_size: f32) -> f32 {
    let min_point_scale = MIN_POINT_SIZE_IN_PX / base_point_size;
    if scaling > 1.0 {
        scaling.max(1.0).asinh() / 1.0_f32.asinh()
    } else {
        min_point_scale.max(scaling)
    }
}

/// Port of `getOpacityDensity` (`opacityBy = 'density'`).
///
/// Lowers opacity in dense regions so overlapping points remain legible, taking
/// the points currently in view into account so sparse areas stay opaque.
/// `p` is the rendered point **diameter** in device pixels (matching regl-scatterplot's
/// `pointSize[0] * pointScale`).
fn get_opacity_density(
    p: f32,
    s: f32,
    width: f32,
    height: f32,
    num_points_in_view: usize,
    render_points_as_squares: bool,
) -> f32 {
    let n = num_points_in_view.max(1) as f32;
    let p = p.max(f32::EPSILON);

    let mut alpha = ((OPACITY_BY_DENSITY_FILL * width * height) / (n * p * p)) * s.min(1.0);

    // Circles only take up (pi r^2) of the unit square.
    if !render_points_as_squares {
        alpha *= 1.0 / (0.25 * std::f32::consts::PI);
    }

    // If the points shrink below the minimum permitted size, compensate via
    // opacity (the size is clamped during rendering). The +0.5 accounts for the
    // slight size increase used for SDF-style antialiasing. Squared because we
    // care about the ratio of areas.
    let clamped_point_device_size = MIN_POINT_SIZE_IN_PX.max(p) + 0.5;
    alpha *= (p / clamped_point_device_size).powi(2);

    // Clamp to [MIN_DENSITY_OPACITY, 1.0] rather than [0, 1] so the plot never fades to
    // fully transparent (regl-scatterplot's documented "never render nothing" low-end clamp).
    alpha.clamp(MIN_DENSITY_OPACITY, 1.0)
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for ZarrPointLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let store = self.store.clone();

        // TODO: include the layer type in the memoization dependencies?
        // But what if we want multiple layers to be able to reuse the same cached data?
        // Then we should also avoid including the layer_id...
        let l_i32_future_deps = vec!["l_bytes".to_string(), self.store_name.clone(), self.layer_params.layer_id.to_string()];
        let l_i32_future = use_memo_vec_i32(async || {
            let labels_array_path = &self.layer_params.color_key.as_ref().expect("Color key");
            let labels_array_future = zarrs::array::Array::async_open(store.clone(), labels_array_path);
            let labels_array = labels_array_future.await.unwrap();
            let labels_subset = labels_array.subset_all();
            let labels_vec = labels_array.async_retrieve_array_subset::<Vec<i64>>(&labels_subset).await?;
            // Convert to i32
            let labels_i32: Vec<i32> = labels_vec.iter().map(|&c| c as i32).collect();
            Ok(labels_i32)
        }, &l_i32_future_deps, self.view_params.cache_enabled);

        // TODO: improve the keys / memoization dependencies to at least include the plot_id and store_name.
        let x_f32_future_deps = vec!["x_bytes".to_string(), self.store_name.clone(), self.layer_params.layer_id.to_string()];
        let x_f32_future = use_memo_vec_f32(async || {
            let x_array_path = &self.layer_params.x_key.as_ref();
            let x_array_future = zarrs::array::Array::async_open(store.clone(), x_array_path);
            let x_array = x_array_future.await.unwrap();
            let x_subset = x_array.subset_all();
            let position_x = x_array.async_retrieve_array_subset::<Vec<f64>>(&x_subset).await?;
            let x_f32_inner: Vec<f32> = position_x.iter().map(|&x| x as f32).collect();
            Ok(x_f32_inner)
        }, &x_f32_future_deps, self.view_params.cache_enabled);

        let y_f32_future_deps = vec!["y_bytes".to_string(), self.store_name.clone(), self.layer_params.layer_id.to_string()];
        let y_f32_future = use_memo_vec_f32(async || {
            let y_array_path = &self.layer_params.y_key.as_ref();
            let y_array_future = zarrs::array::Array::async_open(store.clone(), y_array_path);
            let y_array = y_array_future.await.unwrap();
            let y_subset = y_array.subset_all();
            let position_y = y_array.async_retrieve_array_subset::<Vec<f64>>(&y_subset).await?;
            let y_f32_inner: Vec<f32> = position_y.iter().map(|&y| y as f32).collect();
            Ok(y_f32_inner)
        }, &y_f32_future_deps, self.view_params.cache_enabled);

        // Await in parallel: Use futures::join, similar to Promise.all in JS.
        //let (x_f32, y_f32, l_i32) = futures::join!(x_f32_future, y_f32_future, l_i32_future);

        let futures_try_join_result = futures::try_join!(
            maybe_timeout!(x_f32_future, self.view_params.timeout),
            maybe_timeout!(y_f32_future, self.view_params.timeout),
            maybe_timeout!(l_i32_future, self.view_params.timeout),
        );

        // TODO: load image data as vec of individual chunks (rather than requesting the full slice)
        // to allow for progressive rendering of large images as the chunks load.
        // We want to render the chunks that have loaded prior to the timeout (if there was a timeout specified).
        // First convert the requested slice to the chunk keys?

        let (x_f32, y_f32, l_i32) = match futures_try_join_result {
            Ok((x_f32_result, y_f32_result, l_i32_result)) => {
                // Each result is Result<Arc<Vec<_>>, ArrayError> from the _result cache fns.
                match (x_f32_result, y_f32_result, l_i32_result) {
                    (Ok(x), Ok(y), Ok(l)) => (x, y, l),
                    (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
                        if is_timed_out_zarrs_error(&e) {
                            // TODO: still render something in this case?
                            return PrepareResult { bailed_early: true };
                        } else {
                            panic!("Zarrs error during ZarrPointLayer prepare: {:?}", e);
                        }
                    }
                }
            }
            Err(_) => {
                // Wall-clock timeout from maybe_timeout!
                return PrepareResult { bailed_early: true };
            }
        };

        // Resolve automatically-determined (None) point_radius / point_opacity values
        // using the regl-scatterplot point-scale + density-opacity model.
        let (point_radius, point_opacity) = {
            let auto_radius = self.layer_params.point_radius.is_none();
            let auto_opacity = self.layer_params.point_opacity.is_none();

            if !auto_radius && !auto_opacity {
                (self.layer_params.point_radius.unwrap(), self.layer_params.point_opacity.unwrap())
            } else {
                let dpr = self.view_params.device_pixel_ratio;

                // Visible extent (camera + aspect ratio + margins applied), in the
                // normalized [0, 1] space that the camera and vertex shader operate in —
                // the pluot equivalent of deck/regl's getBounds.
                let visible = get_bounds(&self.view_params);

                let (zoom_x, zoom_y, _translate_x, _translate_y) = camera_matrix_to_zoom_and_translation(self.view_params.camera_view);

                // Plot area in CSS pixels (viewport minus margins, matching get_bounds);
                // pluot's layer_size / Pixels-mode point_radius are CSS pixels.
                let (margin_top, margin_right, margin_bottom, margin_left) = match &self.view_params.margins {
                    Some(m) => (
                        m.margin_top.unwrap_or(0.0),
                        m.margin_right.unwrap_or(0.0),
                        m.margin_bottom.unwrap_or(0.0),
                        m.margin_left.unwrap_or(0.0),
                    ),
                    None => (0.0, 0.0, 0.0, 0.0),
                };

                let viewport_w = self.view_params.width as f32;
                let viewport_h = self.view_params.height as f32;

                let layer_w = viewport_w - (margin_left + margin_right) as f32;
                let layer_h = viewport_h - (margin_top + margin_bottom) as f32;
                let layer_aspect_ratio = layer_w / layer_h;

                let radius_is_data = self.layer_params.point_radius_unit_mode_x == UnitsMode::Data;

                // The number of pixels that correspond to the size (width/height) of the unit square.
                let px_per_data_unit = match &self.view_params.aspect_ratio_mode {
                    AspectRatioMode::Ignore => layer_w.min(layer_h),
                    AspectRatioMode::Contain => {
                        if layer_aspect_ratio > 1.0 {
                            // Wide
                            layer_h
                        } else {
                            // Tall or square
                            layer_w
                        }
                    }
                    AspectRatioMode::Cover => {
                        if layer_aspect_ratio > 1.0 {
                            // Wide
                            layer_w
                        } else {
                            // Tall or square
                            layer_h
                        }
                    }
                };

                // On-screen CSS pixels spanned by one data unit at the current zoom. A
                // data-unit length is magnified by the camera, so unlike px_per_data_unit
                // (camera-independent) this includes zoom_x — matching the vertex shader,
                // which pushes the radius through the camera transform.
                let css_px_per_data_unit = (px_per_data_unit * zoom_x).max(f32::EPSILON);

                let min_point_size_data_units = MIN_POINT_SIZE_IN_PX / css_px_per_data_unit;

                // Auto point size from the regl asinh model, in CSS pixels.
                let auto_radius_px = (DEFAULT_POINT_SIZE_IN_PX * get_asinh_point_scale(zoom_x, DEFAULT_POINT_SIZE_IN_PX)) / 2.0;

                // Resolve point_radius in the layer's configured unit mode. A user-provided
                // radius is used as-is; an auto radius (CSS pixels) is converted to data units
                // when the radius unit mode is Data — dividing by css_px_per_data_unit (not just
                // px_per_data_unit) so the rendered point is auto_radius_px on screen regardless
                // of zoom, matching regl-scatterplot's screen-anchored, asinh-scaled points.
                let point_radius = match self.layer_params.point_radius {
                    Some(radius) => radius,
                    None => if radius_is_data {
                        (auto_radius_px / css_px_per_data_unit).max(min_point_size_data_units * 2.0)
                    } else { auto_radius_px },
                };

                let point_opacity = match self.layer_params.point_opacity {
                    Some(opacity) => opacity,
                    None => {
                        // numPointsInView: count points inside the current view bounds.
                        // Positions are mapped through the layer's model_matrix into the same
                        // normalized [0, 1] space as `visible` (mirroring the vertex shader,
                        // which applies model_matrix before the camera). Identity when None.
                        let model_matrix = self.layer_params.model_matrix;
                        let to_norm = |x: f32, y: f32| -> (f32, f32) {
                            match model_matrix {
                                // Column-major 4x4; (x, y, 0, 1) affine transform.
                                Some(m) => (
                                    m[0] * x + m[4] * y + m[12],
                                    m[1] * x + m[5] * y + m[13],
                                ),
                                None => (x, y),
                            }
                        };
                        let num_points_in_view = x_f32
                            .iter()
                            .zip(y_f32.iter())
                            .filter(|(x, y)| {
                                let (nx, ny) = to_norm(**x, **y);
                                nx >= visible.x_min && nx <= visible.x_max
                                    && ny >= visible.y_min && ny <= visible.y_max
                            })
                            .count();

                        // The density formula needs the rendered point size and the plot area
                        // in the same units. Convert both to device pixels (a Data-unit radius
                        // first to CSS pixels via css_px_per_data_unit — which includes zoom, so
                        // it matches the size actually rendered — then * dpr).
                        let point_radius_css = if radius_is_data { point_radius * css_px_per_data_unit } else { point_radius };
                        let point_radius_device = point_radius_css * dpr;

                        // Compute the plot's x and y range from the view matrix, though these could come from any source
                        // const s = (2 / (2 / camera.view[0])) * (2 / (2 / camera.view[5]));
                        // Reference: https://github.com/flekschas/regl-scatterplot/blob/fbf3204762643f2fe9c432f83f342ab78843ba6d/src/index.js#L1753-L1754
                        let s = (2.0 / (2.0 / zoom_x)) * (2.0 / (2.0 / zoom_y));

                        let render_points_as_squares = self.layer_params.point_shape_mode == PointShapeMode::Square;

                        // getOpacityDensity uses `p` as diameter (matching regl-scatterplot's
                        // `pointSize[0] * pointScale`). pluot stores radii, so multiply by 2.
                        get_opacity_density(
                            point_radius_device * 2.0,
                            s,
                            layer_w * dpr,
                            layer_h * dpr,
                            num_points_in_view,
                            render_points_as_squares,
                        )
                    }
                };
                (point_radius, point_opacity)
            }
        };

        let mut sublayer = PointLayer::new(
            self.view_params.clone(),
            PointLayerParams {
                layer_id: self.layer_params.layer_id.clone(),
                bounds: self.layer_params.bounds.clone(),
                data_unit_mode_x: self.layer_params.data_unit_mode_x,
                data_unit_mode_y: self.layer_params.data_unit_mode_y,
                point_radius,
                point_radius_unit_mode_x: self.layer_params.point_radius_unit_mode_x,
                point_radius_unit_mode_y: self.layer_params.point_radius_unit_mode_y,
                point_shape_mode: self.layer_params.point_shape_mode,
                point_opacity,
                model_matrix: self.layer_params.model_matrix,
                position_x: x_f32.clone(),
                position_y: y_f32.clone(),
                labels_vec: l_i32.clone(),
            }
        );
        sublayer.prepare(gpu_context).await;
        self.inner = Some(sublayer);

        return PrepareResult {
            bailed_early: false,
        };
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for ZarrPointLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        if let Some(inner) = &self.inner {
            DrawToRasterGpu::draw(inner, gpu_context, pass).await;
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for ZarrPointLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for ZarrPointLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        if let Some(inner) = &self.inner {
            DrawToSvg::draw(inner, ctx).await
        }
    }
}

impl PickableLayer for ZarrPointLayer {
    fn pick(&self, screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let DataCoord::TwoD { x: cx, y: cy } = data_coord? else {
            return None;
        };

        if let Some(inner) = &self.inner {
            return PickableLayer::pick(inner, screen_coord, data_coord);
        }
        return None;
    }
}
