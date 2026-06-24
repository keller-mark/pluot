use crate::point;
use crate::point::{orientation, Orientation};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;

const EPSILON: f32 = 1e-6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub p: point::Point,
    pub index: point::Index,
    pub is_top: bool,
}

impl Event {
    pub fn new(p: point::Point, index: point::Index, is_top: bool) -> Self {
        Self { p, index, is_top }
    }
}

impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Event {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.p == other.p {
            if self.is_top == other.is_top {
                self.index.cmp(&other.index)
            } else {
                // Note the reversed comparison for is_top
                other.is_top.cmp(&self.is_top)
            }
        } else {
            // Assuming Point implements PartialOrd
            self.p.cmp(&other.p)
        }
    }
}

#[derive(Default, Clone)]
struct EventData {
    tops: Vec<usize>,
    bottoms: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OrderedPair(point::Index, point::Index);

impl PartialOrd for OrderedPair {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for OrderedPair {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.0.min(self.1), self.0.max(self.1)).cmp(&(other.0.min(other.1), other.0.max(other.1)))
    }
}

impl OrderedPair {
    pub fn new(i1: point::Index, i2: point::Index) -> Self {
        OrderedPair(i1.min(i2), i1.max(i2))
    }

    pub fn first(&self) -> point::Index {
        self.0
    }

    pub fn second(&self) -> point::Index {
        self.1
    }
}

/// Checks if point `q` lies on the segment defined by points `p` and `r`, assuming all three points are collinear.
///
/// # Arguments
///
/// * `p` - A [`Point`](point::Point) representing one endpoint of the segment.
/// * `q` - A [`Point`](point::Point) to check if it lies on the segment.
/// * `r` - A [`Point`](point::Point) representing the other endpoint of the segment.
///
/// # Returns
///
///
/// * `true` - If `q` lies on the segment defined by `p` and `r`.
/// * `false` - If `q` does not lie on the segment.
///
/// # Example
///
/// ```rust
/// use triangulation::point::{Point, Segment};
/// use triangulation::intersection::on_segment_if_collinear;
///
/// let p = Point::new(0.0, 0.0);
/// let r = Point::new(4.0, 4.0);
/// let q = Point::new(2.0, 2.0);
/// let s = Segment::new(p, r);
///
/// assert!(on_segment_if_collinear(&s, q)); // `q` lies on the segment
///
/// let q_outside = Point::new(5.0, 5.0);
/// assert!(!on_segment_if_collinear(&s, q_outside)); // `q_outside` does not lie on the segment
/// ```
pub fn on_segment_if_collinear(s: &point::Segment, q: point::Point) -> bool {
    // TODO We know that point is collinear, so we may use faster code.
    s.point_on_line(q)
}

/// Determines if two segments intersect.
///
/// This function checks whether two line segments, `s1` and `s2`, intersect with each other.
///
/// # Arguments
///
/// * `s1` - A reference to the first [`Segment`](point::Segment).
/// * `s2` - A reference to the second [`Segment`](point::Segment).
///
/// # Returns
///
/// * `true` - If the segments intersect.
/// * `false` - If the segments do not intersect.
///
/// # Examples
///
/// ```rust
/// use triangulation::point::{Point, Segment};
/// use triangulation::intersection::do_intersect;
///
/// let seg1 = Segment::new(Point::new(0.0, 0.0), Point::new(4.0, 4.0));
/// let seg2 = Segment::new(Point::new(0.0, 4.0), Point::new(4.0, 0.0));
///
/// assert!(do_intersect(&seg1, &seg2)); // The segments intersect
///
/// let seg3 = Segment::new(Point::new(0.0, 0.0), Point::new(2.0, 2.0));
/// let seg4 = Segment::new(Point::new(3.0, 3.0), Point::new(4.0, 4.0));
///
/// assert!(!do_intersect(&seg3, &seg4)); // The segments do not intersect
/// ```
pub fn do_intersect(s1: &point::Segment, s2: &point::Segment) -> bool {
    let p1 = s1.bottom;
    let q1 = s1.top;
    let p2 = s2.bottom;
    let q2 = s2.top;

    let o1 = point::orientation(p1, q1, p2);
    let o2 = point::orientation(p1, q1, q2);
    let o3 = point::orientation(p2, q2, p1);
    let o4 = point::orientation(p2, q2, q1);

    if o1 != o2 && o3 != o4 {
        return true;
    }

    if o1 == point::Orientation::Collinear && on_segment_if_collinear(s1, p2) {
        return true;
    }
    if o2 == point::Orientation::Collinear && on_segment_if_collinear(s1, q2) {
        return true;
    }
    if o3 == point::Orientation::Collinear && on_segment_if_collinear(s2, p1) {
        return true;
    }
    if o4 == point::Orientation::Collinear && on_segment_if_collinear(s2, q1) {
        return true;
    }

    false
}

