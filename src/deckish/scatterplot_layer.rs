
use crate::deckish::layer::{DrawToCanvas, PreparedLayer, ViewParams};
use crate::deckish::model::{Model, ModelOptions};
use crate::wgpu;

struct ScatterplotLayer {
    view_params: ViewParams,
    // Other fields specific to scatterplot layer
}

impl ScatterplotLayer {
    fn new(view_params: ViewParams) -> Self {
        Self {
            view_params,
        }
    }

    async fn get_model(&self, device: wgpu::Device, queue: wgpu::Queue) -> Model {


    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for ScatterplotLayer {
    async fn prepare(&self) {
        // Do things that are drawing-backend-agnostic here (e.g., loading point data).
        // This will be called prior to drawing, for both SVG and canvas backends.


    }

}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToCanvas for ScatterplotLayer {
    async fn draw(&self, device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass<'_>) {
        // Implementation for drawing scatterplot to canvas
        let mut model = self.get_model(device, queue).await;
        model.draw(pass);
    }
}
