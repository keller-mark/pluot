//! A typed numeric array (`NumericData`) supporting all the numeric dtypes we
//! accept from callers (8/16/32/64-bit signed/unsigned integers and 32/64-bit
//! floats), plus the helpers to get that data onto the GPU with as little
//! CPU-side transformation as possible.
//!
//! The preferred GPU path is [`NumericData::create_data_texture`], which
//! uploads the flat array into a single-channel 2D texture at its native byte
//! width (see [`crate::shader_modules::TextureDtype`]): 8/16/32-bit dtypes are
//! borrowed zero-copy, and only the three 64-bit dtypes are narrowed to 32 bits
//! (WebGPU defines no 64-bit texture formats). A storage-buffer path
//! ([`as_gpu_buffer`](NumericData::as_gpu_buffer)) is also retained for data too
//! large for a texture's dimension limits.

use std::borrow::Cow;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::shader_modules::{TextureDtype, WgslScalar};
use crate::wgpu;
use crate::log;

/// Typed numeric array supporting multiple dtypes.
///
/// Serialized as an adjacently-tagged enum with `dtype` and `values` fields,
/// e.g. `{"dtype": "Uint16", "values": [1, 2, 3]}`.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "dtype", content = "values")]
pub enum NumericData {
    Uint8(Arc<Vec<u8>>),
    Uint16(Arc<Vec<u16>>),
    Uint32(Arc<Vec<u32>>),
    Uint64(Arc<Vec<u64>>),
    Int8(Arc<Vec<i8>>),
    Int16(Arc<Vec<i16>>),
    Int32(Arc<Vec<i32>>),
    Int64(Arc<Vec<i64>>),
    Float32(Arc<Vec<f32>>),
    Float64(Arc<Vec<f64>>),
}

/// Implement `From<Vec<T>>` and `From<Arc<Vec<T>>>` for every supported dtype.
///
/// Each Rust numeric type maps to exactly one `NumericData` variant, so these
/// conversions are unambiguous and let call sites write `some_vec.into()` (or
/// `NumericData::from(some_vec)`) instead of the verbose
/// `NumericData::Float32(Arc::new(some_vec))`.
///
/// Note: `.into()` on a bare, untyped float literal array (e.g.
/// `vec![0.0, 1.0].into()`) is ambiguous between `f32` and `f64`; annotate the
/// element type (`vec![0.0f32, 1.0]`) or use the explicit variant there.
macro_rules! impl_from_for_numeric_data {
    ($($t:ty => $variant:ident),* $(,)?) => {
        $(
            impl From<Arc<Vec<$t>>> for NumericData {
                fn from(v: Arc<Vec<$t>>) -> Self {
                    NumericData::$variant(v)
                }
            }
            impl From<Vec<$t>> for NumericData {
                fn from(v: Vec<$t>) -> Self {
                    NumericData::$variant(Arc::new(v))
                }
            }
        )*
    };
}

impl_from_for_numeric_data! {
    u8 => Uint8,
    u16 => Uint16,
    u32 => Uint32,
    u64 => Uint64,
    i8 => Int8,
    i16 => Int16,
    i32 => Int32,
    i64 => Int64,
    f32 => Float32,
    f64 => Float64,
}

impl NumericData {
    /// Number of elements in the array.
    pub fn len(&self) -> usize {
        match self {
            NumericData::Uint8(v) => v.len(),
            NumericData::Uint16(v) => v.len(),
            NumericData::Uint32(v) => v.len(),
            NumericData::Uint64(v) => v.len(),
            NumericData::Int8(v) => v.len(),
            NumericData::Int16(v) => v.len(),
            NumericData::Int32(v) => v.len(),
            NumericData::Int64(v) => v.len(),
            NumericData::Float32(v) => v.len(),
            NumericData::Float64(v) => v.len(),
        }
    }

    /// Whether the array is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get element at index as f32.
    pub fn get_f32(&self, idx: usize) -> f32 {
        match self {
            NumericData::Uint8(v) => v[idx] as f32,
            NumericData::Uint16(v) => v[idx] as f32,
            NumericData::Uint32(v) => v[idx] as f32,
            NumericData::Uint64(v) => v[idx] as f32,
            NumericData::Int8(v) => v[idx] as f32,
            NumericData::Int16(v) => v[idx] as f32,
            NumericData::Int32(v) => v[idx] as f32,
            NumericData::Int64(v) => v[idx] as f32,
            NumericData::Float32(v) => v[idx],
            NumericData::Float64(v) => v[idx] as f32,
        }
    }

