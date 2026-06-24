use crate::point::{orientation, Orientation, Point, PointTriangle};
use std::cmp::PartialEq;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct MonotonePolygon {
    /// Represents a y-monotone polygon, which is a polygon whose vertices are split
    /// into two chains (left and right) based on their y-coordinates, and the topmost
    /// and bottommost vertices are connected by these chains.
    ///
    /// A polygon is considered y-monotone if a straight horizontal line (parallel
    /// to the x-axis) intersects it at most twice. In other words, the vertices of the
    /// polygon can be divided into two chains, often referred to as the "left chain"
    /// and the "right chain," where each chain is monotonic with respect to the
    /// y-coordinate (i.e., the y-coordinates of the vertices are sorted either
    /// in strictly increasing or strictly decreasing order).
    ///
    /// The top-most and bottom-most vertices of a y-monotone polygon are common
    /// to both chains, and no other horizontal line intersects more than two points
    /// of the polygon.
    ///
    /// # Fields
    ///
    /// - `top`: The topmost vertex of the polygon.
    /// - `bottom`: An optional bottom vertex of the polygon. If this is `None`,
    ///   the polygon is considered incomplete.
    /// - `left`: A collection of vertices forming the left chain of the polygon.
    ///   The vertices are stored in descending order based on their y-coordinates.
    /// - `right`: A collection of vertices forming the right chain of the polygon.
    ///   The vertices are stored in descending order based on their y-coordinates.
    ///
    /// # Usage
    ///
    /// The `MonotonePolygon` can be used to construct a polygon by adding vertices
    /// to its left and right chains. Typically, it is used in computational geometry
    /// algorithms such as triangulation.
    ///
    /// - To create a fully defined monotone polygon, use `MonotonePolygon::new`.
    /// - For an incomplete polygon where only the top vertex is known, use
    ///   `MonotonePolygon::new_top`.
    /// - To check if the polygon is complete, use the `finished` method.
    ///
    /// # Example
    ///
    /// ```rust
    /// use triangulation::point::Point;
    /// use triangulation::monotone_polygon::MonotonePolygon;
    ///
    /// let top = Point::new(0.0, 10.0);
    /// let bottom = Point::new(0.0, 0.0);
    /// let left_chain = vec![Point::new(-1.0, 8.0), Point::new(-2.0, 6.0)];
    /// let right_chain = vec![Point::new(1.0, 8.0), Point::new(2.0, 6.0)];
    ///
    /// let polygon = MonotonePolygon::new(top, bottom, left_chain, right_chain);
    /// assert!(polygon.finished());
    /// ```
    pub top: Point,
    pub bottom: Option<Point>,
    pub left: Vec<Point>,
    pub right: Vec<Point>,
}

impl MonotonePolygon {
    pub fn new(top: Point, bottom: Point, left: Vec<Point>, right: Vec<Point>) -> Self {
        Self {
            top,
            bottom: Some(bottom),
            left,
            right,
        }
    }

    pub fn new_top(top: Point) -> Self {
        Self {
            top,
            bottom: None,
            left: vec![],
            right: vec![],
        }
    }

    /// Check if monotone polygon is finished by checking if bottom is set  
    pub fn finished(&self) -> bool {
        self.bottom.is_some()
    }
}

/// Builds triangles when the current point is from the opposite edger than the previous one.
///
/// This function is invoked during the y-monotone polygon triangulation process
/// to handle the scenario where the current point is located on the opposite edge
/// compared to the previous point. It generates triangles using the points in the
/// stack as one edge of the triangle, and the current point as the opposite vertex.
///
/// # Arguments
///
/// - `stack`: A mutable reference to a `VecDeque` containing points representing the
///   current chain of the polygon being processed. The points are used to form triangles.
/// - `result`: A mutable reference to a vector of `PointTriangle` that will hold the
///   resulting triangles generated during the triangulation process.
/// - `current_point`: The current point being processed, which belongs to the opposite
///   edge of the polygon relative to the previous point.
fn build_triangles_opposite_edge(
    stack: &mut VecDeque<Point>,
    result: &mut Vec<PointTriangle>,
    current_point: Point,
) {
    #[cfg(debug_assertions)]
    {
        if stack.is_empty() {
            panic!("Cannot build triangles when stack is empty. Ensure the polygon is not empty.");
        }
    }
    for i in 0..stack.len() - 1 {
        result.push(PointTriangle::new(current_point, stack[i], stack[i + 1]));
    }

    let back = stack.pop_back().unwrap(); //  Get the last element
    stack.clear(); // clear all consumed points
    stack.push_back(back); //  add points to stack for next triangle
    stack.push_back(current_point);
}

