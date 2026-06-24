use crate::monotone_polygon::{triangulate_monotone_polygon, MonotonePolygon};
use crate::point::{centroid, orientation, Index, Orientation, Point, Segment, Triangle};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::rc::Rc;

/// A mapping from a point to all points connected to it by segments.
///
/// # Key (Point)
/// Represents a vertex in the geometry.
///
/// # Value (Vec<Point>)
/// Contains a list of points that are connected to the key point by segments.
type PointToSegmentEndPoints = HashMap<Point, Vec<Point>>;

/// Builds a mapping between points and their connected segment endpoints.
///
/// For each segment in the input, creates bidirectional mappings between its endpoints.
/// The resulting map contains entries for both the top and bottom points of each segment,
/// with their respective opposite endpoints stored in the value vectors.
///
/// # Arguments
/// * `segments` - A slice of segments to process
///
/// # Returns
/// A `PointToSegmentEndPoints` containing the mapping where:
/// - Each key is a point from the input segments
/// - Each value is a vector of points connected to the key point
fn get_points_segments(segments: &[Segment]) -> PointToSegmentEndPoints {
    let mut point_to_segments_ends = PointToSegmentEndPoints::new();

    // Populate the map with edges
    for edge in segments.iter() {
        point_to_segments_ends
            .entry(edge.bottom)
            .or_default()
            .push(edge.top);

        point_to_segments_ends
            .entry(edge.top)
            .or_default()
            .push(edge.bottom);
    }

    // Sort each vector of edges
    for edges_vec in point_to_segments_ends.values_mut() {
        edges_vec.sort_by(|a, b| {
            // Note: We reverse the comparison to match the C++ version
            b.cmp(a)
        });
    }

    point_to_segments_ends
}

/// Represents an interval in a sweeping line algorithm for polygon triangulation.
///
/// An interval is defined by two segments (left and right boundaries) and maintains
/// information about monotone polygons being built during the sweep line process.
///
/// # Fields
///
/// * `last_seen` - The most recently processed point on the sweep line
/// * `left_segment` - The leftmost segment that bounds this interval
/// * `right_segment` - The rightmost segment that bounds this interval
/// * `polygons_list` - Collection of monotone polygons being constructed within this interval
///
/// # Notes
///
/// This struct is part of the sweep line algorithm used in polygon triangulation.
/// It keeps track of the state between the left and right segments as the sweep line
/// moves through the polygon, helping to build monotone polygons during the process.
#[derive(Clone)]
struct Interval {
    last_seen: Point,
    left_segment: Segment,
    right_segment: Segment,
    polygons_list: Vec<MonotonePolygon>,
}

impl Interval {
    fn new(p: Point, left: Segment, right: Segment) -> Self {
        Self {
            last_seen: p,
            left_segment: left,
            right_segment: right,
            polygons_list: Vec::new(),
        }
    }

    fn with_polygon(p: Point, left: Segment, right: Segment, polygon: MonotonePolygon) -> Self {
        let polygons_list = vec![polygon];

        Self {
            last_seen: p,
            left_segment: left,
            right_segment: right,
            polygons_list,
        }
    }

    fn replace_segment(&mut self, old_segment: &Segment, new_segment: Segment) {
        if self.left_segment == *old_segment {
            self.left_segment = new_segment;
            return;
        } else if self.right_segment == *old_segment {
            self.right_segment = new_segment;
            return;
        }
        panic!("Segment not found in interval");
    }

    fn opposite_segment(&self, segment: &Segment) -> Segment {
        if *segment == self.left_segment {
            self.right_segment.clone()
        } else if *segment == self.right_segment {
            self.left_segment.clone()
        } else {
            panic!("Segment not found in interval");
        }
    }
}

// Display implementation for Interval
impl fmt::Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Last Seen: {}, Left Segment: {}, Right Segment: {}, Polygons count: {}",
            self.last_seen,
            self.left_segment,
            self.right_segment,
            self.polygons_list.len()
        )
    }
}

// Debug implementation might be useful
impl fmt::Debug for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Interval")
            .field("last_seen", &self.last_seen)
            .field("left_segment", &self.left_segment)
            .field("right_segment", &self.right_segment)
            .field("polygons_count", &self.polygons_list.len())
            .finish()
    }
}

