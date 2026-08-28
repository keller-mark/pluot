//! Shared machinery for turning a list of [`EmphasisCriteria`] into what a
//! layer needs to filter or select its elements, on either the GPU or the
//! CPU.

use glam::Vec4;

use crate::render_traits::EmphasisCriteria;
use crate::shader_modules::{is_included as is_included_wgsl, ShaderBuilder};
use crate::wgpu;

/// Fill/stroke color used for filter-included, but selection-excluded
/// ("background") items when a layer's `background_fill_color` /
/// `background_stroke_color` param is `None`. Kept out of each param's
/// `Default` impl (which is `None`) so the default does not leak into
/// serialized JSON; resolved here instead, at render time.
pub const DEFAULT_BACKGROUND_COLOR: (u8, u8, u8) = (200, 200, 200);

/// Resolve a `background_fill_color`/`background_stroke_color` param to the
/// rgba `Vec4` a layer's GPU uniforms expect, defaulting to
/// [`DEFAULT_BACKGROUND_COLOR`] when `None`.
pub fn background_color_vec4(color: Option<(u8, u8, u8)>) -> Vec4 {
    let (r, g, b) = color.unwrap_or(DEFAULT_BACKGROUND_COLOR);
    Vec4::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
}

/// Resolve a scalar "background" override (fill/stroke opacity, point radius,
/// stroke width) for a single filter-included item on the CPU. Unlike
/// [`DEFAULT_BACKGROUND_COLOR`], there is no universal default value for these
/// (their units depend on layer-level config), so `background_value: None` is
/// a no-op — the item's normal `foreground_value` is used — rather than
/// falling back to a magic constant. Only applies when the item is
/// filter-included but selection-excluded (`!is_selected`) and
/// `enable_background` is set.
pub fn resolve_background_scalar(
    is_selected: bool,
    enable_background: bool,
    background_value: Option<f32>,
    foreground_value: f32,
) -> f32 {
    if !is_selected && enable_background {
        if let Some(value) = background_value {
            return value;
        }
    }
    foreground_value
}

/// A texture bound for a filtering/selection criteria, paired with the sample
/// type its bind-group layout entry must declare.
pub struct PreparedEmphasisTexture {
    pub view: wgpu::TextureView,
    pub sample_type: wgpu::TextureSampleType,
}

/// Everything a layer needs to test a list of [`EmphasisCriteria`] on the GPU.
///
/// The layer binds [`textures`](Self::textures) consecutively starting at the
/// `first_binding` passed to [`prepare_emphasis_criteria`], and injects
/// [`wgsl`](Self::wgsl) into its shader (along with
/// [`crate::shader_modules::common::FLAT_TEXEL_COORD`]), which defines the
/// membership-test function named by the `fn_name` argument — the AND of every
/// criteria in the list.
pub struct PreparedEmphasisCriteria {
    /// Per-element codes/values texture(s), in binding order. One per
    /// criteria that carries per-element data (i.e. every criteria except an
    /// empty-`included_codes` categorical, which needs no texture).
    pub textures: Vec<PreparedEmphasisTexture>,
    /// Assembled WGSL: the value texture bindings plus the
    /// `fn_name(instance_index: u32) -> bool` getter, which ANDs together the
    /// per-criteria predicates. An empty `criteria` list means every item is
    /// included, so `fn_name` always returns `true`.
    pub wgsl: String,
}

