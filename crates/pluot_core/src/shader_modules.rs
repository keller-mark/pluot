//! A very lightweight WGSL shader-module system.
//!
//! (Ideally we could use a more robust system such as WESL, but this adds ~1MB
//! to the WASM binary size, at least last time I tried it.)
//!
//! Injected shader code lives in `.wgsl` files and shaders are built at runtime
//! This module enables shader composition by substituting `{{placeholder}}` tokens in a
//! template string.
//!
//! The whole system is nothing more than repeated string replacement over
//! `{{...}}` tokens. [`ShaderBuilder::build`] returns the finished WGSL source
//! as a `String`, ready to hand to `device.create_shader_module` via
//! `wgpu::ShaderSource::Wgsl`.
//!
//!
//! ```ignore
//! use pluot_core::shader_modules::{common, ShaderBuilder, WgslScalar};
//!
//! let source = ShaderBuilder::new(include_str!("shaders/bitmap_layer.wgsl"))
//!     .inject_function("scale", common::SCALE)
//!     .inject_function("translate", common::TRANSLATE)
//!     .inject_function("get_aspect_ratio_mat", common::GET_ASPECT_RATIO_MAT)
//!     .inject_dtype("img_data", WgslScalar::F32)
//!     .build();
//!
//! let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
//!     label: Some("bitmap_layer.wgsl"),
//!     source: wgpu::ShaderSource::Wgsl(source.into()),
//! });
//! ```

use std::borrow::Cow;

use crate::wgpu;

/// Reusable WGSL functions, embedded at compile time from `wgsl_functions/`.
///
/// Each constant is a single self-contained WGSL function that would otherwise
/// be duplicated across layer shaders. Inject one into a template with
/// [`ShaderBuilder::inject_function`].
pub mod common {
    /// `fn scale(x, y, z) -> mat4x4<f32>` — builds a scaling matrix.
    pub const SCALE: &str = include_str!("wgsl_functions/scale.wgsl");

    /// `fn translate(x, y, z) -> mat4x4<f32>` — builds a translation matrix.
    pub const TRANSLATE: &str = include_str!("wgsl_functions/translate.wgsl");

    /// `fn get_aspect_ratio_mat(...) -> mat4x4<f32>` — aspect-ratio handling.
    ///
    /// Depends on [`SCALE`] and [`TRANSLATE`] also being injected into the same
    /// module (order does not matter to WGSL, but both must be present).
    pub const GET_ASPECT_RATIO_MAT: &str = include_str!("wgsl_functions/get_aspect_ratio_mat.wgsl");

    /// `fn rotate_z(angle_deg) -> mat4x4<f32>` — builds a rotation matrix about
    /// the Z axis (angle in degrees).
    pub const ROTATE_Z: &str = include_str!("wgsl_functions/rotate_z.wgsl");

    /// `fn flat_texel_coord(idx, width) -> vec2<u32>` — maps a flat element
    /// index to 2D texel coordinates for a single-channel data texture (see
    /// [`crate::numeric_data::NumericData::create_data_texture`]).
    pub const FLAT_TEXEL_COORD: &str = include_str!("wgsl_functions/flat_texel_coord.wgsl");
}

/// Per-[`ColorMode`](crate::render_traits::ColorMode) WGSL snippets, each
/// defining `fn get_fill_color(instance_index: u32) -> vec3<f32>` (plus any
/// texture bindings the mode needs). These are templates: the color-mode value
/// texture bindings, sampled types and colormap function are filled in at
/// runtime by [`crate::color_mode::prepare_color_mode`]. All variants that read
/// a value texture assume [`common::FLAT_TEXEL_COORD`] is also injected.
pub mod color {
    /// Static color shared by every element.
    pub const UNIFORM_RGB: &str = include_str!("wgsl_functions/get_fill_color/uniform_rgb.wgsl");

    /// Per-element RGB from three parallel value textures.
    pub const INSTANCED_RGB: &str = include_str!("wgsl_functions/get_fill_color/instanced_rgb.wgsl");

    /// Per-element RGB from one interleaved value texture.
    pub const INSTANCED_RGB_INTERLEAVED: &str =
        include_str!("wgsl_functions/get_fill_color/instanced_rgb_interleaved.wgsl");

    /// Per-element integer labels indexed against a palette texture.
    pub const CATEGORICAL: &str = include_str!("wgsl_functions/get_fill_color/categorical.wgsl");

