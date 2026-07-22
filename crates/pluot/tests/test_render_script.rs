// Test rendering to the code-based GraphicsFormats (Expression*, Script*, Json).
// These use snapshot testing, writing dirty values to snaps-dirty and checking
// against blessed files in snaps-blessed, similar to the existing PNG/SVG
// snapshot tests in this directory.
//
// The `Expression*` formats emit a single expression (a function call or JSX
// element); the `Script*` formats emit a self-contained script with imports.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;
use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_script_snapshot;

use pluot::{
    RenderParams, LayerParams, GraphicsFormat,
    PointLayerParams, PointShapeMode,
    AxisLinearLayerParams, AxisPosition,
    CategoricalColormap, CategoricalParams, ColorMode,
    SizeMode, UnitsMode,
    NumericData,
    ZarrStoreInfo, ZarrStoreParams, HttpStoreParams,
};

// A representative plot exercising the interesting parts of the serializer:
// nested layers, string enums, numeric-data arrays, a camera matrix, and an
// optional margin (with the other margins left as `None`). Only `format`
// varies between the per-language tests.
fn sample_params(format: GraphicsFormat) -> RenderParams {
    RenderParams {
        width: 640,
        height: 480,
        format,
        plot_id: "plot_1".to_string(),
        stores: Some(HashMap::from([(
            "my_store".to_string(),
            ZarrStoreInfo {
                store_params: ZarrStoreParams::HttpStore(HttpStoreParams {
                    url: "https://example.com/my_store.zarr".to_string(),
                    options: None,
                }),
                store_extensions: None,
            },
        )])),
        camera_view: Some([
            0.15, 0.0, 0.0, 0.0,
            0.0, 0.15, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]),
        margin_left: Some(60.0),
        layers: vec![
            LayerParams::PointLayer(PointLayerParams {
                layer_id: "pts".to_string(),
                data_unit_mode_x: UnitsMode::Data,
                data_unit_mode_y: UnitsMode::Data,
                point_shape_mode: PointShapeMode::Circle,
                point_radius: Some(SizeMode::UniformSize(5.0)),
                position_x: NumericData::Float32(Arc::new(vec![0.0, 1.0, 1.0, 0.0])),
                position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 1.0, 1.0])),
                fill_color: Some(ColorMode::Categorical(CategoricalParams {
                    codes: NumericData::Uint8(Arc::new(vec![0, 1, 2, 3])),
                    colormap: CategoricalColormap::Tableau10,
                })),
                ..Default::default()
            }),
            LayerParams::AxisLinearLayer(AxisLinearLayerParams {
                layer_id: "left_axis".to_string(),
                position: AxisPosition::Left,
            }),
        ],
        ..Default::default()
    }
}

#[tokio::test]
async fn test_render_json() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::Json),
        "test_render.json",
    )
    .await;
}

// ── Python ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_render_expression_python() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::ExpressionPython),
        "test_render_expression.py",
    )
    .await;
}

#[tokio::test]
async fn test_render_script_python() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::ScriptPython),
        "test_render_script.py",
    )
    .await;
}

// ── R ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_render_expression_r() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::ExpressionR),
        "test_render_expression.R",
    )
    .await;
}

#[tokio::test]
async fn test_render_script_r() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::ScriptR),
        "test_render_script.R",
    )
    .await;
}

// ── JavaScript ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_render_expression_js() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::ExpressionJs),
        "test_render_expression.js",
    )
    .await;
}

#[tokio::test]
async fn test_render_script_js() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::ScriptJs),
        "test_render_script.js",
    )
    .await;
}

// ── JSX / React / HTML ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_render_expression_jsx() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::ExpressionJsx),
        "test_render_expression.jsx",
    )
    .await;
}

#[tokio::test]
async fn test_render_script_react() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::ScriptReact),
        "test_render_script_react.jsx",
    )
    .await;
}

#[tokio::test]
async fn test_render_script_html() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::ScriptHtml),
        "test_render_script.html",
    )
    .await;
}

// ── Rust ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_render_expression_rust() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::ExpressionRust),
        "test_render_expression.rs.txt",
    )
    .await;
}

#[tokio::test]
async fn test_render_script_rust() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::ScriptRust),
        "test_render_script.rs.txt",
    )
    .await;
}

// ── Bash ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_render_script_bash() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::ScriptBash),
        "test_render_script.sh",
    )
    .await;
}
