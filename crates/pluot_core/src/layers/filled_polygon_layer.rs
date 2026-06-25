// FilledPolygonLayer — GPU render of the triangulated fill interior of a polygon.
// Delegates GPU drawing to FilledCurveLayer (same shader, same flat triangle vertex
// buffer format). The parent PolygonLayer supplies pre-triangulated vertices via earcut.

use crate::render_traits::{DrawToRasterGpu, PreparedLayer};
use crate::render_types::{GpuContext, PrepareResult};
use crate::wgpu;

use super::filled_curve_layer::FilledCurveLayer;

pub(crate) struct FilledPolygonLayer(pub FilledCurveLayer);

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for FilledPolygonLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        self.0.prepare(gpu_context).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for FilledPolygonLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        self.0.draw(gpu_context, pass).await
    }
}
