//! Shared helpers for the `test_render_script_integration*.rs` family of
//! tests: the plot definition every generated-code target renders, plus
//! small process-spawning utilities used to execute that generated code in
//! its own language runtime.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pluot::{
    AxisLinearLayerParams, AxisPosition, CategoricalColormap, CategoricalParams, ColorMode,
    GraphicsFormat, LayerParams, NumericData, PointLayerParams, PointShapeMode, RenderParams,
    SizeMode, UnitsMode,
};

/// The shared SVG snapshot every language's executed output is compared
/// against. Produced directly (not via code generation) by
/// `test_render_script_integration_canonical_svg`.
pub const CANONICAL_SVG_SNAPSHOT: &str = "test_render_script_integration.svg";

/// The same point + axis plot used by `test_render_script.rs`'s
/// `sample_params`, minus the (here, unused) Zarr store, with `format` left
/// for the caller to set.
pub fn plot_params(format: GraphicsFormat) -> RenderParams {
    RenderParams {
        width: 640,
        height: 480,
        format,
        plot_id: "plot_1".to_string(),
        camera_view: Some([
            0.15, 0.0, 0.0, 0.0, 0.0, 0.15, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
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

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root should exist")
}

/// A fresh, empty scratch directory under the system temp dir, unique to this
/// test's name and the current process (so parallel test runs don't collide).
pub fn fresh_scratch_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pluot_integration_{test_name}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

pub fn panic_on_failure(label: &str, output: &std::process::Output) {
    if !output.status.success() {
        panic!(
            "{label} exited with {status}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
            status = output.status,
            stdout = String::from_utf8_lossy(&output.stdout),
            stderr = String::from_utf8_lossy(&output.stderr),
        );
    }
}
