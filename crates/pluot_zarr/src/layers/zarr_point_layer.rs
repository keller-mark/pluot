use std::sync::Arc;
use serde::{Deserialize, Serialize};
use pluot_core::{maybe_timeout, FutureExt, Duration};

use pluot_core::log;
use pluot_core::wgpu;
use pluot_core::zarr::AsyncZarritaStore;
use pluot_core::cache::{get_or_init_store, use_memo_vec_f32, use_memo_vec_i32};
use pluot_core::compute::reduce::reduce_extent;
use pluot_core::zarr::is_timed_out_zarrs_error;
use pluot_core::two::svg::{update_svg, SvgContext};
use pluot_core::render_traits::{DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, PickableLayer, PreparedLayer, ViewParams, AspectRatioMode, UnitsMode, MarginParams};
use pluot_core::layers::point_layer::{PointLayer, PointShapeMode, PointLayerParams};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use pluot_core::render_types::GpuContext;
use pluot_core::LayerPickingResult;
use pluot_core::viewport::DataCoord;
use pluot_core::viewport::ScreenCoord;
use pluot_core::viewport::get_bounds;



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

        let store = get_or_init_store(&store_name, view_params.wait_for_store_gets, view_params.wait_for_store_pushes);
        Self {
            view_params,
            layer_params,
            store,
            store_name,
            inner: None,
        }
    }
}

// Port of the dynamic point-size / opacity heuristics from vitessce:
// https://github.com/vitessce/vitessce/blob/main/packages/view-types/scatterplot/src/shared-spatial-scatterplot/dynamic-opacity.js
const BASE_POINT_SIZE: f32 = 5.0;
const LARGE_DATASET_COUNT: f32 = 10000.0;
const SMALL_DATASET_COUNT: f32 = 100.0;

/// Port of `getInitialPointSize`: the point size (in data/axis units) decreases
/// as the number of points grows, to mitigate overplotting. Ranges from 0.05
/// (<= 100 points) down to 0.0005 (>= 10000 points).
fn get_initial_point_size(num_points: usize) -> f32 {
    BASE_POINT_SIZE / (num_points as f32).clamp(SMALL_DATASET_COUNT, LARGE_DATASET_COUNT)
}

/// Port of `getPointSizeDevicePixels`: converts the axis-space initial point size
/// into device pixels, given the data extent (`x_range`/`y_range`) and the
/// currently-visible extent (`visible_x`/`visible_y`, from `get_bounds`).
///
/// deck.gl computes `(xRange * 2**zoom) / width` — the fraction of the viewport
/// the data spans. In pluot that fraction is `x_range / visible_x`.
fn get_point_size_device_pixels(
    device_pixel_ratio: f32,
    x_range: f32,
    y_range: f32,
    visible_x: f32,
    visible_y: f32,
    width: f32,
    height: f32,
    num_points: usize,
) -> f32 {
    let point_size = get_initial_point_size(num_points);

    // Point size bounds, in screen pixels.
    let point_screen_size_max = 10.0;
    let point_screen_size_min = 2.0 / device_pixel_ratio;

    let x_axis_range = 2.0 / (x_range / visible_x.max(f32::EPSILON));
    let y_axis_range = 2.0 / (y_range / visible_y.max(f32::EPSILON));

    // The diagonal screen size as a fraction of the current diagonal axis range,
    // then converted to device pixels.
    let diagonal_screen_size = (width * width + height * height).sqrt();
    let diagonal_axis_range = (x_axis_range * x_axis_range + y_axis_range * y_axis_range).sqrt();
    let diagonal_fraction = point_size / diagonal_axis_range.max(f32::EPSILON);
    let device_size = diagonal_fraction * diagonal_screen_size;

    device_size.clamp(point_screen_size_min, point_screen_size_max)
}

