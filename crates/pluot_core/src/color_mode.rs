//! Shared machinery for turning a [`ColorMode`] into what a layer needs to
//! color its elements, on either the GPU or the CPU.
//!
//! [`prepare_color_mode`] uploads any per-element value arrays as textures and
//! assembles the matching WGSL (bindings + a `get_fill_color` function) from the
//! reusable snippets in [`crate::shader_modules::color`], so every layer can
//! support the full set of color modes without duplicating this logic. The
//! per-mode WGSL lives in `wgsl_functions/color/`, not in Rust string literals.
//!
//! [`cpu_fill_color`] is the CPU-side equivalent, used by the SVG / software
//! render paths.

use crate::colormaps_quantitative;
use crate::colormaps_categorical;
use crate::numeric_data::NumericData;
use crate::render_traits::{ColorMode, QuantitativeParams};
use crate::shader_modules::{color as color_wgsl, stroke_color as stroke_color_wgsl, colormaps as wgsl_colormaps, ShaderBuilder, TextureDtype};
use crate::wgpu;

/// A texture bound for a color mode, paired with the sample type its bind-group
/// layout entry must declare.
pub struct PreparedColorTexture {
    pub view: wgpu::TextureView,
    pub sample_type: wgpu::TextureSampleType,
}

/// Everything a layer needs to render a [`ColorMode`] on the GPU.
///
/// The layer writes [`mode`](Self::mode), [`static_color`](Self::static_color),
/// [`reverse`](Self::reverse) and [`domain`](Self::domain) into its uniform
/// buffer; binds [`textures`](Self::textures) sequentially starting at the
/// `first_binding` passed to [`prepare_color_mode`]; and injects
/// [`wgsl`](Self::wgsl) into its shader's `{{color_module}}` placeholder (along
/// with [`crate::shader_modules::common::FLAT_TEXEL_COORD`] at
/// `{{flat_texel_coord}}`).
pub struct PreparedColorMode {
    /// Discriminant for the shader's `fill_color_mode` uniform (see
    /// [`ColorMode::shader_mode`]).
    pub mode: u32,
    /// Static RGBA color (0..1) used by `UniformRgb`; ignored by other modes.
    pub static_color: [f32; 4],
    /// 1 if the quantitative colormap should be reversed, else 0.
    pub reverse: u32,
    /// (min, max) normalization domain for the quantitative mode.
    pub domain: [f32; 2],
    /// Color value / palette texture(s), in binding order.
    pub textures: Vec<PreparedColorTexture>,
    /// Assembled WGSL: the color texture bindings plus `get_fill_color`.
    pub wgsl: String,
}

