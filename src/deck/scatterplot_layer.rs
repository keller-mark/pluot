use crate::wgpu;
use crate::deck::model::{Model, GetModel};

// Prototyping an API for layered plotting.

struct ChangeFlags {
    data_changed: Option<String>,
    props_changed: Option<String>,
    viewport_changed: Option<String>,
    extensions_changed: Option<String>,

    // Derived changeflags
    props_or_data_changed: Option<String>,
    something_changed: Option<String>,
}

struct Layer {

}



pub trait Drawable {
    // Return an array of models used by this layer, can be overriden by layer.
    fn get_models(&self) -> Vec<Model>;
    // Called once to set up the initial state: App can create WebGPU resources.
    fn initialize_state(&self, device: &wgpu::Device);

    //  Check if update cycle should run. Default returns changeFlags.propsOrDataChanged.
    fn should_update_state(&self) -> bool;
    //  Default implementation: all attributes will be invalidated and updated when data changes.
    fn update_state(&self, device: &wgpu::Device);
    // Called once when layer is no longer matched and state will be discarded. App can destroy WebGPU resources here.
    fn finalize_state(&self, device: &wgpu::Device);

    // If state has a model, draw it with supplied uniforms
    fn draw(&self, device: &wgpu::Device, pass: &mut wgpu::RenderPass);

    // Default implementation of attribute invalidation, can be redefined.
    fn invalidate_attribute(&self, name: &str, diff_reason: &str);

    // Calls attribute manager to update any WebGPU attributes.
    fn update_attributes(&self, device: &wgpu::Device);

    // LAYER MANAGER API - Should only be called by the LayerManager class
    // Called by layer manager when a new layer is found.
    fn lm_initialize(&self, device: &wgpu::Device);

    // If this layer is new (not matched with an existing layer) oldProps will be empty object.
    fn lm_update(&self, device: &wgpu::Device);

    // Called by manager when layer is about to be disposed.
    // Not guaranteed to be called on application shutdown.
    fn lm_finalize(&self, device: &wgpu::Device);

    // Helper
    fn lm_update_state();

}

// Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/deck.gl/layers/src/scatterplot-layer/scatterplot-layer.h#L35
struct ScatterplotLayer {
    x_data: Option<Vec<f32>>,
    y_data: Option<Vec<f32>>,
    labels_data: Option<Vec<i32>>,
}

struct BitmapLayer {

}

struct CompositeLayer {

}

struct TileLayer {

}

impl GetModel for ScatterplotLayer {
    // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/deck.gl/layers/src/scatterplot-layer/scatterplot-layer.cc#L205
    fn get_model(&self, device: &wgpu::Device) -> Model {
        Model::new(device)
    }
}

pub async fn render_layered_plot(
    context: &mut RenderContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
) -> RenderResult {
    // Get x and y data from the Zarr store.
    let store = context.store;
    let height = context.params.height as f64;
    let width = context.params.width as f64;

    let margin_top = context.params.margin_top.unwrap_or(0.0) as f64;
    let margin_right = context.params.margin_right.unwrap_or(0.0) as f64;
    let margin_bottom = context.params.margin_bottom.unwrap_or(0.0) as f64;
    let margin_left = context.params.margin_left.unwrap_or(0.0) as f64;


    let scatterplot_layer = ScatterplotLayer::new(device);
}
