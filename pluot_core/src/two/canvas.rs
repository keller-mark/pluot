use crate::params::{PlotParams, RenderContext};
use crate::two::shapes::{TwoColor, TwoElement, TwoGroup, TwoText, TwoTextBaseline};
use crate::wgpu;
use vger::Vger;

fn filter_text_elements(elements: &[TwoElement]) -> Vec<TwoText> {
    // Filter to only text elements.
    // This needs to be recursive to handle those within groups.
    // It also needs to clone the text elements and apply any group transforms to them.
    let mut texts = Vec::new();
    for element in elements {
        match element {
            TwoElement::Text(text) => texts.push(text.clone()),
            TwoElement::Group(group) => {
                let mut group_texts = filter_text_elements(&group.elements);
                // Apply group transforms to each text element.
                for text in &mut group_texts {
                    if let Some((tx, ty)) = group.translate {
                        text.x += tx;
                        text.y += ty;
                    }
                    if let Some(group_rotation) = group.rotation {
                        // TODO: fix this to properly rotate around the center of the text box.
                        // Currently it just adds the group rotation to the text rotation.
                        // This is not correct if the text box is not at the origin.
                        // A proper implementation would need to compute the center of the text box,
                        // translate the text box to the origin, apply the rotation, and then translate back.
                        text.rotation = Some(match text.rotation {
                            Some(r) => r + group_rotation,
                            None => group_rotation,
                        });
                    }
                }
                texts.extend(group_texts);
            }
            _ => {}
        }
    }
    texts
}

fn parse_color_with_opacity(color: &TwoColor, opacity: f64) -> vger::color::Color {
    match color {
        TwoColor::Rgb(rgb) => vger::color::Color::new(
            rgb.0 as f32 / 255.0,
            rgb.1 as f32 / 255.0,
            rgb.2 as f32 / 255.0,
            opacity as f32,
        ),
        TwoColor::Rgba(rgba) => vger::color::Color::new(
            rgba.0 as f32 / 255.0,
            rgba.1 as f32 / 255.0,
            rgba.2 as f32 / 255.0,
            (rgba.3 as f32 / 255.0) * opacity as f32,
        ),
    }
}

// We need this function separately to enable recursion.
fn process_shapes(vger: &mut Vger, elements: &[TwoElement], width: f32, height: f32) {
    // Note: The VGER coordinate system (0,0) is in the bottom left.
    // We want to define coordinates in the top left (so it matches SVG).
    // We need to adjust the Y coordinates accordingly.

    for element in elements {
        match element {
            TwoElement::Group(d) => {
                if let Some(translate) = d.translate {
                    vger.save();
                    vger.translate([translate.0 as f32, -translate.1 as f32]);
                }

                process_shapes(vger, &d.elements, width, height);

                if d.translate.is_some() {
                    vger.restore();
                }
            }
            TwoElement::Rectangle(d) => {
                if let Some(rotation) = d.rotation {
                    let cx = d.x + d.width / 2.0;
                    let cy = (height as f64 - d.y) + d.height / 2.0;
                    vger.save();
                    vger.translate([cx as f32, cy as f32]);
                    vger.rotate(rotation as f32);
                    vger.translate([-(cx as f32), -(cy as f32)]);
                }

                if let Some(fill_str) = &d.fill {
                    let color = parse_color_with_opacity(fill_str, d.opacity);
                    let paint = vger.color_paint(color);
                    vger.fill_rect(
                        euclid::rect(
                            d.x as f32,
                            height - d.y as f32 - d.height as f32,
                            d.width as f32,
                            d.height as f32,
                        ),
                        0.0,
                        paint,
                    );
                }

                if let Some(stroke_str) = &d.stroke {
                    let color = parse_color_with_opacity(stroke_str, d.opacity);
                    let paint = vger.color_paint(color);
                    vger.stroke_rect(
                        [d.x as f32, height - d.y as f32 - d.height as f32].into(),
                        [(d.x + d.width) as f32, height - d.y as f32].into(),
                        0.0,
                        d.linewidth as f32,
                        paint,
                    );
                }

                if d.rotation.is_some() {
                    vger.restore();
                }
            }
            TwoElement::Circle(d) => {
                if let Some(fill_str) = &d.fill {
                    let color = parse_color_with_opacity(fill_str, d.opacity);
                    let paint = vger.color_paint(color);
                    vger.fill_circle([d.x as f32, height - d.y as f32], d.radius as f32, paint);
                }

                if let Some(stroke_str) = &d.stroke {
                    let color = parse_color_with_opacity(stroke_str, d.opacity);
                    let paint = vger.color_paint(color);
                    vger.stroke_arc(
                        [d.x as f32, height - d.y as f32],
                        d.radius as f32,
                        d.linewidth as f32,
                        0.0,
                        2.0 * std::f32::consts::PI,
                        paint,
                    );
                }
            }
            TwoElement::Line(d) => {
                if let Some(stroke_str) = &d.stroke {
                    let color = parse_color_with_opacity(stroke_str, d.opacity);
                    let paint = vger.color_paint(color);
                    vger.stroke_segment(
                        [d.x1 as f32, height - d.y1 as f32],
                        [d.x2 as f32, height - d.y2 as f32],
                        d.linewidth as f32,
                        paint,
                    );
                }
            }
            TwoElement::Path(d) => {
                if let Some((start, rest)) = d.points.split_first() {
                    let mut prev = (start.0, start.1);
                    for p in rest {
                        if let Some(stroke_str) = &d.stroke {
                            let color = parse_color_with_opacity(stroke_str, d.opacity);
                            let paint = vger.color_paint(color);
                            vger.stroke_segment(
                                [prev.0 as f32, height - prev.1 as f32],
                                [p.0 as f32, height - p.1 as f32],
                                d.linewidth as f32,
                                paint,
                            );
                            prev.0 = p.0;
                            prev.1 = p.1;
                        }
                    }
                }
            }
            TwoElement::Text(_) => {
                /*
                    // Ignore this if we are instead using Fontdue for rendering text (skipping/ignoring the text elements here).
                    // Alternatively, we could update the VGER shader/font atlas to use the Fontdue logic/implementation.
                    //
                    // TODO: handle text alignment and baseline properly.
                    // Vger's text rendering origin is bottom-left.
                    vger.save();
                    if let Some(rotation) = d.rotation {
                        vger.translate([d.x as f32, height - d.y as f32]);
                        vger.rotate(rotation as f32);
                        vger.translate([-(d.x as f32), -(height - d.y as f32)]);
                    } else {
                        // TODO: does the text height need to be subtracted here (like for the rectangle case)?
                        vger.translate([d.x as f32, height - d.y as f32]);
                    }

                    let color = parse_color_with_opacity(&d.fill, d.opacity);
                    vger.text(&d.text, d.fontsize as u32, color, None);

                    // Note: must have called vger.save() in order to call .restore().
                    vger.restore();
                */
            }
        }
    }
}

pub fn render_shapes(
    context: &mut RenderContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
    elements: &[TwoElement],
) {
    let width = context.params.width as f32;
    let height = context.params.height as f32;
    let vello_view = context
        .vello_tex
        .create_view(&wgpu::TextureViewDescriptor::default());

    // === Render with Vger into our texture ===
    crate::two::text_vger::with_vger_renderer(context.device, context.queue, |vger| {
        vger.begin(width, height, 1.0);

        process_shapes(vger, elements, width, height);

        let desc = wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &vello_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        };

        vger.encode(&desc);
    });

    let text_elements = filter_text_elements(elements);

    crate::two::text_fontdue::render_text(context, encoder, &text_elements);
}