    /// Per-element scalar values mapped through a continuous colormap.
    pub const QUANTITATIVE: &str = include_str!("wgsl_functions/get_fill_color/quantitative.wgsl");
}

/// Stroke-color counterpart of [`color`], for layers that stroke rather than
/// fill (e.g. `LineLayer`). Each snippet defines `fn get_stroke_color(...)` and
/// reads the `stroke_color*` uniforms. Otherwise identical to [`color`];
/// assembled at runtime by [`crate::color_mode::prepare_stroke_color`].
pub mod stroke_color {
    /// Static color shared by every element.
    pub const UNIFORM_RGB: &str = include_str!("wgsl_functions/get_stroke_color/uniform_rgb.wgsl");

    /// Per-element RGB from three parallel value textures.
    pub const INSTANCED_RGB: &str = include_str!("wgsl_functions/get_stroke_color/instanced_rgb.wgsl");

    /// Per-element RGB from one interleaved value texture.
    pub const INSTANCED_RGB_INTERLEAVED: &str =
        include_str!("wgsl_functions/get_stroke_color/instanced_rgb_interleaved.wgsl");

    /// Per-element integer labels indexed against a palette texture.
    pub const CATEGORICAL: &str = include_str!("wgsl_functions/get_stroke_color/categorical.wgsl");

    /// Per-element scalar values mapped through a continuous colormap.
    pub const QUANTITATIVE: &str = include_str!("wgsl_functions/get_stroke_color/quantitative.wgsl");
}

/// Per-[`ColorMode`](crate::render_traits::ColorMode) WGSL snippets for
/// `BitmaskLayer`, each defining `fn
/// get_channel_{{stroke_or_fill_property}}_{{c_idx}}(label_index: u32) ->
/// vec3<f32>` (plus any texture bindings the mode needs). Unlike
/// [`color`] (one `get_fill_color` shared by every instance of a layer), a
/// `BitmaskLayer` may have several channels — each possibly using a different
/// `ColorMode` for its fill and another for its stroke — coexisting in one
/// shader module, so every (channel, fill/stroke) pair gets its own uniquely
/// named function and bindings rather than sharing one. Templates: substitute
/// `{{stroke_or_fill_property}}` for the property (`fill_color` or
/// `stroke_color`, which doubles as the prefix of the `Channel` uniform fields
/// the snippet reads) and `{{c_idx}}` for the channel index, plus any
/// binding-index/dtype placeholders;
/// assembled at runtime by
/// `crate::layers::bitmask_layer::prepare_channel_color`. All variants that
/// read a value texture assume [`common::FLAT_TEXEL_COORD`] is also injected.
/// See [`bitmask_channel`] for the (non-templated) code that calls these.
pub mod get_channel_color {
    /// Static color shared by every object in the channel.
    pub const UNIFORM_RGB: &str = include_str!("wgsl_functions/get_channel_color/uniform_rgb.wgsl");

    /// Per-object RGB from three parallel value textures.
    pub const INSTANCED_RGB: &str = include_str!("wgsl_functions/get_channel_color/instanced_rgb.wgsl");

    /// Per-object RGB from one interleaved value texture.
    pub const INSTANCED_RGB_INTERLEAVED: &str =
        include_str!("wgsl_functions/get_channel_color/instanced_rgb_interleaved.wgsl");

    /// Per-object integer "set color" labels indexed against a palette texture.
    pub const CATEGORICAL: &str = include_str!("wgsl_functions/get_channel_color/categorical.wgsl");

    /// Per-object scalar values mapped through a continuous colormap.
    pub const QUANTITATIVE: &str = include_str!("wgsl_functions/get_channel_color/quantitative.wgsl");
}

/// Scalar counterpart of [`get_channel_color`], for the `BitmaskLayer`
/// properties carried by a [`SizeMode`](crate::render_traits::SizeMode) or an
/// [`OpacityMode`](crate::render_traits::OpacityMode) rather than a
/// `ColorMode`: each snippet defines `fn
/// get_channel_{{stroke_or_fill_property}}_{{c_idx}}(label_index: u32) ->
/// f32`. Substitute `{{stroke_or_fill_property}}` for the property
/// (`fill_opacity`, `stroke_opacity` or `stroke_width`, which doubles as the
/// name of the `Channel` uniform field the uniform variant reads) and
/// `{{c_idx}}` for the channel index; assembled at runtime by
/// `crate::layers::bitmask_layer::prepare_channel_scalar`. The instanced
/// variant assumes [`common::FLAT_TEXEL_COORD`] is also injected.
pub mod get_channel_scalar {
    /// Static value shared by every object in the channel.
    pub const UNIFORM: &str = include_str!("wgsl_functions/get_channel_scalar/uniform.wgsl");

