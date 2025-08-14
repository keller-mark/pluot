use wgpu::{TextureDescriptor, TextureUsages, TextureFormat, Extent3d};

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

// Struct to hold state required for render functions.
pub struct RenderContext<'a> {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub texture_desc_format: &'a wgpu::TextureFormat,
    pub view: wgpu::TextureView,
    pub global_map: MutexGuard<'a, HashMap<String, Vec<i32>>>,
    pub encoder: &'a wgpu::CommandEncoder,
    pub width: u32,
    pub height: u32,
}
