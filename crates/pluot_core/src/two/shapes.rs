// Reference: https://github.com/keller-mark/deck-to-svg/blob/main/lib/src/shapes.js

pub type RgbColor = (u8, u8, u8);
pub type RgbaColor = (u8, u8, u8, u8);

#[derive(Clone, Debug)]
pub enum TwoColor {
    Rgb(RgbColor),
    Rgba(RgbaColor),
}

impl ToString for TwoColor {
    fn to_string(&self) -> String {
        match self {
            TwoColor::Rgb(rgb) => format!("rgb({}, {}, {})", rgb.0, rgb.1, rgb.2),
            TwoColor::Rgba(rgba) => format!("rgba({}, {}, {}, {})", rgba.0, rgba.1, rgba.2, rgba.3),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TwoRectangle {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub stroke: Option<TwoColor>,
    pub fill: Option<TwoColor>,
    // Width of the stroke line if stroke is not null.
    pub linewidth: f64,
    pub opacity: f64,
    pub rotation: Option<f64>,
}

impl Default for TwoRectangle {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            stroke: None,
            fill: Some(TwoColor::Rgb((0, 0, 0))),
            linewidth: 1.0,
            opacity: 1.0,
            rotation: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TwoCircle {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub stroke: Option<TwoColor>,
    pub fill: Option<TwoColor>,
    // Width of the stroke line if stroke is not null.
    pub linewidth: f64,
    pub opacity: f64,
}

impl Default for TwoCircle {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            radius: 0.0,
            stroke: None,
            fill: Some(TwoColor::Rgb((0, 0, 0))),
            linewidth: 1.0,
            opacity: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TwoLine {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub stroke: Option<TwoColor>,
    // Width of the stroke line if stroke is not null.
    pub linewidth: f64,
    pub opacity: f64,
}

impl Default for TwoLine {
    fn default() -> Self {
        Self {
            x1: 0.0,
            y1: 0.0,
            x2: 0.0,
            y2: 0.0,
            stroke: Some(TwoColor::Rgb((0, 0, 0))),
            linewidth: 1.0,
            opacity: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TwoPath {
    pub points: Vec<(f64, f64)>,
    pub stroke: Option<TwoColor>,
    pub fill: Option<TwoColor>,
    // Width of the stroke line if stroke is not null.
    pub linewidth: f64,
    pub opacity: f64,
}

impl Default for TwoPath {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            stroke: Some(TwoColor::Rgb((0, 0, 0))),
            fill: Some(TwoColor::Rgb((255, 255, 255))),
            linewidth: 1.0,
            opacity: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum TwoTextAlign {
    Start,
    Middle,
    End,
}

impl ToString for TwoTextAlign {
    fn to_string(&self) -> String {
        match self {
            TwoTextAlign::Start => "start".to_string(),
            TwoTextAlign::Middle => "middle".to_string(),
            TwoTextAlign::End => "end".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum TwoTextBaseline {
    Alphabetic,
    Top,
    Middle,
    Bottom,
}

impl ToString for TwoTextBaseline {
    fn to_string(&self) -> String {
        match self {
            TwoTextBaseline::Alphabetic => "alphabetic".to_string(),
            TwoTextBaseline::Top => "top".to_string(),
            TwoTextBaseline::Middle => "middle".to_string(),
            TwoTextBaseline::Bottom => "bottom".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum TwoTextOverflow {
    Clip,
    Ellipsis,
}

#[derive(Clone, Debug)]
pub struct TwoText {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub text: String,

    pub fill: TwoColor,
    pub fontsize: f64,
    pub font: String,
    // Corresponds to canvas `context.textAlign`.
    // Possible values: "start", "middle", "end".
    pub align: TwoTextAlign,
    // Corresponds to canvas `context.textBaseline`.
    // Possible values: "alphabetic", "top", "middle", "bottom".
    pub baseline: TwoTextBaseline,
    pub opacity: f64,
    // In degrees.
    pub rotation: Option<f64>,
    // How text that overflows the bounding box should be dealt with.
    // Possible values: null, "clip", "ellipsis".
    pub overflow: Option<TwoTextOverflow>,
}

impl Default for TwoText {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            text: String::new(),
            fill: TwoColor::Rgb((0, 0, 0)),
            fontsize: 14.0,
            font: "Arial,sans-serif".to_string(),
            align: TwoTextAlign::Middle,
            baseline: TwoTextBaseline::Alphabetic,
            opacity: 1.0,
            rotation: None,
            overflow: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TwoGroup {
    pub elements: Vec<TwoElement>,
    // Strings to add optional data attributes for debugging.
    pub layer_type: Option<String>,
    pub layer_id: Option<String>,
    // In radians.
    pub rotation: Option<f64>,
    pub translate: Option<(f64, f64)>,
    // If set, defines a clipping rectangle for the group.
    pub clip_rect: Option<(f64, f64, f64, f64)>, // x, y, width, height // TODO: how does clip rect interact with group translation?
    // TODO: add data- or aria- attributes for accessibility or hooking up event handlers?
}

impl Default for TwoGroup {
    fn default() -> Self {
        Self {
            elements: Vec::new(),
            layer_type: None,
            layer_id: None,
            rotation: None,
            translate: None,
            clip_rect: None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum TwoElement {
    Rectangle(TwoRectangle),
    Circle(TwoCircle),
    Line(TwoLine),
    Path(TwoPath),
    Text(TwoText),
    Group(TwoGroup),
}
