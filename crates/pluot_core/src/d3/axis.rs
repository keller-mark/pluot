// Reference: https://github.com/d3/d3-axis/blob/main/src/axis.js

use crate::d3::scale::{LinearRangeable, Scaleable, Tickable};
use crate::two::shapes::{
    TwoColor, TwoElement, TwoLine, TwoPath, TwoText, TwoTextAlign, TwoTextBaseline,
};

const DEFAULT_TICK_SIZE: f64 = 6.0;
const DEFAULT_TICK_PADDING: f64 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AxisOrientation {
    Top,
    Bottom,
    Left,
    Right,
}

pub struct Axis<D> {
    orient: AxisOrientation,
    tick_count: Option<usize>,
    tick_values: Option<Vec<D>>,
    tick_format: Option<String>,
    tick_size_inner: f64,
    tick_size_outer: f64,
    tick_padding: f64,
    offset: f64,
}

impl<D: Clone + std::fmt::Display> Axis<D> {
    pub fn new(orient: AxisOrientation) -> Self {
        Self {
            orient,
            tick_count: None,
            tick_values: None,
            tick_format: None,
            tick_size_inner: DEFAULT_TICK_SIZE,
            tick_size_outer: DEFAULT_TICK_SIZE,
            tick_padding: DEFAULT_TICK_PADDING,
            offset: 0.5,
        }
    }

    pub fn ticks(mut self, tick_count: Option<usize>, tick_format: Option<String>) -> Self {
        self.tick_count = tick_count;
        self.tick_format = tick_format;
        self
    }

    /// If a values iterable is specified, the specified values are used for ticks rather than the scale’s automatic tick generator.
    pub fn tick_values(mut self, values: Vec<D>) -> Self {
        self.tick_values = Some(values);
        self
    }

    pub fn tick_size(mut self, size: f64) -> Self {
        self.tick_size_inner = size;
        self.tick_size_outer = size;
        self
    }

    pub fn tick_size_inner(mut self, size: f64) -> Self {
        self.tick_size_inner = size;
        self
    }

    pub fn tick_size_outer(mut self, size: f64) -> Self {
        self.tick_size_outer = size;
        self
    }

    pub fn tick_padding(mut self, padding: f64) -> Self {
        self.tick_padding = padding;
        self
    }

    pub fn offset(mut self, offset: f64) -> Self {
        self.offset = offset;
        self
    }