/// Helper function to sort segments that share top points.
/// needed for process intersection point
#[inline]
fn left_right_share_top(s1: &Segment, s2: &Segment) -> Ordering {
    match orientation(s1.bottom, s1.top, s2.bottom) {
        Orientation::CounterClockwise => Ordering::Greater,
        Orientation::Clockwise => Ordering::Less,
        Orientation::Collinear => Ordering::Equal,
    }
}

/// Helper function to sort segments that share bottom points.
/// needed for process intersection point
#[inline]
fn left_right_share_bottom(s1: &Segment, s2: &Segment) -> Ordering {
    match orientation(s1.top, s1.bottom, s2.top) {
        Orientation::CounterClockwise => Ordering::Less,
        Orientation::Clockwise => Ordering::Greater,
        Orientation::Collinear => Ordering::Equal,
    }
}

/// To distinguish left and right segments for split/start point  
fn get_left_right_edges_top(s1: &Segment, s2: &Segment) -> (Segment, Segment) {
    if orientation(s1.bottom, s1.top, s2.bottom) == Orientation::CounterClockwise {
        (s2.clone(), s1.clone())
    } else {
        (s1.clone(), s2.clone())
    }
}

/// To distinguish left and right segments for merge/end point
fn get_left_right_edges_bottom(s1: &Segment, s2: &Segment) -> (Segment, Segment) {
    if orientation(s1.top, s1.bottom, s2.top) == Orientation::Clockwise {
        (s2.clone(), s1.clone())
    } else {
        (s1.clone(), s2.clone())
    }
}

/// Builder for decomposing a polygon into monotone polygons using a sweep line algorithm.
///
/// This struct maintains the state necessary for converting a polygon described by segments
/// into a collection of monotone polygons, which can then be easily triangulated.
///
/// # Fields
/// * `segment_to_line` - Maps segments to their associated intervals in the sweep line.
///   Uses interior mutability (`RefCell`) to allow modification during traversal
///   and reference counting (`Rc`) for shared ownership.
///
/// * `point_to_edges` - Maps each vertex to its connected segment endpoints, maintaining the
///   topological relationships between points in the polygon.
///
/// * `monotone_polygons` - Accumulates the resulting monotone polygons as they are constructed
///   during the sweep line process.
///
/// # Algorithm
/// The builder implements a sweep line algorithm that:
/// 1. Processes vertices in order
/// 2. Maintains active intervals between segments
/// 3. Builds monotone polygons incrementally
///
/// This implementation follows the standard polygon triangulation algorithm that first
/// decomposes a polygon into monotone pieces before triangulation.
struct MonotonePolygonBuilder {
    segment_to_line: HashMap<Segment, Rc<RefCell<Interval>>>,
    point_to_edges: PointToSegmentEndPoints,
    monotone_polygons: Vec<MonotonePolygon>,
}

impl MonotonePolygonBuilder {
    pub fn new(edges: Vec<Segment>) -> Self {
        let point_to_edges = get_points_segments(&edges);
        Self {
            segment_to_line: HashMap::new(),
            point_to_edges,
            monotone_polygons: Vec::new(),
        }
    }

    fn process_end_point(
        &mut self,
        p: Point,
        edge_left: Segment,
        edge_right: Segment,
        interval: Rc<RefCell<Interval>>,
    ) {
        for polygon in &interval.borrow().polygons_list {
            let mut polygon = polygon.clone();
            polygon.bottom = Some(p);
            self.monotone_polygons.push(polygon);
        }

        self.segment_to_line.remove(&edge_left);
        self.segment_to_line.remove(&edge_right);
    }

