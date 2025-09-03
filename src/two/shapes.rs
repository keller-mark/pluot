// Reference: https://github.com/keller-mark/deck-to-svg/blob/main/lib/src/shapes.js

pub struct TwoRectangle {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub stroke: Option<String>,
    pub fill: Option<String>,
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
            fill: Some("#000".to_string()),
            linewidth: 1.0,
            opacity: 1.0,
            rotation: None,
        }
    }
}

pub struct TwoCircle {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub stroke: Option<String>,
    pub fill: Option<String>,
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
            fill: Some("#000".to_string()),
            linewidth: 1.0,
            opacity: 1.0,
        }
    }
}

pub struct TwoLine {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub stroke: Option<String>,
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
            stroke: Some("#000".to_string()),
            linewidth: 1.0,
            opacity: 1.0,
        }
    }
}

pub struct TwoPath {
    pub points: Vec<(f64, f64)>,
    pub stroke: Option<String>,
    pub fill: Option<String>,
    // Width of the stroke line if stroke is not null.
    pub linewidth: f64,
    pub opacity: f64,
}

impl Default for TwoPath {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            stroke: Some("#000".to_string()),
            fill: Some("#fff".to_string()),
            linewidth: 1.0,
            opacity: 1.0,
        }
    }
}

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

pub enum TwoTextOverflow {
    Clip,
    Ellipsis,
}

pub struct TwoText {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub text: String,

    /** @member {string} */
    pub fill: String,
    /** @member {number} */
    pub fontsize: f64,
    /** @member {string} */
    pub font: String,
    /** Corresponds to canvas `context.textAlign`.
     * Possible values: "start", "middle", "end".
     * @member {string} */
    pub align: TwoTextAlign,
    /** Corresponds to canvas `context.textBaseline`.
     * Possible values: "alphabetic", "top", "middle", "bottom".
     * @member {string} */
    pub baseline: TwoTextBaseline,
    /** @member {number} */
    pub opacity: f64,
    /** In radians.
     * @member {number} */
    pub rotation: Option<f64>,
    /** How text that overflows the bounding box should be dealt with.
     * Possible values: null, "clip", "ellipsis".
     * @member {string} */
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
            fill: "#000".to_string(),
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

pub enum TwoElement {
    Rectangle(TwoRectangle),
    Circle(TwoCircle),
    Line(TwoLine),
    Path(TwoPath),
    Text(TwoText),
}
