//! Computing the transformation between two OME-Zarr coordinate systems.
//!
//! OME-Zarr v0.6 (RFC-5) metadata declares named coordinate systems and a set
//! of coordinate transformations between them, which together form a graph. To
//! map points from one coordinate system to another, we find a path through
//! that graph and compose the transformation of each edge, inverting the edges
//! that are traversed against their declared direction.
//!
//! Only the transformation types that can be represented exactly as a single
//! affine matrix are supported: `identity`, `scale`, `translation`, `mapAxis`,
//! `affine`, `rotation`, and `sequence` (of the preceding types). Crossing an
//! edge of any other type is reported as [`TransformationError::UnsupportedType`].
//!
//! Reference: <https://ngff.openmicroscopy.org/rfc/5/index.html>

use std::collections::{HashMap, VecDeque};
use std::fmt;

use super::affine::AffineMatrix;
use super::metadata::{CoordinateSystem, CoordinateSystemRef, CoordinateTransformation, Transformation};

/// Errors produced while building or traversing a [`TransformationGraph`].
#[derive(Debug, Clone, PartialEq)]
pub enum TransformationError {
    /// The metadata does not declare or reference a coordinate system by this name.
    UnknownCoordinateSystem(String),
    /// No sequence of transformations connects the two coordinate systems.
    NoPath {
        /// The requested source coordinate system.
        source: String,
        /// The requested target coordinate system.
        target: String,
    },
    /// A transformation on the path uses a type that cannot be represented as
    /// an affine matrix.
    UnsupportedType {
        /// A label identifying the transformation.
        transformation: String,
        /// The value of the transformation's `type` field.
        type_name: String,
    },
    /// A transformation on the path had to be traversed in reverse but cannot
    /// be inverted.
    NotInvertible {
        /// A label identifying the transformation.
        transformation: String,
        /// Why the inverse could not be computed.
        reason: String,
    },
    /// The number of dimensions of a coordinate system could not be determined,
    /// which is needed to expand dimension-agnostic types such as `identity`.
    UnknownDimensionality(String),
    /// The metadata is malformed or internally inconsistent.
    Invalid(String),
}

impl fmt::Display for TransformationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCoordinateSystem(name) => {
                write!(f, "coordinate system \"{name}\" is not defined")
            }
            Self::NoPath { source, target } => write!(
                f,
                "no sequence of coordinate transformations leads from \"{source}\" to \"{target}\"",
            ),
            Self::UnsupportedType { transformation, type_name } => write!(
                f,
                "coordinate transformation {transformation} uses unsupported type \"{type_name}\"",
            ),
            Self::NotInvertible { transformation, reason } => write!(
                f,
                "coordinate transformation {transformation} cannot be inverted: {reason}",
            ),
            Self::UnknownDimensionality(name) => write!(
                f,
                "cannot determine the number of dimensions of coordinate system \"{name}\"",
            ),
            Self::Invalid(message) => write!(f, "invalid coordinate transformation metadata: {message}"),
        }
    }
}

impl std::error::Error for TransformationError {}

/// Identifies a coordinate system within a [`TransformationGraph`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CoordinateSystemId {
    /// Path of the Zarr node that declares the coordinate system. `None` means
    /// the node whose metadata is being read.
    pub path: Option<String>,
    /// Coordinate system name.
    pub name: String,
}

impl CoordinateSystemId {
    /// An id for a coordinate system declared in the node being read.
    pub fn named(name: impl Into<String>) -> Self {
        Self { path: None, name: name.into() }
    }

    /// An id for the implicit array coordinate system of the Zarr array at
    /// `path`, whose default name is the array path itself.
    pub fn array(path: impl Into<String>) -> Self {
        let path = path.into();
        Self { name: path.clone(), path: Some(path) }
    }
}

impl fmt::Display for CoordinateSystemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.path {
            Some(path) if path != &self.name => write!(f, "{path}:{}", self.name),
            _ => write!(f, "{}", self.name),
        }
    }
}

impl CoordinateSystemRef {
    /// Resolve this reference to a graph node id.
    ///
    /// A reference with only a `path` denotes the implicit array coordinate
    /// system of that Zarr array, whose default name is the array path.
    pub fn id(&self) -> Option<CoordinateSystemId> {
        let name = self.name.clone().or_else(|| self.path.clone())?;
        Some(CoordinateSystemId { path: self.path.clone(), name })
    }
}

