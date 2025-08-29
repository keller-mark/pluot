#![cfg(test)]
use pluot::d3::scale::{Scale, ScaleLinear, ScaleContinuous};

// Reference: https://github.com/d3/d3-scale/blob/main/test/linear-test.js

#[test]
fn test_scale_linear_defaults() {
    let s = ScaleLinear::new();
    assert_eq!(s.get_domain(), (0.0, 1.0));
    assert_eq!(s.get_range(), (0.0, 1.0));
    assert_eq!(s.get_clamp(), false);
}

#[test]
fn test_scale_linear_range_sets_range() {
    let s = ScaleLinear::new().set_range((1.0, 2.0));
    assert_eq!(s.get_domain(), (0.0, 1.0));
    assert_eq!(s.get_range(), (1.0, 2.0));
    assert_eq!(s.scale(&0.5), 1.5);
}

#[test]
fn test_scale_linear_domain_range_sets_domain_and_range() {
    let s = ScaleLinear::new().set_domain((1.0, 2.0)).set_range((3.0, 4.0));
    assert_eq!(s.get_domain(), (1.0, 2.0));
    assert_eq!(s.get_range(), (3.0, 4.0));
    assert_eq!(s.scale(&1.5), 3.5);
}

#[test]
fn test_linear_maps_domain_to_range() {
    assert_eq!(ScaleLinear::new().set_range((1.0, 2.0)).scale(&0.5), 1.5);
}

#[test]
fn test_linear_clamp_true_restricts_output_to_range() {
    let s = ScaleLinear::new().set_clamp(true).set_range((10.0, 20.0));
    assert_eq!(s.scale(&2.0), 20.0);
    assert_eq!(s.scale(&-1.0), 10.0);
}

#[test]
fn test_linear_nice_extends_domain() {
    let s = ScaleLinear::new().set_domain((0.0, 0.96)).nice(None);
    assert_eq!(s.get_domain(), (0.0, 1.0));

    let s = ScaleLinear::new().set_domain((0.0, 96.0)).nice(None);
    assert_eq!(s.get_domain(), (0.0, 100.0));
}

#[test]
fn test_linear_ticks_returns_expected_ticks() {
    let s = ScaleLinear::new();
    let ticks = s.ticks(Some(10));
    let expected: Vec<f64> = vec![
        0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0,
    ];
    assert_eq!(ticks.len(), expected.len());
    for (a, b) in ticks.iter().zip(expected.iter()) {
        assert!((a - b).abs() < 1e-9);
    }

    let s = ScaleLinear::new().set_domain((-100.0, 100.0));
    let ticks = s.ticks(Some(10));
    let expected: Vec<f64> = vec![-100.0, -80.0, -60.0, -40.0, -20.0, 0.0, 20.0, 40.0, 60.0, 80.0, 100.0];
    assert_eq!(ticks, expected);
}

#[test]
fn test_linear_copy_isolates_domain_changes() {
    let x = ScaleLinear::new();
    let mut y = x.clone();
    let x = x.set_domain((1.0, 2.0));
    assert_eq!(y.get_domain(), (0.0, 1.0));
    assert_eq!(x.scale(&1.0), 0.0);
    assert_eq!(y.scale(&1.0), 1.0);

    y = y.set_domain((2.0, 3.0));
    assert_eq!(x.scale(&2.0), 1.0);
    assert_eq!(y.scale(&2.0), 0.0);
    assert_eq!(x.get_domain(), (1.0, 2.0));
    assert_eq!(y.get_domain(), (2.0, 3.0));
}

#[test]
fn test_linear_copy_isolates_range_changes() {
    let x = ScaleLinear::new();
    let mut y = x.clone();
    let x = x.set_range((1.0, 2.0));
    assert_eq!(y.get_range(), (0.0, 1.0));

    y = y.set_range((2.0, 3.0));
    assert_eq!(x.get_range(), (1.0, 2.0));
    assert_eq!(y.get_range(), (2.0, 3.0));
}

#[test]
fn test_linear_copy_isolates_clamp_changes() {
    let x = ScaleLinear::new().set_clamp(true);
    let mut y = x.clone();
    let x = x.set_clamp(false);
    assert_eq!(x.scale(&2.0), 2.0);
    assert_eq!(y.scale(&2.0), 1.0);
    assert_eq!(y.get_clamp(), true);

    y = y.set_clamp(false);
    assert_eq!(x.scale(&2.0), 2.0);
    assert_eq!(y.scale(&2.0), 2.0);
    assert_eq!(x.get_clamp(), false);
}
