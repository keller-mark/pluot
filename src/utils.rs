use std::collections::HashMap;
use std::sync::MutexGuard;

pub struct RenderContext<'a> {
    pub device: &'a wgpu::Device,
    pub texture_desc: &'a wgpu::TextureDescriptor<'a>,
    pub view: &'a wgpu::TextureView,
    pub queue: &'a wgpu::Queue,
    pub data_map: MutexGuard<'a, HashMap<String, Vec<i32>>>,
    pub width: u32,
    pub height: u32,
}