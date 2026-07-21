//! Shared machinery for turning a [`SizeMode`] or [`OpacityMode`] into what a
//! layer needs to size / fade its elements, on either the GPU or the CPU.
//!
//! This is the scalar (single-value-per-element) counterpart of
//! [`crate::color_mode`]. Both modes are structurally identical — a single
//! static value shared by every element, or one per-element value uploaded as a
//! texture — so a single [`prepare_scalar_mode`] core handles both, with
//! [`prepare_size_mode`] / [`prepare_opacity_mode`] wrapping it for the two enum
//! types. The per-mode WGSL lives in `wgsl_functions/get_point_radius/` and
//! `wgsl_functions/get_point_opacity/`.
//!
//! [`cpu_point_radius`] / [`cpu_point_opacity`] are the CPU-side equivalents,
//! used by the SVG / software render paths.

use crate::color_mode::PreparedColorTexture;
use crate::numeric_data::NumericData;
use crate::render_traits::{OpacityMode, SizeMode};
use crate::shader_modules::{
    fill_opacity as fill_opacity_wgsl, opacity as opacity_wgsl, size as size_wgsl,
    stroke_opacity as stroke_opacity_wgsl, stroke_width as stroke_width_wgsl, ShaderBuilder,
};
use crate::wgpu;

/// Default point radius used when no [`SizeMode`] is configured.
pub const DEFAULT_POINT_RADIUS: f32 = 1.0;

/// Default point opacity used when no [`OpacityMode`] is configured.
pub const DEFAULT_POINT_OPACITY: f32 = 1.0;

/// Default stroke width used when no [`SizeMode`] is configured (shared by the
/// line, polygon, and curve layers).
pub const DEFAULT_STROKE_WIDTH: f32 = 1.0;

/// Default stroke opacity used when no [`OpacityMode`] is configured.
pub const DEFAULT_STROKE_OPACITY: f32 = 1.0;

/// Default fill opacity used when no [`OpacityMode`] is configured.
pub const DEFAULT_FILL_OPACITY: f32 = 1.0;

/// Everything a layer needs to render a scalar mode ([`SizeMode`] /
/// [`OpacityMode`]) on the GPU.
///
/// The layer writes [`static_value`](Self::static_value) into its uniform buffer
/// (read by the uniform-mode WGSL); binds [`texture`](Self::texture) at the
/// `first_binding` passed to the `prepare_*` function when present; and injects
/// [`wgsl`](Self::wgsl) into its shader's module placeholder (along with
/// [`crate::shader_modules::common::FLAT_TEXEL_COORD`], required by the instanced
/// variant).
pub struct PreparedScalarMode {
    /// Static scalar value used by the uniform mode (also written to the uniform
    /// buffer as a harmless fallback in the instanced case).
    pub static_value: f32,
    /// Per-element value texture, present only for instanced modes.
    pub texture: Option<PreparedColorTexture>,
    /// Assembled WGSL: the value texture binding (instanced only) plus the
    /// `get_point_radius` / `get_point_opacity` getter function.
    pub wgsl: String,
}

/// Prepare the GPU resources and WGSL for the point [`SizeMode`]. The value
/// texture (instanced mode only) is bound at `first_binding`. `None` behaves
/// like `Some(SizeMode::UniformSize(DEFAULT_POINT_RADIUS))`.
pub fn prepare_size_mode(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: Option<&SizeMode>,
    first_binding: u32,
) -> PreparedScalarMode {
    let (static_value, instanced) = match size {
        None => (DEFAULT_POINT_RADIUS, None),
        Some(SizeMode::UniformSize(v)) => (*v, None),
        Some(SizeMode::InstancedSize(params)) => (DEFAULT_POINT_RADIUS, Some(&params.values)),
    };
    prepare_scalar_mode(
        device,
        queue,
        "point_radius_values",
        size_wgsl::UNIFORM,
        size_wgsl::INSTANCED,
        static_value,
        instanced,
        first_binding,
        "point_radius values Texture",
    )
}