/// Prepare the GPU resources and WGSL for a color mode. Value texture(s) are
/// bound consecutively starting at `first_binding`.
pub fn prepare_color_mode(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    color: &ColorMode,
    first_binding: u32,
) -> PreparedColorMode {
    let mut static_color = [0.0f32, 0.0, 0.0, 1.0];
    let mut reverse = 0u32;
    let mut domain = [0.0f32, 1.0];
    let mut textures: Vec<PreparedColorTexture> = Vec::new();

    let wgsl = match color {
        ColorMode::UniformRgb(opt) => {
            if let Some((r, g, b)) = opt {
                static_color = [*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, 1.0];
            }
            color_wgsl::UNIFORM_RGB.to_string()
        }
        ColorMode::InstancedRgb(params) => {
            let (r_view, r_dtype) =
                params.r_values.create_data_texture(device, queue, "fill_color r Texture");
            let (g_view, g_dtype) =
                params.g_values.create_data_texture(device, queue, "fill_color g Texture");
            let (b_view, b_dtype) =
                params.b_values.create_data_texture(device, queue, "fill_color b Texture");
            let wgsl = ShaderBuilder::new(color_wgsl::INSTANCED_RGB)
                .define_u32("color_binding_0", first_binding)
                .define_u32("color_binding_1", first_binding + 1)
                .define_u32("color_binding_2", first_binding + 2)
                .inject_texture_sample_type("color_r", r_dtype)
                .inject_texture_sample_type("color_g", g_dtype)
                .inject_texture_sample_type("color_b", b_dtype)
                .build();
            textures.push(value_texture(r_view, r_dtype));
            textures.push(value_texture(g_view, g_dtype));
            textures.push(value_texture(b_view, b_dtype));
            wgsl
        }
        ColorMode::InstancedRgbInterleaved(params) => {
            let (view, dtype) =
                params.rgb_values.create_data_texture(device, queue, "fill_color rgb Texture");
            let wgsl = ShaderBuilder::new(color_wgsl::INSTANCED_RGB_INTERLEAVED)
                .define_u32("color_binding_0", first_binding)
                .inject_texture_sample_type("color_rgb", dtype)
                .build();
            textures.push(value_texture(view, dtype));
            wgsl
        }
        ColorMode::Categorical(params) => {
            let (view, dtype) =
                params.values.create_data_texture(device, queue, "fill_color labels Texture");
            let palette: Vec<[f32; 4]> = colormaps_categorical::palette(params.colormap).to_vec();
            let palette_view = create_palette_texture(device, queue, &palette);
            let wgsl = categorical_wgsl(first_binding, dtype);
            textures.push(value_texture(view, dtype));
            textures.push(palette_texture(palette_view));
            wgsl
        }
        ColorMode::CategoricalCustom(params) => {
            let (view, dtype) =
                params.values.create_data_texture(device, queue, "fill_color labels Texture");
            let palette: Vec<[f32; 4]> = params
                .colormap
                .iter()
                .map(|(r, g, b)| [*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, 1.0])
                .collect();
            let palette_view = create_palette_texture(device, queue, &palette);
            let wgsl = categorical_wgsl(first_binding, dtype);
            textures.push(value_texture(view, dtype));
            textures.push(palette_texture(palette_view));
            wgsl
        }
        ColorMode::Quantitative(params) => {
            let (view, dtype) =
                params.values.create_data_texture(device, queue, "fill_color values Texture");
            reverse = if params.reverse { 1 } else { 0 };
            domain = quantitative_domain(params);
            let (cmap_src, cmap_name) = wgsl_colormaps::wgsl_source_and_name(params.colormap);
            let wgsl = ShaderBuilder::new(color_wgsl::QUANTITATIVE)
                .define_u32("color_binding_0", first_binding)
                .inject_texture_sample_type("color_values", dtype)
                .inject_function("colormap_fn_source", cmap_src)
                .define("colormap_fn_name", cmap_name)
                .build();
            textures.push(value_texture(view, dtype));
            wgsl
        }
    };

    PreparedColorMode {
        mode: color.shader_mode(),
        static_color,
        reverse,
        domain,
        textures,
        wgsl,
    }
}

/// Assemble the WGSL for either categorical mode (they share a template): the
/// labels texture at `first_binding`, the palette texture at `first_binding + 1`.
fn categorical_wgsl(first_binding: u32, labels_dtype: TextureDtype) -> String {
    ShaderBuilder::new(color_wgsl::CATEGORICAL)
        .define_u32("color_binding_0", first_binding)
        .define_u32("color_binding_1", first_binding + 1)
        .inject_texture_sample_type("color_labels", labels_dtype)
        .build()
}

