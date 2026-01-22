// Reference: https://github.com/keller-mark/deck-to-svg/blob/main/lib/src/svg.js

use svg::node::element::{Circle, Group, Line, Path, Rectangle, Text};
use svg::Document;

use crate::two::shapes::{TwoColor, TwoElement, TwoTextBaseline};

pub fn init_svg(width: f64, height: f64) -> (Document, Group) {
    let document = Document::new()
        .set("width", width)
        .set("height", height)
        .set("xmlns", "http://www.w3.org/2000/svg");

    let group = Group::new().set("width", width).set("height", height);

    // TODO: is defs needed? or is it possible to put clipPaths anywhere in the node tree?
    let defs = svg::node::element::Definitions::new();

    (document, group)
}

pub fn update_svg(mut group: Group, elements: &[TwoElement]) -> Group {
    for element in elements {
        group = match element {
            TwoElement::Group(d) => {
                let mut sub_group = Group::new();
                if let Some(translate) = d.translate {
                    sub_group = sub_group.set(
                        "transform",
                        format!("translate({},{})", translate.0, translate.1),
                    );
                }
                if let Some(clip_rect) = d.clip_rect {
                    let clip_path_id = if let Some(layer_id) = &d.layer_id {
                        &format!("{}_clip_path", layer_id)
                    } else {
                        // TODO: generate unique IDs if multiple clip paths are needed.
                        // Keep track of used ids by passing down some kind of container struct that wraps the group
                        // to keep track of this state.
                        "clipPath1"
                    };
                    let clip_path = svg::node::element::ClipPath::new()
                        .set("id", clip_path_id)
                        .add(
                            Rectangle::new()
                                .set("x", clip_rect.0)
                                .set("y", clip_rect.1)
                                .set("width", clip_rect.2)
                                .set("height", clip_rect.3),
                        );
                    // TODO: does the clip path need to be within <defs>?
                    // TODO: does it matter if the clipPath is inserted into a translated group?
                    sub_group = sub_group.set("clip-path", format!("url(#{})", clip_path_id));
                    group = group.add(clip_path);
                }
                // Recursion.
                sub_group = update_svg(sub_group, &d.elements);
                group.add(sub_group)
            }
            TwoElement::Rectangle(d) => {
                let mut rect = Rectangle::new()
                    .set("x", d.x)
                    .set("y", d.y)
                    .set("width", d.width)
                    .set("height", d.height)
                    .set("opacity", d.opacity);

                if let Some(fill) = &d.fill {
                    rect = rect.set("fill", fill.to_string());
                }

                if let Some(stroke) = &d.stroke {
                    rect = rect
                        .set("stroke-width", d.linewidth)
                        .set("stroke", stroke.to_string());
                }

                if let Some(rotation) = d.rotation {
                    let deg = rotation.to_degrees();
                    let cx = d.x + d.width / 2.0;
                    let cy = d.y + d.height / 2.0;
                    rect = rect.set("transform", format!("rotate({deg},{cx},{cy})"));
                }
                group.add(rect)
            }
            TwoElement::Circle(d) => {
                let mut circle = Circle::new()
                    .set("cx", d.x)
                    .set("cy", d.y)
                    .set("r", d.radius)
                    .set("opacity", d.opacity);

                if let Some(fill) = &d.fill {
                    circle = circle.set("fill", fill.to_string());
                }

                if let Some(stroke) = &d.stroke {
                    circle = circle
                        .set("stroke-width", d.linewidth)
                        .set("stroke", stroke.to_string());
                }
                group.add(circle)
            }
            TwoElement::Line(d) => {
                let mut line = Line::new()
                    .set("x1", d.x1)
                    .set("y1", d.y1)
                    .set("x2", d.x2)
                    .set("y2", d.y2)
                    .set("opacity", d.opacity);

                if let Some(stroke) = &d.stroke {
                    line = line
                        .set("stroke-width", d.linewidth)
                        .set("stroke", stroke.to_string());
                }

                group.add(line)
            }
            TwoElement::Path(d) => {
                let mut path_d = String::new();
                if let Some((first, rest)) = d.points.split_first() {
                    path_d.push_str(&format!("M {} {}", first.0, first.1));
                    for p in rest {
                        path_d.push_str(&format!(" L {} {}", p.0, p.1));
                    }
                }

                let mut path = Path::new().set("opacity", d.opacity).set("d", path_d);

                if let Some(fill) = &d.fill {
                    path = path.set("fill", fill.to_string());
                } else {
                    path = path.set("fill", "none");
                }

                if let Some(stroke) = &d.stroke {
                    path = path
                        .set("stroke-width", d.linewidth)
                        .set("stroke", stroke.to_string());
                }

                group.add(path)
            }
            TwoElement::Text(d) => {
                let baseline = match d.baseline {
                    TwoTextBaseline::Top => "text-before-edge".to_string(),
                    TwoTextBaseline::Bottom => "text-after-edge".to_string(),
                    _ => d.baseline.to_string(),
                };

                // For now, we just add the text content directly.
                // TODO: implement getComputedWidth to be able to render ellipses when text is too long.
                // // Reference: https://github.com/keller-mark/deck-to-svg/blob/cc7b26333aa2d1f5ff3ade1c243c0e30893518aa/lib/src/svg.js#L121

                let mut text = Text::new(d.text.as_str())
                    .set("x", d.x)
                    .set("y", d.y)
                    .set("text-anchor", d.align.to_string())
                    .set("dominant-baseline", baseline)
                    .set("opacity", d.opacity)
                    .set("fill", d.fill.to_string())
                    .set("font-size", d.fontsize)
                    .set("font-family", d.font.as_str());

                if let Some(rotation) = d.rotation {
                    let deg = rotation.to_degrees();
                    text = text.set("transform", format!("rotate({deg},{},{})", d.x, d.y));
                }
                group.add(text)
            }
        };
    }
    group
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::two::shapes::{
        TwoCircle, TwoLine, TwoPath, TwoRectangle, TwoText, TwoTextAlign, TwoTextBaseline,
    };

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
    fn test_init_svg() {
        let (doc, group) = init_svg(100.0, 200.0);
        let expected_doc_str =
            r#"<svg height="200" width="100" xmlns="http://www.w3.org/2000/svg"/>"#;
        let expected_group_str = r#"<g height="200" width="100"/>"#;
        assert_eq!(doc.to_string(), expected_doc_str);
        assert_eq!(group.to_string(), expected_group_str);
    }

    #[test]
    fn test_update_svg() {
        let elements = vec![
            TwoElement::Rectangle(TwoRectangle {
                x: 10.0,
                y: 20.0,
                width: 30.0,
                height: 40.0,
                opacity: 0.5,
                fill: Some(TwoColor::Rgb((255, 0, 0))),
                stroke: Some(TwoColor::Rgb((0, 0, 255))),
                linewidth: 2.0,
                rotation: Some(std::f64::consts::PI / 4.0),
            }),
            TwoElement::Circle(TwoCircle {
                x: 50.0,
                y: 60.0,
                radius: 15.0,
                opacity: 1.0,
                fill: Some(TwoColor::Rgb((0, 255, 0))),
                stroke: None,
                linewidth: 1.0,
            }),
            TwoElement::Line(TwoLine {
                x1: 70.0,
                y1: 80.0,
                x2: 90.0,
                y2: 100.0,
                opacity: 1.0,
                stroke: Some(TwoColor::Rgb((0, 0, 0))),
                linewidth: 3.0,
            }),
            TwoElement::Path(TwoPath {
                points: vec![(110.0, 120.0), (130.0, 140.0)],
                opacity: 1.0,
                fill: Some(TwoColor::Rgb((0, 255, 255))),
                stroke: Some(TwoColor::Rgb((255, 0, 255))),
                linewidth: 4.0,
            }),
            TwoElement::Text(TwoText {
                text: "Hello".to_string(),
                x: 150.0,
                y: 160.0,
                width: 100.0,
                height: 100.0,
                opacity: 1.0,
                fill: TwoColor::Rgb((0, 128, 255)),
                fontsize: 12.0,
                font: "Arial".to_string(),
                align: TwoTextAlign::Middle,
                baseline: TwoTextBaseline::Middle,
                rotation: None,
                overflow: None,
            }),
        ];

        let (_, group) = init_svg(200.0, 200.0);
        let updated_group = update_svg(group, &elements);

        let expected_svg_str = r#"
            <g height="200" width="200">
                <rect fill="rgb(255, 0, 0)" height="40" opacity="0.5" stroke="rgb(0, 0, 255)" stroke-width="2" transform="rotate(45,25,40)" width="30" x="10" y="20"/>
                <circle cx="50" cy="60" fill="rgb(0, 255, 0)" opacity="1" r="15"/>
                <line opacity="1" stroke="rgb(0, 0, 0)" stroke-width="3" x1="70" x2="90" y1="80" y2="100"/>
                <path d="M 110 120 L 130 140" fill="rgb(0, 255, 255)" opacity="1" stroke="rgb(255, 0, 255)" stroke-width="4"/>
                <text dominant-baseline="middle" fill="rgb(0, 128, 255)" font-family="Arial" font-size="12" opacity="1" text-anchor="middle" x="150" y="160">Hello</text>
            </g>
        "#;

        assert_strings_equal_ignore_whitespace(&updated_group.to_string(), expected_svg_str);
    }
}