    fn process_merge_point(&mut self, p: Point, edge_left: Segment, edge_right: Segment) {
        let left_interval_ref = self
            .segment_to_line
            .get_mut(&edge_left)
            .expect("Left edge interval not found")
            .clone();
        let right_interval_ref = self
            .segment_to_line
            .get(&edge_right)
            .expect("Right edge interval not found")
            .clone();

        if !Rc::ptr_eq(&left_interval_ref, &right_interval_ref) {
            #[cfg(debug_assertions)]
            {
                if right_interval_ref.borrow().right_segment == edge_right {
                    panic!(
                        "The right edge of merge point should be the left edge of the right interval.\n\
                        Got interval: {:?} and edge: {:?}",
                        right_interval_ref, edge_right
                    );
                }
            }

            self.segment_to_line.remove(&edge_right);
            self.segment_to_line.remove(&edge_left);

            left_interval_ref.borrow_mut().right_segment =
                right_interval_ref.borrow().right_segment.clone();

            #[cfg(debug_assertions)]
            {
                if !self
                    .segment_to_line
                    .contains_key(&right_interval_ref.borrow().right_segment)
                {
                    panic!("Segment not found in the map2");
                }
            }

            self.segment_to_line.insert(
                right_interval_ref.borrow().right_segment.clone(),
                left_interval_ref.clone(),
            );
            left_interval_ref.borrow_mut().last_seen = p;

            // Update polygons
            if let Some(last_polygon) = left_interval_ref.borrow_mut().polygons_list.last_mut() {
                last_polygon.right.push(p);
            }
            if let Some(first_polygon) = right_interval_ref.borrow_mut().polygons_list.first_mut() {
                first_polygon.left.push(p);
            }

            // Move polygons from right interval to left interval
            left_interval_ref
                .borrow_mut()
                .polygons_list
                .append(&mut right_interval_ref.borrow_mut().polygons_list);
        } else {
            // This is the end point
            self.process_end_point(p, edge_left, edge_right, left_interval_ref);
        }
    }

    fn process_normal_point(
        &mut self,
        p: Point,
        edge_top: Segment,
        edge_bottom: Segment,
    ) -> Result<(), String> {
        let interval_ref = self
            .segment_to_line
            .get_mut(&edge_top)
            .ok_or_else(|| "Segment not found in the map".to_string())?
            .clone();

        let mut interval = interval_ref.borrow_mut();

        if interval.polygons_list.len() > 1 {
            if edge_top == interval.right_segment {
                // We are on right side of the interval
                // End all polygons except the first one
                for poly in interval.polygons_list.iter_mut().skip(1) {
                    let mut polygon = poly.clone();
                    polygon.bottom = Some(p);
                    self.monotone_polygons.push(polygon);
                }
            } else {
                // We are on left side of the interval
                // End all polygons except the last one
                if let Some((last, all_but_last)) = interval.polygons_list.split_last_mut() {
                    for poly in all_but_last {
                        let mut polygon = poly.clone();
                        polygon.bottom = Some(p);
                        self.monotone_polygons.push(polygon);
                    }
                    interval.polygons_list[0] = last.to_owned();
                }
            }
            interval.polygons_list.truncate(1);
        }

        // Update the remaining polygon
        if edge_top == interval.right_segment {
            if let Some(polygon) = interval.polygons_list.first_mut() {
                polygon.right.push(p);
            }
        } else if let Some(polygon) = interval.polygons_list.first_mut() {
            polygon.left.push(p);
        }

        self.segment_to_line
            .insert(edge_bottom.clone(), interval_ref.clone());
        interval.last_seen = p;
        interval.replace_segment(&edge_top, edge_bottom);
        let val = self.segment_to_line.remove(&edge_top);
        if val.is_none() {
            panic!("Segment not found in the map");
        }

        #[cfg(debug_assertions)]
        {
            if !self.segment_to_line.contains_key(&interval.left_segment) {
                return Err("Left segment not found in the map".to_string());
            }
            if !self.segment_to_line.contains_key(&interval.right_segment) {
                return Err("Right segment not found in the map".to_string());
            }
        }

        Ok(())
    }

    fn process_start_point(&mut self, p: Point, edge_left: Segment, edge_right: Segment) {
        let mut_interval = Interval::with_polygon(
            p,
            edge_left.clone(),
            edge_right.clone(),
            MonotonePolygon::new_top(p),
        );

        let line_interval = Rc::new(RefCell::new(mut_interval));
        self.segment_to_line
            .insert(edge_left, line_interval.clone());
        self.segment_to_line.insert(edge_right, line_interval);
    }