/// Checks if two segments share an endpoint.
///
/// This function determines whether two segments, each defined by
/// two endpoints, share any endpoint. Specifically, it checks if
/// the bottom or top endpoint of the first segment is equal to the
/// bottom or top endpoint of the second segment.
///
/// # Arguments
///
/// * `s1` - The first segment.
/// * `s2` - The second segment.
///
/// # Returns
///
/// `true` if the segments share at least one endpoint, `false` otherwise.
///
/// # Example
///
/// ```
/// use triangulation::point::{Point, Segment};
/// use triangulation::intersection::do_share_endpoint;
///
/// let s1 = Segment::new(Point::new(0.0, 0.0), Point::new(1.0, 1.0));
/// let s2 = Segment::new(Point::new(1.0, 1.0), Point::new(2.0, 2.0));
/// assert!(do_share_endpoint(&s1, &s2)); // Shared endpoint
///
/// let s3 = Segment::new(Point::new(0.0, 0.0), Point::new(1.0, 1.0));
/// let s4 = Segment::new(Point::new(2.0, 2.0), Point::new(3.0, 3.0));
/// assert!(!do_share_endpoint(&s3, &s4)); // No shared endpoint
/// ```
pub fn do_share_endpoint(s1: &point::Segment, s2: &point::Segment) -> bool {
    s1.bottom == s2.bottom || s1.bottom == s2.top || s1.top == s2.bottom || s1.top == s2.top
}

#[derive(Debug, PartialEq)]
pub enum Intersection {
    NoIntersection,
    PointIntersection(point::Point),
    CollinearNoOverlap,
    CollinearWithOverlap((point::Point, point::Point)),
}

