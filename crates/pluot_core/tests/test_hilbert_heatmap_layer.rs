#![cfg(all(test, not(target_arch = "wasm32"), not(feature = "lacks_gpu")))]

use std::sync::Arc;

use pluot_core::layers::heatmap_layer::HeatmapColormap;
use pluot_core::layers::hilbert_heatmap_layer::{HilbertHeatmapLayer, HilbertHeatmapLayerParams, HilbertHeatmapPickingInfo};
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
    let HeatmapColormap::CategoricalCustom(colors) = layer_params().colormap.unwrap() else { unreachable!() };

    let mut num_drawn = 0;
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
            num_drawn += 1;
            items_seen.insert(info.index);
        }
    }
    let expected_items: std::collections::BTreeSet<usize> = (0..NUM_ITEMS).filter(|i| i % 7 != 0).collect();
    assert_eq!(items_seen, expected_items);
    assert!(num_drawn > (LAYER_WIDTH * LAYER_HEIGHT) as usize / 2);
}

#[tokio::test]
async fn items_fill_the_curve_in_order() {
    let layer = HilbertHeatmapLayer::new(ViewParams { width: WIDTH, height: HEIGHT, ..Default::default() }, HilbertHeatmapLayerParams {
        bounds: Some(bounds()),
        ordered_indices: NumericData::from((0..NUM_ITEMS as u32).collect::<Vec<u32>>()),
        values: NumericData::Float32(Arc::new((0..NUM_ITEMS).map(|i| i as f32).collect())),
        cell_size: Some(2.0),
        ..Default::default()
    });
    let grid = layer.grid();
    assert_eq!((grid.width, grid.height), (LAYER_WIDTH / 2, LAYER_HEIGHT / 2));

    let mut by_position = Vec::new();
    for y in 0..LAYER_HEIGHT / 2 {
        for x in 0..LAYER_WIDTH / 2 {
            let screen_coord = ScreenCoord { x: (MARGIN_LEFT + 2 * x) as f32 + 1.0, y: (HEIGHT - MARGIN_TOP - 2 * y) as f32 - 1.0 };
            let info: HilbertHeatmapPickingInfo = layer.pick(screen_coord, None).unwrap().into_info().unwrap();
            by_position.push((info.curve_position, info.index));
        }
    }
    by_position.sort();
    let positions: Vec<u32> = by_position.iter().map(|&(position, _)| position).collect();
    assert_eq!(positions, (0..grid.len()).collect::<Vec<u32>>());
    let items: Vec<usize> = by_position.iter().map(|&(_, index)| index).collect();
    assert!(items.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!((items[0], items[items.len() - 1]), (0, NUM_ITEMS - 1));
}
