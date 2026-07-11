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

    /// Inject a reusable WGSL function (a compile-time snippet, e.g. from
    /// [`common`]) at `{{name}}`.
    pub fn inject_function(self, name: &str, source: &str) -> Self {
        self.define(name, source)
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