/// Builds triangles when the current point is along the same edge chain as the previous point.
///
/// This function processes points that are part of the same chain (left or right) of the y-monotone
/// polygon and builds triangles based on the expected orientation. It ensures that the formed triangles
/// are valid, following the order of the points in the chain.
///
/// # Arguments
///
/// - `stack`: A mutable reference to a `VecDeque` holding points that are part of the current chain
///   being processed. These points are used as vertices of the triangle.
/// - `result`: A mutable reference to a `Vec<PointTriangle>` that stores the triangles generated
///   during the triangulation process.
/// - `current_point`: The point currently being processed, which is on the same edge chain as the
///   previous point.
/// - `expected_orientation`: The expected orientation of the triangles to be formed, determined based
///   on whether the current chain belongs to the left or right edge of the polygon.
fn build_triangles_current_edge(
    stack: &mut VecDeque<Point>,
    result: &mut Vec<PointTriangle>,
    current_point: Point,
    expected_orientation: Side,
) {
    let mut i = stack.len() - 1;
    // Decide which orientation depending on from which chain current point is
    let orientation_ = if expected_orientation == Side::Left {
        Orientation::CounterClockwise
    } else {
        Orientation::Clockwise
    };
    while i > 0 && orientation(stack[i - 1], stack[i], current_point) == orientation_ {
        result.push(PointTriangle::new(current_point, stack[i], stack[i - 1]));
        i -= 1;
    }

    stack.truncate(i + 1); // remove all consumed points

    stack.push_back(current_point);
}

/// Enum to store information from which chain of
/// y-monotone polygon points comes
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Side {
    TopOrBottom,
    Left,
    Right,
}

/// Triangulates a y-monotone polygon into a set of triangles.
///
/// This function takes a y-monotone polygon as input and performs triangulation,
/// splitting it into a collection of triangles. The algorithm relies on the
/// structure of the polygon, dividing it into left and right chains based on
/// y-coordinates, and processes the vertices using a stack-based approach.
/// Triangles are formed either between points on the same chain or points across
/// opposite chains.
///
/// # Arguments
///
/// - `polygon`: A reference to a `MonotonePolygon` structure, which contains
///   the top, bottom, left chain, and right chain vertices of the y-monotone polygon.
///
/// # Returns
///
/// A vector of `PointTriangle` representing the resulting triangles formed
/// during the triangulation process.
///
/// # Example
///
/// ```rust
/// use triangulation::point::{Point, PointTriangle};
/// use triangulation::monotone_polygon::{MonotonePolygon, triangulate_monotone_polygon};
///
/// let top = Point::new(0.0, 10.0);
/// let bottom = Point::new(0.0, 0.0);
/// let left_chain = vec![Point::new(-1.0, 8.0), Point::new(-2.0, 6.0)];
/// let right_chain = vec![Point::new(1.0, 8.0), Point::new(2.0, 6.0)];
///
/// let polygon = MonotonePolygon::new(top, bottom, left_chain, right_chain);
/// let triangles = triangulate_monotone_polygon(&polygon);
///
/// assert_eq!(triangles.len(), 4);
/// ```
pub fn triangulate_monotone_polygon(polygon: &MonotonePolygon) -> Vec<PointTriangle> {
    if !polygon.finished() {
        panic!("Cannot triangulate an unfinished polygon. Ensure the bottom is set before calling this function.");
    }
    let mut result = Vec::new();
    let mut left_index = 0;
    let mut right_index = 0;
    let mut stack: VecDeque<Point> = VecDeque::new(); // Using VecDeque for O(1) push_front
    let mut points = Vec::with_capacity(polygon.left.len() + polygon.right.len() + 2);

    result.reserve(polygon.left.len() + polygon.right.len());
    points.reserve(polygon.left.len() + polygon.right.len() + 2);

    points.push((polygon.top, Side::TopOrBottom));

    while left_index < polygon.left.len() && right_index < polygon.right.len() {
        if polygon.left[left_index] < polygon.right[right_index] {
            points.push((polygon.right[right_index], Side::Right));
            right_index += 1;
        } else {
            points.push((polygon.left[left_index], Side::Left));
            left_index += 1;
        }
    }

    while left_index < polygon.left.len() {
        points.push((polygon.left[left_index], Side::Left));
        left_index += 1;
    }

    while right_index < polygon.right.len() {
        points.push((polygon.right[right_index], Side::Right));
        right_index += 1;
    }

    points.push((polygon.bottom.unwrap(), Side::TopOrBottom));

    stack.push_back(points[0].0);
    stack.push_back(points[1].0);
    let mut side = &points[1].1;

    for (p, s) in points.iter().skip(2) {
        if *side == *s {
            build_triangles_current_edge(&mut stack, &mut result, *p, *side);
        } else {
            build_triangles_opposite_edge(&mut stack, &mut result, *p);
        }
        side = s;
    }

    result
}