/// Prepare the GPU resources and WGSL for a list of [`EmphasisCriteria`] —
/// either the `filtering_criteria` or `selection_criteria` of a layer, AND-ed
/// together. An empty list means every item is included.
///
/// `fn_name` is the WGSL function name to define (e.g. `is_filtered_in` /
/// `is_selected_in`); `var_name` is the WGSL variable-name stem for the value
/// textures and must be unique within the shader so that filtering and
/// selection criteria can coexist without colliding (e.g. `filter_data` /
/// `select_data`). Value textures (one per categorical/quantitative criteria)
/// are bound consecutively starting at `first_binding`.
pub fn prepare_emphasis_criteria(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    criteria: &[EmphasisCriteria],
    // TODO: use enums to more strongly type these strings
    fn_name: &str,
    var_name: &str,
    first_binding: u32,
) -> PreparedEmphasisCriteria {
    let mut textures: Vec<PreparedEmphasisTexture> = Vec::new();
    let mut wgsl_parts: Vec<String> = Vec::new();
    let mut term_fn_names: Vec<String> = Vec::new();

    for (i, criterion) in criteria.iter().enumerate() {
        let term_fn_name = format!("{fn_name}_{i}");
        let term_var_name = format!("{var_name}_{i}");
        let binding = first_binding + textures.len() as u32;

        match criterion {
            EmphasisCriteria::Categorical(params) if params.included_codes.is_empty() => {
                wgsl_parts.push(
                    ShaderBuilder::new(is_included_wgsl::EMPTY)
                        .define("criteria_fn_name", &term_fn_name)
                        .build(),
                );
            }
            EmphasisCriteria::Categorical(params) => {
                let (view, dtype) = params.codes.create_data_texture(
                    device, queue, &format!("{term_fn_name} codes Texture"),
                );
                let included_codes = params
                    .included_codes
                    .iter()
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                wgsl_parts.push(
                    ShaderBuilder::new(is_included_wgsl::CATEGORICAL)
                        .define("criteria_fn_name", &term_fn_name)
                        .define("criteria_data_var", &term_var_name)
                        .define_bidx("criteria_data", binding)
                        .inject_texture_sample_type("criteria_data", dtype)
                        .define_u32("criteria_included_len", params.included_codes.len() as u32)
                        .define("criteria_included_codes", &included_codes)
                        .build(),
                );
                textures.push(PreparedEmphasisTexture { view, sample_type: dtype.binding_sample_type() });
            }
            EmphasisCriteria::Quantitative(params) => {
                let (view, dtype) = params.values.create_data_texture(
                    device, queue, &format!("{term_fn_name} values Texture"),
                );
                wgsl_parts.push(
                    ShaderBuilder::new(is_included_wgsl::QUANTITATIVE)
                        .define("criteria_fn_name", &term_fn_name)
                        .define("criteria_data_var", &term_var_name)
                        .define_bidx("criteria_data", binding)
                        .inject_texture_sample_type("criteria_data", dtype)
                        .define("criteria_min_value", &wgsl_float(params.min.unwrap_or(f32::MIN)))
                        .define("criteria_max_value", &wgsl_float(params.max.unwrap_or(f32::MAX)))
                        .build(),
                );
                textures.push(PreparedEmphasisTexture { view, sample_type: dtype.binding_sample_type() });
            }
        }
        term_fn_names.push(term_fn_name);
    }

    // Wrapper function ANDing every per-criteria predicate together. An empty
    // `criteria` list vacuously ANDs to `true` (every item is included).
    let and_expr = if term_fn_names.is_empty() {
        "true".to_string()
    } else {
        term_fn_names.iter().map(|n| format!("{n}(instance_index)")).collect::<Vec<_>>().join(" && ")
    };
    wgsl_parts.push(format!(
        "fn {fn_name}(instance_index: u32) -> bool {{\n    return {and_expr};\n}}\n"
    ));

    PreparedEmphasisCriteria { textures, wgsl: wgsl_parts.join("\n") }
}

/// Format an `f32` as a WGSL floating-point literal, e.g. `1e30` or
/// `-3.4028235e38`. Always includes an exponent so the omitted-bound sentinels
/// (`f32::MIN`/`f32::MAX`) stay compact.
fn wgsl_float(value: f32) -> String {
    format!("{value:e}")
}

/// Resolve whether item `index` meets every criteria in `criteria` (AND-ed
/// together) on the CPU — shared by the filtering and selection criteria
/// (called with `filtering_criteria` / `selection_criteria` respectively),
/// and by SVG rendering. An empty list means every item is included.
pub fn cpu_is_included(criteria: &[EmphasisCriteria], index: usize) -> bool {
    criteria.iter().all(|criterion| match criterion {
        EmphasisCriteria::Categorical(params) => {
            let code = params.codes.get_f32(index) as i64;
            params.included_codes.contains(&code)
        }
        EmphasisCriteria::Quantitative(params) => {
            let value = params.values.get_f32(index);
            params.min.map_or(true, |min| value >= min) && params.max.map_or(true, |max| value <= max)
        }
    })
}
