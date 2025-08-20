use crate::zarr::{AsyncZarritaStore};
use std::sync::Arc;

// TODO: define RenderParams here (rather than lib.rs).
// Then, pass RenderParams via RenderContext.

pub struct RenderContext<'a> {
    pub store: &'a Arc<AsyncZarritaStore>,
    pub device: &'a wgpu::Device,
    pub texture_desc: &'a wgpu::TextureDescriptor<'a>,
    pub view: &'a wgpu::TextureView,
    pub queue: &'a wgpu::Queue,
    pub width: u32,
    pub height: u32,
}