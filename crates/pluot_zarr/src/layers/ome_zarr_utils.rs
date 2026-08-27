use serde::{Deserialize, Serialize};
use ome_zarr_metadata::v0_5::{
    Axis, AxisType, AxisUnit, AxisUnitSpace,
};
use pluot_core::layers::bitmask_layer::BitmaskChannelSettings;
use pluot_core::render_traits::{ColorMode, EmphasisCriteria, OpacityMode, SizeMode};

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


// These utils are shared between ome_zarr_bitmap_layer and ome_zarr_multiscale_layer,
// so we put them in a separate module to avoid circular dependencies.

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmeZarrChannelSetting {
    /// Index in the C dimension of the zarr array.
    // TODO: also support specifying channel identifiers by their string name.
    pub c_index: u32,
    /// Min/max window for normalization.
    pub window: (f32, f32),
    /// RGB color as floats in [0.0, 1.0].
    pub color: (f32, f32, f32),
}

// The render settings below are inlined from `BitmaskChannelSettings` rather
// than nested under it, so they default to the same values as they would
// there. These delegate to `BitmaskChannelSettings::default()` instead of
// repeating its literals, so the two cannot drift apart.
fn default_channel_stroked() -> bool {
    BitmaskChannelSettings::default().stroked
}
fn default_channel_filled() -> bool {
    BitmaskChannelSettings::default().filled
}
fn default_channel_stroke_color() -> Option<ColorMode> {
    BitmaskChannelSettings::default().stroke_color
}
fn default_channel_stroke_width() -> Option<SizeMode> {
    BitmaskChannelSettings::default().stroke_width
}
fn default_channel_stroke_opacity() -> Option<OpacityMode> {
    BitmaskChannelSettings::default().stroke_opacity
}
fn default_channel_fill_color() -> Option<ColorMode> {
    BitmaskChannelSettings::default().fill_color
}
fn default_channel_fill_opacity() -> Option<OpacityMode> {
    BitmaskChannelSettings::default().fill_opacity
}

/// Per-channel settings for [`crate::layers::ome_zarr_bitmask_layer::OmeZarrBitmaskLayer`]
/// and [`crate::layers::ome_zarr_bitmask_multiscale_layer::OmeZarrBitmaskMultiscaleLayer`].
/// The bitmask counterpart of [`OmeZarrChannelSetting`]: instead of an
/// intensity window and pseudocolor, carries the [`BitmaskChannelSettings`]
/// (stroked/filled, plus each one's color and opacity) used to render this
/// channel's segmentation mask, inlined alongside `c_index` rather than nested
/// under it.
///
/// Only `c_index` is required -- every render setting falls back to the same
/// default [`BitmaskChannelSettings`] uses.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmeZarrBitmaskChannelSetting {
    /// Index in the C dimension of the zarr array.
    // TODO: also support specifying channel identifiers by their string name.
    pub c_index: u32,

    /// Whether to stroke the outline of each object in this channel.
    #[serde(default = "default_channel_stroked")]
    pub stroked: bool,

    /// Whether to fill the interior of each object in this channel.
    #[serde(default = "default_channel_filled")]
    pub filled: bool,

    /// How to color each object's outline.
    #[serde(default = "default_channel_stroke_color")]
    pub stroke_color: Option<ColorMode>,

    /// Outline thickness, in the units given by the layer's
    /// `stroke_width_unit_mode`, used when `stroked` is true.
    #[serde(default = "default_channel_stroke_width")]
    pub stroke_width: Option<SizeMode>,

    /// Opacity multiplier for the outline (0.0 to 1.0).
    #[serde(default = "default_channel_stroke_opacity")]
    pub stroke_opacity: Option<OpacityMode>,

    /// How to color each object's interior.
    #[serde(default = "default_channel_fill_color")]
    pub fill_color: Option<ColorMode>,

    /// Opacity multiplier for the interior (0.0 to 1.0).
    #[serde(default = "default_channel_fill_opacity")]
    pub fill_opacity: Option<OpacityMode>,

    /// Criteria AND-ed together to determine the selected ("foreground") /
    /// filtered-in ("background") set of objects in this channel. An empty
    /// list means every object is included.
    #[serde(default)]
    pub selection_criteria: Vec<EmphasisCriteria>,
    #[serde(default)]
    pub filtering_criteria: Vec<EmphasisCriteria>,

    /// Fill/stroke colors used for filter-included, but selection-excluded
    /// ("background") objects in this channel.
    #[serde(default)]
    pub background_fill_color: Option<(u8, u8, u8)>,
    #[serde(default)]
    pub background_stroke_color: Option<(u8, u8, u8)>,
}

/// Drops `c_index` -- which selects *which* slice of the C dimension to load,
/// not how to render it -- and passes the rest through to the inner
/// [`crate::layers::ome_zarr_bitmask_layer::OmeZarrBitmaskLayer`]'s
/// `BitmaskLayer`.
///
/// Written out field-by-field (no `..Default::default()`) so that a new
/// [`BitmaskChannelSettings`] field fails to compile here until it is either
/// inlined above or deliberately left out.
impl From<&OmeZarrBitmaskChannelSetting> for BitmaskChannelSettings {
    fn from(cs: &OmeZarrBitmaskChannelSetting) -> Self {
        Self {
            stroked: cs.stroked,
            filled: cs.filled,
            stroke_color: cs.stroke_color.clone(),
            stroke_width: cs.stroke_width.clone(),
            stroke_opacity: cs.stroke_opacity.clone(),
            fill_color: cs.fill_color.clone(),
            fill_opacity: cs.fill_opacity.clone(),
            selection_criteria: cs.selection_criteria.clone(),
            filtering_criteria: cs.filtering_criteria.clone(),
            background_fill_color: cs.background_fill_color,
            background_stroke_color: cs.background_stroke_color,
        }
    }
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

