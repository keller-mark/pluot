use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt;
use std::hash::{Hash, Hasher};

pub(crate) type Coord = f32;
pub(crate) type Index = usize;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: Coord,
    pub y: Coord,
}

impl Point {
    pub const fn new(x: Coord, y: Coord) -> Self {
        Self { x, y }
    }
    pub fn new_i(x: i32, y: i32) -> Self {
        Self::new(x as f32, y as f32)
    }

    pub fn add(&self, other: &Point) -> Vector {
        Vector {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }

    pub fn sub(&self, other: &Point) -> Vector {
        Vector {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }

    pub fn add_vector(&self, vector: &Vector) -> Point {
        Point {
            x: self.x + vector.x,
            y: self.y + vector.y,
        }
    }
}

impl Eq for Point {}

#[allow(clippy::non_canonical_partial_ord_impl)]
impl PartialOrd for Point {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.y == other.y {
            self.x.partial_cmp(&other.x)
        } else {
            self.y.partial_cmp(&other.y)
        }
    }
}

impl Ord for Point {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl Hash for Point {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let x_hash = (self.x.to_bits() as u64).rotate_left(16);
        let y_hash = self.y.to_bits() as u64;
        state.write_u64(x_hash ^ y_hash);
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "(x={}, y={})", self.x, self.y)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Vector {
    pub x: Coord,
    pub y: Coord,
}

impl Vector {
    pub fn new(x: Coord, y: Coord) -> Self {
        Self { x, y }
    }

