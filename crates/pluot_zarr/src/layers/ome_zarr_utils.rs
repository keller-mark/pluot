use serde::{Deserialize, Serialize};
use ome_zarr_metadata::v0_5::{
    Axis, AxisType, AxisUnit, AxisUnitSpace,
};

use crate::ome_zarr_transformations::affine::AffineMatrix;
use crate::ome_zarr_transformations::metadata::CoordinateSystemAxis;

pub fn axis_unit_space_to_coefficient_and_exponent(unit: &AxisUnitSpace) -> (f64, i32) {
    // Returns the coefficient and exponent for converting non-SI units to meters
    // (in scientific notation format where the tuple is `(coefficient, exponent)` meaning `coefficient x 10^exponent` meters)
    // Reference: https://github.com/hms-dbmi/viv/blob/6cf019ac1608242682109ffe93d412103667271d/packages/layers/src/utils.js#L158C1-L181C1
    match unit {
        // SI prefixes with positive exponents (multiples of meter)
        AxisUnitSpace::Yottameter => (1.0, 24),
        AxisUnitSpace::Zettameter => (1.0, 21),
        AxisUnitSpace::Exameter => (1.0, 18),
        AxisUnitSpace::Petameter => (1.0, 15),
        AxisUnitSpace::Terameter => (1.0, 12),
        AxisUnitSpace::Gigameter => (1.0, 9),
        AxisUnitSpace::Megameter => (1.0, 6),
        AxisUnitSpace::Kilometer => (1.0, 3),
        AxisUnitSpace::Hectometer => (1.0, 2),
        // TODO: decameter is not currently part of AxisUnitSpace, but it would be (1.0, 1).
        AxisUnitSpace::Meter => (1.0, 0),
        // SI prefixes with negative exponents (submultiples of meter)
        AxisUnitSpace::Decimeter => (1.0, -1),
        AxisUnitSpace::Centimeter => (1.0, -2),
        AxisUnitSpace::Millimeter => (1.0, -3),
        AxisUnitSpace::Micrometer => (1.0, -6),
        AxisUnitSpace::Nanometer => (1.0, -9),
        AxisUnitSpace::Angstrom => (1.0, -10), // Note: not SI since between -9 to -12 exponents.
        AxisUnitSpace::Picometer => (1.0, -12),
        AxisUnitSpace::Femtometer => (1.0, -15),
        AxisUnitSpace::Attometer => (1.0, -18),
        AxisUnitSpace::Zeptometer => (1.0, -21),
        AxisUnitSpace::Yoctometer => (1.0, -24),
        // Non-SI units with coefficients relative to meter
        AxisUnitSpace::Inch => (2.54, -2),      // 0.0254 m = 2.54 x 10⁻² m
        AxisUnitSpace::Foot => (3.048, -1),     // 0.3048 m = 3.048 x 10⁻¹ m
        AxisUnitSpace::Yard => (9.144, -1),     // 0.9144 m = 9.144 x 10⁻¹ m
        AxisUnitSpace::Mile => (1.609344, 3),   // 1609.344 m = 1.609344 x 10³ m
        AxisUnitSpace::Parsec => (3.0857, 16),  // ~3.0857 x 10¹⁶ m
        // TODO: would it be better to just interpret as meters if unrecognized?
        _ => panic!("Unrecognized AxisUnitSpace unit: {:?}", unit),
    }
}


/// Name given to the coordinate system that OME-Zarr v0.4 and v0.5 leave
/// implicit: the physical space that every resolution level of a multiscale
/// image maps into.
pub const INTRINSIC_COORDINATE_SYSTEM: &str = "intrinsic";

