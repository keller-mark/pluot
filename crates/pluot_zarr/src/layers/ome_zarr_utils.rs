use serde::{Deserialize, Serialize};

pub fn to_y_slice(start: u64, end: u64, height: u64) -> (u64, u64) {
    // OME-Zarr uses a coordinate system where (0, 0) is the top-left corner, and Y increases downwards.
    // We want to convert to a coordinate system where (0, 0) is the bottom-left corner, and Y increases upwards.
    // So we need to flip the Y coordinates.
    let y_start = height - end;
    let y_end = height - start;
    (y_start, y_end)
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_y_slice() {
        let height = 100;
        let (y_start, y_end) = to_y_slice(10, 20, height);
        assert_eq!(y_start, 80);
        assert_eq!(y_end, 90);

        let (y_start, y_end) = to_y_slice(0, 100, height);
        assert_eq!(y_start, 0);
        assert_eq!(y_end, 100);

        let (y_start, y_end) = to_y_slice(0, 1, height);
        assert_eq!(y_start, 99);
        assert_eq!(y_end, 100);
    }

    #[test]
    fn test_ome_dim_order_new() {
        let order = OmeDimensionOrder::new(vec![OmeDim::T, OmeDim::Z, OmeDim::C, OmeDim::Y, OmeDim::X]);
        assert_eq!(order.num_dims(), 5);
        assert_eq!(order.index_of(OmeDim::X), Some(4));
        assert_eq!(order.index_of(OmeDim::T), Some(0));
        assert!(order.has_dim(OmeDim::C));
        assert_eq!(order.to_string(), "TZCYX");
    }

    #[test]
    fn test_ome_dim_order_from_str() {
        let order = OmeDimensionOrder::try_from("CZYX").unwrap();
        assert_eq!(order.num_dims(), 4);
        assert_eq!(order.index_of(OmeDim::C), Some(0));
        assert_eq!(order.index_of(OmeDim::Z), Some(1));
        assert_eq!(order.index_of(OmeDim::Y), Some(2));
        assert_eq!(order.index_of(OmeDim::X), Some(3));
        assert!(!order.has_dim(OmeDim::T));
        assert_eq!(order.to_string(), "CZYX");
    }

    #[test]
    fn test_ome_dim_order_lowercase() {
        // Lowercase input is accepted; order is preserved, output is uppercase.
        let order = OmeDimensionOrder::try_from("tczyx").unwrap();
        assert_eq!(order.to_string(), "TCZYX");
    }

    #[test]
    fn test_ome_dim_order_into_string() {
        let order = OmeDimensionOrder::new(vec![OmeDim::C, OmeDim::Y, OmeDim::X]);
        let s: String = order.into();
        assert_eq!(s, "CYX");
    }

    #[test]
    fn test_ome_dim_order_err_no_x() {
        assert!(OmeDimensionOrder::try_from("CY").is_err());
    }

    #[test]
    fn test_ome_dim_order_err_no_y() {
        assert!(OmeDimensionOrder::try_from("CX").is_err());
    }

    #[test]
    fn test_ome_dim_order_err_duplicate() {
        assert!(OmeDimensionOrder::try_from("XYXY").is_err());
    }

    #[test]
    fn test_ome_dim_order_err_invalid_char() {
        assert!(OmeDimensionOrder::try_from("AXY").is_err());
    }

    #[test]
    #[should_panic]
    fn test_ome_dim_order_new_panics_on_duplicate() {
        OmeDimensionOrder::new(vec![OmeDim::X, OmeDim::Y, OmeDim::X]);
    }

    #[test]
    fn test_ome_dim_order_serde_roundtrip() {
        let order = OmeDimensionOrder::new(vec![OmeDim::T, OmeDim::C, OmeDim::Z, OmeDim::Y, OmeDim::X]);
        let json = serde_json::to_string(&order).unwrap();
        assert_eq!(json, "\"TCZYX\"");
        let decoded: OmeDimensionOrder = serde_json::from_str(&json).unwrap();
        assert_eq!(order, decoded);
    }
}