    pub fn scale(&self, factor: Coord) -> Vector {
        Vector {
            x: self.x * factor,
            y: self.y * factor,
        }
    }
}

impl std::ops::Add<Vector> for Point {
    type Output = Point;
    fn add(self, other: Vector) -> Point {
        Point {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}
impl std::ops::Sub for Point {
    type Output = Vector;
    fn sub(self, other: Point) -> Vector {
        Vector {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl std::ops::Add for Vector {
    type Output = Vector;

    fn add(self, other: Vector) -> Vector {
        Vector {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl std::ops::Sub for Vector {
    type Output = Vector;

    fn sub(self, other: Vector) -> Vector {
        Vector {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}
impl std::ops::Div<Coord> for Vector {
    type Output = Vector;

    fn div(self, rhs: Coord) -> Self::Output {
        Vector {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl std::ops::Mul<Coord> for Vector {
    type Output = Vector;

    fn mul(self, rhs: Coord) -> Self::Output {
        Vector {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl std::ops::Neg for Vector {
    type Output = Vector;

    fn neg(self) -> Self::Output {
        Vector {
            x: -self.x,
            y: -self.y,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub top: Point,
    pub bottom: Point,
}

impl Segment {
    pub fn new(p1: Point, p2: Point) -> Self {
        if p1 == p2 {
            panic!("Segment cannot have two identical points: {}", p1);
        }

        if p1 < p2 {
            Self {
                bottom: p1,
                top: p2,
            }
        } else {
            Self {
                bottom: p2,
                top: p1,
            }
        }
    }

    pub fn new_i(p1: (i32, i32), p2: (i32, i32)) -> Self {
        Self::new(Point::new_i(p1.0, p1.1), Point::new_i(p2.0, p2.1))
    }

    pub fn new_f(p1: (f32, f32), p2: (f32, f32)) -> Self {
        Self::new(Point::new(p1.0, p1.1), Point::new(p2.0, p2.1))
    }

    pub fn is_horizontal(&self) -> bool {
        self.bottom.y == self.top.y
    }

    pub fn is_vertical(&self) -> bool {
        self.bottom.x == self.top.x
    }

    pub fn point_on_line_x(&self, y: Coord) -> Coord {
        if self.bottom.y == self.top.y {
            self.bottom.x
        } else {
            self.bottom.x
                + (y - self.bottom.y)
                    * ((self.top.x - self.bottom.x) / (self.top.y - self.bottom.y))
        }
    }

    pub fn point_projection_factor(&self, p: Point) -> Coord {
        let numerator = (p.x - self.top.x) * (self.bottom.x - self.top.x)
            + (p.y - self.top.y) * (self.bottom.y - self.top.y);
        let denominator =
            (self.top.x - self.bottom.x).powi(2) + (self.top.y - self.bottom.y).powi(2);

        numerator / denominator
    }

    pub fn point_on_line(&self, p: Point) -> bool {
        if self.is_horizontal() {
            return self.bottom.x <= p.x && p.x <= self.top.x;
        }

        if self.is_vertical() {
            return self.bottom.y <= p.y && p.y <= self.top.y;
        }

        let x_coord = self.point_on_line_x(p.y);
        self.bottom.x <= x_coord && x_coord <= self.top.x
    }
}

impl fmt::Display for Segment {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[bottom={}, top={}]", self.bottom, self.top)
    }
}

impl Hash for Segment {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bottom.hash(state);
        self.top.hash(state);
    }
}

impl PartialEq for Segment {
    fn eq(&self, other: &Self) -> bool {
        self.bottom == other.bottom && self.top == other.top
    }
}

impl Eq for Segment {}

#[allow(clippy::non_canonical_partial_ord_impl)]
impl PartialOrd for Segment {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.bottom == other.bottom {
            self.top.partial_cmp(&other.top)
        } else {
            self.bottom.partial_cmp(&other.bottom)
        }
    }
}

impl Ord for Segment {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

#[derive(Debug, Clone)]
/// Represents a triangle using indices of its three vertices.
///
/// # Fields
/// * `x` - The index of the first vertex.
/// * `y` - The index of the second vertex.
/// * `z` - The index of the third vertex.
///
/// # Examples
/// ```
/// use triangulation::point::Triangle;
///
/// let triangle = Triangle::new(0, 1, 2);
/// assert_eq!(triangle.x, 0);
/// assert_eq!(triangle.y, 1);
/// assert_eq!(triangle.z, 2);
/// ```
pub struct Triangle {
    pub x: Index,
    pub y: Index,
    pub z: Index,
}

impl Triangle {
    pub fn new(x: Index, y: Index, z: Index) -> Self {
        Triangle { x, y, z }
    }

    /// return copy of Triangle with indexes shifted by a given value.
    pub fn shifted_by(&self, shift: Index) -> Self {
        Triangle {
            x: self.x + shift,
            y: self.y + shift,
            z: self.z + shift,
        }
    }
}

/// Represents a triangle using three points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointTriangle {
    pub p1: Point,
    pub p2: Point,
    pub p3: Point,
}

impl PointTriangle {
    pub fn new(p1: Point, p2: Point, p3: Point) -> Self {
        // Check if points are ordered counter-clockwise.
        if (p2.x - p1.x) * (p3.y - p1.y) - (p2.y - p1.y) * (p3.x - p1.x) < 0.0 {
            // Reorder points to be counter-clockwise.
            Self { p1: p3, p2, p3: p1 }
        } else {
            Self { p1, p2, p3 }
        }
    }
}

/// Calculates the Euclidean distance between two points.
///
/// # Arguments
/// * `p1` - The first point.
/// * `p2` - The second point.
///
/// # Returns
/// The distance between `p1` and `p2` as a `Coord`.
///
/// # Examples
/// ```
/// use triangulation::point::{Point, vector_length};
///
/// let point1 = Point { x: 3.0, y: 0.0 };
/// let point2 = Point { x: 0.0, y: 4.0 };
///
/// let distance = vector_length(point1, point2);
/// assert_eq!(distance, 5.0);
/// ```
pub fn vector_length(p1: Point, p2: Point) -> Coord {
    let dx = p1.x - p2.x;
    let dy = p1.y - p2.y;
    (dx * dx + dy * dy).sqrt()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Orientation {
    Collinear,
    Clockwise,
    CounterClockwise,
}

/// Determines the orientation of three points (p, q, r).
///
/// This function calculates the orientation of the ordered triplet (p, q, r).
///
/// # Arguments
///
/// * `p` - The first [`Point`].
/// * `q` - The second [`Point`].
/// * `r` - The third [`Point`].
///
/// # Returns
///
///  Proper Orientation Enum
///
/// # Note
///
/// Due to floating-point arithmetic, the results may be sensitive to numerical precision
/// when points are nearly collinear.
///
/// # Example
///
/// ```rust
/// use triangulation::point::{Point, orientation, Orientation};
///
/// let p = Point::new(0.0, 0.0);
/// let q = Point::new(1.0, 1.0);
/// let r = Point::new(2.0, 2.0);
///
/// assert_eq!(orientation(p, q, r), Orientation::Collinear); // Collinear points
///
/// let r_clockwise = Point::new(2.0, 0.0);
/// assert_eq!(orientation(p, q, r_clockwise), Orientation::Clockwise); // Clockwise orientation
///
/// let r_counterclockwise = Point::new(0.0, 2.0);
/// assert_eq!(orientation(p, q, r_counterclockwise), Orientation::CounterClockwise); // Counterclockwise orientation
///
/// ```
pub const fn orientation(p: Point, q: Point, r: Point) -> Orientation {
    let val1 = (q.y - p.y) * (r.x - q.x);
    let val2 = (r.y - q.y) * (q.x - p.x);
    if val1 == val2 {
        Orientation::Collinear
    } else if val1 > val2 {
        Orientation::Clockwise
    } else {
        Orientation::CounterClockwise
    }
}

/// Calculates a deduplicated list of edges (as `Segment`s) from the input list of polygons.
///
/// This function processes a list of polygons (each represented as a vector of `Point`s)
/// and generates a collection of unique `Segment`s that represent edges of the polygons.
///
/// If an edge appears more than once (e.g., due to multiple polygons sharing edges, or polygon with holes),
/// the duplicates are deduplicated modulo 2 (ex. edge that appears 2, 4, 6 times is not returned,
/// edges, that is present 1, 3, 99 times is returned once)
///
/// # Arguments
/// * `polygon_list` - A slice of vectors, where each vector represents a polygon
///   as a list of `Point`s.
///
/// # Returns
/// A `Vec<Segment>` containing all edges deduplicated modulo 2.
///
/// # Examples
/// ```
/// use triangulation::point::{Point, Segment, calc_dedup_edges};
///
/// let polygon1 = vec![
///     Point::new(0.0, 0.0),
///     Point::new(1.0, 0.0),
///     Point::new(1.0, 1.0),
///     Point::new(0.0, 0.0),
/// ];
///
/// let polygon2 = vec![
///     Point::new(1.0, 0.0),
///     Point::new(2.0, 0.0),
///     Point::new(2.0, 1.0),
///     Point::new(1.0, 0.0),
/// ];
///
/// let edges = calc_dedup_edges(&[polygon1, polygon2]);
/// assert_eq!(edges.len(), 6); // Deduplicated edges
/// ```
#[inline]
pub fn calc_dedup_edges(polygon_list: &[Vec<Point>]) -> Vec<Segment> {
    for (i, polygon) in polygon_list.iter().enumerate() {
        if polygon.len() < 3 {
            panic!("Polygon at index {} has fewer than 3 points", i);
        }
        // Check for collinearity of all points
        if polygon.len() >= 3 {
            let first_orientation = orientation(polygon[polygon.len() - 1], polygon[0], polygon[1])
                == Orientation::Collinear;
            let last_orientation = orientation(
                polygon[polygon.len() - 2],
                polygon[polygon.len() - 1],
                polygon[0],
            ) == Orientation::Collinear;
            let all_collinear = polygon.windows(3).all(|window| {
                orientation(window[0], window[1], window[2]) == Orientation::Collinear
            });
            if first_orientation && last_orientation && all_collinear {
                panic!("All points in polygon at index {} are collinear", i);
            }
        }
    }

    let mut edges_set = HashSet::new();

    for polygon in polygon_list {
        // Process edges between consecutive points
        for window in polygon.windows(2) {
            let edge = Segment::new(window[0], window[1]);
            if !edges_set.remove(&edge) {
                edges_set.insert(edge);
            }
        }

        // Process edge between last and first point if they're different
        if let (Some(back), Some(front)) = (polygon.last(), polygon.first()) {
            if back != front {
                let edge = Segment::new(*back, *front);
                if !edges_set.remove(&edge) {
                    edges_set.insert(edge);
                }
            }
        }
    }
    edges_set.into_iter().collect()
}

pub fn centroid(points: &[Point]) -> Point {
    if points.is_empty() {
        panic!("Cannot calculate centroid of an empty points list");
    }
    let sum = points.iter().fold(Point::new(0.0, 0.0), |acc, p| {
        Point::new(acc.x + p.x, acc.y + p.y)
    });
    let n = points.len() as f32;
    Point::new(sum.x / n, sum.y / n)
}
