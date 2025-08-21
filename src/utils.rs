use crate::zarr::{AsyncZarritaStore};
use std::sync::Arc;

// TODO: define RenderParams here (rather than lib.rs).
// Then, pass RenderParams via RenderContext.
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct RenderParams {
    pub width: u32,
    pub height: u32,
    pub zoom: Option<f32>,
    #[serde(rename = "targetX")]
    pub target_x: Option<f32>,
    #[serde(rename = "targetY")]
    pub target_y: Option<f32>,
    // We need a plot ID for cacheing of certain intermediate expensive computations per plot.
    // Note that solely data-dependent computations should be cached via the (store_name, key) tuple.
    #[serde(rename = "plotId")]
    pub plot_id: String,
    #[serde(rename = "plotType")]
    pub plot_type: String,
    #[serde(rename = "storeName")]
    pub store_name: String,
}
pub struct RenderContext<'a> {
    pub store: &'a Arc<AsyncZarritaStore>,
    pub device: &'a wgpu::Device,
    pub texture_desc: &'a wgpu::TextureDescriptor<'a>,
    pub view: &'a wgpu::TextureView,
    pub queue: &'a wgpu::Queue,
    pub params: &'a RenderParams,
}