/// Which way an edge is being traversed relative to its declared direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    /// From the transformation's `input` to its `output`.
    Forward,
    /// From the transformation's `output` to its `input`, requiring the
    /// transformation to be inverted.
    Reverse,
}

/// One declared coordinate transformation, as an edge of the graph.
struct Edge {
    from: CoordinateSystemId,
    to: CoordinateSystemId,
    transformation: CoordinateTransformation,
}

/// A graph whose nodes are coordinate systems and whose edges are coordinate
/// transformations.
///
/// Edges can be traversed in either direction: following an edge backwards
/// inverts its transformation, which is what lets a target coordinate system be
/// reached even when the metadata only declares the transformation the other
/// way round.
#[derive(Default)]
pub struct TransformationGraph {
    /// Coordinate systems whose axes are declared in the metadata.
    coordinate_systems: HashMap<CoordinateSystemId, CoordinateSystem>,
    /// Edges, in declaration order.
    edges: Vec<Edge>,
    /// Nodes in first-seen order, so that traversal is deterministic.
    nodes: Vec<CoordinateSystemId>,
    /// For each node, the edges incident on it and the direction they would be
    /// traversed in when leaving that node.
    adjacency: HashMap<CoordinateSystemId, Vec<(usize, Direction)>>,
}

impl TransformationGraph {
    /// An empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a graph from the `ome` attributes object of a Zarr group.
    ///
    /// Coordinate systems and transformations are collected from `ome.scene`,
    /// from each entry of `ome.multiscales`, and from each `datasets` entry of
    /// those multiscales. Transformations that do not declare both an `input`
    /// and an `output` (the OME-Zarr v0.4/v0.5 style) are not edges of the
    /// graph and are ignored here.
    pub fn from_ome_attributes(ome: &serde_json::Value) -> Result<Self, TransformationError> {
        let mut graph = Self::new();
        if let Some(scene) = ome.get("scene") {
            graph.add_json(scene)?;
        }
        if let Some(multiscales) = ome.get("multiscales").and_then(|v| v.as_array()) {
            for multiscale in multiscales {
                graph.add_json(multiscale)?;
                if let Some(datasets) = multiscale.get("datasets").and_then(|v| v.as_array()) {
                    for dataset in datasets {
                        graph.add_json(dataset)?;
                    }
                }
            }
        }
        Ok(graph)
    }

    /// Add any `coordinateSystems` and `coordinateTransformations` declared
    /// directly on `value`.
    fn add_json(&mut self, value: &serde_json::Value) -> Result<(), TransformationError> {
        if let Some(systems) = value.get("coordinateSystems") {
            let systems: Vec<CoordinateSystem> = serde_json::from_value(systems.clone())
                .map_err(|e| TransformationError::Invalid(format!("coordinateSystems: {e}")))?;
            self.add_coordinate_systems(systems);
        }
        if let Some(transformations) = value.get("coordinateTransformations") {
            let transformations: Vec<CoordinateTransformation> =
                serde_json::from_value(transformations.clone()).map_err(|e| {
                    TransformationError::Invalid(format!("coordinateTransformations: {e}"))
                })?;
            self.add_transformations(transformations);
        }
        Ok(())
    }

    /// Declare coordinate systems, making their axes available to the graph.
    pub fn add_coordinate_systems(&mut self, systems: impl IntoIterator<Item = CoordinateSystem>) {
        for system in systems {
            let id = CoordinateSystemId::named(system.name.clone());
            self.register_node(&id);
            self.coordinate_systems.insert(id, system);
        }
    }

    /// Add transformations as edges. Transformations without both an `input` and
    /// an `output` are skipped, since they do not connect two coordinate systems.
    pub fn add_transformations(
        &mut self,
        transformations: impl IntoIterator<Item = CoordinateTransformation>,
    ) {
        for transformation in transformations {
            let (Some(input), Some(output)) = (
                transformation.input.as_ref().and_then(CoordinateSystemRef::id),
                transformation.output.as_ref().and_then(CoordinateSystemRef::id),
            ) else {
                continue;
            };
            let edge_i = self.edges.len();
            self.register_node(&input);
            self.register_node(&output);
            self.adjacency
                .entry(input.clone())
                .or_default()
                .push((edge_i, Direction::Forward));
            self.adjacency
                .entry(output.clone())
                .or_default()
                .push((edge_i, Direction::Reverse));
            self.edges.push(Edge { from: input, to: output, transformation });
        }
    }