/// Prepare the GPU resources and WGSL for the point [`OpacityMode`]. The value
/// texture (instanced mode only) is bound at `first_binding`. `None` behaves
/// like `Some(OpacityMode::UniformOpacity(DEFAULT_POINT_OPACITY))`.
pub fn prepare_opacity_mode(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    opacity: Option<&OpacityMode>,
    first_binding: u32,
) -> PreparedScalarMode {
    let (static_value, instanced) = match opacity {
        None => (DEFAULT_POINT_OPACITY, None),
        Some(OpacityMode::UniformOpacity(v)) => (*v, None),
        Some(OpacityMode::InstancedOpacity(params)) => (DEFAULT_POINT_OPACITY, Some(&params.values)),
    };
    prepare_scalar_mode(
        device,
        queue,
        "point_opacity_values",
        opacity_wgsl::UNIFORM,
        opacity_wgsl::INSTANCED,
        static_value,
        instanced,
        first_binding,
        "point_opacity values Texture",
    )
}

/// Prepare the GPU resources and WGSL for the stroke [`SizeMode`] (stroke
/// width). The value texture (instanced mode only) holds one value per element
/// and is bound at `first_binding`. `None` behaves like
/// `Some(SizeMode::UniformSize(DEFAULT_STROKE_WIDTH))`. Shared by the line,
/// polygon, and curve layers.
pub fn prepare_stroke_width_mode(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: Option<&SizeMode>,
    first_binding: u32,
) -> PreparedScalarMode {
    let (static_value, instanced) = match size {
        None => (DEFAULT_STROKE_WIDTH, None),
        Some(SizeMode::UniformSize(v)) => (*v, None),
        Some(SizeMode::InstancedSize(params)) => (DEFAULT_STROKE_WIDTH, Some(&params.values)),
    };
    prepare_scalar_mode(
        device,
        queue,
        "stroke_width_values",
        stroke_width_wgsl::UNIFORM,
        stroke_width_wgsl::INSTANCED,
        static_value,
        instanced,
        first_binding,
        "stroke_width values Texture",
    )
}

/// Prepare the GPU resources and WGSL for the stroke [`OpacityMode`]. The value
/// texture (instanced mode only) holds one value per element and is bound at
/// `first_binding`. `None` behaves like
/// `Some(OpacityMode::UniformOpacity(DEFAULT_STROKE_OPACITY))`. Shared by the
/// line, polygon, and curve layers.
pub fn prepare_stroke_opacity_mode(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    opacity: Option<&OpacityMode>,
    first_binding: u32,
) -> PreparedScalarMode {
    let (static_value, instanced) = match opacity {
        None => (DEFAULT_STROKE_OPACITY, None),
        Some(OpacityMode::UniformOpacity(v)) => (*v, None),
        Some(OpacityMode::InstancedOpacity(params)) => (DEFAULT_STROKE_OPACITY, Some(&params.values)),
    };
    prepare_scalar_mode(
        device,
        queue,
        "stroke_opacity_values",
        stroke_opacity_wgsl::UNIFORM,
        stroke_opacity_wgsl::INSTANCED,
        static_value,
        instanced,
        first_binding,
        "stroke_opacity values Texture",
    )
}

/// Prepare the GPU resources and WGSL for the polygon fill [`OpacityMode`]. The
/// value texture (instanced mode only) holds one value per polygon and is bound
/// at `first_binding`. `None` behaves like
/// `Some(OpacityMode::UniformOpacity(DEFAULT_FILL_OPACITY))`.
pub fn prepare_fill_opacity_mode(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    opacity: Option<&OpacityMode>,
    first_binding: u32,
) -> PreparedScalarMode {
    let (static_value, instanced) = match opacity {
        None => (DEFAULT_FILL_OPACITY, None),
        Some(OpacityMode::UniformOpacity(v)) => (*v, None),
        Some(OpacityMode::InstancedOpacity(params)) => (DEFAULT_FILL_OPACITY, Some(&params.values)),
    };
    prepare_scalar_mode(
        device,
        queue,
        "fill_opacity_values",
        fill_opacity_wgsl::UNIFORM,
        fill_opacity_wgsl::INSTANCED,
        static_value,
        instanced,
        first_binding,
        "fill_opacity values Texture",
    )
}

