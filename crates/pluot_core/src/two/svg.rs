// Reference: https://github.com/keller-mark/deck-to-svg/blob/main/lib/src/svg.js

use std::collections::HashMap;

use svg::node::element::{Circle, Definitions, Group, Image, Line, Path, Rectangle, Text};
use svg::Document;

use crate::two::shapes::{TwoColor, TwoElement, TwoTextBaseline};

/// Container that pairs an SVG `Document` and `Group` with shared rendering
/// state threaded through the recursive `update_svg` call tree.
///
/// Currently it tracks clip-path definitions so that:
/// - identical clip rectangles are reused (same `id`, no duplicate `<clipPath>`
///   elements emitted), and
/// - anonymous clip paths get stable, unique IDs even when multiple groups lack
///   an explicit `layer_id`.
///
/// All `<clipPath>` elements are collected in `clip_paths` and emitted into a
/// `<defs>` block at serialization time, in conformance with the SVG spec.
pub struct SvgContext {
    /// The `<svg>` document wrapper (used when serializing with `include_document = true`).
    pub document: Document,
    pub group: Group,
    /// Map from the bit-patterns of `(x, y, width, height)` --> clip-path id.
    /// Keyed on bit-patterns so that identical `f64` values map to the same id
    /// without any floating-point tolerance concerns.
    clip_path_ids: HashMap<(u64, u64, u64, u64), String>,
    next_clip_id: u32,
    /// Accumulated `<clipPath>` elements to be written into `<defs>` on serialization.
    clip_paths: Vec<svg::node::element::ClipPath>,
}

impl SvgContext {
    pub fn new(document: Document, group: Group) -> Self {
        Self {
            document,
            group,
            clip_path_ids: HashMap::new(),
            next_clip_id: 1,
            clip_paths: Vec::new(),
        }
    }

    /// Serialize the context to an SVG string.
    ///
    /// - `include_document = true`: wraps the group inside the `<svg>` document element.
    /// - `include_document = false`: returns only the inner `<g>` group string.
    ///
    /// If any clip paths were accumulated during `update_svg`, they are emitted
    /// inside a `<defs>` element that precedes the main group, per the SVG spec.
    pub fn to_svg_string(&self, include_document: bool) -> String {
        // Build <defs> if needed, then the root group.
        let root_group: svg::node::element::Element = if self.clip_paths.is_empty() {
            self.group.clone().into()
        } else {
            let mut defs = Definitions::new();
            for cp in &self.clip_paths {
                defs = defs.add(cp.clone());
            }
            // Wrap defs + group in an outer <g> so the returned string is always a single element.
            // TODO: is this extra <g> parent of <defs> valid/allowed per the SVG spec? How to avoid this?
            Group::new().add(defs).add(self.group.clone()).into()
        };

        if include_document {
            self.document.clone().add(root_group).to_string()
        } else {
            root_group.to_string()
        }
    }

    /// Returns `(id, is_new)`.
    ///
    /// If `clip_rect` has been seen before the existing id is returned and
    /// `is_new` is `false` — no `<clipPath>` element should be emitted.
    ///
    /// On the first occurrence `preferred_id` is used when provided, otherwise
    /// a fresh `clipPathN` id is generated.  `is_new` is `true` and the caller
    /// should emit the corresponding `<clipPath>` element.
    fn get_or_create_clip_path_id(
        &mut self,
        clip_rect: (f64, f64, f64, f64),
        preferred_id: Option<String>,
    ) -> (String, bool) {
        let key = (
            clip_rect.0.to_bits(),
            clip_rect.1.to_bits(),
            clip_rect.2.to_bits(),
            clip_rect.3.to_bits(),
        );
        if let Some(existing) = self.clip_path_ids.get(&key) {
            return (existing.clone(), false);
        }
        let id = preferred_id.unwrap_or_else(|| {
            let id = format!("clipPath{}", self.next_clip_id);
            self.next_clip_id += 1;
            id
        });
        self.clip_path_ids.insert(key, id.clone());
        (id, true)
    }
}

pub fn init_svg(width: f64, height: f64) -> SvgContext {
    let document = Document::new()
        .set("width", width)
        .set("height", height)
        .set("xmlns", "http://www.w3.org/2000/svg");

    let group = Group::new().set("width", width).set("height", height);

    SvgContext::new(document, group)
}

