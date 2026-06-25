// FilledCurveLayer — GPU render of the triangulated fill interior of a curve path.
// Receives a pre-triangulated flat list of model-space vertices (produced by the
// CPU-side ear-clipping / sweep-line pass in CurveLayer) and projects them through
// the standard data/pixel-unit pipeline.

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};

use crate::render_traits::{AspectRatioAlignmentMode, AspectRatioMode, DrawToRasterGpu, PreparedLayer, UnitsMode, ViewParams};
use crate::render_types::{GpuContext, PrepareResult};
use crate::wgpu;

// 38 vertices per instance (round joins, JOIN_RESOLUTION=8): (16+3)*2
pub const VERTS_PER_INSTANCE: u32 = 38;

#[derive(ShaderType, Debug)]
struct FilledCurveLayerUniforms {
    layer_size: Vec2,
    camera_view: Mat4,
    data_unit_mode_x: u32,
    data_unit_mode_y: u32,
    aspect_ratio_mode: u32,
    aspect_ratio_alignment_mode: u32,
    model_matrix: Mat4,
    fill_color: Vec4,
}

pub(crate) struct FilledCurveLayer {
    pub view_params: ViewParams,
    pub fill_color: Vec4,
    pub fill_vertices: Vec<(f32, f32)>,
    pub data_unit_mode_x: u32,
    pub data_unit_mode_y: u32,
    pub aspect_ratio_mode: u32,
    pub aspect_ratio_alignment_mode: u32,
    pub model_matrix: Mat4,
    pub margin_left: f64,
    pub margin_top: f64,
    pub margin_right: f64,
    pub margin_bottom: f64,
    pub camera_view: [f32; 16],
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for FilledCurveLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        PrepareResult { bailed_early: false }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for FilledCurveLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        if self.fill_vertices.is_empty() {
            return;
        }

        let GpuContext { device, queue } = gpu_context;

        let viewport_w = self.view_params.width as f32;
        let viewport_h = self.view_params.height as f32;
        let layer_w = viewport_w - (self.margin_left + self.margin_right) as f32;
        let layer_h = viewport_h - (self.margin_top + self.margin_bottom) as f32;

        let uniform_struct = FilledCurveLayerUniforms {
            layer_size: Vec2::new(layer_w, layer_h),
            camera_view: Mat4::from_cols_array(&self.camera_view),
            data_unit_mode_x: self.data_unit_mode_x,
            data_unit_mode_y: self.data_unit_mode_y,
            aspect_ratio_mode: self.aspect_ratio_mode,
            aspect_ratio_alignment_mode: self.aspect_ratio_alignment_mode,
            model_matrix: self.model_matrix,
            fill_color: self.fill_color,
        };

        let mut buf = UniformBuffer::new(Vec::<u8>::new());
        buf.write(&uniform_struct).unwrap();
        let uniform_bytes = buf.into_inner();

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("FilledCurve Uniform"),
            size: uniform_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("FilledCurve BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/filled_curve_layer.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("FilledCurve PLD"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("FilledCurve RPD"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_fill"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
            multiview_mask: None,
        });

        // Pack fill vertices into a flat f32 storage buffer.
        let mut fill_data: Vec<f32> = Vec::with_capacity(self.fill_vertices.len() * 2);
        for (x, y) in &self.fill_vertices {
            fill_data.push(*x);
            fill_data.push(*y);
        }
        let fill_bytes: &[u8] = bytemuck::cast_slice(&fill_data);
        let fill_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("FilledCurve Vertices"),
            size: fill_bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&fill_buf, 0, fill_bytes);

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("FilledCurve BG"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: fill_buf.as_entire_binding(),
                },
            ],
        });

        pass.set_viewport(
            self.margin_left as f32,
            self.margin_top as f32,
            layer_w,
            layer_h,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(
            self.margin_left as u32,
            self.margin_top as u32,
            layer_w as u32,
            layer_h as u32,
        );

        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..(self.fill_vertices.len() as u32), 0..1);
    }
}
