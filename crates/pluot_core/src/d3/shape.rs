//! Ports of the d3-shape area generator and curves: https://github.com/d3/d3-shape
//! Instead of drawing to a canvas-like context, the generated paths are returned as [`PathCommand`]s.

use crate::curve_and_polygon_utils::PathCommand;

/// Where a curve is relative to an area's two lines, as tracked by d3's `_line` (0, 1, or NaN).
#[derive(Debug, Clone, Copy, PartialEq)]
enum AreaLine {
    NotInArea,
    Upper,
    Lower,
}

/// A curve receives the points of one line at a time, between `line_start` and `line_end`.
/// Within an area, the upper line is followed by the lower line, which is closed back to the upper.
pub trait Curve {
    fn area_start(&mut self);
    fn area_end(&mut self);
    fn line_start(&mut self);
    fn line_end(&mut self, path: &mut Vec<PathCommand>);
    fn point(&mut self, path: &mut Vec<PathCommand>, x: f64, y: f64);
}

fn move_to(x: f64, y: f64) -> PathCommand {
    PathCommand::MoveTo { x: x as f32, y: y as f32 }
}

fn line_to(x: f64, y: f64) -> PathCommand {
    PathCommand::LineTo { x: x as f32, y: y as f32 }
}

impl AreaLine {
    fn start_command(self, x: f64, y: f64) -> PathCommand {
        if self == AreaLine::Lower { line_to(x, y) } else { move_to(x, y) }
    }

    fn should_close(self, num_points: usize) -> bool {
        self == AreaLine::Lower || (self == AreaLine::NotInArea && num_points == 1)
    }

    fn next(self) -> Self {
        match self {
            AreaLine::NotInArea => AreaLine::NotInArea,
            AreaLine::Upper => AreaLine::Lower,
            AreaLine::Lower => AreaLine::Upper,
        }
    }
}

/// Straight line segments between points (`d3.curveLinear`).
#[derive(Debug, Clone)]
pub struct CurveLinear {
    line: AreaLine,
    num_points: usize,
}

impl Default for CurveLinear {
    fn default() -> Self {
        Self { line: AreaLine::NotInArea, num_points: 0 }
    }
}

impl Curve for CurveLinear {
    fn area_start(&mut self) {
        self.line = AreaLine::Upper;
    }

    fn area_end(&mut self) {
        self.line = AreaLine::NotInArea;
    }

    fn line_start(&mut self) {
        self.num_points = 0;
    }

    fn line_end(&mut self, path: &mut Vec<PathCommand>) {
        if self.line.should_close(self.num_points) {
            path.push(PathCommand::Close);
        }
        self.line = self.line.next();
    }

    fn point(&mut self, path: &mut Vec<PathCommand>, x: f64, y: f64) {
        path.push(if self.num_points == 0 { self.line.start_command(x, y) } else { line_to(x, y) });
        self.num_points += 1;
    }
}

/// A cubic basis spline through the points, which starts and ends at the first and last
/// points but does not otherwise pass through them (`d3.curveBasis`).
#[derive(Debug, Clone)]
pub struct CurveBasis {
    line: AreaLine,
    num_points: usize,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Default for CurveBasis {
    fn default() -> Self {
        Self { line: AreaLine::NotInArea, num_points: 0, x0: f64::NAN, y0: f64::NAN, x1: f64::NAN, y1: f64::NAN }
    }
}

impl CurveBasis {
    fn bezier_to(&self, path: &mut Vec<PathCommand>, x: f64, y: f64) {
        path.push(PathCommand::CubicTo {
            x1: ((2.0 * self.x0 + self.x1) / 3.0) as f32,
            y1: ((2.0 * self.y0 + self.y1) / 3.0) as f32,
            x2: ((self.x0 + 2.0 * self.x1) / 3.0) as f32,
            y2: ((self.y0 + 2.0 * self.y1) / 3.0) as f32,
            x: ((self.x0 + 4.0 * self.x1 + x) / 6.0) as f32,
            y: ((self.y0 + 4.0 * self.y1 + y) / 6.0) as f32,
        });
    }
}

impl Curve for CurveBasis {
    fn area_start(&mut self) {
        self.line = AreaLine::Upper;
    }

    fn area_end(&mut self) {
        self.line = AreaLine::NotInArea;
    }

    fn line_start(&mut self) {
        (self.x0, self.y0, self.x1, self.y1) = (f64::NAN, f64::NAN, f64::NAN, f64::NAN);
        self.num_points = 0;
    }

    fn line_end(&mut self, path: &mut Vec<PathCommand>) {
        if self.num_points >= 3 {
            self.bezier_to(path, self.x1, self.y1);
        }
        if self.num_points >= 2 {
            path.push(line_to(self.x1, self.y1));
        }
        if self.line.should_close(self.num_points) {
            path.push(PathCommand::Close);
        }
        self.line = self.line.next();
    }

    fn point(&mut self, path: &mut Vec<PathCommand>, x: f64, y: f64) {
        match self.num_points {
            0 => path.push(self.line.start_command(x, y)),
            1 => {}
            2 => {
                path.push(line_to((5.0 * self.x0 + self.x1) / 6.0, (5.0 * self.y0 + self.y1) / 6.0));
                self.bezier_to(path, x, y);
            }
            _ => self.bezier_to(path, x, y),
        }
        self.num_points += 1;
        (self.x0, self.x1) = (self.x1, x);
        (self.y0, self.y1) = (self.y1, y);
    }
}

/// One sample of an area: the area spans from `(x0, y0)` on its lower line to `(x1, y1)` on its upper line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AreaPoint {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// The closed path of the area between two lines (as in `d3.area()`): the upper line runs
/// forwards through `points`, and the lower line runs back from the last point to the first.
pub fn area(curve: &mut impl Curve, points: &[AreaPoint]) -> Vec<PathCommand> {
    let mut path = Vec::new();
    if points.is_empty() {
        return path;
    }
    curve.area_start();
    curve.line_start();
    for point in points {
        curve.point(&mut path, point.x1, point.y1);
    }
    curve.line_end(&mut path);
    curve.line_start();
    for point in points.iter().rev() {
        curve.point(&mut path, point.x0, point.y0);
    }
    curve.line_end(&mut path);
    curve.area_end();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
