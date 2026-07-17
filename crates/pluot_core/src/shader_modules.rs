//! A very lightweight WGSL shader-module system.
//!
//! Shaders live as `.wgsl` files and are embedded at compile time via
//! `include_str!`. This module provides two forms of shader composition, both
//! built on the same primitive: substituting `{{placeholder}}` tokens in a
//! template string.
//!
//! 1. **Compile-time function injection.** Reusable WGSL functions that would
//!    otherwise be copy-pasted across many shaders (e.g. `scale`, `translate`,
//!    `get_aspect_ratio_mat`) live in their own `.wgsl` files under
//!    `wgsl_functions/` and are embedded as `&'static str` constants (see
//!    [`common`]). A shader template references one with a `{{name}}`
//!    placeholder; [`ShaderBuilder::inject_function`] substitutes the embedded
//!    source.
//!
//! 2. **Runtime dtype injection.** A template can leave the element type of a
//!    storage array or the sampled type of a texture as a placeholder, e.g.
//!    `var<storage, read> data: array<{{dtype}}>;` or
//!    `var img: texture_2d_array<{{dtype}}>;`. [`ShaderBuilder::inject_dtype`] /
//!    [`ShaderBuilder::inject_texture_sample_type`] fill it in at runtime (from
//!    a [`WgslScalar`] or [`TextureDtype`]), so the same shader source can be
//!    specialized per data dtype. Textures additionally let 8/16/32-bit data
//!    live on the GPU at native width (see [`TextureDtype`]).
//!
//! The whole system is nothing more than repeated `str::replace` over
//! `{{...}}` tokens. [`ShaderBuilder::build`] returns the finished WGSL source
//! as a `String`, ready to hand to `device.create_shader_module` via
//! `wgpu::ShaderSource::Wgsl`.
//!
//! (Ideally we could use a more robust system such as WESL, but this adds ~1MB
//! to the WASM binary size, at least last time I tried it.)
//!
//! ```ignore
//! use pluot_core::shader_modules::{common, ShaderBuilder, WgslScalar};
//!
//! let source = ShaderBuilder::new(include_str!("shaders/bitmap_layer.wgsl"))
//!     .inject_function("scale", common::SCALE)
//!     .inject_function("translate", common::TRANSLATE)
//!     .inject_function("get_aspect_ratio_mat", common::GET_ASPECT_RATIO_MAT)
//!     .inject_dtype("img_data_dtype", WgslScalar::F32)
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
    pub const UNIFORM_RGB: &str = include_str!("wgsl_functions/color/uniform_rgb.wgsl");

    /// Per-element RGB from three parallel value textures.
    pub const INSTANCED_RGB: &str = include_str!("wgsl_functions/color/instanced_rgb.wgsl");

    /// Per-element RGB from one interleaved value texture.
    pub const INSTANCED_RGB_INTERLEAVED: &str =
        include_str!("wgsl_functions/color/instanced_rgb_interleaved.wgsl");

    /// Per-element integer labels indexed against a palette texture.
    pub const CATEGORICAL: &str = include_str!("wgsl_functions/color/categorical.wgsl");

    /// Per-element scalar values mapped through a continuous colormap.
    pub const QUANTITATIVE: &str = include_str!("wgsl_functions/color/quantitative.wgsl");
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
    pub const AUTUMN: &str = include_str!("wgsl_functions/colormaps/autumn.wgsl");

    /// `fn bone(x: f32) -> vec4<f32>`
    pub const BONE: &str = include_str!("wgsl_functions/colormaps/bone.wgsl");

    /// `fn cool(x: f32) -> vec4<f32>`
    pub const COOL: &str = include_str!("wgsl_functions/colormaps/cool.wgsl");

    /// `fn copper(x: f32) -> vec4<f32>`
    pub const COPPER: &str = include_str!("wgsl_functions/colormaps/copper.wgsl");

    /// `fn density(x: f32) -> vec4<f32>`
    pub const DENSITY: &str = include_str!("wgsl_functions/colormaps/density.wgsl");

    /// `fn greys(x: f32) -> vec4<f32>`
    pub const GREYS: &str = include_str!("wgsl_functions/colormaps/greys.wgsl");

    /// `fn hot(x: f32) -> vec4<f32>`
    pub const HOT: &str = include_str!("wgsl_functions/colormaps/hot.wgsl");

    /// `fn inferno(x: f32) -> vec4<f32>`
    pub const INFERNO: &str = include_str!("wgsl_functions/colormaps/inferno.wgsl");

    /// `fn jet(x: f32) -> vec4<f32>`
    pub const JET: &str = include_str!("wgsl_functions/colormaps/jet.wgsl");

    /// `fn magma(x: f32) -> vec4<f32>`
    pub const MAGMA: &str = include_str!("wgsl_functions/colormaps/magma.wgsl");

    /// `fn plasma(x: f32) -> vec4<f32>`
    pub const PLASMA: &str = include_str!("wgsl_functions/colormaps/plasma.wgsl");

    /// `fn spring(x: f32) -> vec4<f32>`
    pub const SPRING: &str = include_str!("wgsl_functions/colormaps/spring.wgsl");

    /// `fn summer(x: f32) -> vec4<f32>`
    pub const SUMMER: &str = include_str!("wgsl_functions/colormaps/summer.wgsl");

    /// `fn viridis(x: f32) -> vec4<f32>`
    pub const VIRIDIS: &str = include_str!("wgsl_functions/colormaps/viridis.wgsl");

    /// `fn winter(x: f32) -> vec4<f32>`
    pub const WINTER: &str = include_str!("wgsl_functions/colormaps/winter.wgsl");

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

    /// Inject a reusable WGSL function (a compile-time snippet, e.g. from
    /// [`common`]) at `{{name}}`.
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

    /// Inject a storage-array element dtype at `{{name}}` (chosen at runtime).
    pub fn inject_dtype(self, name: &str, dtype: WgslScalar) -> Self {
        self.define(name, dtype.as_wgsl())
    }

    /// Inject a texture sampled type at `{{name}}`, i.e. the `T` in
    /// `texture_2d<T>` / `texture_2d_array<T>`, chosen at runtime from a
    /// [`TextureDtype`].
    pub fn inject_texture_sample_type(self, name: &str, dtype: TextureDtype) -> Self {
        self.inject_dtype(name, dtype.sample_type())
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
        let template = "{{fn}}\nvar<storage, read> d: array<{{dtype}}>;";
        let out = ShaderBuilder::new(template)
            .inject_function("fn", "fn foo() {}")
            .inject_dtype("dtype", WgslScalar::U32)
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
        let out = ShaderBuilder::new("{{t}} and {{t}}").inject_dtype("t", WgslScalar::F32).build();
        assert_eq!(out, "f32 and f32");
    }
}
