// Roughly corresponds to https://github.com/UnfoldedInc/deck.gl-native/blob/master/cpp/modules/deck.gl/core/src/lib/deck.cc


struct Deck {
    // Width of the viewport, in pixels.
    width: u32,
    // Height of the viewport, in pixels.
    height: u32,

    device_pixel_ratio: f32,

    layers: Vec<Layer>,
    views: Vec<View>,

    // Analogous to viewState/initialViewState from DeckGL
    camera_view: [f32; 16],

    // Internal fields
    view_manager: ViewManager,
    context: LayerContext,
    layer_manager: LayerManager,

    viewport_uniforms_buffer: wgpu::Buffer,

}

// Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/webgpu/src/webgpu-helpers.cc#L43
fn create_buffer(device: &wgpu::Device, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage,
        mapped_at_creation: false,
    })
}

// Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/deck.gl/core/src/shaderlib/project/viewport-uniforms.cc#L142
fn get_uniforms_from_viewport() {
    // TODO
}

impl Deck {
    pub fn new(device: wgpu::Device) -> Self {
        // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/deck.gl/core/src/lib/deck.cc#L99
        let viewport_uniforms_size = 0; // TODO
        let viewport_uniforms_buffer = create_buffer(&device, viewport_uniforms_size, wgpu::BufferUsages::UNIFORM);
        Deck {
            // TODO: fix
            width: 100,
            height: 100,
            device_pixel_ratio: 1.0,
            layers: Vec::new(),
            views: Vec::new(),
            camera_view: [0.0; 16],
            view_manager: ViewManager::new(),
            context: LayerContext::new(),
            layer_manager: LayerManager::new(),
            viewport_uniforms_buffer,
        }
    }

    // Draws the current Deck state into the given textureView.
    // TODO: need to return a bailed_early indicator?
    // TODO: need to return a Result<(), Error>? what type of error?
    pub fn draw(&self, texture_view: &wgpu::TextureView) {
        // TODO
    }

    // Check if a redraw is needed.
    // Returns an optional string summarizing the redraw reason.
    pub fn needs_redraw(&self) {
        // TODO
    }

    fn draw_layers(&self, pass: wgpu::RenderPass) {
        // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/deck.gl/core/src/lib/deck.cc#L195

        // TODO: do we want/need to support multiple viewports (see this->viewManager->getViewports() logic in cpp code)

        let layers = self.layer_manager.get_layers();
        for layer in layers {
            let model_matrix = layer.get_model_matrix();
            let layer_models = layer.get_models();


            // TODO
            layer.draw(&self.context, pass, uniforms);
        }

    }


}