/// Upgrade an OME-Zarr `ome` attributes object in place, rewriting v0.4 and
/// v0.5 `multiscales` metadata into the v0.6 (RFC-5) shape so that one code path
/// can read any version.
///
/// Before v0.6, a multiscale image declared its axes in an `axes` list and left
/// the coordinate systems its transformations connect implicit. For each such
/// entry this:
/// - declares a `coordinateSystems` entry named [`INTRINSIC_COORDINATE_SYSTEM`]
///   holding the entry's `axes`, and
/// - rewrites each dataset's transformations as a single transformation from
///   that dataset's array coordinate system to the intrinsic coordinate system.
///
/// A dataset's own transformations are applied in order and then the
/// multiscale-wide `coordinateTransformations` on top, so a dataset with more
/// than one transformation to apply becomes a `sequence` of them. A dataset with
/// none becomes an `identity`.
///
/// Metadata that already declares `input` and `output` is left alone, so v0.6
/// metadata passes through unchanged.
pub fn upgrade_ome_multiscales(ome: &mut serde_json::Value) {
    let Some(multiscales) = ome.get_mut("multiscales").and_then(|v| v.as_array_mut()) else {
        return;
    };
    for multiscale in multiscales {
        // The transformations applied to every level, after each level's own.
        let shared: Vec<serde_json::Value> = multiscale
            .get("coordinateTransformations")
            .and_then(|v| v.as_array())
            .map(|list| list.iter().filter(|t| is_legacy(t)).cloned().collect())
            .unwrap_or_default();

        if multiscale.get("coordinateSystems").is_none() {
            if let Some(axes) = multiscale.get("axes").cloned() {
                multiscale["coordinateSystems"] = serde_json::json!([
                    { "name": INTRINSIC_COORDINATE_SYSTEM, "axes": axes },
                ]);
            }
        }

        let Some(datasets) = multiscale.get_mut("datasets").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        for dataset in datasets {
            let Some(path) = dataset.get("path").and_then(|p| p.as_str()).map(str::to_string) else {
                continue;
            };
            let steps: Vec<serde_json::Value> = dataset
                .get("coordinateTransformations")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if steps.iter().any(|step| !is_legacy(step)) {
                // Already in the v0.6 shape.
                continue;
            }
            let steps: Vec<serde_json::Value> =
                steps.into_iter().chain(shared.iter().cloned()).collect();
            let mut transformation = match <[serde_json::Value; 1]>::try_from(steps) {
                Ok([only]) => only,
                Err(steps) => match steps.is_empty() {
                    true => serde_json::json!({ "type": "identity" }),
                    false => serde_json::json!({ "type": "sequence", "transformations": steps }),
                },
            };
            transformation["input"] = serde_json::json!({ "path": path });
            transformation["output"] = serde_json::json!({ "name": INTRINSIC_COORDINATE_SYSTEM });
            dataset["coordinateTransformations"] = serde_json::json!([transformation]);
        }
    }
}

/// Whether a coordinate transformation predates v0.6, i.e. does not declare the
/// coordinate systems it connects.
fn is_legacy(transformation: &serde_json::Value) -> bool {
    transformation.get("input").is_none() && transformation.get("output").is_none()
}

/// Factor converting one unit of a spatial axis to meters.
///
/// Axes that do not declare a unit are interpreted as micrometers, matching the
/// default used elsewhere for OME-Zarr images.
pub fn axis_unit_to_meters(unit: Option<&str>) -> Result<f64, String> {
    let Some(unit) = unit else {
        return Ok(1e-6);
    };
    let space: AxisUnitSpace = serde_json::from_value(serde_json::Value::String(unit.to_string()))
        .map_err(|_| format!("unrecognized spatial axis unit \"{}\"", unit))?;
    let (coefficient, exponent) = axis_unit_space_to_coefficient_and_exponent(&space);
    Ok(coefficient * 10_f64.powi(exponent))
}

