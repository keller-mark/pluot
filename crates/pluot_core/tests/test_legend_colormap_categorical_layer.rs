use std::sync::Arc;

use pluot_core::composite_layers::legend_colormap_categorical_layer::{
    LegendColormapCategoricalLayer, LegendColormapCategoricalLayerParams, LegendColormapCategoricalPickingInfo,
};
use pluot_core::render_traits::{MarginParams, PickableLayer, PreparedLayer, ViewParams};
use pluot_core::viewport::ScreenCoord;

const WIDTH: u32 = 200;
const HEIGHT: u32 = 100;
const TITLE_HEIGHT: f32 = 16.0;
const ROW_HEIGHT: f32 = 14.0;

fn categories() -> Arc<Vec<String>> {
    Arc::new((0..10).map(|code| format!("category {code}")).collect())
}

async fn prepared_legend(codes: Option<Vec<u32>>, margin_top: f32) -> LegendColormapCategoricalLayer {
    let mut layer = LegendColormapCategoricalLayer::new(ViewParams { width: WIDTH, height: HEIGHT, ..Default::default() }, LegendColormapCategoricalLayerParams {
        layer_id: "legend".to_string(),
        bounds: Some(MarginParams { margin_left: Some(100.0), margin_right: Some(0.0), margin_top: Some(margin_top), margin_bottom: Some(0.0) }),
        title: "Title".to_string(),
        categories: categories(),
        codes,
        ..Default::default()
    });
    assert!(!layer.prepare(None).await.bailed_early);
    layer
}

fn picked_code(layer: &LegendColormapCategoricalLayer, row: usize) -> Option<u32> {
    let y = HEIGHT as f32 - TITLE_HEIGHT - (row as f32 + 0.5) * ROW_HEIGHT;
    picked_code_at(layer, y)
}

fn picked_code_at(layer: &LegendColormapCategoricalLayer, y: f32) -> Option<u32> {
    let info: LegendColormapCategoricalPickingInfo = layer.pick(ScreenCoord { x: 150.0, y }, None)?.into_info()?;
    assert_eq!(info.category, format!("category {}", info.code));
    Some(info.code)
}

#[tokio::test]
async fn lists_the_given_codes_in_order() {
    let layer = prepared_legend(Some(vec![7, 2, 4]), 0.0).await;
    assert_eq!((0..4).map(|row| picked_code(&layer, row)).collect::<Vec<_>>(), vec![Some(7), Some(2), Some(4), None]);
    assert!(layer.pick(ScreenCoord { x: 50.0, y: HEIGHT as f32 - TITLE_HEIGHT - 7.0 }, None).is_none(), "left of the legend");
}

#[tokio::test]
async fn summarizes_the_rows_that_do_not_fit() {
    // 100 - 16 (title) = 84 pixels fit 6 rows, the last of which summarizes the 5 hidden categories.
    let layer = prepared_legend(None, 0.0).await;
    let picked: Vec<Option<u32>> = (0..7).map(|row| picked_code(&layer, row)).collect();
    assert_eq!(picked, vec![Some(0), Some(1), Some(2), Some(3), Some(4), None, None]);
}

#[tokio::test]
async fn lists_the_given_codes_below_the_top_margin() {
    let layer = prepared_legend(Some(vec![3, 1]), 20.0).await;
    let row_y = |row: f32| HEIGHT as f32 - 20.0 - TITLE_HEIGHT - (row + 0.5) * ROW_HEIGHT;
    assert_eq!([0.0, 1.0, 2.0].map(|row| picked_code_at(&layer, row_y(row))), [Some(3), Some(1), None]);
}
