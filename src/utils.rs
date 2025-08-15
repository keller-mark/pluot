use vello::wgpu;

pub struct RenderContext<'a> {
    pub store_name: String,
    pub device: &'a wgpu::Device,
    pub texture_desc: &'a wgpu::TextureDescriptor<'a>,
    pub view: &'a wgpu::TextureView,
    pub queue: &'a wgpu::Queue,
    pub width: u32,
    pub height: u32,
}