    fn find_interval_with_point(&self, p: Point) -> Option<Rc<RefCell<Interval>>> {
        for (segment, interval) in self.segment_to_line.iter() {
            if *segment == interval.borrow().right_segment {
                // as each interval is listed twice we
                // skip this pointed by right segment to avoid duplication
                continue;
            }
            if interval.borrow().left_segment.point_on_line_x(p.y) < p.x
                && interval.borrow().right_segment.point_on_line_x(p.y) > p.x
            {
                // we are inside the interval
                return Some(interval.clone());
            }
        }
        None
    }

    fn process_split_point(&mut self, p: Point, edge_left: Segment, edge_right: Segment) {
        if let Some(interval) = self.find_interval_with_point(p) {
            let right_segment = interval.borrow().right_segment.clone();
            self.segment_to_line
                .remove(&interval.borrow_mut().right_segment);
            interval.borrow_mut().right_segment = edge_left.clone();
            interval.borrow_mut().last_seen = p;
            self.segment_to_line
                .insert(edge_left.clone(), interval.clone());

            let new_interval = Rc::new(RefCell::new(Interval::new(
                p,
                edge_right.clone(),
                right_segment.clone(),
            )));
            self.segment_to_line
                .insert(edge_right.clone(), new_interval.clone());
            self.segment_to_line
                .insert(right_segment, new_interval.clone());

            if interval.borrow().polygons_list.len() == 1 {
                let mut new_polygon = if interval.borrow().polygons_list[0].right.is_empty() {
                    MonotonePolygon::new_top(interval.borrow().polygons_list[0].top)
                } else {
                    MonotonePolygon::new_top(
                        *interval.borrow().polygons_list[0].right.last().unwrap(),
                    )
                };
                new_polygon.left.push(p);
                new_interval.borrow_mut().polygons_list.push(new_polygon);
                interval.borrow_mut().polygons_list[0].right.push(p);
            }
            if interval.borrow().polygons_list.len() >= 2 {
                interval.borrow_mut().polygons_list[0].right.push(p);
                interval
                    .borrow_mut()
                    .polygons_list
                    .last_mut()
                    .unwrap()
                    .left
                    .push(p);
                for polygon in interval
                    .borrow()
                    .polygons_list
                    .iter()
                    .skip(1)
                    .take(interval.borrow().polygons_list.len() - 2)
                {
                    let mut poly = polygon.clone();
                    poly.bottom = Some(p);
                    self.monotone_polygons.push(poly);
                }
                if let Some(last_polygon) = interval.borrow_mut().polygons_list.pop() {
                    new_interval.borrow_mut().polygons_list.push(last_polygon)
                }
                interval.borrow_mut().polygons_list.truncate(1);
            }
        } else {
            self.process_start_point(p, edge_left, edge_right);
        }
    }

    fn process_intersection_point(&mut self, p: Point, edges: Vec<Segment>) -> Result<(), String> {
        let mut processed_segments = BTreeSet::new();
        let mut segments_to_normal_process = Vec::new();
        let mut top_segments = Vec::new();
        let mut bottom_segments = Vec::new();

        for edge in edges.iter() {
            if processed_segments.contains(edge) {
                continue;
            }
            if self.segment_to_line.contains_key(edge) {
                let interval = self.segment_to_line.get(edge).unwrap();
                let opposite_edge = interval.borrow().opposite_segment(edge);
                if edge.bottom == p && opposite_edge.bottom == p {
                    self.process_end_point(
                        p,
                        edge.clone(),
                        opposite_edge.clone(),
                        interval.clone(),
                    );
                    processed_segments.insert(opposite_edge);
                    processed_segments.insert(edge.clone());
                    continue;
                }
            }
            if edge.top == p {
                bottom_segments.push(edge.clone());
            } else {
                top_segments.push(edge.clone());
            }
            segments_to_normal_process.push(edge.clone());
        }
        if bottom_segments.is_empty() && top_segments.is_empty() {
            return Ok(());
        }

        top_segments.sort_by(left_right_share_bottom);
        bottom_segments.sort_by(left_right_share_top);

        let mut bottom_begin = bottom_segments.iter().peekable();
        let mut top_begin = top_segments.iter();
        if !top_segments.is_empty() {
            let first_top = top_segments.first().unwrap();
            if *first_top == self.segment_to_line[first_top].borrow().right_segment {
                top_begin.next();
                let first_bottom = bottom_begin.next().unwrap();
                self.process_normal_point(p, first_top.clone(), first_bottom.clone())?;
            }
            let top_segments_last = top_segments.last().unwrap();
            if top_begin.count() > 0
                && *top_segments_last
                    == self.segment_to_line[top_segments_last]
                        .borrow()
                        .left_segment
            {
                let last_bottom = bottom_begin.next_back().unwrap();
                self.process_normal_point(p, top_segments_last.clone(), last_bottom.clone())?;
            }
        }
        while bottom_begin.peek().is_some() {
            self.process_start_point(
                p,
                bottom_begin.next().unwrap().clone(),
                bottom_begin.next().unwrap().clone(),
            );
        }

        Ok(())
    }
}