/// Finds the intersection point of two line segments, if it exists.
///
/// This function calculates the intersection point of two given line segments.
/// Each segment is defined by two endpoints. If the segments do not intersect,
/// or are collinear and overlapping, the function returns a vector of the shared points.
/// If they are collinear and don't overlap, an empty vector is returned.
/// If they intersect at a single point, the function returns a vector containing that single point.
/// If the segments are not collinear but intersect, the function returns a vector containing the intersection point.
///
/// # Arguments
///
/// * `s1` - The first line segment.
/// * `s2` - The second line segment.
///
/// # Returns
///
/// An element of Intersection enum with intersection points
///
/// # Example
///
/// ```
/// use triangulation::point::{Point, Segment};
/// use triangulation::intersection::{find_intersection, Intersection};
///
/// let s1 = Segment::new(Point::new(0.0, 0.0), Point::new(2.0, 2.0));
/// let s2 = Segment::new(Point::new(0.0, 2.0), Point::new(2.0, 0.0));
/// let intersection = find_intersection(&s1, &s2);
/// assert_eq!(intersection, Intersection::PointIntersection(Point::new(1.0, 1.0))); // Intersecting segments
///
/// let s3 = Segment::new(Point::new(0.0, 0.0), Point::new(1.0, 1.0));
/// let s4 = Segment::new(Point::new(2.0, 2.0), Point::new(3.0, 3.0));
/// let intersection = find_intersection(&s3, &s4);
/// assert!(matches!(intersection, Intersection::CollinearNoOverlap)); // Non-intersecting segments
///
/// let s5 = Segment::new(Point::new(0.0, 0.0), Point::new(2.0, 0.0));
/// let s6 = Segment::new(Point::new(1.0, 0.0), Point::new(3.0, 0.0));
/// let intersection = find_intersection(&s5, &s6);
/// assert!(matches!(intersection, Intersection::CollinearWithOverlap(_))); // Overlapping collinear segments
///
///
/// ```
pub fn find_intersection(s1: &point::Segment, s2: &point::Segment) -> Intersection {
    let a1 = s1.top.y - s1.bottom.y;
    let b1 = s1.bottom.x - s1.top.x;
    let a2 = s2.top.y - s2.bottom.y;
    let b2 = s2.bottom.x - s2.top.x;
    let det = a1 * b2 - a2 * b1;

    if det == 0.0 {
        // collinear case
        let mut res = Vec::new();
        if s1.point_on_line(s2.bottom) {
            res.push(s2.bottom);
        }
        if s1.point_on_line(s2.top) {
            res.push(s2.top);
        }
        if s2.point_on_line(s1.bottom) {
            res.push(s1.bottom);
        }
        if s2.point_on_line(s1.top) {
            res.push(s1.top);
        }

        // remove duplicates from the collinear intersection case
        res.sort();
        res.dedup();
        if res.is_empty() {
            return Intersection::CollinearNoOverlap;
        }
        if res.len() == 1 {
            return Intersection::PointIntersection(res[0]);
        }
        return Intersection::CollinearWithOverlap((res[0], res[1]));
    }

    let t = ((s2.top.x - s1.top.x) * (s2.bottom.y - s2.top.y)
        - (s2.top.y - s1.top.y) * (s2.bottom.x - s2.top.x))
        / det;

    // clip to handle problems with floating point precision
    if t < 0.0 {
        return if t > -EPSILON {
            Intersection::PointIntersection(s1.top)
        } else {
            Intersection::NoIntersection
        };
    }
    if t > 1.0 {
        return if t < 1.0 + EPSILON {
            Intersection::PointIntersection(s1.bottom)
        } else {
            Intersection::NoIntersection
        };
    }

    let x = s1.top.x + t * b1;
    let y = s1.top.y + t * (-a1);
    Intersection::PointIntersection(point::Point { x, y })
}

/// Finds intersections among a set of line segments.
///
/// This function takes a vector of line segments and returns a set of pairs of
/// segment indices that intersect. The pairs are ordered to ensure uniqueness
/// regardless of the order of segments in the input vector.
///
/// # Arguments
///
/// * `segments` - A vector of [`Segment`](point::Segment) representing the line segments.
///
/// # Returns
///
/// A [`HashSet`] of [`OrderedPair`], where each `OrderedPair` contains the indices of two intersecting segments.
///
/// # Example
///
/// ```
/// use triangulation::point::{Point, Segment};
/// use triangulation::intersection::{find_intersections, OrderedPair};
/// use std::collections::HashSet;
///
/// let segments = vec![
///     Segment::new(Point::new(0.0, 0.0), Point::new(2.0, 2.0)),
///     Segment::new(Point::new(0.0, 2.0), Point::new(2.0, 0.0)),
/// ];
/// let intersections = find_intersections(&segments);
///
/// let expected_intersections: HashSet<OrderedPair> = [(0, 1)].iter().map(|&(a, b)| OrderedPair::new(a, b)).collect();
/// assert_eq!(intersections, expected_intersections);
/// ```
pub fn find_intersections(segments: &[point::Segment]) -> HashSet<OrderedPair> {
    let mut intersections = HashSet::new();
    let mut intersection_events: BTreeMap<point::Point, EventData> = BTreeMap::new();
    for (i, segment) in segments.iter().enumerate() {
        intersection_events
            .entry(segment.top)
            .or_default()
            .tops
            .push(i);
        intersection_events
            .entry(segment.bottom)
            .or_default()
            .bottoms
            .push(i);
    }

    let mut active: BTreeMap<point::Point, HashSet<point::Index>> = BTreeMap::new();

    while let Some((&point, event_data)) = intersection_events.iter().next_back() {
        for &event_index in &event_data.tops {
            for active_el in active.iter() {
                for &index in active_el.1 {
                    if do_intersect(&segments[event_index], &segments[index])
                        && !do_share_endpoint(&segments[event_index], &segments[index])
                    {
                        intersections.insert(OrderedPair::new(event_index, index));
                    }
                }
            }
        }
        active
            .entry(point)
            .or_default()
            .extend(event_data.tops.iter());

        for &event_index in &event_data.bottoms {
            if let Some(entry) = active.get_mut(&segments[event_index].top) {
                entry.remove(&event_index);
                if entry.is_empty() {
                    active.remove(&segments[event_index].top);
                }
            }
        }

        intersection_events.remove(&point);
    }

    intersections
}

