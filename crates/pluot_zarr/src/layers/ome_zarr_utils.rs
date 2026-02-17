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


pub enum OmeDimension {
    C,
    Z,
    T,
    X,
    Y,
}

impl OmeDimension {
    /// Returns the string representation of the dimension order (e.g., "CYX").
    pub fn as_str(&self) -> &'static str {
        match self {
            OmeDimension::C => "C",
            OmeDimension::Z => "Z",
            OmeDimension::T => "T",
            OmeDimension::X => "X",
            OmeDimension::Y => "Y",
        }
    }

    pub fn as_char(&self) -> char {
        self.as_str().chars().next().unwrap()
    }
}

pub struct OmeDimensionOrder {
    dimension_order: String,
}

impl OmeDimensionOrder {
    pub fn new(dimension_order: String) -> Self {
        // Validate that dimension_order only contains valid characters (e.g. 'C', 'Z', 'T', 'X', 'Y').
        for c in dimension_order.chars() {
            if !matches!(c, 'C' | 'Z' | 'T' | 'X' | 'Y') {
                panic!("Invalid character '{}' in dimension order '{}'", c, dimension_order);
            }
        }
        // Validate that dimension_order contains both X and Y


        // Validate that there are no duplicate characters in dimension_order

        Self { dimension_order }
    }

    /// Returns the number of dimensions.
    pub fn num_dims(&self) -> usize {
        self.dimension_order.len()
    }

    pub fn has_dim(&self, dim: OmeDimension) -> bool {
        self.dimension_order.contains(dim.as_str())
    }

    /// Returns the position of the channel dimension in the shape array, if present.
    pub fn index_of(&self, dim: OmeDimension) -> Option<usize> {
        self.dimension_order.chars()
            .position(|c| c == dim.as_char())
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
}