    /// Per-object value from a value texture.
    pub const INSTANCED: &str = include_str!("wgsl_functions/get_channel_scalar/instanced.wgsl");
}

/// Shared (non-templated) WGSL functions used by `BitmaskLayer`'s fragment
/// shader to loop over its channels, plus the two small templates needed to
/// dispatch to a per-channel getter (see [`get_channel_color`] and
/// [`get_channel_scalar`]).
///
/// [`CHANNEL_SAMPLE`] and [`CHANNEL_IS_EDGE`] are ordinary WGSL functions
/// (parameterized by `channel_index`, not templated) injected exactly once,
/// regardless of channel count, and called from a real `for` loop over
/// `u.num_channels` in `bitmask_layer.wgsl`'s `fs_main` -- unlike
/// [`get_channel_color`], no per-channel unrolling is needed here, since
/// sampling/edge-detection is identical logic for every channel. Only
/// resolving a channel's *color*, *opacity* and *stroke width* differs per
/// `ColorMode`/`OpacityMode`/`SizeMode`, which is why
/// [`CHANNEL_COLOR_DISPATCH`] and [`CHANNEL_SCALAR_DISPATCH`] (small generated
/// `switch`es over channel index) are still needed to call the right
/// per-channel getter. Assumes `mask_data`, `flat_texel_coord` (see
/// [`common::FLAT_TEXEL_COORD`]) and the layer's `Uniforms`/`Channel` structs
/// are already in scope.
pub mod bitmask_channel {
    /// `fn bitmask_sample(channel_index: u32, px: vec2<u32>) -> i32` — reads
    /// the object id at `px` for one channel of the shared, multi-channel
    /// `mask_data` texture.
    pub const CHANNEL_SAMPLE: &str = include_str!("wgsl_functions/bitmask/channel_sample.wgsl");

    /// `fn bitmask_is_edge(channel_index, px, raw_label, img_w, img_h,
    /// stroke_width) -> bool` — an approximate object-boundary test, used to
    /// render outline-only channels. Takes `px` as a continuous position in
    /// mask-texel space and a (possibly fractional) `stroke_width` in the same
    /// units, so that outline thickness does not quantize to the mask's
    /// resolution; callers resolve the width with [`CHANNEL_STROKE_WIDTH`]
    /// first. Depends on [`CHANNEL_SAMPLE`] also being injected.
    pub const CHANNEL_IS_EDGE: &str = include_str!("wgsl_functions/bitmask/channel_is_edge.wgsl");

    /// `fn bitmask_stroke_width_texels(stroke_width: f32) -> f32` — converts a
    /// stroke width given in screen pixels, data units or a fraction of the
    /// layer height into the mask texels [`CHANNEL_IS_EDGE`] measures in.
    /// Assumes `translate`, `scale` and `get_aspect_ratio_mat` (see
    /// [`common`]) are also injected.
    pub const CHANNEL_STROKE_WIDTH: &str =
        include_str!("wgsl_functions/bitmask/channel_stroke_width.wgsl");

    /// `fn get_channel_{{stroke_or_fill_property}}(channel_index: u32,
    /// label_index: u32) -> vec3<f32>` — dispatches to the per-channel
    /// `get_channel_{{stroke_or_fill_property}}_{{c_idx}}` (see
    /// [`get_channel_color`]) matching `channel_index`. Template: substitute
    /// `{{stroke_or_fill_property}}` with the property (`fill_color` or
    /// `stroke_color`) and `{{switch_cases}}` with one `case
    /// N: { return get_channel_<stroke_or_fill_property>_N(label_index); }` per channel.
    pub const CHANNEL_COLOR_DISPATCH: &str = include_str!("wgsl_functions/bitmask/channel_color_dispatch.wgsl");

