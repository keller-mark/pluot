#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    RenderParams, LayerParams,
    Point3dLayerParams, PointShapeMode,
    CategoricalColormap, CategoricalParams, CategoricalCustomParams, ColorMode,
    QuantitativeParams, QuantitativeColormap,
    NumericData,
};

// Point3dLayer has no aspect-ratio/data-unit-mode handling and no SVG support
// (3D rendering is raster-only), so these tests are simpler than the 2D
// primitive layer tests: one canvas size, one camera, raster snapshot only.
// They exist primarily to exercise `fill_color: ColorMode` end-to-end.

// Helper: 4 points arranged in a square, 5 units in front of the (identity) camera.
fn corner_points_3d() -> Point3dLayerParams {
    Point3dLayerParams {
        layer_id: "my_point_3d_layer".to_string(),
        bounds: None,
        point_radius: 6.0,
        point_shape_mode: PointShapeMode::Square,
        position_x: NumericData::Float32(Arc::new(vec![-1.0, 1.0, 1.0, -1.0])),
        position_y: NumericData::Float32(Arc::new(vec![-1.0, -1.0, 1.0, 1.0])),
        position_z: NumericData::Float32(Arc::new(vec![-5.0, -5.0, -5.0, -5.0])),
        ..Default::default()
    }
}

fn layer_params(point_params: Point3dLayerParams) -> Vec<LayerParams> {
    vec![LayerParams::Point3dLayer(point_params)]
}

#[tokio::test]
async fn test_point_3d_layer_default_uniform_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_points_3d()),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_3d_layer_default_uniform_color").await;
}

#[tokio::test]
async fn test_point_3d_layer_categorical_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(Point3dLayerParams {
            fill_color: Some(ColorMode::Categorical(CategoricalParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                colormap: CategoricalColormap::Tableau10,
            })),
            ..corner_points_3d()
        }),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_3d_layer_categorical_color").await;
}

#[tokio::test]
async fn test_point_3d_layer_categorical_custom_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(Point3dLayerParams {
            fill_color: Some(ColorMode::CategoricalCustom(CategoricalCustomParams {
                values: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                colormap: vec![
                    (255, 0, 0),
                    (0, 200, 0),
                    (0, 0, 255),
                    (200, 200, 0),
                ],
            })),
            ..corner_points_3d()
        }),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_3d_layer_categorical_custom_color").await;
}

#[tokio::test]
async fn test_point_3d_layer_quantitative_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(Point3dLayerParams {
            fill_color: Some(ColorMode::Quantitative(QuantitativeParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 0.33, 0.67, 1.0])),
                colormap: QuantitativeColormap::Viridis,
                reverse: false,
                domain: None,
            })),
            ..corner_points_3d()
        }),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_3d_layer_quantitative_color").await;
}
