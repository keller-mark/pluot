//! Shared machinery for turning an [`EmphasisCriteria`] into what a layer
//! needs to filter or select its elements, on either the GPU or the CPU. See
//! `.claude/skills/pluot-filter-select-highlight` for the general filtering,
//! selection, and highlighting semantics.

use crate::render_traits::EmphasisCriteria;
use crate::shader_modules::{is_included as is_included_wgsl, ShaderBuilder};
use crate::wgpu;

/// A texture bound for a filtering/selection criteria, paired with the sample
/// type its bind-group layout entry must declare.
pub struct PreparedEmphasisTexture {
    pub view: wgpu::TextureView,
    pub sample_type: wgpu::TextureSampleType,
}

/// Everything a layer needs to test an [`EmphasisCriteria`] on the GPU.
///
/// The layer binds [`texture`](Self::texture) at the `first_binding` passed to
/// [`prepare_emphasis_criteria`] when present, and injects
/// [`wgsl`](Self::wgsl) into its shader (along with
/// [`crate::shader_modules::common::FLAT_TEXEL_COORD`]), which defines the
/// membership-test function named by the `fn_name` argument.
pub struct PreparedEmphasisCriteria {
    /// Per-element codes/values texture, present only for the categorical
    /// (non-empty) and quantitative variants.
    pub texture: Option<PreparedEmphasisTexture>,
    /// Assembled WGSL: the value texture binding (when present) plus the
    /// `fn_name(instance_index: u32) -> bool` getter.
    pub wgsl: String,
}

/// Prepare the GPU resources and WGSL for one [`EmphasisCriteria`] — either
/// the `filtering_criteria` or `selection_criteria` of a layer. `None` means
/// every item is included, per
/// `.claude/skills/pluot-filter-select-highlight`.
///
/// `fn_name` is the WGSL function name to define (e.g. `is_filtered_in` /
/// `is_selected_in`); `var_name` is the WGSL variable-name stem for the value
/// texture and must be unique within the shader so that filtering and
/// selection criteria can coexist without colliding (e.g. `filter_data` /
/// `select_data`). The value texture (categorical/quantitative only) is bound
/// at `first_binding`.
pub fn prepare_emphasis_criteria(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    criteria: Option<&EmphasisCriteria>,
    fn_name: &str,
    var_name: &str,
    first_binding: u32,
) -> PreparedEmphasisCriteria {
    match criteria {
        None => PreparedEmphasisCriteria {
            texture: None,
            wgsl: ShaderBuilder::new(is_included_wgsl::NONE)
                .define("criteria_fn_name", fn_name)
                .build(),
        },
        Some(EmphasisCriteria::Categorical(params)) if params.included_codes.is_empty() => {
            PreparedEmphasisCriteria {
                texture: None,
                wgsl: ShaderBuilder::new(is_included_wgsl::EMPTY)
                    .define("criteria_fn_name", fn_name)
                    .build(),
            }
        }
        Some(EmphasisCriteria::Categorical(params)) => {
            let (view, dtype) =
                params.codes.create_data_texture(device, queue, &format!("{fn_name} codes Texture"));
            let included_codes = params
                .included_codes
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let wgsl = ShaderBuilder::new(is_included_wgsl::CATEGORICAL)
                .define("criteria_fn_name", fn_name)
                .define("criteria_data_var", var_name)
                .define_bidx("criteria_data", first_binding)
                .inject_texture_sample_type("criteria_data", dtype)
                .define_u32("criteria_included_len", params.included_codes.len() as u32)
                .define("criteria_included_codes", &included_codes)
                .build();
            PreparedEmphasisCriteria {
                texture: Some(PreparedEmphasisTexture { view, sample_type: dtype.binding_sample_type() }),
                wgsl,
            }
        }
        Some(EmphasisCriteria::Quantitative(params)) => {
            let (view, dtype) =
                params.values.create_data_texture(device, queue, &format!("{fn_name} values Texture"));
            let wgsl = ShaderBuilder::new(is_included_wgsl::QUANTITATIVE)
                .define("criteria_fn_name", fn_name)
                .define("criteria_data_var", var_name)
                .define_bidx("criteria_data", first_binding)
                .inject_texture_sample_type("criteria_data", dtype)
                .define("criteria_min_value", &wgsl_float(params.min.unwrap_or(f32::MIN)))
                .define("criteria_max_value", &wgsl_float(params.max.unwrap_or(f32::MAX)))
                .build();
            PreparedEmphasisCriteria {
                texture: Some(PreparedEmphasisTexture { view, sample_type: dtype.binding_sample_type() }),
                wgsl,
            }
        }
    }
}

/// Format an `f32` as a WGSL floating-point literal, e.g. `1e30` or
/// `-3.4028235e38`. Always includes an exponent so the omitted-bound sentinels
/// (`f32::MIN`/`f32::MAX`) stay compact.
fn wgsl_float(value: f32) -> String {
    format!("{value:e}")
}

/// Resolve whether item `index` meets `criteria` on the CPU — shared by the
/// filtering and selection criteria (called with `filtering_criteria` /
/// `selection_criteria` respectively), and by SVG rendering. `None` means
/// every item is included, per
/// `.claude/skills/pluot-filter-select-highlight`.
pub fn cpu_is_included(criteria: Option<&EmphasisCriteria>, index: usize) -> bool {
    match criteria {
        None => true,
        Some(EmphasisCriteria::Categorical(params)) => {
            let code = params.codes.get_f32(index) as i64;
            params.included_codes.contains(&code)
        }
        Some(EmphasisCriteria::Quantitative(params)) => {
            let value = params.values.get_f32(index);
            params.min.map_or(true, |min| value >= min) && params.max.map_or(true, |max| value <= max)
        }
    }
}