/// Calculates the edges of polygons from a list of polygons, provided as
/// a list of points for each polygon.
///
/// Each polygon is represented by a vector of points, where each point is
/// defined as an instance of `point::Point`. The function iterates through
/// each point in each polygon and creates edges (segments) between
/// consecutive points as well as between the last point and the first point
/// of each polygon.
///
/// # Arguments
///
/// * `polygon_list` - A slice of vectors, with each vector containing `Point` instances
///   representing a polygon
///
/// # Returns
///
/// A vector of `Segment` instances representing the edges of all polygons in the input list
pub fn calc_edges(polygon_list: &[Vec<point::Point>]) -> Vec<point::Segment> {
    // Calculate total number of points for capacity pre-allocation
    let points_count: usize = polygon_list.iter().map(|polygon| polygon.len()).sum();

    // Pre-allocate the vector with the calculated capacity
    let mut edges = Vec::with_capacity(points_count);

    // Process each polygon
    for polygon in polygon_list {
        // Create edges between consecutive points
        for window in polygon.windows(2) {
            edges.push(point::Segment::new(window[0], window[1]));
        }

        // Create edge between last and first point if they're different
        if let (Some(&first), Some(&last)) = (polygon.first(), polygon.last()) {
            if last != first {
                edges.push(point::Segment::new(last, first));
            }
        }
    }

    edges
}

