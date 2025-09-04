/*
use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight,
};
use wgpu::MultisampleState;


// Begin text rendering things.
    // Set up text renderer
    // Create a new FontSystem, that allows access to any installed system fonts
    // Timing: This function takes some time to run.
    // On the release build, it can take up to a second, while debug builds can take up to ten times longer.
    // For this reason, it should only be called once, and the resulting FontSystem should be shared.
    let mut font_system = FontSystem::new();

    // Note: in wasm32-unknown-unknown there is no OS font access, so nothing can be found.
    // In WebGPU/WASM you must provide at least one font yourself
    // and register it with the FontSystem before you create buffers / shape text.
    // Embed a font so WASM has something to use.
    let font_bytes: &[u8] = include_bytes!("fonts/Inter-Bold.ttf");
    font_system.db_mut().load_font_data(Cow::Borrowed(font_bytes).to_vec());
    // TODO: alternatively, the font bytes can be passed from JS more dynamically,
    // so that they do not need to be embedded into the WASM binary.


    let mut swash_cache = SwashCache::new();
    let cache = Cache::new(&context.device);
    let mut viewport = Viewport::new(&context.device, &cache);
    viewport.update(&context.queue, Resolution {
        width: context.width as u32,
        height: context.height as u32
    });
    let swapchain_format = context.texture_desc.format;
    let mut atlas = TextAtlas::new(
        &context.device, &context.queue, &cache, swapchain_format,
    );
    let mut text_renderer =
        TextRenderer::new(&mut atlas, &context.device, MultisampleState::default(), None);
    let attrs = Attrs::new()
        .family(Family::Name("Inter"))
        .weight(Weight::BOLD);
    let mut text_buffer = Buffer::new(&mut font_system, Metrics::new(30.0, 42.0));

    text_buffer.set_size(
        &mut font_system,
        Some(context.width as f32),
        None,
    );
    text_buffer.set_text(
        &mut font_system,
        "Hello world! \nThis is rendered with Glyphon\nThe text below should be partially clipped.\na b c d e f g h i j k l m n o p q r s t u v w x y z",
        &attrs,
        Shaping::Advanced
    );
    text_buffer.shape_until_scroll(&mut font_system, false);

    text_renderer
        .prepare(
            &context.device,
            &context.queue,
            &mut font_system,
            &mut atlas,
            &viewport,
            [TextArea {
                buffer: &text_buffer,
                left: 0.0,
                top: 0.0,
                scale: 1.0,
                // The visible bounds of the text area.
                // This is used to clip the text and doesn’t have to match the left and top values.
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: context.width as i32,
                    bottom: context.height as i32,
                },
                default_color: Color::rgb(255, 0, 255),
                custom_glyphs: &[],
            }],
            &mut swash_cache,
        )
        .unwrap();


    // ...

    render_pass.set_pipeline(&render_pipeline);
    render_pass.draw(0..3, 0..1);

    text_renderer.render(&atlas, &viewport, &mut render_pass).unwrap();

    // End the renderpass.
    atlas.trim();
    drop(render_pass);
*/
