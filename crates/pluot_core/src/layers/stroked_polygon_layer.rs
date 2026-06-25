// StrokedPolygonLayer — GPU render of the stroked outline of a polygon.
// Delegates GPU drawing to StrokedCurveLayer (same round-joins shader, same
// pre-flattened polyline format). The parent PolygonLayer closes each ring
// by appending the first point so the last segment rejoins the start.

use crate::render_traits::{DrawToRasterGpu, PreparedLayer};
use crate::render_types::{GpuContext, PrepareResult};
use crate::wgpu;

use super::stroked_curve_layer::StrokedCurveLayer;

pub(crate) struct StrokedPolygonLayer(pub StrokedCurveLayer);

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for StrokedPolygonLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        self.0.prepare(gpu_context).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for StrokedPolygonLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        self.0.draw(gpu_context, pass).await
    }
}