    /// Get element at index as f64.
    ///
    /// Useful for CPU-side math (such as picking distance) where the wider
    /// mantissa preserves more precision for large 32/64-bit integer
    /// coordinates than [`get_f32`](Self::get_f32) would.
    pub fn get_f64(&self, idx: usize) -> f64 {
        match self {
            NumericData::Uint8(v) => v[idx] as f64,
            NumericData::Uint16(v) => v[idx] as f64,
            NumericData::Uint32(v) => v[idx] as f64,
            NumericData::Uint64(v) => v[idx] as f64,
            NumericData::Int8(v) => v[idx] as f64,
            NumericData::Int16(v) => v[idx] as f64,
            NumericData::Int32(v) => v[idx] as f64,
            NumericData::Int64(v) => v[idx] as f64,
            NumericData::Float32(v) => v[idx] as f64,
            NumericData::Float64(v) => v[idx],
        }
    }

    /// Format the element at `idx` in its native dtype, for display (e.g. the
    /// value shown by picking). Integer dtypes render without a decimal point,
    /// avoiding the lossy detour through a float that
    /// [`get_f32`](Self::get_f32) would impose.
    pub fn format_element(&self, idx: usize) -> String {
        match self {
            NumericData::Uint8(v) => v[idx].to_string(),
            NumericData::Uint16(v) => v[idx].to_string(),
            NumericData::Uint32(v) => v[idx].to_string(),
            NumericData::Uint64(v) => v[idx].to_string(),
            NumericData::Int8(v) => v[idx].to_string(),
            NumericData::Int16(v) => v[idx].to_string(),
            NumericData::Int32(v) => v[idx].to_string(),
            NumericData::Int64(v) => v[idx].to_string(),
            NumericData::Float32(v) => v[idx].to_string(),
            NumericData::Float64(v) => v[idx].to_string(),
        }
    }