    /// The render settings are inlined alongside `c_index` rather than nested
    /// under a `settings` key.
    #[test]
    fn test_bitmask_channel_setting_inlined_fields() {
        let cs: OmeZarrBitmaskChannelSetting = serde_json::from_str(
            r#"{
                "c_index": 3,
                "stroked": true,
                "filled": true,
                "stroke_color": {"color_mode": "UniformRgb", "color_params": [0, 0, 255]},
                "stroke_width": {"size_mode": "UniformSize", "size_params": 2.0},
                "stroke_opacity": {"opacity_mode": "UniformOpacity", "opacity_params": 0.5},
                "fill_color": {"color_mode": "UniformRgb", "color_params": [255, 0, 0]},
                "fill_opacity": {"opacity_mode": "UniformOpacity", "opacity_params": 0.25}
            }"#,
        )
        .unwrap();

        assert_eq!(cs.c_index, 3);
        assert!(cs.stroked);
        assert!(cs.filled);
        assert!(matches!(cs.stroke_color, Some(ColorMode::UniformRgb((0, 0, 255)))));
        assert!(matches!(cs.stroke_width, Some(SizeMode::UniformSize(w)) if w == 2.0));
        assert!(matches!(cs.stroke_opacity, Some(OpacityMode::UniformOpacity(a)) if a == 0.5));
        assert!(matches!(cs.fill_color, Some(ColorMode::UniformRgb((255, 0, 0)))));
        assert!(matches!(cs.fill_opacity, Some(OpacityMode::UniformOpacity(a)) if a == 0.25));

        // Everything but c_index passes through to the inner layer's settings.
        let inner = BitmaskChannelSettings::from(&cs);
        assert!(inner.stroked);
        assert!(inner.filled);
        assert!(matches!(inner.stroke_color, Some(ColorMode::UniformRgb((0, 0, 255)))));
        assert!(matches!(inner.stroke_width, Some(SizeMode::UniformSize(w)) if w == 2.0));
        assert!(matches!(inner.stroke_opacity, Some(OpacityMode::UniformOpacity(a)) if a == 0.5));
        assert!(matches!(inner.fill_color, Some(ColorMode::UniformRgb((255, 0, 0)))));
        assert!(matches!(inner.fill_opacity, Some(OpacityMode::UniformOpacity(a)) if a == 0.25));
    }

    /// Only `c_index` is required; the inlined render settings fall back to
    /// the same defaults `BitmaskChannelSettings` uses.
    ///
    /// Compared as serialized JSON rather than field by field: the render
    /// settings' modes carry `NumericData` and so have no `PartialEq`, and this
    /// way the assertion covers *every* field at once -- an inlined field whose
    /// `#[serde(default = ...)]` stops delegating to
    /// `BitmaskChannelSettings::default()` fails here without the test needing
    /// to name it.
    #[test]
    fn test_bitmask_channel_setting_defaults_match_bitmask_channel_settings() {
        let cs: OmeZarrBitmaskChannelSetting =
            serde_json::from_str(r#"{"c_index": 2}"#).unwrap();
        assert_eq!(cs.c_index, 2);

        let from_defaults = serde_json::to_value(BitmaskChannelSettings::from(&cs)).unwrap();
        let defaults = serde_json::to_value(BitmaskChannelSettings::default()).unwrap();
        assert_eq!(from_defaults, defaults);
    }

    /// `c_index` selects which slice of the C dimension to load, so unlike the
    /// render settings it has no meaningful default.
    #[test]
    fn test_bitmask_channel_setting_requires_c_index() {
        let result: Result<OmeZarrBitmaskChannelSetting, _> =
            serde_json::from_str(r#"{"filled": true}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_bitmask_channel_setting_serde_roundtrip() {
        let cs = OmeZarrBitmaskChannelSetting {
            c_index: 1,
            stroked: true,
            filled: false,
            stroke_color: Some(ColorMode::UniformRgb((255, 0, 0))),
            stroke_width: Some(SizeMode::UniformSize(3.0)),
            stroke_opacity: Some(OpacityMode::UniformOpacity(0.25)),
            fill_color: None,
            fill_opacity: None,
            selection_criteria: vec![],
            filtering_criteria: vec![],
            background_fill_color: Some((200, 200, 200)),
            background_stroke_color: Some((200, 200, 200)),
        };
        let json = serde_json::to_string(&cs).unwrap();
        // Inlined, i.e. no nested `settings` object.
        assert!(!json.contains("settings"), "unexpected nesting in {json}");

        let decoded: OmeZarrBitmaskChannelSetting = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.c_index, 1);
        assert!(decoded.stroked);
        assert!(!decoded.filled);
        assert!(matches!(decoded.stroke_color, Some(ColorMode::UniformRgb((255, 0, 0)))));
        assert!(matches!(decoded.stroke_width, Some(SizeMode::UniformSize(w)) if w == 3.0));
        assert!(matches!(decoded.stroke_opacity, Some(OpacityMode::UniformOpacity(a)) if a == 0.25));
        assert!(decoded.fill_color.is_none());
        assert!(decoded.fill_opacity.is_none());
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
