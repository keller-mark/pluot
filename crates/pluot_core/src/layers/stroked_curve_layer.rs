// draw_stroked_curve_gpu — GPU render of a stroked polyline with round joins/caps.
// Used by CurveLayer.

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use kurbo::{CubicBez, ParamCurve};

use crate::render_types::GpuContext;
use crate::wgpu;

// Must match VERTS_PER_INSTANCE_F = 38 in stroked_curve_layer.wgsl.
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
}

/// Flatten a sub-path into a polyline of model-space (x, y) points.
/// Consecutive duplicates are removed to avoid zero-length shader segments.
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_stroked_curve_gpu(
    gpu_context: &GpuContext<'_>,
    pass: &mut wgpu::RenderPass,
    polylines: &[Vec<(f32, f32)>],
    stroke_color: Vec4,
    stroke_width: f32,
    layer_w: f32,
    layer_h: f32,
    camera_view: &[f32; 16],
    data_unit_mode_x: u32,
    data_unit_mode_y: u32,
    aspect_ratio_mode: u32,
    aspect_ratio_alignment_mode: u32,
    model_matrix: Mat4,
    margin_left: f64,
    margin_top: f64,
    margin_right: f64,
    margin_bottom: f64,
) {
    let has_work = polylines.iter().any(|p| p.len() >= 2);
    if !has_work {
        return;
    }

    let GpuContext { device, queue } = gpu_context;

    pass.set_viewport(margin_left as f32, margin_top as f32, layer_w, layer_h, 0.0, 1.0);
    pass.set_scissor_rect(margin_left as u32, margin_top as u32, layer_w as u32, layer_h as u32);

    // Build flat buffers for all polylines in one pass.
    // pts_data: interleaved x,y for every point across all polylines.
    // seg_data: [poly_start, poly_end, local_b] per segment (12 bytes each, 3×u32).
    let mut pts_data: Vec<f32> = Vec::new();
    let mut seg_data: Vec<u32> = Vec::new();
    let mut total_segments: u32 = 0;

    for polyline in polylines {
        if polyline.len() < 2 {
            continue;
        }
        let poly_start = (pts_data.len() / 2) as u32;
        for (x, y) in polyline {
            pts_data.push(*x);
            pts_data.push(*y);
        }
        let poly_end = poly_start + polyline.len() as u32 - 1;
        for local_b in 0..(polyline.len() as u32 - 1) {
            seg_data.push(poly_start);
            seg_data.push(poly_end);
            seg_data.push(local_b);
            total_segments += 1;
        }
    }

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
            wgpu::BindGroupLayoutEntry {
                binding: 2,
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

    let uniform_struct = StrokedCurveLayerUniforms {
        layer_size: Vec2::new(layer_w, layer_h),
        camera_view: Mat4::from_cols_array(camera_view),
        data_unit_mode_x,
        data_unit_mode_y,
        stroke_width,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        model_matrix,
        stroke_color,
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

    let pts_bytes: &[u8] = bytemuck::cast_slice(&pts_data);
    let pts_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("StrokedCurve Points"),
        size: pts_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&pts_buf, 0, pts_bytes);

    let seg_bytes: &[u8] = bytemuck::cast_slice(&seg_data);
    let seg_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("StrokedCurve Segments"),
        size: seg_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&seg_buf, 0, seg_bytes);

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
            wgpu::BindGroupEntry {
                binding: 2,
                resource: seg_buf.as_entire_binding(),
            },
        ],
    });

    pass.set_pipeline(&pipeline);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.draw(0..VERTS_PER_INSTANCE, 0..total_segments);
}
