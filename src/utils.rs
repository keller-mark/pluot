pub struct RenderContext<'a> {
    pub store_name: String,
    pub device: &'a vello::wgpu::Device,
    pub texture_desc: &'a vello::wgpu::TextureDescriptor<'a>,
    pub view: &'a vello::wgpu::TextureView,
    pub queue: &'a vello::wgpu::Queue,
    pub width: u32,
    pub height: u32,
}