    /// Convert the entire data array to f32 in one go.
    /// For Float32 data, this borrows the existing slice via bytemuck (zero-copy).
    /// For other dtypes, values are batch-converted to f32 via iterators.
    pub fn as_f32(&self) -> Cow<'_, [f32]> {
        match self {
            NumericData::Float32(v) => Cow::Borrowed(v.as_slice()),
            NumericData::Uint8(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Uint16(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Uint32(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Uint64(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Int8(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Int16(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Int32(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Int64(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
            NumericData::Float64(v) => Cow::Owned(v.iter().map(|&x| x as f32).collect()),
        }
    }

    /// Returns the raw little-endian bytes to upload as a *storage buffer*,
    /// together with the WGSL scalar type the shader should read them as.
    ///
    /// 32-bit dtypes (`f32`/`u32`/`i32`) are uploaded natively (zero-copy),
    /// which both avoids a conversion pass and preserves full precision for
    /// integer data that would otherwise be clobbered by an `as f32` cast.
    /// All other dtypes are converted to `f32` on the CPU. Element size stays
    /// 4 bytes in every case, so the buffer layout is identical to before.
    ///
    /// Retained for potential future use: layers now upload numeric data as a
    /// texture (see [`create_data_texture`](Self::create_data_texture)), which
    /// supports 8/16-bit dtypes at native width, but a storage buffer remains
    /// the right tool for data too large for a texture's dimension limits.
    #[allow(dead_code)]
    pub fn as_gpu_buffer(&self) -> (Cow<'_, [u8]>, WgslScalar) {
        match self {
            NumericData::Float32(v) => (Cow::Borrowed(bytemuck::cast_slice(v)), WgslScalar::F32),
            NumericData::Uint32(v) => (Cow::Borrowed(bytemuck::cast_slice(v)), WgslScalar::U32),
            NumericData::Int32(v) => (Cow::Borrowed(bytemuck::cast_slice(v)), WgslScalar::I32),
            _ => {
                // Convert to f32, then copy into an owned byte buffer (the f32
                // temporary cannot be borrowed past this scope).
                let bytes = bytemuck::cast_slice(&self.as_f32()).to_vec();
                (Cow::Owned(bytes), WgslScalar::F32)
            }
        }
    }

    /// Returns the raw little-endian bytes to upload into an image *texture*,
    /// together with the texture dtype the shader should read them as.
    ///
    /// The flat memory layout is preserved exactly — no reordering — so the
    /// shader can keep indexing with per-dimension strides. 8/16/32-bit dtypes
    /// are borrowed zero-copy and stored on the GPU at their native width.
    ///
    /// WebGPU defines no 64-bit texture formats, so the three 64-bit dtypes are
    /// the only ones that must be narrowed to 32 bits on the CPU (`u64`→`u32`,
    /// `i64`→`i32`, `f64`→`f32`); every other dtype is uploaded as-is.
    pub fn as_texture_data(&self) -> (Cow<'_, [u8]>, TextureDtype) {
        match self {
            NumericData::Uint8(v) => (Cow::Borrowed(bytemuck::cast_slice(v)), TextureDtype::U8),
            NumericData::Uint16(v) => (Cow::Borrowed(bytemuck::cast_slice(v)), TextureDtype::U16),
            NumericData::Uint32(v) => (Cow::Borrowed(bytemuck::cast_slice(v)), TextureDtype::U32),
            NumericData::Int8(v) => (Cow::Borrowed(bytemuck::cast_slice(v)), TextureDtype::I8),
            NumericData::Int16(v) => (Cow::Borrowed(bytemuck::cast_slice(v)), TextureDtype::I16),
            NumericData::Int32(v) => (Cow::Borrowed(bytemuck::cast_slice(v)), TextureDtype::I32),
            NumericData::Float32(v) => (Cow::Borrowed(bytemuck::cast_slice(v)), TextureDtype::F32),
            NumericData::Uint64(v) => {
                let narrowed: Vec<u32> = v.iter().map(|&x| x as u32).collect();
                (Cow::Owned(bytemuck::cast_slice(&narrowed).to_vec()), TextureDtype::U32)
            }
            NumericData::Int64(v) => {
                let narrowed: Vec<i32> = v.iter().map(|&x| x as i32).collect();
                (Cow::Owned(bytemuck::cast_slice(&narrowed).to_vec()), TextureDtype::I32)
            }
            NumericData::Float64(v) => {
                let narrowed: Vec<f32> = v.iter().map(|&x| x as f32).collect();
                (Cow::Owned(bytemuck::cast_slice(&narrowed).to_vec()), TextureDtype::F32)
            }
        }
    }

    /// Upload this array into a single-channel (red-only) 2D texture and return
    /// a view of it, plus the [`TextureDtype`] the shader must read it as (used
    /// to pick the matching bind-group sample type and inject the shader's
    /// sampled type via [`crate::shader_modules::ShaderBuilder`]).
    ///
    /// The flat array is reshaped into rows — element `idx` lives at texel
    /// `(idx % width, idx / width)` — with the width maximized up to the
    /// device's limit so the row count (height) stays small. The data is *not*
    /// reordered, so the shader recomputes the same flat `idx` (e.g. from
    /// per-instance index or per-dimension strides) and maps it back to 2D
    /// texel coordinates. See [`as_texture_data`](Self::as_texture_data) for the
    /// native-width / zero-copy behavior.
    pub fn create_data_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
    ) -> (wgpu::TextureView, TextureDtype) {
        let (data_bytes, dtype) = self.as_texture_data();

        let bytes_per_texel = dtype.bytes_per_texel();
        let num_texels = data_bytes.len() as u32 / bytes_per_texel;
        let max_dim = device.limits().max_texture_dimension_2d;
        let tex_width = num_texels.min(max_dim).max(1);
        let tex_height = num_texels.div_ceil(tex_width).max(1);
        if tex_height > max_dim {
            log(&format!(
                "{label}: data ({num_texels} texels) exceeds the maximum texture \
                 size ({max_dim}x{max_dim}); it will be truncated. Consider \
                 tiling or downsampling.",
            ));
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: tex_width,
                height: tex_height.min(max_dim),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: dtype.texture_format(),
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Upload the tightly-packed data. The full rows go in one copy; any
        // trailing partial row goes in a second copy. (`queue.write_texture`
        // imposes no bytes-per-row alignment, unlike buffer-to-texture copies.)
        // Texels past the end of the data are never indexed by the shader.
        let full_rows = num_texels / tex_width;
        let remainder = num_texels % tex_width;
        if full_rows > 0 {
            queue.write_texture(
                texture.as_image_copy(),
                &data_bytes[..(full_rows * tex_width * bytes_per_texel) as usize],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(tex_width * bytes_per_texel),
                    rows_per_image: Some(full_rows),
                },
                wgpu::Extent3d {
                    width: tex_width,
                    height: full_rows,
                    depth_or_array_layers: 1,
                },
            );
        }
        if remainder > 0 {
            let row_start = (full_rows * tex_width * bytes_per_texel) as usize;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: full_rows, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &data_bytes[row_start..],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(remainder * bytes_per_texel),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: remainder,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (view, dtype)
    }
}
