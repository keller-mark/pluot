//! This library provides computational algorithms for triangulation.
//!
//! These algorithms are designed for performance when working with polygons.

pub mod face_triangulation;
pub mod intersection;
pub mod monotone_polygon;
pub mod path_triangulation;
pub mod point;

pub use crate::face_triangulation::{
    is_convex, sweeping_line_triangulation, triangulate_convex_polygon,
};
pub use crate::intersection::split_polygons_on_repeated_edges;
pub use crate::path_triangulation::PathTriangulation;
pub use crate::path_triangulation::{triangulate_path_edge, triangulate_paths_edge};
pub use crate::point::{Point, Segment, Triangle};