/// Enum for encode point type for sweeping line algorithm  
#[derive(Debug, PartialEq, Eq)]
pub enum PointType {
    Intersection(Vec<Segment>),
    Split(Segment, Segment),
    Merge(Segment, Segment),
    Normal(Segment, Segment),
}

/// Get point type based on adjacent segm
fn get_point_type(p: Point, point_to_edges: &PointToSegmentEndPoints) -> PointType {
    match point_to_edges.get(&p) {
        None => panic!("Point not found in the map"),
        Some(opposite_point) => {
            if opposite_point.is_empty() {
                panic!("Empty point found in the map");
            }

            // Convert edge info to segments
            let segments: Vec<Segment> = opposite_point
                .iter()
                .map(|opposite_point| Segment::new(p, *opposite_point))
                .collect();

            if segments.len() != 2 {
                return PointType::Intersection(segments);
            }

            let (seg1, seg2) = (segments[0].clone(), segments[1].clone());

            // Both opposite points are less than p -> Split point
            if opposite_point[0] < p && opposite_point[1] < p {
                let (left, right) = get_left_right_edges_top(&seg1, &seg2);
                return PointType::Split(left, right);
            }
            // Both opposite points are greater than p -> Merge point
            if p < opposite_point[0] && p < opposite_point[1] {
                let (left, right) = get_left_right_edges_bottom(&seg1, &seg2);
                return PointType::Merge(left, right);
            }
            // Otherwise it's a normal point
            PointType::Normal(seg1, seg2)
        }
    }
}

/// Triangulates a collection of monotone polygons and returns the resulting triangles with vertex indices.
///
/// This function converts geometric triangulation (using points) into a topological representation
/// using vertex indices. Each resulting triangle references vertices by their indices in the input
/// points array rather than by their geometric coordinates.
///
/// # Arguments
///
/// * `monotone_polygons` - A slice of monotone polygons to be triangulated
/// * `points` - A slice of all vertices in the geometry. Used to map Points to their indices
///
/// # Returns
///
/// A vector of triangles where each triangle's vertices are represented by indices into the `points` array.
fn triangulate_monotone_polygons(
    monotone_polygons: &[MonotonePolygon],
    points: &[Point],
) -> Vec<Triangle> {
    let mut triangles = vec![];
    let point_to_index = points
        .iter()
        .enumerate()
        .map(|(i, p)| (*p, i))
        .collect::<HashMap<Point, Index>>();
    for monotone_polygon in monotone_polygons {
        let point_triangles = triangulate_monotone_polygon(monotone_polygon);
        for triangle in point_triangles {
            triangles.push(Triangle::new(
                point_to_index[&triangle.p1],
                point_to_index[&triangle.p2],
                point_to_index[&triangle.p3],
            ));
        }
    }
    triangles
}