/// Build the column-major 4x4 pixel-to-world model matrix for one resolution
/// level from a transformation into a target coordinate system.
///
/// `transformation` maps points in the level's array coordinate system (array
/// index order, Y increasing downwards) to the target coordinate system, whose
/// component order and units are given by `target_axes`.
///
/// The returned matrix follows the convention of `multiscale_utils`: it maps
/// Y-up pixel coordinates at this level to world coordinates in meters. Two
/// axis flips are folded in to get there. The Y-up pixel row is converted back
/// to an array row using `level_height`, and the target Y coordinate is negated
/// because world Y increases upwards while the Y axis of an OME-Zarr image
/// coordinate system increases downwards. Positions along the target Y axis are
/// therefore negative, but they are absolute: two images sharing a target
/// coordinate system line up in world space.
///
/// Array dimensions other than X and Y are held fixed, since a single slice is
/// rendered at a time. This only matters for transformations that mix those
/// dimensions into the target X or Y coordinate.
pub fn target_coordinate_system_model_matrix(
    transformation: &AffineMatrix,
    dimension_order: &OmeDimensionOrder,
    target_axes: &[CoordinateSystemAxis],
    level_height: u64,
    target_z: u64,
    target_t: u64,
) -> Result<[f32; 16], String> {
    let n_in = transformation.n_in();
    if n_in != dimension_order.num_dims() {
        return Err(format!(
            "the transformation takes {}-dimensional points but the array has {} dimensions",
            n_in,
            dimension_order.num_dims(),
        ));
    }
    if transformation.n_out() != target_axes.len() {
        return Err(format!(
            "the transformation produces {}-dimensional points but the target coordinate system has {} axes",
            transformation.n_out(),
            target_axes.len(),
        ));
    }
    let x_dim = dimension_order.index_of(OmeDim::X).unwrap();
    let y_dim = dimension_order.index_of(OmeDim::Y).unwrap();

    let find_target_axis = |name: &str| {
        target_axes
            .iter()
            .position(|axis| axis.name == name)
            .ok_or_else(|| format!("the target coordinate system has no \"{}\" axis", name))
    };
    let target_x = find_target_axis("x")?;
    let target_y = find_target_axis("y")?;
    let x_to_meters = axis_unit_to_meters(target_axes[target_x].unit.as_deref())?;
    let y_to_meters = axis_unit_to_meters(target_axes[target_y].unit.as_deref())?;

    // The value each non-X/Y array dimension is held at.
    let fixed_at = |dim: usize| match dimension_order.dims()[dim] {
        OmeDim::Z => target_z as f64,
        OmeDim::T => target_t as f64,
        _ => 0.0,
    };

    // Rewrite one output component as a function of (px, py) in Y-up pixel
    // coordinates: the array row is `level_height - py`, so the Y column changes
    // sign and contributes `level_height` times its coefficient to the constant.
    let constant = |row: usize| {
        transformation.get(row, n_in)
            + transformation.get(row, y_dim) * level_height as f64
            + (0..n_in)
                .filter(|&dim| dim != x_dim && dim != y_dim)
                .map(|dim| transformation.get(row, dim) * fixed_at(dim))
                .sum::<f64>()
    };

    let m00 = x_to_meters * transformation.get(target_x, x_dim);
    let m01 = -x_to_meters * transformation.get(target_x, y_dim);
    let m03 = x_to_meters * constant(target_x);
    let m10 = -y_to_meters * transformation.get(target_y, x_dim);
    let m11 = y_to_meters * transformation.get(target_y, y_dim);
    let m13 = -y_to_meters * constant(target_y);

    Ok([
        m00 as f32, m10 as f32, 0.0, 0.0,
        m01 as f32, m11 as f32, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        m03 as f32, m13 as f32, 0.0, 1.0,
    ])
}

/// The physical size of one pixel along the world Y and X axes, as
/// `[scale_y, scale_x]` to match `ResolutionLevel::scale`.
///
/// This is the world-space length of the displacement produced by stepping one
/// pixel along the array's Y or X axis, so it reduces to the axis scale for a
/// model matrix that only scales, and stays meaningful under rotation and shear.
pub fn model_matrix_pixel_size(model_matrix: &[f32; 16]) -> [f64; 2] {
    // Column-major: column `col` starts at index `col * 4`.
    let column_length = |col: usize| {
        (model_matrix[col * 4] as f64).hypot(model_matrix[col * 4 + 1] as f64)
    };
    [column_length(1), column_length(0)]
}


// These utils are shared between ome_zarr_bitmap_layer and ome_zarr_multiscale_layer,
// so we put them in a separate module to avoid circular dependencies.

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmeZarrChannelSetting {
    /// Index in the C dimension of the zarr array.
    pub c_index: u32,
    /// Min/max window for normalization.
    pub window: (f32, f32),
    /// RGB color as floats in [0.0, 1.0].
    pub color: (f32, f32, f32),
}

/// Axis-aligned physical rectangle for a tile.
#[derive(Debug, Clone, Copy)]
pub struct PhysicalRect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl PhysicalRect {
    /// Returns true if `other` is entirely contained within `self`.
    pub fn contains(&self, other: &PhysicalRect) -> bool {
        self.x0 <= other.x0 && self.x1 >= other.x1 && self.y0 <= other.y0 && self.y1 >= other.y1
    }
}

/// Check if two axis-aligned rects overlap (share any area).
pub fn rects_overlap(a: &PhysicalRect, b: &PhysicalRect) -> bool {
    a.x0 < b.x1 && a.x1 > b.x0 && a.y0 < b.y1 && a.y1 > b.y0
}