    /// `fn get_channel_{{stroke_or_fill_property}}(channel_index: u32,
    /// label_index: u32) -> f32` — the scalar counterpart of
    /// [`CHANNEL_COLOR_DISPATCH`], dispatching to the per-channel
    /// `get_channel_{{stroke_or_fill_property}}_{{c_idx}}` (see
    /// [`get_channel_scalar`]) matching `channel_index`. Template: substitute
    /// `{{stroke_or_fill_property}}` with the property (`fill_opacity`,
    /// `stroke_opacity` or `stroke_width`) and `{{switch_cases}}` as above.
    pub const CHANNEL_SCALAR_DISPATCH: &str = include_str!("wgsl_functions/bitmask/channel_scalar_dispatch.wgsl");

    /// `fn get_channel_{{stroke_or_fill_property}}(channel_index: u32,
    /// label_index: u32) -> bool` — the boolean counterpart of
    /// [`CHANNEL_SCALAR_DISPATCH`], dispatching to the per-channel
    /// `get_channel_{{stroke_or_fill_property}}_{{c_idx}}` function matching
    /// `channel_index` (assembled by
    /// `crate::emphasis_mode::prepare_emphasis_criteria`, one per channel, for
    /// `is_filtered_in` / `is_selected_in`). Template: substitute
    /// `{{stroke_or_fill_property}}` with the property (`is_filtered_in` or
    /// `is_selected_in`) and `{{switch_cases}}` as above. Defaults to `true`
    /// (include) for an out-of-range channel index.
    pub const CHANNEL_BOOL_DISPATCH: &str = include_str!("wgsl_functions/bitmask/channel_bool_dispatch.wgsl");
}

/// Per-[`SizeMode`](crate::render_traits::SizeMode) WGSL snippets, each defining
/// `fn get_point_radius(instance_index: u32) -> f32`. The uniform variant reads
/// the `point_radius` uniform; the instanced variant reads a per-element value
/// texture (its binding index and sampled type filled in at runtime by
/// [`crate::scalar_mode::prepare_size_mode`]). The instanced variant assumes
/// [`common::FLAT_TEXEL_COORD`] is also injected.
pub mod size {
    /// Static radius shared by every point.
    pub const UNIFORM: &str = include_str!("wgsl_functions/get_point_radius/uniform.wgsl");

    /// Per-element radius from a value texture.
    pub const INSTANCED: &str = include_str!("wgsl_functions/get_point_radius/instanced.wgsl");
}

/// Per-[`OpacityMode`](crate::render_traits::OpacityMode) WGSL snippets, each
/// defining `fn get_point_opacity(instance_index: u32) -> f32`. The uniform
/// variant reads the `point_opacity` uniform; the instanced variant reads a
/// per-element value texture (its binding index and sampled type filled in at
/// runtime by [`crate::scalar_mode::prepare_opacity_mode`]). The instanced
/// variant assumes [`common::FLAT_TEXEL_COORD`] is also injected.
pub mod opacity {
    /// Static opacity shared by every point.
    pub const UNIFORM: &str = include_str!("wgsl_functions/get_point_opacity/uniform.wgsl");

    /// Per-element opacity from a value texture.
    pub const INSTANCED: &str = include_str!("wgsl_functions/get_point_opacity/instanced.wgsl");
}

/// Per-[`SizeMode`](crate::render_traits::SizeMode) WGSL snippets, each defining
/// `fn get_stroke_width(poly_index: u32) -> f32`. The uniform variant reads the
/// `stroke_width` uniform; the instanced variant reads a per-element value
/// texture (its binding index and sampled type filled in at runtime by
/// [`crate::scalar_mode::prepare_stroke_width_mode`]). The instanced variant
/// assumes [`common::FLAT_TEXEL_COORD`] is also injected. Shared by the line,
/// polygon, and curve layers (`poly_index` is the per-element index).
pub mod stroke_width {
    /// Static width shared by every stroke.
    pub const UNIFORM: &str = include_str!("wgsl_functions/get_stroke_width/uniform.wgsl");

    /// Per-element width from a value texture.
    pub const INSTANCED: &str = include_str!("wgsl_functions/get_stroke_width/instanced.wgsl");
}