    fn register_node(&mut self, id: &CoordinateSystemId) {
        if !self.adjacency.contains_key(id) {
            self.nodes.push(id.clone());
            self.adjacency.insert(id.clone(), Vec::new());
        }
    }

    /// Whether the graph knows about this coordinate system, either because its
    /// axes were declared or because a transformation references it.
    pub fn contains(&self, id: &CoordinateSystemId) -> bool {
        self.adjacency.contains_key(id)
    }

    /// The coordinate systems in the graph, in first-seen order.
    pub fn node_ids(&self) -> &[CoordinateSystemId] {
        &self.nodes
    }

    /// The declared axes of a coordinate system, if the metadata declares them.
    pub fn coordinate_system(&self, id: &CoordinateSystemId) -> Option<&CoordinateSystem> {
        self.coordinate_systems.get(id)
    }

    /// Find the coordinate system with the given name, ignoring which Zarr node
    /// declares it. A coordinate system declared in the node being read takes
    /// precedence over one declared in a subgroup or array.
    pub fn resolve_name(&self, name: &str) -> Option<&CoordinateSystemId> {
        let mut nested = None;
        for id in &self.nodes {
            if id.name == name {
                if id.path.is_none() {
                    return Some(id);
                }
                nested = nested.or(Some(id));
            }
        }
        nested
    }

    /// The number of dimensions of a coordinate system.
    ///
    /// Taken from the declared axes when available, and otherwise inferred from
    /// the parameters of a transformation incident on the coordinate system.
    /// This lets implicit array coordinate systems, whose axes are usually not
    /// spelled out, still be used as a source.
    pub fn ndim(&self, id: &CoordinateSystemId) -> Option<usize> {
        if let Some(system) = self.coordinate_systems.get(id) {
            return Some(system.axes.len());
        }
        self.edges.iter().find_map(|edge| {
            if &edge.from == id {
                input_ndim(&edge.transformation.transformation)
            } else if &edge.to == id {
                output_ndim(&edge.transformation.transformation)
            } else {
                None
            }
        })
    }

    /// Compute the affine transformation mapping points in `source` to points in
    /// `target`, composing the transformations along a shortest path through the
    /// graph and inverting any edge traversed against its declared direction.
    pub fn transformation_between(
        &self,
        source: &CoordinateSystemId,
        target: &CoordinateSystemId,
    ) -> Result<AffineMatrix, TransformationError> {
        let ndim = self
            .ndim(source)
            .ok_or_else(|| TransformationError::UnknownDimensionality(source.to_string()))?;
        self.transformation_between_with_ndim(source, target, ndim)
    }

    /// As [`Self::transformation_between`], but with the dimensionality of the
    /// source coordinate system supplied by the caller.
    ///
    /// Use this for coordinate systems whose axes the metadata leaves implicit,
    /// such as a Zarr array's, where the caller already knows the array shape
    /// and the graph could otherwise only guess.
    pub fn transformation_between_with_ndim(
        &self,
        source: &CoordinateSystemId,
        target: &CoordinateSystemId,
        ndim: usize,
    ) -> Result<AffineMatrix, TransformationError> {
        for id in [source, target] {
            if !self.contains(id) {
                return Err(TransformationError::UnknownCoordinateSystem(id.to_string()));
            }
        }
        let path = self.find_path(source, target).ok_or_else(|| TransformationError::NoPath {
            source: source.to_string(),
            target: target.to_string(),
        })?;

        let mut matrix = AffineMatrix::identity(ndim);
        for (edge_i, direction) in path {
            let transformation = &self.edges[edge_i].transformation;
            let step = to_matrix(transformation, matrix.n_out())?;
            let step = match direction {
                Direction::Forward => step,
                Direction::Reverse => {
                    step.inverse().map_err(|reason| TransformationError::NotInvertible {
                        transformation: transformation.label(),
                        reason,
                    })?
                }
            };
            matrix = matrix.then(&step).map_err(|e| {
                TransformationError::Invalid(format!(
                    "coordinate transformation {}: {e}",
                    transformation.label(),
                ))
            })?;
        }
        Ok(matrix)
    }