/// Port of `getPointOpacity`: lowers opacity for dense point clouds to avoid
/// overplotting. `x_range`/`y_range` are the data extent and `visible_x`/
/// `visible_y` are the visible extent (from `get_bounds`). `width`/`height` are
/// the plot area in pixels.
fn get_point_opacity(
    x_range: f32,
    y_range: f32,
    visible_x: f32,
    visible_y: f32,
    width: f32,
    height: f32,
    num_points: usize,
) -> f32 {
    let n = num_points as f32;

    // deck.gl: X = maxY - minY (visible y span), Y = maxX - minX (visible x span).
    let x = visible_y.max(f32::EPSILON);
    let y = visible_x.max(f32::EPSILON);
    let x0 = x_range;
    let y0 = y_range;
    let w = width;
    let h = height;

    // Average fill density (deck.gl default when none is provided).
    let rho = (1.0 / 10.0_f32.powf(n.log10() - 3.0)).min(1.0);

    // p (the pixel length/width of a point) is 1 for us, so it drops out.
    let alpha = ((rho * w * h) / n) * (y0 / y) * (x0 / x);
    alpha.clamp(2.01 / 255.0, 1.0)
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

        // Resolve automatically-determined (None) point_radius / point_opacity values.
        // When either is None, compute the extent (min/max) of the X and Y positions so we
        // can derive "good" defaults from the data range and the number of points.
        let (point_radius, point_opacity) = {
            let auto_radius = self.layer_params.point_radius.is_none();
            let auto_opacity = self.layer_params.point_opacity.is_none();

            if !auto_radius && !auto_opacity {
                // Both provided: no need to compute the extent.
                (self.layer_params.point_radius.unwrap(), self.layer_params.point_opacity.unwrap())
            } else {
                let x_for_extent = x_f32.clone();
                let y_for_extent = y_f32.clone();

                // Cache the extent so repeated prepares (e.g. on pan/zoom) reuse it.
                // Returns [x_min, x_max, y_min, y_max].
                let extent_future_deps = vec![
                    "point_extent".to_string(),
                    self.store_name.clone(),
                    self.layer_params.layer_id.clone(),
                    self.layer_params.x_key.clone(),
                    self.layer_params.y_key.clone(),
                ];
                let extent = use_memo_vec_f32(async || {
                    let (x_min, x_max) = reduce_extent(gpu_context, x_for_extent).await;
                    let (y_min, y_max) = reduce_extent(gpu_context, y_for_extent).await;
                    Ok::<Vec<f32>, std::convert::Infallible>(vec![x_min, x_max, y_min, y_max])
                }, &extent_future_deps, self.view_params.cache_enabled)
                    .await
                    .expect("Extent computation failed in ZarrPointLayer.prepare");

                let (x_min, x_max, y_min, y_max) = (extent[0], extent[1], extent[2], extent[3]);
                let num_points = x_f32.len();

                // Data extent (in pluot's (0,1) data space for Data unit mode).
                let x_range = (x_max - x_min).abs();
                let y_range = (y_max - y_min).abs();

                // Currently-visible extent (camera + aspect ratio + margins applied),
                // the pluot equivalent of deck.gl's OrthographicView.getBounds().
                let visible = get_bounds(&self.view_params);
                let visible_x = (visible.x_max - visible.x_min).abs();
                let visible_y = (visible.y_max - visible.y_min).abs();

                // Plot area in pixels (viewport minus margins), matching get_bounds.
                let (margin_top, margin_right, margin_bottom, margin_left) = match &self.view_params.margins {
                    Some(m) => (
                        m.margin_top.unwrap_or(0.0),
                        m.margin_right.unwrap_or(0.0),
                        m.margin_bottom.unwrap_or(0.0),
                        m.margin_left.unwrap_or(0.0),
                    ),
                    None => (0.0, 0.0, 0.0, 0.0),
                };
                let layer_w = (self.view_params.width as f32 - (margin_left + margin_right)).max(1.0);
                let layer_h = (self.view_params.height as f32 - (margin_top + margin_bottom)).max(1.0);

                let point_radius = match self.layer_params.point_radius {
                    Some(radius) => radius,
                    None => get_point_size_device_pixels(
                        self.view_params.device_pixel_ratio,
                        x_range,
                        y_range,
                        visible_x,
                        visible_y,
                        layer_w,
                        layer_h,
                        num_points,
                    ),
                };
                let point_opacity = match self.layer_params.point_opacity {
                    Some(opacity) => opacity,
                    None => get_point_opacity(
                        x_range,
                        y_range,
                        visible_x,
                        visible_y,
                        layer_w,
                        layer_h,
                        num_points,
                    ),
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
                // TODO: if point_radius is None, override the point_radius_unit_mode values to always be UnitsMode::Pixels.
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
