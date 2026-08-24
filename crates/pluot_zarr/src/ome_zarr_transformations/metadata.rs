//! Deserialization of the OME-Zarr coordinate systems and coordinate
//! transformations metadata introduced by RFC-5 (OME-Zarr v0.6).
//!
//! These types are deliberately version-agnostic: they read the
//! `coordinateSystems` and `coordinateTransformations` keys wherever they
//! appear, without validating the `ome.version` field, so that the same code
//! works for scene metadata, `multiscales` metadata, and `datasets` entries.
//!
//! Reference: <https://ngff.openmicroscopy.org/rfc/5/index.html>

use serde::{de, Deserialize, Deserializer};

/// A single axis of a coordinate system.
#[derive(Deserialize, Debug, Clone)]
pub struct CoordinateSystemAxis {
    /// Axis name, unique within its coordinate system, e.g. `"x"`.
    pub name: String,
    /// Axis type, e.g. `"space"`, `"time"`, `"channel"`, or `"array"`.
    #[serde(rename = "type", default)]
    pub axis_type: Option<String>,
    /// Physical unit, e.g. `"micrometer"`.
    #[serde(default)]
    pub unit: Option<String>,
}

/// A named coordinate system: an ordered list of axes.
///
/// The axis order fixes the component order of every point and every
/// transformation parameter expressed in this coordinate system.
#[derive(Deserialize, Debug, Clone)]
pub struct CoordinateSystem {
    /// Name, unique within the Zarr node that declares it.
    pub name: String,
    /// Ordered axes.
    pub axes: Vec<CoordinateSystemAxis>,
}

/// A reference to a coordinate system, as it appears in the `input` or `output`
/// field of a coordinate transformation.
///
/// Deserializes from either an object (`{"name": "intrinsic", "path": "0"}`) or
/// a bare string naming the coordinate system.
#[derive(Debug, Clone)]
pub struct CoordinateSystemRef {
    /// Name of the referenced coordinate system, if given. When absent, `path`
    /// refers to the implicit array coordinate system of a Zarr array, whose
    /// default name is the array path.
    pub name: Option<String>,
    /// Path of the Zarr node that declares the coordinate system, if given.
    pub path: Option<String>,
}

impl<'de> Deserialize<'de> for CoordinateSystemRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Name(String),
            Object {
                #[serde(default)]
                name: Option<String>,
                #[serde(default)]
                path: Option<String>,
            },
        }
        Ok(match Raw::deserialize(deserializer)? {
            Raw::Name(name) => Self { name: Some(name), path: None },
            Raw::Object { name, path } => Self { name, path },
        })
    }
}

/// The parameters of a coordinate transformation, keyed by its `type` field.
///
/// Only the types that can be represented exactly as a single affine matrix are
/// modeled. Anything else — including a `scale`, `translation`, or `affine`
/// whose parameters are stored in a Zarr array via `path` — deserializes to
/// [`Transformation::Unsupported`], which only fails if the graph traversal
/// actually needs to cross that edge.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Transformation {
    /// Leaves points unchanged.
    Identity,
    /// Multiplies each component by the corresponding factor.
    Scale {
        /// One factor per axis.
        scale: Vec<f64>,
    },
    /// Adds an offset to each component.
    Translation {
        /// One offset per axis.
        translation: Vec<f64>,
    },
    /// Permutes components: output component `i` comes from input component
    /// `mapAxis[i]`.
    MapAxis {
        /// One input axis index per output axis.
        #[serde(rename = "mapAxis")]
        map_axis: Vec<usize>,
    },
    /// A general affine, as `n_out` rows of `n_in + 1` values.
    Affine {
        /// Row-major matrix; the last value in each row is the translation.
        affine: Vec<Vec<f64>>,
    },
    /// A square orthonormal matrix with no translation.
    Rotation {
        /// Row-major square matrix.
        rotation: Vec<Vec<f64>>,
    },
    /// Applies a list of transformations in order.
    Sequence {
        /// The transformations to apply, first to last.
        transformations: Vec<CoordinateTransformation>,
    },
    /// A transformation type this implementation cannot represent as a matrix.
    Unsupported,
}

/// A coordinate transformation together with the coordinate systems it connects.
///
/// The `input` and `output` fields are absent for transformations nested inside
/// a [`Transformation::Sequence`].
#[derive(Debug, Clone)]
pub struct CoordinateTransformation {
    /// Optional human-readable name, used in error messages.
    pub name: Option<String>,
    /// The coordinate system the transformation maps from.
    pub input: Option<CoordinateSystemRef>,
    /// The coordinate system the transformation maps to.
    pub output: Option<CoordinateSystemRef>,
    /// The raw `type` string, retained so unsupported types can be named.
    pub type_name: String,
    /// The transformation parameters.
    pub transformation: Transformation,
}

impl CoordinateTransformation {
    /// A short identifier for use in error messages.
    pub fn label(&self) -> String {
        match &self.name {
            Some(name) => format!("\"{name}\""),
            None => format!("of type \"{}\"", self.type_name),
        }
    }
}

impl<'de> Deserialize<'de> for CoordinateTransformation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// The fields that surround the `type`-keyed parameters.
        #[derive(Deserialize)]
        struct Endpoints {
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            input: Option<CoordinateSystemRef>,
            #[serde(default)]
            output: Option<CoordinateSystemRef>,
            #[serde(rename = "type")]
            type_name: String,
        }

        let value = serde_json::Value::deserialize(deserializer)?;
        let endpoints = Endpoints::deserialize(&value).map_err(de::Error::custom)?;
        let transformation =
            Transformation::deserialize(&value).unwrap_or(Transformation::Unsupported);
        Ok(Self {
            name: endpoints.name,
            input: endpoints.input,
            output: endpoints.output,
            type_name: endpoints.type_name,
            transformation,
        })
    }
}

/// One `ome.multiscales` entry, in the v0.6 shape.
///
/// Only the fields the renderer needs are modeled; the downscaling `type` and
/// `metadata` fields are ignored, as is the v0.4/v0.5 `axes` list, which
/// `upgrade_ome_multiscales` turns into a `coordinateSystems` entry.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MultiscaleImage {
    /// Optional name of the multiscale image.
    #[serde(default)]
    pub name: Option<String>,
    /// Coordinate systems declared by this multiscale image, including the
    /// intrinsic one that its datasets map into.
    #[serde(default)]
    pub coordinate_systems: Vec<CoordinateSystem>,
    /// The resolution levels, finest first.
    pub datasets: Vec<MultiscaleDataset>,
    /// Transformations relating this image's coordinate systems to others.
    #[serde(default)]
    pub coordinate_transformations: Vec<CoordinateTransformation>,
}

/// One resolution level of a [`MultiscaleImage`].
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MultiscaleDataset {
    /// Path of the Zarr array, relative to the multiscales group.
    pub path: String,
    /// Transformations from this level's array coordinate system.
    #[serde(default)]
    pub coordinate_transformations: Vec<CoordinateTransformation>,
}

/// The `ome.scene` metadata object: a set of coordinate systems and the
/// transformations relating them.
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    /// Coordinate systems declared by the scene.
    #[serde(default)]
    pub coordinate_systems: Vec<CoordinateSystem>,
    /// Transformations between coordinate systems.
    #[serde(default)]
    pub coordinate_transformations: Vec<CoordinateTransformation>,
}