/// Triangulates a polygon using a sweep line algorithm.
///
/// Implements polygon triangulation by first decomposing the input into monotone polygons
/// and then triangulating each monotone piece. Uses a top-to-bottom sweep line approach.
///
/// # Algorithm Steps
/// 1. Builds initial point-to-edge relationships
/// 2. Processes vertices in descending y-coordinate order
/// 3. Handles four types of vertices:
///    - Intersection points (multiple segments meet)
///    - Split points (vertex splits the polygon)
///    - Merge points (vertex merges polygon parts)
///    - Normal points (regular vertices)
///
/// # Arguments
/// * `edges` - Vector of segments describing the polygon boundary
///
/// # Returns
/// A tuple containing:
/// * `Vec<Triangle>` - The resulting triangulation as vertex-indexed triangles
/// * `Vec<Point>` - Sorted list of all vertices (used for index reference)
///
/// # Panics
/// May panic if the input geometry is invalid or contains self-intersections
/// that cannot be properly handled.
///
/// # Example
/// ```
/// use triangulation::{sweeping_line_triangulation,Point,Segment};
///
/// let edges = vec![
///     Segment::new(Point::new(0.0, 0.0), Point::new(1.0, 0.0)),
///     Segment::new(Point::new(1.0, 0.0), Point::new(0.5, 1.0)),
///     Segment::new(Point::new(0.5, 1.0), Point::new(0.0, 0.0)),
/// ];
/// let (triangles, points) = sweeping_line_triangulation(edges);
/// ```
pub fn sweeping_line_triangulation(edges: Vec<Segment>) -> (Vec<Triangle>, Vec<Point>) {
    let mut builder = MonotonePolygonBuilder::new(edges);
    let mut points = builder
        .point_to_edges
        .keys()
        .cloned()
        .collect::<Vec<Point>>();
    points.sort();
    points.reverse();
    for p in &points {
        let point_type = get_point_type(*p, &builder.point_to_edges);
        match point_type {
            PointType::Intersection(li) => {
                builder.process_intersection_point(*p, li).unwrap();
            }
            PointType::Split(left, right) => {
                builder.process_split_point(*p, left, right);
            }
            PointType::Merge(left, right) => {
                builder.process_merge_point(*p, left, right);
            }
            PointType::Normal(top, bottom) => {
                builder.process_normal_point(*p, top, bottom).unwrap();
            }
        }
        #[cfg(debug_assertions)]
        {
            for segment in builder.segment_to_line.keys() {
                let val = builder.segment_to_line.get(segment).unwrap().borrow();
                if val.left_segment != segment.clone() && val.right_segment != segment.clone() {
                    panic!("Segment not found in the map");
                }
            }
        }
    }
    (
        triangulate_monotone_polygons(&builder.monotone_polygons, &points),
        points,
    )
}

/// Check if a polygon is convex.
///
/// This function determines if a given polygon is convex by examining its vertices.
/// A polygon is convex if all its interior angles are less than or equal to 180 degrees.
/// This is determined by checking the orientations of consecutive vertex triplets.
///
/// The polygon is considered non-convex if:
/// - At least one vertex has clockwise orientation while another has counterclockwise orientation
/// - All vertices are collinear
///
/// # Arguments
/// * `points` - A slice containing the polygon vertices in order
///
/// # Returns
/// `true` if the polygon is convex, `false` otherwise
pub fn is_convex(points: &[Point]) -> bool {
    if points.len() < 3 {
        return false;
    }
    if points.len() == 3 {
        return true;
    }

    let mut orientation_ = Orientation::Collinear;
    let mut triangle_orientation;
    let mut idx = 0;

    // Find first non-collinear orientation
    for i in 0..points.len() - 2 {
        triangle_orientation = orientation(points[i], points[i + 1], points[i + 2]);
        if triangle_orientation != Orientation::Collinear {
            orientation_ = triangle_orientation;
            idx = i;
            break;
        }
    }

    // If all points are collinear, it's not a convex polygon
    if orientation_ == Orientation::Collinear {
        return false;
    }

    // Check remaining vertices
    for i in idx..points.len() - 2 {
        triangle_orientation = orientation(points[i], points[i + 1], points[i + 2]);
        if triangle_orientation != Orientation::Collinear && triangle_orientation != orientation_ {
            return false;
        }
    }

    // Check wrapping vertices
    triangle_orientation = orientation(
        points[points.len() - 2],
        points[points.len() - 1],
        points[0],
    );
    if triangle_orientation != Orientation::Collinear && triangle_orientation != orientation_ {
        return false;
    }

    triangle_orientation = orientation(points[points.len() - 1], points[0], points[1]);
    if triangle_orientation != Orientation::Collinear && triangle_orientation != orientation_ {
        return false;
    }

    // Calculate centroid
    let centroid = centroid(points);

    // Check if polygon is simple (no self-intersections)
    if orientation_ == Orientation::CounterClockwise {
        is_simple_polygon(points.iter(), centroid)
    } else {
        is_simple_polygon(points.iter().rev(), centroid)
    }
}

