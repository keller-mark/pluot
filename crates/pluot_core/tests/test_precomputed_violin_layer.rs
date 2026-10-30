use std::sync::Arc;

use pluot_core::composite_layers::bar_plot_layer::BarOrientation;
use pluot_core::composite_layers::curve_layer::PathCommand;
use pluot_core::composite_layers::precomputed_violin_layer::{normalized_spans, PrecomputedViolinLayer, PrecomputedViolinLayerParams, ViolinDensityNorm, ViolinScales};
use pluot_core::d3::scale::Scaleable;
use pluot_core::render_traits::{ExtentableLayer, ViewParams};

fn params(groups: &[&str], bin_counts: Vec<Vec<u32>>) -> PrecomputedViolinLayerParams {
    PrecomputedViolinLayerParams {
        groups: Arc::new(groups.iter().map(|g| g.to_string()).collect()),
        bin_starts: Arc::new(vec![0.0, 1.0, 2.0]),
        bin_counts: Arc::new(bin_counts),
        ..Default::default()
    }
}

#[test]
fn bands_are_centered_on_the_axis_ticks() {
    let scales = ViolinScales::new(&params(&["a", "b", "c", "d"], vec![]), 400.0);
    let centers: Vec<f64> = ["a", "b", "c", "d"].iter().map(|g| scales.band_center(&g.to_string())).collect();
    for (center, expected) in centers.iter().zip([50.0, 150.0, 250.0, 350.0]) {
        assert!((center - expected).abs() < 1e-9, "{center} != {expected}");
    }
}

#[test]
fn a_full_span_fills_its_band() {
    let scales = ViolinScales::new(&params(&["a", "b"], vec![]), 200.0);
    assert_eq!(scales.width_scale.scale(&-1.0), 0.0);
    assert!((scales.width_scale.scale(&1.0) - scales.group_scale.bandwidth()).abs() < 1e-9);
}

#[test]
fn width_norm_gives_every_violin_the_same_peak_width() {
    let spans = normalized_spans(&[vec![1, 4, 0], vec![2, 2, 2], vec![0, 0, 0]], ViolinDensityNorm::Width);
    assert_eq!(spans, vec![vec![0.25, 1.0, 0.0], vec![1.0, 1.0, 1.0], vec![0.0, 0.0, 0.0]]);
}

#[test]
fn count_norm_scales_peak_widths_by_number_of_observations() {
    let spans = normalized_spans(&[vec![1, 4, 0], vec![2, 2, 6]], ViolinDensityNorm::Count);
    assert_eq!(spans, vec![vec![0.125, 0.5, 0.0], vec![1.0 / 3.0, 1.0 / 3.0, 1.0]]);
}

#[test]
fn area_norm_gives_every_violin_the_same_area() {
    let spans = normalized_spans(&[vec![1, 4, 0], vec![20, 20, 20]], ViolinDensityNorm::Area);
    let areas: Vec<f64> = spans.iter().map(|s| s.iter().sum()).collect();
    assert!((areas[0] - areas[1]).abs() < 1e-9);
    assert_eq!(spans[0][1], 1.0);
}

fn value_coords(outline: &[PathCommand], orientation: &BarOrientation) -> Vec<f32> {
    outline.iter()
        .filter_map(|command| match *command {
            PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } | PathCommand::CubicTo { x, y, .. } => Some(match orientation {
                BarOrientation::Vertical => y,
                BarOrientation::Horizontal => x,
            }),
            _ => None,
        })
        .collect()
}

#[test]
fn violin_outline_spans_the_bins_in_data_units_along_the_value_axis() {
    let p = params(&["a"], vec![vec![1, 2, 0]]);
    let scales = ViolinScales::new(&p, 100.0);
    for orientation in [BarOrientation::Vertical, BarOrientation::Horizontal] {
        let outline = scales.violin_outline(&"a".to_string(), &p.bin_starts, &[0.5, 1.0, 0.0], &orientation);
        assert!(matches!(outline.last(), Some(PathCommand::Close)));
        let values = value_coords(&outline, &orientation);
        assert_eq!(values.iter().copied().fold(f32::MAX, f32::min), 0.0);
        assert_eq!(values.iter().copied().fold(f32::MIN, f32::max), 2.0);
    }
}

#[test]
fn extent_spans_the_value_domain_along_the_value_axis() {
    let layer = |orientation| PrecomputedViolinLayer::new(ViewParams::default(), PrecomputedViolinLayerParams {
        orientation: Some(orientation),
        value_domain: Some((-1.0, 5.0)),
        ..params(&["a"], vec![vec![1, 2, 0]])
    });
    assert_eq!(layer(BarOrientation::Vertical).extent().unwrap().y, (-1.0, 5.0));
    assert_eq!(layer(BarOrientation::Horizontal).extent().unwrap().x, (-1.0, 5.0));
}