/// Finds and processes all intersection points between polygons in the given list.
///
/// This function performs the following steps:
/// 1. Converts polygons to edges
/// 2. Finds all intersection points between edges
/// 3. For each intersection, calculates projection factors and stores the intersection points
/// 4. Creates new polygons that include the intersection points
///
/// # Arguments
///
/// * `polygon_list` - A slice of vectors where each vector contains points defining a polygon
///
/// # Returns
///
/// A vector of modified polygons (as vectors of points) where each polygon includes
/// the intersection points with other polygons. If no intersections are found,
/// returns a copy of the input polygons.
///
/// # Examples
///
/// ```
/// use triangulation::point::Point;
/// use triangulation::intersection::find_intersection_points;
/// let polygon1 = vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0), Point::new(0.0, 1.0)];
/// let polygon2 = vec![Point::new(0.0, 0.5), Point::new(1.0, 0.5), Point::new(0.5, 0.0)];
/// let polygons = vec![polygon1, polygon2];
/// let result = find_intersection_points(&polygons);
/// // Result will contain the original polygons with additional points at their intersections
/// ```
///
/// # Note
///
/// The function preserves the order of the original polygons but may add additional points
/// where intersections occur. The resulting polygons maintain their closed nature
/// (first point equals last point if that was true in the input).
pub fn find_intersection_points(polygon_list: &[Vec<point::Point>]) -> Vec<Vec<point::Point>> {
    // Calculate edges from the polygon list
    let edges = calc_edges(polygon_list);

    // Find intersections using the existing function
    let intersections = find_intersections(&edges);
    if intersections.is_empty() {
        return polygon_list.to_vec();
    }

    // Create a HashMap to store intersection points for each edge
    let mut intersections_points: HashMap<usize, Vec<(point::Coord, point::Point)>> =
        HashMap::new();

    // Process each intersection
    for intersection in intersections {
        let inter_points =
            find_intersection(&edges[intersection.first()], &edges[intersection.second()]);

        // Handle the intersection points based on the intersection type
        match inter_points {
            Intersection::PointIntersection(point) => {
                intersections_points
                    .entry(intersection.first())
                    .or_default()
                    .push((
                        edges[intersection.first()].point_projection_factor(point),
                        point,
                    ));
                intersections_points
                    .entry(intersection.second())
                    .or_default()
                    .push((
                        edges[intersection.second()].point_projection_factor(point),
                        point,
                    ));
            }
            Intersection::CollinearWithOverlap((p1, p2)) => {
                // Handle collinear overlap case if needed
                // to add both points to both edges
                for point in [p1, p2] {
                    intersections_points
                        .entry(intersection.first())
                        .or_default()
                        .push((
                            edges[intersection.first()].point_projection_factor(point),
                            point,
                        ));
                    intersections_points
                        .entry(intersection.second())
                        .or_default()
                        .push((
                            edges[intersection.second()].point_projection_factor(point),
                            point,
                        ));
                }
            }
            _ => {} // No intersection or collinear no overlap cases
        }
    }

    // Sort and add endpoint markers for each edge's intersections
    for (edge_idx, points) in intersections_points.iter_mut() {
        let edge = &edges[*edge_idx];
        points.push((-1.0, edge.top));
        points.push((2.0, edge.bottom));
        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    }

    // Create new polygons with intersection points
    let mut new_polygons_list = Vec::with_capacity(polygon_list.len());

    let mut polygon_shift = 0;

    for polygon in polygon_list {
        let mut new_polygon = Vec::with_capacity(polygon.len() * 2);
        new_polygon.push(polygon[0]);

        for (i, point) in polygon.iter().enumerate() {
            if new_polygon.last() != Some(point) {
                new_polygon.push(*point);
            }

            if let Some(new_points) = intersections_points.get(&(i + polygon_shift)) {
                if new_points[0].1 == *point {
                    // Forward iteration
                    for (_, intersection_point) in new_points.iter().skip(1) {
                        if new_polygon.last() != Some(intersection_point) {
                            new_polygon.push(*intersection_point);
                        }
                    }
                } else {
                    // Reverse iteration
                    for (_, intersection_point) in new_points.iter().rev().skip(1) {
                        if new_polygon.last() != Some(intersection_point) {
                            new_polygon.push(*intersection_point);
                        }
                    }
                }
            }
        }

        // Remove duplicate end point if it exists
        if new_polygon.len() > 1 && new_polygon.first() == new_polygon.last() {
            new_polygon.pop();
        }

        new_polygons_list.push(new_polygon);
        polygon_shift += polygon.len();
    }

    new_polygons_list
}

/// Splits a list of polygons into collinear and non-collinear groups.
///
/// Returns a tuple where the first element contains polygons whose consecutive triplets of vertices (including wrap-around triplets) are all collinear, and the second element contains all other polygons.
fn filter_collinear_polygons(
    polygon_list: &[Vec<point::Point>],
) -> (Vec<Vec<point::Point>>, Vec<Vec<point::Point>>) {
    polygon_list.iter().cloned().partition(|polygon| {
        let n = polygon.len();
        if n < 3 {
            return true;
        }

        // Check if all triplets of vertices, including the ones that wrap around, are collinear.
        let are_all_vertices_collinear = polygon
            .windows(3)
            // Check the main body of the polygon
            .all(|w| orientation(w[0], w[1], w[2]) == Orientation::Collinear)
            // And also check the two triplets that wrap around the start/end
            && orientation(polygon[n - 2], polygon[n - 1], polygon[0]) == Orientation::Collinear
            && orientation(polygon[n - 1], polygon[0], polygon[1]) == Orientation::Collinear;

        are_all_vertices_collinear
    })
}

#[derive(Default)]
struct GraphNode {
    edges: Vec<point::Point>,
    visited: bool,
    sub_index: usize,
}