/// Check for a simple polygon where all angles have the same orientation but do not have self-intersections.
///
/// # Arguments
/// * `begin` - Iterator to the first point of the polygon
/// * `end` - Iterator to the end of the polygon
/// * `centroid` - Centroid of the polygon
///
/// # Returns
/// `true` if the polygon is simple, `false` otherwise
fn is_simple_polygon<'a, I>(mut iter: I, centroid: Point) -> bool
where
    I: Iterator<Item = &'a Point>,
{
    let first = iter.next().unwrap();
    let start_angle = f32::atan2(first.y - centroid.y, first.x - centroid.x);
    // We calculate the angle between starting point, centroid and current point
    // so we start with 0.0
    let mut prev_angle = 0.0;

    for point in iter {
        let mut angle = f32::atan2(point.y - centroid.y, point.x - centroid.x) - start_angle;
        if angle < 0.0 {
            angle += 2.0 * std::f32::consts::PI;
        }
        if angle < prev_angle {
            return false;
        }
        prev_angle = angle;
    }
    true
}

pub fn triangulate_convex_polygon(points: &[Point]) -> Vec<Triangle> {
    let mut triangles = Vec::new();
    for i in 1..points.len() - 1 {
        triangles.push(Triangle::new(0, i as Index, (i + 1) as Index));
    }
    triangles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::point::calc_dedup_edges;
    use rstest::rstest;

    const DIAMOND: [Point; 4] = [
        Point::new(1.0, 0.0),
        Point::new(2.0, 1.0),
        Point::new(1.0, 2.0),
        Point::new(0.0, 1.0),
    ];

    #[rstest]
    fn test_get_points_edges_diamond() {
        let edges = calc_dedup_edges(&vec![DIAMOND.to_vec()]);
        let point_to_edges = get_points_segments(&edges);
        assert_eq!(point_to_edges.len(), 4);
        assert_eq!(
            point_to_edges[&Point::new(1.0, 0.0)],
            vec![Point::new(2.0, 1.0), Point::new(0.0, 1.0)]
        );
        assert_eq!(
            point_to_edges[&Point::new(2.0, 1.0)],
            vec![Point::new(1.0, 2.0), Point::new(1.0, 0.0)]
        );
        assert_eq!(
            point_to_edges[&Point::new(1.0, 2.0)],
            vec![Point::new(2.0, 1.0), Point::new(0.0, 1.0)]
        );
        assert_eq!(
            point_to_edges[&Point::new(0.0, 1.0)],
            vec![Point::new(1.0, 2.0), Point::new(1.0, 0.0)]
        );
    }

    #[rstest]
    fn test_get_point_type() {
        let edges = calc_dedup_edges(&vec![DIAMOND.to_vec()]);
        let point_to_edges = get_points_segments(&edges);
        assert_eq!(
            get_point_type(Point::new(1.0, 2.0), &point_to_edges),
            PointType::Split(
                Segment::new(Point::new(1.0, 2.0), Point::new(0.0, 1.0)),
                Segment::new(Point::new(1.0, 2.0), Point::new(2.0, 1.0))
            )
        );
        assert_eq!(
            get_point_type(Point::new(1.0, 0.0), &point_to_edges),
            PointType::Merge(
                Segment::new(Point::new(1.0, 0.0), Point::new(0.0, 1.0)),
                Segment::new(Point::new(1.0, 0.0), Point::new(2.0, 1.0)),
            )
        );
        assert_eq!(
            get_point_type(Point::new(2.0, 1.0), &point_to_edges),
            PointType::Normal(
                Segment::new(Point::new(2.0, 1.0), Point::new(1.0, 2.0)),
                Segment::new(Point::new(2.0, 1.0), Point::new(1.0, 0.0))
            )
        );
        assert_eq!(
            get_point_type(Point::new(0.0, 1.0), &point_to_edges),
            PointType::Normal(
                Segment::new(Point::new(0.0, 1.0), Point::new(1.0, 2.0)),
                Segment::new(Point::new(0.0, 1.0), Point::new(1.0, 0.0))
            )
        );
    }

    #[rstest]
    fn tets_sort_segments_comparator_share_bottom() {
        let mut segments = vec![
            Segment::new(Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 1.0 }),
            Segment::new(Point { x: 0.0, y: 0.0 }, Point { x: 0.0, y: 1.0 }),
            Segment::new(Point { x: 0.0, y: 0.0 }, Point { x: -1.0, y: 1.0 }),
        ];

        let expected_segments = vec![
            Segment::new(Point { x: 0.0, y: 0.0 }, Point { x: -1.0, y: 1.0 }),
            Segment::new(Point { x: 0.0, y: 0.0 }, Point { x: 0.0, y: 1.0 }),
            Segment::new(Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 1.0 }),
        ];
        segments.sort_by(left_right_share_bottom);
        assert_eq!(segments, expected_segments);
        // bottom_segments.sort_by(left_right_share_top);
    }
    #[rstest]
    fn tets_sort_segments_comparator_share_top() {
        let mut segments = vec![
            Segment::new(Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: -1.0 }),
            Segment::new(Point { x: 0.0, y: 0.0 }, Point { x: 0.0, y: -1.0 }),
            Segment::new(Point { x: 0.0, y: 0.0 }, Point { x: -1.0, y: -1.0 }),
        ];

        let expected_segments = vec![
            Segment::new(Point { x: 0.0, y: 0.0 }, Point { x: -1.0, y: -1.0 }),
            Segment::new(Point { x: 0.0, y: 0.0 }, Point { x: 0.0, y: -1.0 }),
            Segment::new(Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: -1.0 }),
        ];
        segments.sort_by(left_right_share_top);
        assert_eq!(segments, expected_segments);
    }

    fn rotation_matrix(angle: f32) -> [[f32; 2]; 2] {
        let angle = angle.to_radians();
        let (sin, cos) = angle.sin_cos();
        [[cos, -sin], [sin, cos]]
    }

    fn generate_polygon_by_angle(
        n: i32,
        reversed: bool,
        rotation_angle: f32,
        angle: f32,
    ) -> Vec<Point> {
        let rotation = rotation_matrix(rotation_angle);
        let mut points = Vec::new();
        let mut current_angle: f32 = 0.0;
        for _ in 0..n {
            let (sin, cos) = current_angle.to_radians().sin_cos();
            let point = Point::new(cos, sin);
            points.push(point);
            current_angle += angle;
        }
        if reversed {
            points.reverse();
        }
        points
            .iter()
            .map(|p| {
                Point::new(
                    rotation[0][0] * p.x + rotation[0][1] * p.y,
                    rotation[1][0] * p.x + rotation[1][1] * p.y,
                )
            })
            .collect()
    }

    fn generate_regular_polygon(n: i32, reversed: bool, rotation_angle: f32) -> Vec<Point> {
        let angle = 360.0 / n as f32;
        generate_polygon_by_angle(n, reversed, rotation_angle, angle)
    }

    fn generate_self_intersecting_polygon(
        n: i32,
        reversed: bool,
        rotation_angle: f32,
    ) -> Vec<Point> {
        let angle = 2.0 * 360.0 / n as f32;
        generate_polygon_by_angle(n, reversed, rotation_angle, angle)
    }

    #[rstest]
    #[case(3, true, 0.0)]
    #[case(5, true, 0.0)]
    #[case(5, false, 0.0)]
    #[case(5, true, 5.0)]
    #[case(5, false, 5.0)]
    #[case(10, true, 75.0)]
    #[case(10, false, 75.0)]
    #[case(20, true, 345.0)]
    #[case(20, false, 345.0)]
    fn test_is_convex_regular_polygon(
        #[case] n: i32,
        #[case] reversed: bool,
        #[case] rotation: f32,
    ) {
        let points = generate_regular_polygon(n, reversed, rotation);
        assert!(is_convex(&points));
    }

    #[rstest]
    #[case(5, true, 0.0)]
    #[case(5, false, 0.0)]
    #[case(5, true, 5.0)]
    #[case(5, false, 5.0)]
    #[case(11, true, 75.0)]
    #[case(11, false, 75.0)]
    #[case(21, true, 345.0)]
    #[case(21, false, 345.0)]
    fn test_is_convex_self_intersection(
        #[case] n: i32,
        #[case] reversed: bool,
        #[case] rotation: f32,
    ) {
        let points = generate_self_intersecting_polygon(n, reversed, rotation);
        assert!(!is_convex(&points));
    }
}
