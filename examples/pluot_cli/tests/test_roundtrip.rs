// Given a JSON plotting params input,
// render to an SVG or PNG graphics output with `embed_params: true`.
// Then, use the decoding functionality to verify that the embedded JSON representation is correct.

use std::path::PathBuf;
use std::process::{Command, Output};

// A minimal single-layer RenderParams JSON input. `examples/layers.in.json`
// predates the `SizeMode`/`NumericData` serialization refactor and no longer
// deserializes, so this test uses its own known-good fixture instead.
const INPUT_JSON: &str = r#"{
  "plot_type": "LayeredPlot",
  "plot_params": {
    "layers": [
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "layer_2",
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "point_radius_unit_mode_x": "Pixels",
          "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Square",
          "point_radius": { "size_mode": "UniformSize", "size_params": 15.0 },
          "bounds": {
            "margin_top": 0,
            "margin_right": 0,
            "margin_bottom": 0,
            "margin_left": 0
          },
          "position_x": { "dtype": "Float32", "values": [100.0, 100.0] },
          "position_y": { "dtype": "Float32", "values": [100.0, 200.0] }
        }
      }
    ]
  }
}"#;

fn run_cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pluot_cli"))
        .args(args)
        .output()
        .expect("failed to run pluot_cli")
}

/// Render `INPUT_JSON` (via stdin) with `--embed_params`, decode the output
/// file, and assert that the decoded JSON is identical to the input JSON.
fn assert_roundtrip(output_file_name: &str) {
    let output: PathBuf = std::env::temp_dir().join(output_file_name);

    let mut render_cmd = Command::new(env!("CARGO_BIN_EXE_pluot_cli"));
    render_cmd
        .args([
            "--output",
            output.to_str().unwrap(),
            "--width",
            "300",
            "--height",
            "300",
            "--embed-params",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = render_cmd.spawn().expect("failed to spawn pluot_cli");
    {
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(INPUT_JSON.as_bytes())
            .unwrap();
    }
    let render_output = child.wait_with_output().expect("failed to wait on pluot_cli");
    assert!(
        render_output.status.success(),
        "render failed: {}",
        String::from_utf8_lossy(&render_output.stderr)
    );

    let decode_output = run_cli(&["--decode", output.to_str().unwrap()]);
    assert!(
        decode_output.status.success(),
        "decode failed: {}",
        String::from_utf8_lossy(&decode_output.stderr)
    );

    let decoded_json: serde_json::Value =
        serde_json::from_slice(&decode_output.stdout).expect("decoded output is not valid JSON");
    let input_json: serde_json::Value = serde_json::from_str(INPUT_JSON).unwrap();

    assert_eq!(
        decoded_json, input_json,
        "decoded RenderParams JSON does not match the original input"
    );

    let _ = std::fs::remove_file(&output);
}

#[test]
fn test_roundtrip_svg() {
    assert_roundtrip("pluot_test_roundtrip.svg");
}

#[test]
fn test_roundtrip_png() {
    // `.via_svg.png` renders via resvg (CPU-only), avoiding a dependency on
    // GPU availability in the test environment.
    assert_roundtrip("pluot_test_roundtrip.via_svg.png");
}
