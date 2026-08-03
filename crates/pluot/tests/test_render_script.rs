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
    CodeFormat,
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
        schema_version: None,
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
        ],
        ..Default::default()
    }
}

#[tokio::test]
async fn test_render_json() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::Vector),
        CodeFormat::Json,
        "test_render_code_json.json",
    )
    .await;
}

// ── Python ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_render_expression_python() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::Vector),
        CodeFormat::ExpressionPython,
        "test_render_code_expression.py",
    )
    .await;
}

#[tokio::test]
async fn test_render_script_python() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::Raster),
        CodeFormat::ScriptPython,
        "test_render_code_script.py",
    )
    .await;
}

// ── R ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_render_expression_r() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::Vector),
        CodeFormat::ExpressionR,
        "test_render_code_expression.R",
    )
    .await;
}

#[tokio::test]
async fn test_render_script_r() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::Raster),
        CodeFormat::ScriptR,
        "test_render_code_script.R",
    )
    .await;
}

// ── JavaScript ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_render_expression_js() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::Vector),
        CodeFormat::ExpressionJs,
        "test_render_code_expression.js",
    )
    .await;
}

#[tokio::test]
async fn test_render_script_js() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::Raster),
        CodeFormat::ScriptJs,
        "test_render_code_script.js",
    )
    .await;
}

// ── JSX / React / HTML ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_render_expression_jsx() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::Vector),
        CodeFormat::ExpressionJsx,
        "test_render_code_expression.jsx",
    )
    .await;
}

#[tokio::test]
async fn test_render_script_react() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::Raster),
        CodeFormat::ScriptReact,
        "test_render_code_script_react.jsx",
    )
    .await;
}

#[tokio::test]
async fn test_render_script_html() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::Vector),
        CodeFormat::ScriptHtml,
        "test_render_code_script.html",
    )
    .await;
}

#[tokio::test]
async fn test_render_script_react_html() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::Vector),
        CodeFormat::ScriptHtmlReact,
        "test_render_code_script_react.html",
    )
    .await;
}

// ── Rust ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_render_expression_rust() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::Vector),
        CodeFormat::ExpressionRust,
        "test_render_code_expression.rs.txt",
    )
    .await;
}

#[tokio::test]
async fn test_render_script_rust() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::Raster),
        CodeFormat::ScriptRust,
        "test_render_code_script.rs.txt",
    )
    .await;
}

// ── Bash ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_render_script_bash() {
    render_and_check_script_snapshot(
        sample_params(GraphicsFormat::Raster),
        CodeFormat::ScriptBash,
        "test_render_code_script.sh",
    )
    .await;
}