/// Stroke-color counterpart of [`prepare_color_mode`], for layers that stroke
/// rather than fill (e.g. `LineLayer`). Identical logic, but assembles the
/// `stroke_color` WGSL variants (defining `get_stroke_color` and reading the
/// `stroke_color*` uniforms). Value texture(s) are bound consecutively starting
/// at `first_binding`.
pub fn prepare_stroke_color(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    color: &ColorMode,
    first_binding: u32,
) -> PreparedColorMode {
    let mut static_color = [0.0f32, 0.0, 0.0, 1.0];
    let mut reverse = 0u32;
    let mut domain = [0.0f32, 1.0];
    let mut textures: Vec<PreparedColorTexture> = Vec::new();

    let wgsl = match color {
        ColorMode::UniformRgb(opt) => {
            if let Some((r, g, b)) = opt {
                static_color = [*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, 1.0];
            }
            stroke_color_wgsl::UNIFORM_RGB.to_string()
        }
        ColorMode::InstancedRgb(params) => {
            let (r_view, r_dtype) =
                params.r_values.create_data_texture(device, queue, "stroke_color r Texture");
            let (g_view, g_dtype) =
                params.g_values.create_data_texture(device, queue, "stroke_color g Texture");
            let (b_view, b_dtype) =
                params.b_values.create_data_texture(device, queue, "stroke_color b Texture");
            let wgsl = ShaderBuilder::new(stroke_color_wgsl::INSTANCED_RGB)
                .define_u32("color_binding_0", first_binding)
                .define_u32("color_binding_1", first_binding + 1)
                .define_u32("color_binding_2", first_binding + 2)
                .inject_texture_sample_type("color_r", r_dtype)
                .inject_texture_sample_type("color_g", g_dtype)
                .inject_texture_sample_type("color_b", b_dtype)
                .build();
            textures.push(value_texture(r_view, r_dtype));
            textures.push(value_texture(g_view, g_dtype));
            textures.push(value_texture(b_view, b_dtype));
            wgsl
        }
        ColorMode::InstancedRgbInterleaved(params) => {
            let (view, dtype) =
                params.rgb_values.create_data_texture(device, queue, "stroke_color rgb Texture");
            let wgsl = ShaderBuilder::new(stroke_color_wgsl::INSTANCED_RGB_INTERLEAVED)
                .define_u32("color_binding_0", first_binding)
                .inject_texture_sample_type("color_rgb", dtype)
                .build();
            textures.push(value_texture(view, dtype));
            wgsl
        }
        ColorMode::Categorical(params) => {
            let (view, dtype) =
                params.values.create_data_texture(device, queue, "stroke_color labels Texture");
            let palette: Vec<[f32; 4]> = colormaps_categorical::palette(params.colormap).to_vec();
            let palette_view = create_palette_texture(device, queue, &palette);
            let wgsl = categorical_stroke_wgsl(first_binding, dtype);
            textures.push(value_texture(view, dtype));
            textures.push(palette_texture(palette_view));
            wgsl
        }
        ColorMode::CategoricalCustom(params) => {
            let (view, dtype) =
                params.values.create_data_texture(device, queue, "stroke_color labels Texture");
            let palette: Vec<[f32; 4]> = params
                .colormap
                .iter()
                .map(|(r, g, b)| [*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, 1.0])
                .collect();
            let palette_view = create_palette_texture(device, queue, &palette);
            let wgsl = categorical_stroke_wgsl(first_binding, dtype);
            textures.push(value_texture(view, dtype));
            textures.push(palette_texture(palette_view));
            wgsl
        }
        ColorMode::Quantitative(params) => {
            let (view, dtype) =
                params.values.create_data_texture(device, queue, "stroke_color values Texture");
            reverse = if params.reverse { 1 } else { 0 };
            domain = quantitative_domain(params);
            let (cmap_src, cmap_name) = wgsl_colormaps::wgsl_source_and_name(params.colormap);
            let wgsl = ShaderBuilder::new(stroke_color_wgsl::QUANTITATIVE)
                .define_u32("color_binding_0", first_binding)
                .inject_texture_sample_type("color_values", dtype)
                .inject_function("colormap_fn_source", cmap_src)
                .define("colormap_fn_name", cmap_name)
                .build();
            textures.push(value_texture(view, dtype));
            wgsl
        }
    };

    PreparedColorMode {
        mode: color.shader_mode(),
        static_color,
        reverse,
        domain,
        textures,
        wgsl,
    }
}

/// Stroke-color counterpart of [`categorical_wgsl`].
fn categorical_stroke_wgsl(first_binding: u32, labels_dtype: TextureDtype) -> String {
    ShaderBuilder::new(stroke_color_wgsl::CATEGORICAL)
        .define_u32("color_binding_0", first_binding)
        .define_u32("color_binding_1", first_binding + 1)
        .inject_texture_sample_type("color_labels", labels_dtype)
        .build()
}

fn value_texture(view: wgpu::TextureView, dtype: TextureDtype) -> PreparedColorTexture {
    PreparedColorTexture { view, sample_type: dtype.binding_sample_type() }
}

