#![cfg(all(test, not(target_arch = "wasm32"), not(feature = "lacks_gpu")))]

use std::sync::Arc;

use pluot_core::layers::heatmap_layer::HeatmapColormap;
use pluot_core::layers::hilbert_heatmap_layer::{grid_dimensions, HilbertHeatmapLayer, HilbertHeatmapLayerParams, HilbertHeatmapPickingInfo};
use pluot_core::numeric_data::{NumericData, NumericValue};
use pluot_core::params::{GraphicsFormat, LayerParams, LayeredPlotRenderParams, PlotParams, RenderBackend, RenderParams};
use pluot_core::render_traits::{BooleanCriteriaParams, EmphasisCriteria, MarginParams, PickableLayer, ViewParams};
use pluot_core::viewport::ScreenCoord;

const WIDTH: u32 = 37;
const HEIGHT: u32 = 23;
const MARGIN_LEFT: u32 = 5;
const MARGIN_TOP: u32 = 3;
const LAYER_WIDTH: u32 = WIDTH - MARGIN_LEFT - 2;
const LAYER_HEIGHT: u32 = HEIGHT - MARGIN_TOP - 4;
const NUM_ITEMS: usize = 50;

fn bounds() -> MarginParams {
    MarginParams {
        margin_left: Some(MARGIN_LEFT as f32),
        margin_top: Some(MARGIN_TOP as f32),
        margin_right: Some(2.0),
        margin_bottom: Some(4.0),
    }
}

/// Items listed in reverse, each colored by its own index, where items divisible by 7 are
/// filtered out and odd items are unselected.
fn layer_params() -> HilbertHeatmapLayerParams {
    let ordered_indices: Vec<u32> = (0..NUM_ITEMS as u32).rev().collect();
    let codes: Vec<i16> = (0..NUM_ITEMS as i16).collect();
    HilbertHeatmapLayerParams {
        layer_id: "hilbert".to_string(),
        bounds: Some(bounds()),
        ordered_indices: NumericData::from(ordered_indices),
        values: NumericData::from(codes),
        colormap: Some(HeatmapColormap::CategoricalCustom((0..NUM_ITEMS).map(|i| (i as u8 * 5, 255 - i as u8 * 5, 7)).collect())),
        filtering_criteria: vec![EmphasisCriteria::Boolean(BooleanCriteriaParams {
            mask: NumericData::from((0..NUM_ITEMS).map(|i| (i % 7 != 0) as u8).collect::<Vec<u8>>()),
        })],
        selection_criteria: vec![EmphasisCriteria::Boolean(BooleanCriteriaParams {
            mask: NumericData::from((0..NUM_ITEMS).map(|i| (i % 2 == 0) as u8).collect::<Vec<u8>>()),
        })],
        background_opacity: Some(0.5),
        ..Default::default()
    }
}

async fn render_rgba(params: &HilbertHeatmapLayerParams) -> Vec<u8> {
    let render_params = RenderParams {
        width: WIDTH,
        height: HEIGHT,
        format: GraphicsFormat::Raster,
        render_backend: Some(RenderBackend::Gpu),
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: vec![LayerParams {
                layer_type: "HilbertHeatmapLayer".to_string(),
                layer_params: serde_json::to_value(params).unwrap(),
            }],
        }),
        ..Default::default()
    };
    pluot_core::render(render_params, None).await
}

