use crate::two::shapes::{TwoElement, TwoTextBaseline};
use crate::utils::{PlotParams, RenderContext};
use crate::wgpu;
use vger::Vger;

// TODO: operate the opposite way. ensure that all color fields of TwoElements are [r, g, b[, a]] tuples,
// and only translate them to strings as-needed (e.g., for SVG rendering, using "rgb()" or "rgba()").
fn parse_color_with_opacity(s: &str, opacity: f64) -> vger::color::Color {
    let mut color = vger::color::Color::hex(s).unwrap_or(vger::color::Color::WHITE);
    color.a = opacity as f32;
    color
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

        for element in elements {
            match element {
                TwoElement::Rectangle(d) => {
                    if let Some(rotation) = d.rotation {
                        let cx = d.x + d.width / 2.0;
                        let cy = d.y + d.height / 2.0;
                        vger.save();
                        vger.translate([cx as f32, cy as f32]);
                        vger.rotate(rotation as f32);
                        vger.translate([-(cx as f32), -(cy as f32)]);
                    }

                    if let Some(fill_str) = &d.fill {
                        let color = parse_color_with_opacity(fill_str, d.opacity);
                        let paint = vger.color_paint(color);
                        vger.fill_rect(
                            euclid::rect(d.x as f32, d.y as f32, d.width as f32, d.height as f32),
                            0.0,
                            paint,
                        );
                    }

                    if let Some(stroke_str) = &d.stroke {
                        let color = parse_color_with_opacity(stroke_str, d.opacity);
                        let paint = vger.color_paint(color);
                        vger.stroke_rect(
                            [d.x as f32, d.y as f32].into(),
                            [(d.x + d.width) as f32, (d.y + d.height) as f32].into(),
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
                        vger.fill_circle([d.x as f32, d.y as f32], d.radius as f32, paint);
                    }

                    if let Some(stroke_str) = &d.stroke {
                        let color = parse_color_with_opacity(stroke_str, d.opacity);
                        let paint = vger.color_paint(color);
                        vger.stroke_arc(
                            [d.x as f32, d.y as f32],
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
                            [d.x1 as f32, d.y1 as f32],
                            [d.x2 as f32, d.y2 as f32],
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
                                    [prev.0 as f32, prev.1 as f32],
                                    [p.0 as f32, p.1 as f32],
                                    d.linewidth as f32,
                                    paint,
                                );
                                prev.0 = p.0;
                                prev.1 = p.1;
                            }
                        }
                    }
                }
                TwoElement::Text(d) => {
                    // TODO: either use Fontdue for rendering text (skipping/ignoring the text elements here), or update the VGER shader/font atlas to use the Fontdue logic/implementation.
                    //
                    //
                    // TODO: handle text alignment and baseline properly.
                    // Vger's text rendering origin is bottom-left.
                    if let Some(rotation) = d.rotation {
                        vger.save();
                        vger.translate([d.x as f32, d.y as f32]);
                        vger.rotate(rotation as f32);
                        vger.translate([-(d.x as f32), -(d.y as f32)]);
                    } else {
                        vger.translate([d.x as f32, d.y as f32]);
                    }

                    let color = parse_color_with_opacity(&d.fill, d.opacity);
                    vger.text(&d.text, d.fontsize as u32, color, None);

                    if d.rotation.is_some() {
                        vger.restore();
                    }
                }
            }
        }

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
            occlusion_query_set: None,
            timestamp_writes: None,
        };

        vger.encode(&desc);
    });
}