fn palette_texture(view: wgpu::TextureView) -> PreparedColorTexture {
    // Palettes are uploaded as Rgba32Float and read via textureLoad (no sampler).
    PreparedColorTexture { view, sample_type: wgpu::TextureSampleType::Float { filterable: false } }
}

/// Upload a palette as a 1-row `Rgba32Float` texture, one texel per color, and
/// return a view of it. WGSL textures cannot have zero width, so an empty
/// palette falls back to a single opaque-black texel.
fn create_palette_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    colors: &[[f32; 4]],
) -> wgpu::TextureView {
    let fallback = [[0.0f32, 0.0, 0.0, 1.0]];
    let colors: &[[f32; 4]] = if colors.is_empty() { &fallback } else { colors };
    let width = colors.len() as u32;

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fill_color palette Texture"),
        size: wgpu::Extent3d { width, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        bytemuck::cast_slice(colors),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 16), // 4 channels * 4 bytes (f32)
            rows_per_image: Some(1),
        },
        wgpu::Extent3d { width, height: 1, depth_or_array_layers: 1 },
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// The (min, max) normalization domain for a quantitative color mode: the
/// caller-supplied domain if present, otherwise the data's own min/max (falling
/// back to (0, 1) for empty or non-finite data).
pub fn quantitative_domain(params: &QuantitativeParams) -> [f32; 2] {
    if let Some((lo, hi)) = params.domain {
        return [lo, hi];
    }
    let values = params.values.as_f32();
    let (min, max) = values
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(mn, mx), &v| (mn.min(v), mx.max(v)));
    if min.is_finite() && max.is_finite() {
        [min, max]
    } else {
        [0.0, 1.0]
    }
}

fn to_u8(channel: f32) -> u8 {
    (channel * 255.0).round().clamp(0.0, 255.0) as u8
}

fn label_index(values: &NumericData, index: usize, len: usize) -> usize {
    let raw = values.get_f32(index) as i64;
    let len = len.max(1) as i64;
    (raw.rem_euclid(len)) as usize
}

/// Resolve the fill color of element `index` on the CPU, as an `(r, g, b)`
/// triple. `quant_domain` is the normalization domain for the quantitative mode
/// (see [`quantitative_domain`]); it is ignored by the other modes.
pub fn cpu_fill_color(color: &ColorMode, index: usize, quant_domain: [f32; 2]) -> (u8, u8, u8) {
    match color {
        ColorMode::UniformRgb(opt) => opt.unwrap_or((0, 0, 0)),
        ColorMode::InstancedRgb(params) => (
            to_u8(params.r_values.get_f32(index) / 255.0),
            to_u8(params.g_values.get_f32(index) / 255.0),
            to_u8(params.b_values.get_f32(index) / 255.0),
        ),
        ColorMode::InstancedRgbInterleaved(params) => {
            let base = index * 3;
            (
                to_u8(params.rgb_values.get_f32(base) / 255.0),
                to_u8(params.rgb_values.get_f32(base + 1) / 255.0),
                to_u8(params.rgb_values.get_f32(base + 2) / 255.0),
            )
        }
        ColorMode::Categorical(params) => {
            let palette = colormaps_categorical::palette(params.colormap);
            let rgba = palette[label_index(&params.values, index, palette.len())];
            (to_u8(rgba[0]), to_u8(rgba[1]), to_u8(rgba[2]))
        }
        ColorMode::CategoricalCustom(params) => {
            if params.colormap.is_empty() {
                return (0, 0, 0);
            }
            params.colormap[label_index(&params.values, index, params.colormap.len())]
        }
        ColorMode::Quantitative(params) => {
            let [lo, hi] = quant_domain;
            let mut x = ((params.values.get_f32(index) - lo) / (hi - lo).max(1e-20)).clamp(0.0, 1.0);
            if params.reverse {
                x = 1.0 - x;
            }
            let rgba = colormaps_quantitative::sample(params.colormap, x);
            (to_u8(rgba[0]), to_u8(rgba[1]), to_u8(rgba[2]))
        }
    }
}