/// Per-[`OpacityMode`](crate::render_traits::OpacityMode) WGSL snippets, each
/// defining `fn get_stroke_opacity(poly_index: u32) -> f32`. The uniform variant
/// reads the `stroke_opacity` uniform; the instanced variant reads a per-element
/// value texture (its binding index and sampled type filled in at runtime by
/// [`crate::scalar_mode::prepare_stroke_opacity_mode`]). The instanced variant
/// assumes [`common::FLAT_TEXEL_COORD`] is also injected. Shared by the line,
/// polygon, and curve layers.
pub mod stroke_opacity {
    /// Static opacity shared by every stroke.
    pub const UNIFORM: &str = include_str!("wgsl_functions/get_stroke_opacity/uniform.wgsl");

    /// Per-element opacity from a value texture.
    pub const INSTANCED: &str = include_str!("wgsl_functions/get_stroke_opacity/instanced.wgsl");
}

/// Per-[`OpacityMode`](crate::render_traits::OpacityMode) WGSL snippets, each
/// defining `fn get_fill_opacity(color_index: u32) -> f32`. The uniform variant
/// reads the `fill_opacity` uniform; the instanced variant reads a per-polygon
/// value texture (its binding index and sampled type filled in at runtime by
/// [`crate::scalar_mode::prepare_fill_opacity_mode`]). The instanced variant
/// assumes [`common::FLAT_TEXEL_COORD`] is also injected.
pub mod fill_opacity {
    /// Static opacity shared by every polygon fill.
    pub const UNIFORM: &str = include_str!("wgsl_functions/get_fill_opacity/uniform.wgsl");

    /// Per-polygon opacity from a value texture.
    pub const INSTANCED: &str = include_str!("wgsl_functions/get_fill_opacity/instanced.wgsl");
}

/// Per-[`EmphasisCriteria`](crate::render_traits::EmphasisCriteria) WGSL
/// snippets used to test filtering/selection membership. Each variant defines `fn
/// {{criteria_fn_name}}(instance_index: u32) -> bool`; the categorical and
/// quantitative variants additionally declare a per-element value texture at
/// `{{criteria_data_var}}` (binding index and sampled type filled in at
/// runtime). Assembled at runtime by
/// [`crate::emphasis_mode::prepare_emphasis_criteria`], once per criteria
/// (`filtering_criteria` / `selection_criteria`) so both can coexist in the
/// same shader module without name or binding collisions. The categorical and
/// quantitative variants assume [`common::FLAT_TEXEL_COORD`] is also injected.
pub mod is_included {
    /// Explicit empty inclusion list: no item is included (texture-free).
    pub const EMPTY: &str = include_str!("wgsl_functions/get_is_included/empty.wgsl");

    /// Per-element category code tested against an inline array of included codes.
    pub const CATEGORICAL: &str = include_str!("wgsl_functions/get_is_included/categorical.wgsl");

    /// Per-element scalar value tested against a two-sided `[min, max]` range.
    pub const QUANTITATIVE_RANGE: &str =
        include_str!("wgsl_functions/get_is_included/quantitative_range.wgsl");

    /// Per-element scalar value tested against either a lower or upper bound but not both.
    pub const QUANTITATIVE_ONE_SIDED: &str =
        include_str!("wgsl_functions/get_is_included/quantitative_one_sided.wgsl");
}

/// Colormap WGSL functions, embedded at compile time from
/// `wgsl_functions/colormaps/`.
///
/// Each constant is a single self-contained WGSL function `fn name(x: f32) ->
/// vec4<f32>` mapping a normalized scalar to an RGBA color, ported from
/// [Vitessce's GLSL colormaps](https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/index.js).
/// Inject one into a template with [`ShaderBuilder::inject_function`].
pub mod colormaps {
    /// `fn autumn(x: f32) -> vec4<f32>`
    pub const AUTUMN: &str = include_str!("wgsl_functions/colormap_quantitative/autumn.wgsl");

    /// `fn blues(x: f32) -> vec4<f32>`
    pub const BLUES: &str = include_str!("wgsl_functions/colormap_quantitative/blues.wgsl");

    /// `fn bone(x: f32) -> vec4<f32>`
    pub const BONE: &str = include_str!("wgsl_functions/colormap_quantitative/bone.wgsl");

    /// `fn cool(x: f32) -> vec4<f32>`
    pub const COOL: &str = include_str!("wgsl_functions/colormap_quantitative/cool.wgsl");

    /// `fn copper(x: f32) -> vec4<f32>`
    pub const COPPER: &str = include_str!("wgsl_functions/colormap_quantitative/copper.wgsl");

