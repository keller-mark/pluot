use std::sync::Arc;

use pluot_core::composite_layers::curve_layer::PathCommand;
use pluot_core::composite_layers::precomputed_violin_matrix_layer::{resolve_fill_value_domain, PrecomputedViolinMatrixLayer, PrecomputedViolinMatrixLayerParams, PrecomputedViolinMatrixPickingInfo, ViolinMatrixScales};
use pluot_core::render_traits::{PickableLayer, ViewParams};
use pluot_core::viewport::ScreenCoord;

fn labels(names: &[&str]) -> Arc<Vec<String>> {
    Arc::new(names.iter().map(|name| name.to_string()).collect())
}

fn params(var_names: &[&str], groups: &[&str]) -> PrecomputedViolinMatrixLayerParams {
    PrecomputedViolinMatrixLayerParams {
        var_names: labels(var_names),
        groups: labels(groups),
        bin_starts: Arc::new(vec![0.0, 1.0, 2.0]),
        ..Default::default()
    }
}

fn y_coords(outline: &[PathCommand]) -> Vec<f32> {
    outline.iter()
        .filter_map(|command| match *command {
            PathCommand::MoveTo { y, .. } | PathCommand::LineTo { y, .. } | PathCommand::CubicTo { y, .. } => Some(y),
            _ => None,
        })
        .collect()
}

#[test]
fn var_names_are_columns_unless_axes_are_swapped() {
    let p = params(&["g1", "g2"], &["a", "b", "c", "d"]);
    let scales = ViolinMatrixScales::new(&p, 200.0, 400.0);
    assert_eq!(scales.cell_origin(&"c".to_string(), &"g2".to_string()).1, 200.0);
    assert!((scales.cell_origin(&"c".to_string(), &"g2".to_string()).0 - 105.0).abs() < 1e-9);

    let swapped = ViolinMatrixScales::new(&PrecomputedViolinMatrixLayerParams { swap_axes: Some(true), ..p }, 400.0, 200.0);
    assert_eq!(swapped.cell_origin(&"c".to_string(), &"g2".to_string()).1, 100.0);
    assert!((swapped.cell_origin(&"c".to_string(), &"g2".to_string()).0 - 205.0).abs() < 1e-9);
}

#[test]
fn violins_span_the_padded_row_along_the_value_axis() {
    let p = params(&["g1"], &["a", "b"]);
    let scales = ViolinMatrixScales::new(&p, 100.0, 200.0);
    let origin = scales.cell_origin(&"b".to_string(), &"g1".to_string());
    let outline = scales.violin_outline(origin, &p.bin_starts, &[0.5, 1.0, 0.25]);
    let ys = y_coords(&outline);
    assert_eq!(ys.iter().copied().fold(f32::MAX, f32::min), 105.0);
    assert_eq!(ys.iter().copied().fold(f32::MIN, f32::max), 195.0);
}

#[test]
fn band_index_at_divides_the_axis_into_equal_steps() {
    assert_eq!(ViolinMatrixScales::band_index_at(0.0, 300.0, 3), Some(0));
    assert_eq!(ViolinMatrixScales::band_index_at(150.0, 300.0, 3), Some(1));
    assert_eq!(ViolinMatrixScales::band_index_at(300.0, 300.0, 3), Some(2));
    assert_eq!(ViolinMatrixScales::band_index_at(-1.0, 300.0, 3), None);
    assert_eq!(ViolinMatrixScales::band_index_at(10.0, 300.0, 0), None);
}

#[test]
fn fill_value_domain_ignores_missing_and_non_finite_values() {
    let p = params(&[], &[]);
    assert_eq!(resolve_fill_value_domain(&p, &[None, Some(f32::NAN), Some(2.0), Some(5.0)]), (2.0, 5.0));
    assert_eq!(resolve_fill_value_domain(&p, &[None]), (0.0, 1.0));
    assert_eq!(resolve_fill_value_domain(&p, &[Some(3.0), Some(3.0)]), (3.0, 4.0));
}

#[test]
fn picks_the_cell_under_the_cursor() {
    let view_params = ViewParams { width: 300, height: 200, ..Default::default() };
    let layer = PrecomputedViolinMatrixLayer::new(view_params, params(&["g1", "g2", "g3"], &["a", "b"]));
    let result = layer.pick(ScreenCoord { x: 250.0, y: 150.0 }, None).unwrap();
    let info: PrecomputedViolinMatrixPickingInfo = result.into_info().unwrap();
    assert_eq!(info, PrecomputedViolinMatrixPickingInfo { index: 5, var_name: "g3".to_string(), group: "b".to_string() });
}
