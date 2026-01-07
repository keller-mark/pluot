
use crate::deckish::layer::{DrawToCanvas, PreparedLayer, ViewParams};
use crate::deckish::model::Model;

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

impl PreparedLayer for ScatterplotLayer {
    async fn prepare(&self) {
        // Do things that are drawing-backend-agnostic here (e.g., loading point data).
        // This will be called prior to drawing, for both SVG and canvas backends.


    }

}

impl DrawToCanvas for ScatterplotLayer {
    async fn draw(&self, device: wgpu::Device, queue: wgpu::Queue, encoder: &wgpu::CommandEncoder) {
        // Implementation for drawing scatterplot to canvas
    }
}