    /// Breadth-first search for a shortest edge path from `source` to `target`.
    ///
    /// Shortest is preferable to any path here because each additional edge adds
    /// a matrix multiplication, and each reversed edge a matrix inversion.
    fn find_path(
        &self,
        source: &CoordinateSystemId,
        target: &CoordinateSystemId,
    ) -> Option<Vec<(usize, Direction)>> {
        if source == target {
            return Some(Vec::new());
        }
        // Maps each visited node to the edge that reached it, for backtracking.
        let mut came_from: HashMap<&CoordinateSystemId, (usize, Direction)> = HashMap::new();
        let mut queue = VecDeque::from([source]);
        let mut found = false;

        while let Some(node) = queue.pop_front() {
            for &(edge_i, direction) in self.adjacency.get(node).into_iter().flatten() {
                let edge = &self.edges[edge_i];
                let next = match direction {
                    Direction::Forward => &edge.to,
                    Direction::Reverse => &edge.from,
                };
                if next == source || came_from.contains_key(next) {
                    continue;
                }
                came_from.insert(next, (edge_i, direction));
                if next == target {
                    found = true;
                    queue.clear();
                    break;
                }
                queue.push_back(next);
            }
        }
        if !found {
            return None;
        }

        let mut path = Vec::new();
        let mut node = target;
        while node != source {
            let &(edge_i, direction) = came_from.get(node)?;
            path.push((edge_i, direction));
            node = match direction {
                Direction::Forward => &self.edges[edge_i].from,
                Direction::Reverse => &self.edges[edge_i].to,
            };
        }
        path.reverse();
        Some(path)
    }
}

/// Convert a single transformation to an affine matrix.
///
/// `ndim` is the dimensionality of the points the transformation is being
/// applied to, which is needed for the dimension-agnostic `identity` type.
fn to_matrix(
    transformation: &CoordinateTransformation,
    ndim: usize,
) -> Result<AffineMatrix, TransformationError> {
    let invalid = |e: String| {
        TransformationError::Invalid(format!(
            "coordinate transformation {}: {e}",
            transformation.label(),
        ))
    };
    match &transformation.transformation {
        Transformation::Identity => Ok(AffineMatrix::identity(ndim)),
        Transformation::Scale { scale } => Ok(AffineMatrix::from_scale(scale)),
        Transformation::Translation { translation } => {
            Ok(AffineMatrix::from_translation(translation))
        }
        Transformation::MapAxis { map_axis } => {
            AffineMatrix::from_map_axis(map_axis).map_err(invalid)
        }
        Transformation::Affine { affine } => AffineMatrix::from_affine(affine).map_err(invalid),
        Transformation::Rotation { rotation } => {
            AffineMatrix::from_rotation(rotation).map_err(invalid)
        }
        Transformation::Sequence { transformations } => {
            let mut matrix = AffineMatrix::identity(ndim);
            for inner in transformations {
                let step = to_matrix(inner, matrix.n_out())?;
                matrix = matrix.then(&step).map_err(invalid)?;
            }
            Ok(matrix)
        }
        Transformation::Unsupported => Err(TransformationError::UnsupportedType {
            transformation: transformation.label(),
            type_name: transformation.type_name.clone(),
        }),
    }
}

/// The input dimensionality implied by a transformation's parameters, if any.
fn input_ndim(transformation: &Transformation) -> Option<usize> {
    match transformation {
        Transformation::Scale { scale } => Some(scale.len()),
        Transformation::Translation { translation } => Some(translation.len()),
        Transformation::MapAxis { map_axis } => Some(map_axis.len()),
        Transformation::Affine { affine } => affine.first().map(|row| row.len() - 1),
        Transformation::Rotation { rotation } => rotation.first().map(Vec::len),
        Transformation::Sequence { transformations } => transformations
            .first()
            .and_then(|inner| input_ndim(&inner.transformation)),
        Transformation::Identity | Transformation::Unsupported => None,
    }
}

/// The output dimensionality implied by a transformation's parameters, if any.
fn output_ndim(transformation: &Transformation) -> Option<usize> {
    match transformation {
        Transformation::Affine { affine } => Some(affine.len()),
        Transformation::Rotation { rotation } => Some(rotation.len()),
        Transformation::Sequence { transformations } => transformations
            .last()
            .and_then(|inner| output_ndim(&inner.transformation)),
        other => input_ndim(other),
    }
}