/// Shared core for [`prepare_size_mode`] / [`prepare_opacity_mode`]. When
/// `instanced_values` is `None` the WGSL is the (placeholder-free) uniform
/// template; otherwise the values are uploaded as a texture and the instanced
/// template is specialized with the texture's binding index and sampled type
/// (`var_prefix` is the shared WGSL variable-name stem, e.g. `point_radius_values`).
#[allow(clippy::too_many_arguments)]
fn prepare_scalar_mode(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    var_prefix: &str,
    uniform_wgsl: &str,
    instanced_wgsl: &str,
    static_value: f32,
    instanced_values: Option<&NumericData>,
    first_binding: u32,
    texture_label: &str,
) -> PreparedScalarMode {
    match instanced_values {
        None => PreparedScalarMode {
            static_value,
            texture: None,
            wgsl: uniform_wgsl.to_string(),
        },
        Some(values) => {
            let (view, dtype) = values.create_data_texture(device, queue, texture_label);
            let wgsl = ShaderBuilder::new(instanced_wgsl)
                .define_bidx(var_prefix, first_binding)
                .inject_texture_sample_type(var_prefix, dtype)
                .build();
            PreparedScalarMode {
                static_value,
                texture: Some(PreparedColorTexture {
                    view,
                    sample_type: dtype.binding_sample_type(),
                }),
                wgsl,
            }
        }
    }
}

/// Resolve the radius of point `index` on the CPU. `None` resolves to
/// [`DEFAULT_POINT_RADIUS`].
pub fn cpu_point_radius(size: Option<&SizeMode>, index: usize) -> f32 {
    match size {
        None => DEFAULT_POINT_RADIUS,
        Some(SizeMode::UniformSize(v)) => *v,
        Some(SizeMode::InstancedSize(params)) => params.values.get_f32(index),
    }
}

/// Resolve the opacity of point `index` on the CPU. `None` resolves to
/// [`DEFAULT_POINT_OPACITY`].
pub fn cpu_point_opacity(opacity: Option<&OpacityMode>, index: usize) -> f32 {
    match opacity {
        None => DEFAULT_POINT_OPACITY,
        Some(OpacityMode::UniformOpacity(v)) => *v,
        Some(OpacityMode::InstancedOpacity(params)) => params.values.get_f32(index),
    }
}

/// Resolve the stroke width of element `index` on the CPU. `None` resolves to
/// [`DEFAULT_STROKE_WIDTH`].
pub fn cpu_stroke_width(size: Option<&SizeMode>, index: usize) -> f32 {
    match size {
        None => DEFAULT_STROKE_WIDTH,
        Some(SizeMode::UniformSize(v)) => *v,
        Some(SizeMode::InstancedSize(params)) => params.values.get_f32(index),
    }
}

/// Resolve the stroke opacity of element `index` on the CPU. `None` resolves to
/// [`DEFAULT_STROKE_OPACITY`].
pub fn cpu_stroke_opacity(opacity: Option<&OpacityMode>, index: usize) -> f32 {
    match opacity {
        None => DEFAULT_STROKE_OPACITY,
        Some(OpacityMode::UniformOpacity(v)) => *v,
        Some(OpacityMode::InstancedOpacity(params)) => params.values.get_f32(index),
    }
}

/// Resolve the fill opacity of polygon `index` on the CPU. `None` resolves to
/// [`DEFAULT_FILL_OPACITY`].
pub fn cpu_fill_opacity(opacity: Option<&OpacityMode>, index: usize) -> f32 {
    match opacity {
        None => DEFAULT_FILL_OPACITY,
        Some(OpacityMode::UniformOpacity(v)) => *v,
        Some(OpacityMode::InstancedOpacity(params)) => params.values.get_f32(index),
    }
}
