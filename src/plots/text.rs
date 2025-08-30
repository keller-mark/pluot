use std::borrow::Cow;

use crate::render::with_vello_renderer;

use skrifa::MetadataProvider;
use vello::wgpu;
use vello::{
    peniko::{Blob, Brush, Color, Fill, Font},
    kurbo::{Affine, Circle, Ellipse, Line, RoundedRect, Stroke},
    AaConfig, AaSupport, Renderer, RendererOptions, Scene,
};

const FONT_BYTES: &[u8] = include_bytes!("fonts/Inter-Bold.ttf").as_slice();

/// Add shapes to a vello scene. This does not actually render the shapes, but adds them
/// to the Scene data structure which represents a set of objects to draw.
pub fn add_shapes_to_scene(scene: &mut Scene) {
    // Draw an outlined rectangle
    let stroke = Stroke::new(6.0);
    let rect = RoundedRect::new(10.0, 10.0, 240.0, 240.0, 20.0);
    let rect_stroke_color = Color::new([0.9804, 0.702, 0.5294, 1.]);
    scene.stroke(&stroke, Affine::IDENTITY, rect_stroke_color, None, &rect);

    // Draw a filled circle
    let circle = Circle::new((420.0, 200.0), 120.0);
    let circle_fill_color = Color::new([0.9529, 0.5451, 0.6588, 1.]);
    scene.fill(
        vello::peniko::Fill::NonZero,
        Affine::IDENTITY,
        circle_fill_color,
        None,
        &circle,
    );

    // Draw a filled ellipse
    let ellipse = Ellipse::new((250.0, 420.0), (100.0, 160.0), -90.0);
    let ellipse_fill_color = Color::new([0.7961, 0.651, 0.9686, 1.]);
    scene.fill(
        vello::peniko::Fill::NonZero,
        Affine::IDENTITY,
        ellipse_fill_color,
        None,
        &ellipse,
    );

    // Draw a straight line
    let line = Line::new((260.0, 20.0), (620.0, 100.0));
    let line_stroke_color = Color::new([0.5373, 0.7059, 0.9804, 1.]);
    scene.stroke(&stroke, Affine::IDENTITY, line_stroke_color, None, &line);
}

pub fn add_text_to_scene(scene: &mut Scene) {
    // Load a font from bytes (you can replace this with any TTF/OTF you own).
    let font_bytes: Cow<'static, [u8]> = Cow::from(FONT_BYTES);
    let blob = Blob::new(std::sync::Arc::new(font_bytes));
    let peniko_font = Font::new(blob, 0);

    // TODO: explore using Parley https://github.com/linebender/parley

    // Reference: https://github.com/linebender/vello/blob/main/examples/scenes/src/simple_text.rs
    // Build a simple “Hello, world” glyph run using Skrifa:
    let font_ref = skrifa::FontRef::new(peniko_font.data.as_ref()).expect("parse font");

    // choose a pixel size and compute scale factor from design units to px
    let px_size: f32 = 64.0;

    // map chars -> glyph ids, accumulate x advances
    let cmap = font_ref.charmap();
    let axes = font_ref.axes();
    let font_size = skrifa::instance::Size::new(px_size);
    let variations: &[(&str, f32)] = &[];
    let var_loc = axes.location(variations.iter().copied());
    let metrics = font_ref.metrics(font_size, &var_loc);
    let line_height = metrics.ascent - metrics.descent + metrics.leading;
    let glyph_metrics = font_ref.glyph_metrics(font_size, &var_loc);

    let text = "Hello, world!";
    let mut pen_x = 0_f32;
    let pen_y = line_height;
    let mut glyphs = Vec::with_capacity(text.len());

    for ch in text.chars() {
        if let Some(gid) = cmap.map(ch) {
            // advance in *pixels*
            let adv: f32 = glyph_metrics
                .advance_width(gid)
                .unwrap_or(0.0); // in px because we passed Size::new(px_size)
            glyphs.push(vello::Glyph {
                id: gid.to_u32(),
                x: pen_x,
                y: pen_y,
            });
            pen_x += adv;
        }
    }

    // Draw the glyph run: white fill
    scene
        .draw_glyphs(&peniko_font)
        .font_size(px_size)
        .hint(true)
        .brush(&Brush::Solid(Color::from_rgb8(240, 0, 245)))
        .draw(Fill::NonZero, glyphs.into_iter());
}