/// Splits multiple polygons into smaller polygons based on edge intersections and repeated edges using a DFS graph traversal.
///
/// This function performs two main operations:
/// 1. Finds intersection points between edges of all input polygons and splits edges at these points
/// 2. Identifies duplicated edges within the resulting polygons and splits them into multiple
///    disjoint polygonal components
///
/// The splitting is guided by the unique edges that are determined through a deduplication process.
/// All polygon intersections in the result will occur only at points, not along edges.
///
/// # Arguments
/// * `polygon_list` - A slice of vectors, where each vector contains `Point`s representing a polygon
///
/// # Returns
/// A tuple containing:
/// * A `Vec<Vec<Point>>` representing the split polygons as individual vectors of points
/// * A `Vec<Segment>` containing the deduplicated list of edges used during splitting
///
/// # Purpose
/// This function is designed for edge triangulation (from `path_triangulation.rs`) and
/// provides the edges that can be further used for face triangulation. It ensures that
/// polygon intersections occur only at vertices, not along edges.
///
/// # Examples
/// ```
/// use triangulation::point::{Point, Segment};
/// use triangulation::intersection::split_polygons_on_repeated_edges;
///
/// // Create two intersecting rectangles
/// let polygon1 = vec![
///     Point::new(0.0, 0.0),
///     Point::new(3.0, 0.0),
///     Point::new(3.0, 3.0),
///     Point::new(0.0, 3.0),
///     Point::new(0.0, 0.0),
///     Point::new(1.0, 1.0),
///     Point::new(1.0, 2.0),
///     Point::new(2.0, 2.0),
///     Point::new(2.0, 1.0),
///     Point::new(1.0, 1.0)
/// ];
///
/// let (polygons, edges) = split_polygons_on_repeated_edges(&vec![polygon1]);
///
/// // The polygons are split at intersection points
/// assert_eq!(polygons.len(), 2); // More polygons after splitting at intersections
/// ```
#[inline]
pub fn split_polygons_on_repeated_edges(
    polygon_list: &[Vec<point::Point>],
) -> (Vec<Vec<point::Point>>, Vec<point::Segment>) {
    let (mut collinear_polygons, normal_polygons) = filter_collinear_polygons(polygon_list);
    let intersected = find_intersection_points(&normal_polygons);
    let edges_dedup = point::calc_dedup_edges(&intersected);
    let mut edge_map: HashMap<point::Point, GraphNode> = HashMap::new();
    let mut sub_polygons: Vec<Vec<point::Point>> = Vec::new();
    let mut visited_edges: HashSet<point::Segment> = HashSet::new();

    // Build undirected graph
    for edge in edges_dedup.iter() {
        let p1 = edge.top;
        let p2 = edge.bottom;
        edge_map.entry(p1).or_default().edges.push(p2);
        edge_map.entry(p2).or_default().edges.push(p1);
    }

    let points = edge_map.keys().cloned().collect::<Vec<_>>();

    for point in points.iter() {
        let mut node = edge_map.get_mut(point).unwrap();
        if node.visited {
            continue;
        }
        node.visited = true;
        let mut new_polygon = Vec::new();
        new_polygon.push(*point);
        while node.sub_index < node.edges.len() {
            let next_point = node.edges[node.sub_index];
            node.sub_index += 1;
            let new_segment = point::Segment::new(*new_polygon.last().unwrap(), next_point);
            if visited_edges.contains(&new_segment) {
                continue;
            }
            visited_edges.insert(new_segment);
            node = edge_map.get_mut(&next_point).unwrap();
            node.visited = true;
            new_polygon.push(next_point);
        }
        while new_polygon.first() == new_polygon.last() && new_polygon.len() > 1 {
            new_polygon.pop();
        }
        if new_polygon.len() >= 3 {
            sub_polygons.push(new_polygon);
        }
    }
    sub_polygons.append(&mut collinear_polygons);
    (sub_polygons, edges_dedup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    fn test_filter_collinear_polygons() {
        let polygons = vec![
            vec![
                point::Point::new(0.0, 0.0),
                point::Point::new(1.0, 1.0),
                point::Point::new(2.0, 2.0),
            ], // collinear
            vec![
                point::Point::new(0.0, 0.0),
                point::Point::new(1.0, 0.0),
                point::Point::new(1.0, 1.0),
            ], // not collinear
        ];
        let (collinear, normal) = filter_collinear_polygons(&polygons);
        assert_eq!(collinear.len(), 1);
        assert_eq!(normal.len(), 1);
    }
}