    /// `fn density(x: f32) -> vec4<f32>`
    pub const DENSITY: &str = include_str!("wgsl_functions/colormap_quantitative/density.wgsl");

    /// `fn greens(x: f32) -> vec4<f32>`
    pub const GREENS: &str = include_str!("wgsl_functions/colormap_quantitative/greens.wgsl");

    /// `fn greys(x: f32) -> vec4<f32>`
    pub const GREYS: &str = include_str!("wgsl_functions/colormap_quantitative/greys.wgsl");

    /// `fn hot(x: f32) -> vec4<f32>`
    pub const HOT: &str = include_str!("wgsl_functions/colormap_quantitative/hot.wgsl");

    /// `fn inferno(x: f32) -> vec4<f32>`
    pub const INFERNO: &str = include_str!("wgsl_functions/colormap_quantitative/inferno.wgsl");

    /// `fn jet(x: f32) -> vec4<f32>`
    pub const JET: &str = include_str!("wgsl_functions/colormap_quantitative/jet.wgsl");

    /// `fn magma(x: f32) -> vec4<f32>`
    pub const MAGMA: &str = include_str!("wgsl_functions/colormap_quantitative/magma.wgsl");

    /// `fn oranges(x: f32) -> vec4<f32>`
    pub const ORANGES: &str = include_str!("wgsl_functions/colormap_quantitative/oranges.wgsl");

    /// `fn plasma(x: f32) -> vec4<f32>`
    pub const PLASMA: &str = include_str!("wgsl_functions/colormap_quantitative/plasma.wgsl");

    /// `fn purples(x: f32) -> vec4<f32>`
    pub const PURPLES: &str = include_str!("wgsl_functions/colormap_quantitative/purples.wgsl");

    /// `fn reds(x: f32) -> vec4<f32>`
    pub const REDS: &str = include_str!("wgsl_functions/colormap_quantitative/reds.wgsl");

    /// `fn spring(x: f32) -> vec4<f32>`
    pub const SPRING: &str = include_str!("wgsl_functions/colormap_quantitative/spring.wgsl");

    /// `fn summer(x: f32) -> vec4<f32>`
    pub const SUMMER: &str = include_str!("wgsl_functions/colormap_quantitative/summer.wgsl");

    /// `fn viridis(x: f32) -> vec4<f32>`
    pub const VIRIDIS: &str = include_str!("wgsl_functions/colormap_quantitative/viridis.wgsl");

    /// `fn winter(x: f32) -> vec4<f32>`
    pub const WINTER: &str = include_str!("wgsl_functions/colormap_quantitative/winter.wgsl");

    use crate::render_traits::QuantitativeColormap;

    /// The embedded WGSL source and the name of the `fn <name>(x: f32) ->
    /// vec4<f32>` it defines, for a given [`QuantitativeColormap`]. Inject the
    /// source with [`super::ShaderBuilder::inject_function`] and call the named
    /// function to sample the colormap on the GPU.
    pub fn wgsl_source_and_name(colormap: QuantitativeColormap) -> (&'static str, &'static str) {
        match colormap {
            QuantitativeColormap::Plasma => (PLASMA, "plasma"),
            QuantitativeColormap::Viridis => (VIRIDIS, "viridis"),
            QuantitativeColormap::Greys => (GREYS, "greys"),
            QuantitativeColormap::Magma => (MAGMA, "magma"),
            QuantitativeColormap::Jet => (JET, "jet"),
            QuantitativeColormap::Bone => (BONE, "bone"),
            QuantitativeColormap::Copper => (COPPER, "copper"),
            QuantitativeColormap::Density => (DENSITY, "density"),
            QuantitativeColormap::Inferno => (INFERNO, "inferno"),
            QuantitativeColormap::Cool => (COOL, "cool"),
            QuantitativeColormap::Hot => (HOT, "hot"),
            QuantitativeColormap::Spring => (SPRING, "spring"),
            QuantitativeColormap::Summer => (SUMMER, "summer"),
            QuantitativeColormap::Autumn => (AUTUMN, "autumn"),
            QuantitativeColormap::Winter => (WINTER, "winter"),
            QuantitativeColormap::Blues => (BLUES, "blues"),
            QuantitativeColormap::Greens => (GREENS, "greens"),
            QuantitativeColormap::Oranges => (ORANGES, "oranges"),
            QuantitativeColormap::Purples => (PURPLES, "purples"),
            QuantitativeColormap::Reds => (REDS, "reds"),
        }
    }
}

