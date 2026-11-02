use pluot_core::composite_layers::curve_layer::PathCommand;
use pluot_core::d3::shape::{area, AreaPoint, CurveBasis, CurveLinear};

fn to_svg(path: &[PathCommand]) -> String {
    path.iter()
        .map(|command| match *command {
            PathCommand::MoveTo { x, y } => format!("M{x},{y}"),
            PathCommand::LineTo { x, y } => format!("L{x},{y}"),
            PathCommand::CubicTo { x1, y1, x2, y2, x, y } => format!("C{x1},{y1},{x2},{y2},{x},{y}"),
            PathCommand::Close => "Z".to_string(),
            _ => unreachable!(),
        })
        .collect()
}

fn points(samples: &[(f64, f64, f64)]) -> Vec<AreaPoint> {
    samples.iter().map(|&(x0, x1, y)| AreaPoint { x0, y0: y, x1, y1: y }).collect()
}

#[test]
fn linear_area_matches_d3() {
    let path = area(&mut CurveLinear::default(), &points(&[(0.0, 1.0, 0.0), (0.0, 2.0, 3.0)]));
    assert_eq!(to_svg(&path), "M1,0L2,3L0,3L0,0Z");
}

#[test]
fn basis_area_matches_d3() {
    let path = area(&mut CurveBasis::default(), &points(&[(-6.0, 6.0, 0.0), (-12.0, 12.0, 6.0), (0.0, 0.0, 12.0)]));
    assert_eq!(
        to_svg(&path),
        "M6,0L7,1C8,2,10,4,9,6C8,8,4,10,2,11L0,12L0,12L-2,11C-4,10,-8,8,-9,6C-10,4,-8,2,-7,1L-6,0Z",
    );
}

#[test]
fn basis_area_with_two_points_is_a_quadrilateral() {
    let path = area(&mut CurveBasis::default(), &points(&[(0.0, 1.0, 0.0), (0.0, 2.0, 3.0)]));
    assert_eq!(to_svg(&path), "M1,0L2,3L0,3L0,0Z");
}

#[test]
fn empty_area_has_no_path() {
    assert!(area(&mut CurveBasis::default(), &[]).is_empty());
}