/// Compute the bounding box of a set of rects.
pub fn bounding_box(rects: &[&PhysicalRect]) -> PhysicalRect {
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for r in rects {
        x0 = x0.min(r.x0);
        y0 = y0.min(r.y0);
        x1 = x1.max(r.x1);
        y1 = y1.max(r.y1);
    }
    PhysicalRect { x0, y0, x1, y1 }
}


/// A single OME-NGFF dimension axis.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum OmeDim { T, Z, C, Y, X }

impl OmeDim {
    pub fn as_char(self) -> char {
        match self {
            OmeDim::T => 'T',
            OmeDim::Z => 'Z',
            OmeDim::C => 'C',
            OmeDim::Y => 'Y',
            OmeDim::X => 'X',
        }
    }

    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'T' | 't' => Some(OmeDim::T),
            'Z' | 'z' => Some(OmeDim::Z),
            'C' | 'c' => Some(OmeDim::C),
            'Y' | 'y' => Some(OmeDim::Y),
            'X' | 'x' => Some(OmeDim::X),
            _ => None,
        }
    }
}

impl std::fmt::Display for OmeDim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_char())
    }
}

/// Ordered list of unique OME-NGFF dimensions, e.g. `[T, Z, C, Y, X]` for `"TZCYX"`.
///
/// Invariants enforced by the constructor:
/// - All elements are unique.
/// - Both `X` and `Y` are present.
/// - At most 5 dimensions (one of each variant).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OmeDimensionOrder(Vec<OmeDim>);

impl OmeDimensionOrder {
    /// Construct from an ordered list of `OmeDim` values.
    /// Panics if invariants are violated.
    pub fn new(dims: Vec<OmeDim>) -> Self {
        assert!(dims.len() <= 5, "OmeDimensionOrder cannot have more than 5 dimensions");

        // Check for duplicates.
        for i in 0..dims.len() {
            for j in (i + 1)..dims.len() {
                assert_ne!(dims[i], dims[j], "Duplicate dimension '{}'", dims[i]);
            }
        }

        // X and Y must both be present.
        assert!(dims.contains(&OmeDim::X), "OmeDimensionOrder must contain X");
        assert!(dims.contains(&OmeDim::Y), "OmeDimensionOrder must contain Y");

        Self(dims)
    }

    /// Returns the number of dimensions.
    pub fn num_dims(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the given dimension is present.
    pub fn has_dim(&self, dim: OmeDim) -> bool {
        self.0.contains(&dim)
    }

    /// Returns the index (position in the order) of the given dimension, if present.
    pub fn index_of(&self, dim: OmeDim) -> Option<usize> {
        self.0.iter().position(|&d| d == dim)
    }

    /// Returns a slice of the ordered dimensions.
    pub fn dims(&self) -> &[OmeDim] {
        &self.0
    }
}

impl std::fmt::Display for OmeDimensionOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for d in &self.0 {
            write!(f, "{}", d)?;
        }
        Ok(())
    }
}

impl TryFrom<&str> for OmeDimensionOrder {
    type Error = String;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let dims: Result<Vec<OmeDim>, _> = s
            .chars()
            .map(|c| OmeDim::from_char(c).ok_or_else(|| format!("Invalid dimension character '{}'", c)))
            .collect();
        let dims = dims?;

        // Reuse new() for invariant checks, converting panics to errors.
        if dims.len() > 5 {
            return Err(format!("Too many dimensions: {}", dims.len()));
        }
        for i in 0..dims.len() {
            for j in (i + 1)..dims.len() {
                if dims[i] == dims[j] {
                    return Err(format!("Duplicate dimension '{}'", dims[i]));
                }
            }
        }
        if !dims.contains(&OmeDim::X) {
            return Err("OmeDimensionOrder must contain X".to_string());
        }
        if !dims.contains(&OmeDim::Y) {
            return Err("OmeDimensionOrder must contain Y".to_string());
        }

        Ok(Self(dims))
    }
}

impl TryFrom<String> for OmeDimensionOrder {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        OmeDimensionOrder::try_from(s.as_str())
    }
}

impl From<OmeDimensionOrder> for String {
    fn from(order: OmeDimensionOrder) -> String {
        order.to_string()
    }
}

impl Serialize for OmeDimensionOrder {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for OmeDimensionOrder {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        OmeDimensionOrder::try_from(s.as_str()).map_err(serde::de::Error::custom)
    }
}