    // TODO: remove this function (and its tests)
    // as it is no longer used. Or refactor it so it is more useful.
    pub fn generate_elements(
        &self,
        scale: &(impl Tickable<D> + Scaleable<D, f64> + LinearRangeable<f64>),
    ) -> Vec<TwoElement> {
        let mut elements = Vec::new();

        let values = self
            .tick_values
            .clone()
            .unwrap_or_else(|| scale.ticks(self.tick_count));

        // TODO: implement tick formatting functions.
        // TODO: add a scale.tick_format function on the Scale trait.
        /*let format = self
        .tick_format
        .as_ref()
        .map(|f| Box::new(f) as Box<dyn Fn(&T) -> String>)
        .unwrap_or_else(|| scale.tick_format(self.tick_arguments.len()));*/

        let spacing = self.tick_size_inner.max(0.0) + self.tick_padding;
        let range = scale.get_range();
        let range0 = range.0 + self.offset;
        let range1 = range.1 + self.offset;

        let k = match self.orient {
            AxisOrientation::Top | AxisOrientation::Left => -1.0,
            AxisOrientation::Bottom | AxisOrientation::Right => 1.0,
        };

        // Domain path
        let path_points = match self.orient {
            AxisOrientation::Left | AxisOrientation::Right => {
                if self.tick_size_outer > 0.0 {
                    vec![
                        (k * self.tick_size_outer, range0),
                        (self.offset, range0),
                        (self.offset, range1),
                        (k * self.tick_size_outer, range1),
                    ]
                } else {
                    vec![(self.offset, range0), (self.offset, range1)]
                }
            }
            AxisOrientation::Top | AxisOrientation::Bottom => {
                if self.tick_size_outer > 0.0 {
                    vec![
                        (range0, k * self.tick_size_outer),
                        (range0, self.offset),
                        (range1, self.offset),
                        (range1, k * self.tick_size_outer),
                    ]
                } else {
                    vec![(range0, self.offset), (range1, self.offset)]
                }
            }
        };

        elements.push(TwoElement::Path(TwoPath {
            points: path_points,
            stroke: Some(TwoColor::Rgb((0, 0, 0))),
            fill: None,
            linewidth: 1.0,
            opacity: 1.0,
            fill_opacity: 1.0,
            stroke_opacity: 1.0,
        }));

        // Ticks
        for value in values {
            let position = scale.scale(&value) + self.offset;

            // Tick line
            let (x1, y1, x2, y2) = match self.orient {
                AxisOrientation::Left | AxisOrientation::Right => {
                    (0.0, position, k * self.tick_size_inner, position)
                }
                AxisOrientation::Top | AxisOrientation::Bottom => {
                    (position, 0.0, position, k * self.tick_size_inner)
                }
            };
            elements.push(TwoElement::Line(TwoLine {
                x1,
                y1,
                x2,
                y2,
                stroke: Some(TwoColor::Rgb((0, 0, 0))),
                linewidth: 1.0,
                opacity: 1.0,
            }));

            // Tick text
            let (x, y, align, baseline) = match self.orient {
                AxisOrientation::Top => (
                    position,
                    k * spacing,
                    TwoTextAlign::Middle,
                    TwoTextBaseline::Bottom, // Approximates "0em" dy
                ),
                AxisOrientation::Bottom => (
                    position,
                    k * spacing,
                    TwoTextAlign::Middle,
                    TwoTextBaseline::Top, // Approximates "0.71em" dy
                ),
                AxisOrientation::Left => (
                    k * spacing,
                    position,
                    TwoTextAlign::End,
                    TwoTextBaseline::Middle, // Approximates "0.32em" dy
                ),
                AxisOrientation::Right => (
                    k * spacing,
                    position,
                    TwoTextAlign::Start,
                    TwoTextBaseline::Middle, // Approximates "0.32em" dy
                ),
            };

            elements.push(TwoElement::Text(TwoText {
                x,
                y,
                // For now, limit to 3 significant digits
                text: format!("{:.3}", value)
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_string(),
                //text: format(&value),
                align,
                baseline,
                fill: TwoColor::Rgb((0, 0, 0)),
                ..Default::default()
            }));
        }

        elements
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::d3::scale::ScaleLinear;
    use crate::two::svg::{init_svg, update_svg};

    /// Helper function to compare two strings, ignoring newlines and leading/trailing whitespace on each line.
    fn assert_strings_equal_ignore_whitespace(actual: &str, expected: &str) {
        let actual_processed: String = actual
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect();
        let expected_processed: String = expected
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect();
        assert_eq!(actual_processed, expected_processed);
    }

    #[test]
    fn test_axis_bottom() {
        let mut scale = ScaleLinear::new();
        scale.set_domain((0.0, 10.0));
        scale.set_range((0.0, 500.0));

        let axis = Axis::new(AxisOrientation::Bottom).ticks(Some(5), None);
        let elements = axis.generate_elements(&scale);

        let mut ctx = init_svg(500.0, 30.0);
        update_svg(&mut ctx, &elements);

        let expected_svg_str = r#"
            <g height="30" width="500">
                <path d="M 0.5 6 L 0.5 0.5 L 500.5 0.5 L 500.5 6" fill="none" opacity="1" stroke="rgb(0, 0, 0)" stroke-width="1"/>
                <line opacity="1" stroke="rgb(0, 0, 0)" stroke-width="1" x1="0.5" x2="0.5" y1="0" y2="6"/>
                <text dominant-baseline="text-before-edge" fill="rgb(0, 0, 0)" font-family="Helvetica,sans-serif" font-size="14" font-style="normal" font-weight="normal" opacity="1" text-anchor="middle" x="0.5" y="9">
                    0
                </text>
                <line opacity="1" stroke="rgb(0, 0, 0)" stroke-width="1" x1="100.5" x2="100.5" y1="0" y2="6"/>
                <text dominant-baseline="text-before-edge" fill="rgb(0, 0, 0)" font-family="Helvetica,sans-serif" font-size="14" font-style="normal" font-weight="normal" opacity="1" text-anchor="middle" x="100.5" y="9">
                    2
                </text>
                <line opacity="1" stroke="rgb(0, 0, 0)" stroke-width="1" x1="200.5" x2="200.5" y1="0" y2="6"/>
                <text dominant-baseline="text-before-edge" fill="rgb(0, 0, 0)" font-family="Helvetica,sans-serif" font-size="14" font-style="normal" font-weight="normal" opacity="1" text-anchor="middle" x="200.5" y="9">
                    4
                </text>
                <line opacity="1" stroke="rgb(0, 0, 0)" stroke-width="1" x1="300.5" x2="300.5" y1="0" y2="6"/>
                <text dominant-baseline="text-before-edge" fill="rgb(0, 0, 0)" font-family="Helvetica,sans-serif" font-size="14" font-style="normal" font-weight="normal" opacity="1" text-anchor="middle" x="300.5" y="9">
                    6
                </text>
                <line opacity="1" stroke="rgb(0, 0, 0)" stroke-width="1" x1="400.5" x2="400.5" y1="0" y2="6"/>
                <text dominant-baseline="text-before-edge" fill="rgb(0, 0, 0)" font-family="Helvetica,sans-serif" font-size="14" font-style="normal" font-weight="normal" opacity="1" text-anchor="middle" x="400.5" y="9">
                    8
                </text>
                <line opacity="1" stroke="rgb(0, 0, 0)" stroke-width="1" x1="500.5" x2="500.5" y1="0" y2="6"/>
                <text dominant-baseline="text-before-edge" fill="rgb(0, 0, 0)" font-family="Helvetica,sans-serif" font-size="14" font-style="normal" font-weight="normal" opacity="1" text-anchor="middle" x="500.5" y="9">
                    10
                </text>
            </g>
        "#;
        assert_strings_equal_ignore_whitespace(&ctx.group.to_string(), expected_svg_str);
    }

    #[test]
    fn test_axis_left() {
        let mut scale = ScaleLinear::new();
        scale.set_domain((0.0, 10.0));
        scale.set_range((0.0, 500.0));

        let axis = Axis::new(AxisOrientation::Left).ticks(Some(5), None);
        let elements = axis.generate_elements(&scale);

        let mut ctx = init_svg(30.0, 500.0);
        update_svg(&mut ctx, &elements);

        let expected_svg_str = r#"
            <g height="500" width="30">
                <path d="M -6 0.5 L 0.5 0.5 L 0.5 500.5 L -6 500.5" fill="none" opacity="1" stroke="rgb(0, 0, 0)" stroke-width="1"/>
                <line opacity="1" stroke="rgb(0, 0, 0)" stroke-width="1" x1="0" x2="-6" y1="0.5" y2="0.5"/>
                <text dominant-baseline="middle" fill="rgb(0, 0, 0)" font-family="Helvetica,sans-serif" font-size="14" font-style="normal" font-weight="normal" opacity="1" text-anchor="end" x="-9" y="0.5">
                    0
                </text>
                <line opacity="1" stroke="rgb(0, 0, 0)" stroke-width="1" x1="0" x2="-6" y1="100.5" y2="100.5"/>
                <text dominant-baseline="middle" fill="rgb(0, 0, 0)" font-family="Helvetica,sans-serif" font-size="14" font-style="normal" font-weight="normal" opacity="1" text-anchor="end" x="-9" y="100.5">
                    2
                </text>
                <line opacity="1" stroke="rgb(0, 0, 0)" stroke-width="1" x1="0" x2="-6" y1="200.5" y2="200.5"/>
                <text dominant-baseline="middle" fill="rgb(0, 0, 0)" font-family="Helvetica,sans-serif" font-size="14" font-style="normal" font-weight="normal" opacity="1" text-anchor="end" x="-9" y="200.5">
                    4
                </text>
                <line opacity="1" stroke="rgb(0, 0, 0)" stroke-width="1" x1="0" x2="-6" y1="300.5" y2="300.5"/>
                <text dominant-baseline="middle" fill="rgb(0, 0, 0)" font-family="Helvetica,sans-serif" font-size="14" font-style="normal" font-weight="normal" opacity="1" text-anchor="end" x="-9" y="300.5">
                    6
                </text>
                <line opacity="1" stroke="rgb(0, 0, 0)" stroke-width="1" x1="0" x2="-6" y1="400.5" y2="400.5"/>
                <text dominant-baseline="middle" fill="rgb(0, 0, 0)" font-family="Helvetica,sans-serif" font-size="14" font-style="normal" font-weight="normal" opacity="1" text-anchor="end" x="-9" y="400.5">
                    8
                </text>
                <line opacity="1" stroke="rgb(0, 0, 0)" stroke-width="1" x1="0" x2="-6" y1="500.5" y2="500.5"/>
                <text dominant-baseline="middle" fill="rgb(0, 0, 0)" font-family="Helvetica,sans-serif" font-size="14" font-style="normal" font-weight="normal" opacity="1" text-anchor="end" x="-9" y="500.5">
                    10
                </text>
            </g>
        "#;
        assert_strings_equal_ignore_whitespace(&ctx.group.to_string(), expected_svg_str);
    }
}