#[tokio::test]
async fn gpu_cells_match_cpu_picking() {
    let params = layer_params();
    let rgba = render_rgba(&params).await;
    let layer = HilbertHeatmapLayer::new(ViewParams { width: WIDTH, height: HEIGHT, ..Default::default() }, params);
    let grid = layer.grid();
    assert_eq!((grid.width, grid.height, grid.cell_size()), (10, 5, (3.0, 3.2)), "50 items fill the 30x16 bounds as a 10x5 grid");
    let HeatmapColormap::CategoricalCustom(colors) = layer_params().colormap.unwrap() else { unreachable!() };

    let mut items_seen = std::collections::BTreeSet::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let pixel = &rgba[((y * WIDTH + x) * 4) as usize..][..4];
            let picked = layer.pick(ScreenCoord { x: x as f32 + 0.5, y: (HEIGHT - y) as f32 - 0.5 }, None);
            let Some(picked) = picked else {
                assert_eq!(pixel[3], 0, "pixel ({x}, {y}) is empty on the CPU but drawn on the GPU: {pixel:?}");
                continue;
            };
            let info: HilbertHeatmapPickingInfo = picked.into_info().unwrap();
            assert_eq!(info.value, NumericValue::Int(info.index as i64));
            assert!(info.index % 7 != 0);
            let (r, g, b) = colors[info.index];
            let expected_alpha = if info.is_selected { 255 } else { 128 };
            // Un-premultiplying semi-transparent pixels can round each channel by one.
            let color_matches = pixel[..3].iter().zip([r, g, b]).all(|(&actual, expected)| actual.abs_diff(expected) <= 1);
            assert!(color_matches, "pixel ({x}, {y}) is {pixel:?} but should show item {} as {:?}", info.index, (r, g, b));
            assert!(pixel[3].abs_diff(expected_alpha) <= 1, "pixel ({x}, {y}) alpha {} for item {}", pixel[3], info.index);
            items_seen.insert(info.index);
        }
    }
    let expected_items: std::collections::BTreeSet<usize> = (0..NUM_ITEMS).filter(|i| i % 7 != 0).collect();
    assert_eq!(items_seen, expected_items);
}

#[tokio::test]
async fn each_item_takes_one_cell_in_order_until_the_curve_ends() {
    let layer = HilbertHeatmapLayer::new(ViewParams { width: WIDTH, height: HEIGHT, ..Default::default() }, HilbertHeatmapLayerParams {
        bounds: Some(bounds()),
        ordered_indices: NumericData::from((0..NUM_ITEMS as u32).collect::<Vec<u32>>()),
        values: NumericData::Float32(Arc::new((0..NUM_ITEMS).map(|i| i as f32).collect())),
        ..Default::default()
    });
    let grid = layer.grid();
    let (cell_width, cell_height) = grid.cell_size();

    let mut by_position = Vec::new();
    for y in 0..grid.height {
        for x in 0..grid.width {
            let screen_coord = ScreenCoord {
                x: MARGIN_LEFT as f32 + (x as f32 + 0.5) * cell_width,
                y: (HEIGHT - MARGIN_TOP) as f32 - (y as f32 + 0.5) * cell_height,
            };
            let Some(picked) = layer.pick(screen_coord, None) else { continue };
            let info: HilbertHeatmapPickingInfo = picked.into_info().unwrap();
            by_position.push((info.curve_position, info.index));
        }
    }
    by_position.sort();
    let expected: Vec<(u32, usize)> = (0..NUM_ITEMS).map(|i| (i as u32, i)).collect();
    assert_eq!(by_position, expected);
}

#[test]
fn the_grid_follows_the_aspect_ratio_with_a_cell_per_item() {
    let square: Vec<(u32, u32)> = [0, 1, 2, 10, 49, 50, 57].into_iter().map(|n| grid_dimensions(n, 1.0)).collect();
    assert_eq!(square, vec![(0, 0), (1, 1), (1, 2), (3, 4), (7, 7), (7, 8), (8, 8)]);
    assert_eq!(grid_dimensions(800, 2.0), (40, 20));
    assert_eq!(grid_dimensions(800, 0.5), (20, 40));
    assert_eq!(grid_dimensions(3, 100.0), (3, 1));
    assert_eq!(grid_dimensions(3, 0.0), (0, 0));
}

#[test]
fn the_curve_ends_less_than_a_row_short_of_the_full_grid() {
    for n in 1..500 {
        for aspect_ratio in [0.25, 0.7, 1.0, 1.875, 4.0] {
            let (width, height) = grid_dimensions(n, aspect_ratio);
            assert!((width * height) as usize >= n && ((width * height) as usize - n) < width as usize, "{n} items, {aspect_ratio}");
        }
    }
}