/// A WGSL scalar type usable as the element type of a storage array.
///
/// WGSL storage buffers only support 32-bit host-shareable scalars, so this is
/// intentionally limited to `f32`, `u32` and `i32`. Wider (`u64`/`f64`) or
/// narrower (`u8`/`u16`) numeric dtypes must be widened/converted to one of
/// these on the CPU before upload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WgslScalar {
    F32,
    U32,
    I32,
}

impl WgslScalar {
    /// The WGSL spelling of this scalar type (`"f32"`, `"u32"` or `"i32"`).
    pub fn as_wgsl(self) -> &'static str {
        match self {
            WgslScalar::F32 => "f32",
            WgslScalar::U32 => "u32",
            WgslScalar::I32 => "i32",
        }
    }
}

/// A numeric texel dtype for a single-channel (red-only) 2D texture.
///
/// Unlike a storage-buffer array (which WGSL limits to 32-bit scalars), a
/// texture stores each texel at its native byte width while the shader always
/// reads it as one of three 32-bit WGSL *sampled types* (`f32`/`u32`/`i32`) —
/// narrower integer formats are zero/sign-extended on read. This is what lets
/// 8/16/32-bit image data be uploaded to the GPU without any CPU-side widening,
/// and lets 32-bit integer data keep full precision (no lossy `as f32` cast).
///
/// WebGPU defines no 64-bit texture formats, so 64-bit source data must be
/// narrowed to 32 bits on the CPU before upload — that narrowing is the
/// caller's responsibility (see `BitmapLayer`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureDtype {
    U8,
    U16,
    U32,
    I8,
    I16,
    I32,
    F32,
}

impl TextureDtype {
    /// The WGSL sampled type (`f32`/`u32`/`i32`) used as the `T` in
    /// `texture_2d<T>` / `texture_2d_array<T>` for this dtype.
    pub fn sample_type(self) -> WgslScalar {
        match self {
            TextureDtype::U8 | TextureDtype::U16 | TextureDtype::U32 => WgslScalar::U32,
            TextureDtype::I8 | TextureDtype::I16 | TextureDtype::I32 => WgslScalar::I32,
            TextureDtype::F32 => WgslScalar::F32,
        }
    }

    /// The single-channel `wgpu::TextureFormat` that stores this dtype natively.
    pub fn texture_format(self) -> wgpu::TextureFormat {
        match self {
            TextureDtype::U8 => wgpu::TextureFormat::R8Uint,
            TextureDtype::U16 => wgpu::TextureFormat::R16Uint,
            TextureDtype::U32 => wgpu::TextureFormat::R32Uint,
            TextureDtype::I8 => wgpu::TextureFormat::R8Sint,
            TextureDtype::I16 => wgpu::TextureFormat::R16Sint,
            TextureDtype::I32 => wgpu::TextureFormat::R32Sint,
            TextureDtype::F32 => wgpu::TextureFormat::R32Float,
        }
    }

    /// The `wgpu::TextureSampleType` to declare in a bind group layout entry.
    ///
    /// Must agree with [`sample_type`](Self::sample_type) and
    /// [`texture_format`](Self::texture_format). Float textures are declared
    /// non-filterable, since these are read via `textureLoad` (no sampler) and
    /// `R32Float` is not filterable without an optional feature.
    pub fn binding_sample_type(self) -> wgpu::TextureSampleType {
        match self.sample_type() {
            WgslScalar::F32 => wgpu::TextureSampleType::Float { filterable: false },
            WgslScalar::U32 => wgpu::TextureSampleType::Uint,
            WgslScalar::I32 => wgpu::TextureSampleType::Sint,
        }
    }

    /// Number of bytes per texel (equivalently, per source element).
    pub fn bytes_per_texel(self) -> u32 {
        match self {
            TextureDtype::U8 | TextureDtype::I8 => 1,
            TextureDtype::U16 | TextureDtype::I16 => 2,
            TextureDtype::U32 | TextureDtype::I32 | TextureDtype::F32 => 4,
        }
    }
}

/// Builds a WGSL shader source string by substituting `{{placeholder}}` tokens
/// in a template.
///
/// The builder holds a [`Cow`] over the template so that a build with no
/// substitutions allocates nothing; the first substitution promotes it to an
/// owned `String`.
pub struct ShaderBuilder<'a> {
    source: Cow<'a, str>,
}