/// Append `elements` into `ctx.group`, threading clip-path state through `ctx`.
pub fn update_svg(ctx: &mut SvgContext, elements: &[TwoElement]) {
    let mut group = std::mem::replace(&mut ctx.group, Group::new());

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
                    // TODO: use layer_type here?
                    // Or change layer_id to group_id,
                    // and the caller can handle "{layer_type}_{layer_id}" concatenation.
                    let preferred = d.layer_id.as_ref().map(|id| format!("{}_clip_path", id));
                    // Generate unique IDs if multiple clip paths are needed. Keep track of used ids.
                    let (clip_path_id, is_new) =
                        ctx.get_or_create_clip_path_id(clip_rect, preferred);

                    if is_new {
                        let clip_path = svg::node::element::ClipPath::new()
                            .set("id", clip_path_id.as_str())
                            .add(
                                Rectangle::new()
                                    .set("x", clip_rect.0)
                                    .set("y", clip_rect.1)
                                    .set("width", clip_rect.2)
                                    .set("height", clip_rect.3),
                            );
                        // Collect the <clipPath> for later emission inside <defs>.
                        ctx.clip_paths.push(clip_path);
                    }

                    // TODO: does it matter if the clipPath is inserted into a translated group?
                    sub_group =
                        sub_group.set("clip-path", format!("url(#{})", clip_path_id));
                }

                let mut sub_ctx = SvgContext {
                    document: ctx.document.clone(),
                    group: sub_group,
                    clip_path_ids: std::mem::take(&mut ctx.clip_path_ids),
                    next_clip_id: ctx.next_clip_id,
                    clip_paths: std::mem::take(&mut ctx.clip_paths),
                };
                // Recursion.
                update_svg(&mut sub_ctx, &d.elements);
                ctx.clip_path_ids = sub_ctx.clip_path_ids;
                ctx.next_clip_id = sub_ctx.next_clip_id;
                ctx.clip_paths = sub_ctx.clip_paths;

                group.add(sub_ctx.group)
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
                let mut path = Path::new().set("opacity", d.opacity).set("d", d.d.as_str());

                if let Some(fill) = &d.fill {
                    path = path.set("fill", fill.to_string());
                    if d.fill_opacity < 1.0 {
                        path = path.set("fill-opacity", d.fill_opacity);
                    }
                } else {
                    path = path.set("fill", "none");
                }

                if let Some(stroke) = &d.stroke {
                    path = path
                        .set("stroke-width", d.linewidth)
                        .set("stroke", stroke.to_string());
                    if d.stroke_opacity < 1.0 {
                        path = path.set("stroke-opacity", d.stroke_opacity);
                    }
                    if let Some(linejoin) = &d.stroke_linejoin {
                        path = path.set("stroke-linejoin", linejoin.as_str());
                    }
                    if let Some(linecap) = &d.stroke_linecap {
                        path = path.set("stroke-linecap", linecap.as_str());
                    }
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
                    .set("font-family", d.font_family.as_str())
                    .set("font-weight", d.font_weight.as_str()) // TODO: omit when "normal" to minimize SVG size
                    .set("font-style", d.font_style.as_str());  // TODO: omit when "normal" to minimize SVG size

                if let Some(rotation) = d.rotation {
                    text = text.set("transform", format!("rotate({} {} {})", rotation, d.x, d.y));
                }
                group.add(text)
            }
            TwoElement::Image(d) => {
                let mut image = Image::new()
                    .set("x", d.x)
                    .set("y", d.y)
                    .set("width", d.width)
                    .set("height", d.height)
                    .set("href", d.href.as_str())
                    .set("opacity", d.opacity);

                if let Some(image_rendering_style) = &d.image_rendering_style {
                    let style_str = image_rendering_style.to_string();
                    image = image.set("style", format!("image-rendering: {}", style_str));
                }

                group.add(image)
            }
        };
    }

    ctx.group = group;
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
        let ctx = init_svg(100.0, 200.0);
        let expected_doc_str =
            r#"<svg height="200" width="100" xmlns="http://www.w3.org/2000/svg"/>"#;
        let expected_group_str = r#"<g height="200" width="100"/>"#;
        assert_eq!(ctx.document.to_string(), expected_doc_str);
        assert_eq!(ctx.group.to_string(), expected_group_str);
    }

    #[test]
    fn test_to_svg_string() {
        let ctx = init_svg(50.0, 60.0);
        let without_doc = ctx.to_svg_string(false);
        let with_doc = ctx.to_svg_string(true);
        assert_eq!(without_doc, r#"<g height="60" width="50"/>"#);
        assert!(with_doc.starts_with("<svg"));
        assert!(with_doc.contains(r#"width="50""#));
        assert!(with_doc.contains("<g"));
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
                d: "M 110 120 L 130 140".to_string(),
                opacity: 1.0,
                fill: Some(TwoColor::Rgb((0, 255, 255))),
                fill_opacity: 1.0,
                stroke: Some(TwoColor::Rgb((255, 0, 255))),
                stroke_opacity: 1.0,
                linewidth: 4.0,
                stroke_linejoin: None,
                stroke_linecap: None,
            }),
            TwoElement::Text(TwoText {
                text: "Hello".to_string(),
                x: 150.0,
                y: 160.0,
                opacity: 1.0,
                fill: TwoColor::Rgb((0, 128, 255)),
                fontsize: 12.0,
                font_family: "Arial".to_string(),
                font_weight: "normal".to_string(),
                font_style: "normal".to_string(),
                align: TwoTextAlign::Middle,
                baseline: TwoTextBaseline::Middle,
                rotation: None,
                overflow: None,
            }),
        ];

        let mut ctx = init_svg(200.0, 200.0);
        update_svg(&mut ctx, &elements);

        let expected_svg_str = r#"
            <g height="200" width="200">
                <rect fill="rgb(255, 0, 0)" height="40" opacity="0.5" stroke="rgb(0, 0, 255)" stroke-width="2" transform="rotate(45,25,40)" width="30" x="10" y="20"/>
                <circle cx="50" cy="60" fill="rgb(0, 255, 0)" opacity="1" r="15"/>
                <line opacity="1" stroke="rgb(0, 0, 0)" stroke-width="3" x1="70" x2="90" y1="80" y2="100"/>
                <path d="M 110 120 L 130 140" fill="rgb(0, 255, 255)" opacity="1" stroke="rgb(255, 0, 255)" stroke-width="4"/>
                <text dominant-baseline="middle" fill="rgb(0, 128, 255)" font-family="Arial" font-size="12" font-style="normal" font-weight="normal" opacity="1" text-anchor="middle" x="150" y="160">Hello</text>
            </g>
        "#;

        assert_strings_equal_ignore_whitespace(&ctx.group.to_string(), expected_svg_str);
    }

    #[test]
    fn test_clip_path_deduplication() {
        use crate::two::shapes::TwoGroup;

        let clip_rect = (0.0_f64, 0.0_f64, 100.0_f64, 100.0_f64);

        // Two groups with the same clip rect but different layer_ids — the second
        // should reuse the first clip-path id and not emit a duplicate <clipPath>.
        let elements = vec![
            TwoElement::Group(TwoGroup {
                layer_id: Some("layerA".to_string()),
                clip_rect: Some(clip_rect),
                ..TwoGroup::default()
            }),
            TwoElement::Group(TwoGroup {
                layer_id: Some("layerB".to_string()),
                clip_rect: Some(clip_rect),
                ..TwoGroup::default()
            }),
        ];

        let mut ctx = init_svg(200.0, 200.0);
        update_svg(&mut ctx, &elements);

        let svg_str = ctx.to_svg_string(false);
        // Only one <clipPath> element should appear, inside <defs>.
        assert_eq!(svg_str.matches("<clipPath").count(), 1);
        assert!(svg_str.contains("<defs>"));
        // Both sub-groups should reference the same clip-path id (layerA wins).
        assert_eq!(svg_str.matches("url(#layerA_clip_path)").count(), 2);
    }

    #[test]
    fn test_clip_path_unique_ids_for_distinct_rects() {
        use crate::two::shapes::TwoGroup;

        let elements = vec![
            TwoElement::Group(TwoGroup {
                clip_rect: Some((0.0, 0.0, 50.0, 50.0)),
                ..TwoGroup::default()
            }),
            TwoElement::Group(TwoGroup {
                clip_rect: Some((10.0, 10.0, 80.0, 80.0)),
                ..TwoGroup::default()
            }),
        ];

        let mut ctx = init_svg(200.0, 200.0);
        update_svg(&mut ctx, &elements);

        let svg_str = ctx.to_svg_string(false);
        // Two distinct rects --> two distinct <clipPath> elements, inside <defs>.
        assert_eq!(svg_str.matches("<clipPath").count(), 2);
        assert!(svg_str.contains("<defs>"));
        assert!(svg_str.contains("clipPath1"));
        assert!(svg_str.contains("clipPath2"));
    }
}
