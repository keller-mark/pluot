// StrokedCurveLayer — GPU render of a stroked curve path using round joins and caps.
//
// The CPU side pre-flattens each sub-path (list of cubic Bezier segments) into a
// simple polyline (flat list of model-space vec2 points). The GPU side implements
// the 4-point window approach from webgpu-instanced-lines: each instance draws the
// segment between points[i] and points[i+1], with round-join geometry at both ends.
// This eliminates the transparent notches that appear when plain quads meet at an
// angle for thick stroked curves.
//
// Draw call structure: one draw call per sub-path (because each sub-path has its
// own point count and point buffer; mixing sub-paths would produce spurious joins
// across sub-path boundaries).

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use kurbo::{CubicBez, ParamCurve};

use crate::render_traits::{DrawToRasterGpu, PreparedLayer, ViewParams};
use crate::render_types::{GpuContext, PrepareResult};
use crate::wgpu;

// VERTS_PER_INSTANCE must match the shader constant VERTS_PER_INSTANCE_F = 38.
// With JOIN_RESOLUTION=8: (8*2 + 3) * 2 = 38.
const VERTS_PER_INSTANCE: u32 = 38;

#[derive(ShaderType, Debug)]
struct StrokedCurveLayerUniforms {
    layer_size: Vec2,
    camera_view: Mat4,
    data_unit_mode_x: u32,
    data_unit_mode_y: u32,
    stroke_width: f32,
    aspect_ratio_mode: u32,
    aspect_ratio_alignment_mode: u32,
    model_matrix: Mat4,
    stroke_color: Vec4,
    point_count: u32,
}

pub(crate) struct StrokedCurveLayer {
    pub view_params: ViewParams,
    pub stroke_color: Vec4,
    pub stroke_width: f32,
    /// Pre-flattened model-space points per sub-path.
    pub polylines: Vec<Vec<(f32, f32)>>,
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

/// Flatten a single sub-path (list of cubic Bezier segments) into a polyline.
/// Returns model-space (x, y) points evaluated at t = 0, 1/s, 2/s, …, 1.
/// Consecutive duplicate points are removed to avoid zero-length segments in
/// the shader.
pub(crate) fn flatten_subpath(subpath: &[CubicBez], subdivisions: u32) -> Vec<(f32, f32)> {
    let mut pts: Vec<(f32, f32)> = Vec::new();
    if subpath.is_empty() {
        return pts;
    }
    let push = |pts: &mut Vec<(f32, f32)>, p: (f32, f32)| {
        if let Some(&last) = pts.last() {
            if (last.0 - p.0).abs() < 1e-9 && (last.1 - p.1).abs() < 1e-9 {
                return;
            }
        }
        pts.push(p);
    };
    let p0 = subpath[0].p0;
    push(&mut pts, (p0.x as f32, p0.y as f32));
    for seg in subpath {
        for step in 1..=subdivisions {
            let t = step as f64 / subdivisions as f64;
            let p = seg.eval(t);
            push(&mut pts, (p.x as f32, p.y as f32));
        }
    }
    pts
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for StrokedCurveLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        PrepareResult { bailed_early: false }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for StrokedCurveLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let GpuContext { device, queue } = gpu_context;

        let viewport_w = self.view_params.width as f32;
        let viewport_h = self.view_params.height as f32;
        let layer_w = viewport_w - (self.margin_left + self.margin_right) as f32;
        let layer_h = viewport_h - (self.margin_top + self.margin_bottom) as f32;

        // Apply viewport / scissor once for all sub-path draw calls.
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

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("StrokedCurve BGL"),
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

        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/stroked_curve_layer.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("StrokedCurve PLD"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("StrokedCurve RPD"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_stroke"),
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
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
            multiview_mask: None,
        });

        // One draw call per sub-path: each has its own uniform (point_count differs)
        // and its own point storage buffer.
        for polyline in &self.polylines {
            if polyline.len() < 2 {
                continue;
            }

            let point_count = polyline.len() as u32;
            let instance_count = point_count - 1;

            let uniform_struct = StrokedCurveLayerUniforms {
                layer_size: Vec2::new(layer_w, layer_h),
                camera_view: Mat4::from_cols_array(&self.camera_view),
                data_unit_mode_x: self.data_unit_mode_x,
                data_unit_mode_y: self.data_unit_mode_y,
                stroke_width: self.stroke_width,
                aspect_ratio_mode: self.aspect_ratio_mode,
                aspect_ratio_alignment_mode: self.aspect_ratio_alignment_mode,
                model_matrix: self.model_matrix,
                stroke_color: self.stroke_color,
                point_count,
            };

            let mut ub = UniformBuffer::new(Vec::<u8>::new());
            ub.write(&uniform_struct).unwrap();
            let uniform_bytes = ub.into_inner();

            let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("StrokedCurve Uniform"),
                size: uniform_bytes.len() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&uniform_buf, 0, &uniform_bytes);

            // Pack polyline points into a flat f32 buffer (2 floats per point).
            let mut pts_data: Vec<f32> = Vec::with_capacity(polyline.len() * 2);
            for (x, y) in polyline {
                pts_data.push(*x);
                pts_data.push(*y);
            }
            let pts_bytes: &[u8] = bytemuck::cast_slice(&pts_data);
            let pts_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("StrokedCurve Points"),
                size: pts_bytes.len() as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&pts_buf, 0, pts_bytes);

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("StrokedCurve BG"),
                layout: &bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: pts_buf.as_entire_binding(),
                    },
                ],
            });

            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..VERTS_PER_INSTANCE, 0..instance_count);
        }
    }
}