impl<'a> ShaderBuilder<'a> {
    /// Start from a shader template, typically `include_str!`-ed at the call site.
    pub fn new(template: &'a str) -> Self {
        Self {
            source: Cow::Borrowed(template),
        }
    }

    /// Replace every occurrence of `{{name}}` with `value`.
    ///
    /// This is the single primitive underlying
    /// [`inject_function`](Self::inject_function) and
    /// [`inject_dtype`](Self::inject_dtype); use it directly for any other
    /// substitution.
    pub fn define(mut self, name: &str, value: &str) -> Self {
        let placeholder = format!("{{{{{name}}}}}");
        if self.source.contains(&placeholder) {
            self.source = Cow::Owned(self.source.replace(&placeholder, value));
        }
        self
    }

    /// Replace every occurrence of `{{name}}` with the decimal spelling of an
    /// unsigned integer. Handy for binding indices and array lengths chosen at
    /// runtime, avoiding a `.to_string()` at the call site.
    pub fn define_u32(self, name: &str, value: u32) -> Self {
        self.define(name, &value.to_string())
    }

    /// Inject a `@binding` index at `{{var_name}}_bidx` (chosen at runtime).
    pub fn define_bidx(self, var_name: &str, binding_index: u32) -> Self {
        self.define_u32(&format!("{var_name}_bidx"), binding_index)
    }

    /// Inject a reusable WGSL function (a compile-time snippet, e.g. from
    /// [`common`]) at `{{name}}`.
    // TODO: use a special prefix for injected functions to make the shaders more clear/readable.
    pub fn inject_function(self, name: &str, source: &str) -> Self {
        self.define(name, source)
    }

    /// Inject a reusable WGSL function (from [`common`]) only when `source` is
    /// `Some`; otherwise leave the template untouched. Useful for dependencies
    /// that are only needed by some runtime configurations.
    pub fn inject_optional_function(self, name: &str, source: Option<&str>) -> Self {
        match source {
            Some(source) => self.define(name, source),
            None => self,
        }
    }

    /// Inject a storage-array element dtype at `{{var_name}}_dtype` (chosen at
    /// runtime).
    pub fn inject_dtype(self, var_name: &str, dtype: WgslScalar) -> Self {
        self.define(&format!("{var_name}_dtype"), dtype.as_wgsl())
    }

    /// Inject a texture sampled type at `{{var_name}}_dtype`, i.e. the `T` in
    /// `texture_2d<T>` / `texture_2d_array<T>`, chosen at runtime from a
    /// [`TextureDtype`].
    pub fn inject_texture_sample_type(self, var_name: &str, dtype: TextureDtype) -> Self {
        self.inject_dtype(var_name, dtype.sample_type())
    }

    /// Finish building and return the WGSL source string.
    ///
    /// In debug builds this asserts that no `{{...}}` placeholders were left
    /// unsubstituted, catching template typos and missing injections early.
    /// (WGSL itself never uses a literal `{{`, so this is unambiguous.)
    pub fn build(self) -> String {
        let out = self.source.into_owned();
        debug_assert!(
            !out.contains("{{"),
            "shader template has unsubstituted placeholder(s): {:?}",
            out.split("{{")
                .skip(1)
                .filter_map(|s| s.split("}}").next())
                .collect::<Vec<_>>()
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_functions_and_dtype() {
        let template = "{{fn}}\nvar<storage, read> d: array<{{d_dtype}}>;";
        let out = ShaderBuilder::new(template)
            .inject_function("fn", "fn foo() {}")
            .inject_dtype("d", WgslScalar::U32)
            .build();
        assert_eq!(out, "fn foo() {}\nvar<storage, read> d: array<u32>;");
    }

    #[test]
    fn no_substitution_is_zero_copy() {
        let template = "fn main() {}";
        let builder = ShaderBuilder::new(template);
        assert!(matches!(builder.source, Cow::Borrowed(_)));
        assert_eq!(builder.build(), template);
    }

    #[test]
    fn replaces_all_occurrences() {
        let out = ShaderBuilder::new("{{t_dtype}} and {{t_dtype}}")
            .inject_dtype("t", WgslScalar::F32)
            .build();
        assert_eq!(out, "f32 and f32");
    }
}
