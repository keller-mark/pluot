//! Regression tests for the `ShaderBuilder` usage performed by
//! `BitmaskLayer` (see `crate::layers::bitmask_layer`): each `ColorMode`
//! template under `wgsl_functions/get_channel_color/`, the per-channel
//! dispatch template under `wgsl_functions/bitmask/`, and the full outer
//! `bitmask_layer.wgsl` assembly.
//!
//! These assert exact string equality against golden output rather than just
//! "does it compile", specifically to catch the class of bug that motivated
//! this file: a `.wgsl` doc comment mentioning a template placeholder (e.g.
//! `` `{{ch}}` ``) literally gets matched and substituted too, since
//! `ShaderBuilder` does plain text substitution with no awareness of
//! comments -- in one case this silently spliced multi-line generated code
//! into a `//` comment, breaking the surrounding shader. `ShaderBuilder`'s
//! own `build()` catches *unsubstituted* placeholders via a `debug_assert`,
//! but not corruption from a placeholder that *does* get replaced somewhere
//! unintended -- only an exact-output comparison catches that.

use pluot_core::shader_modules::{bitmask_channel, common, get_channel_color, ShaderBuilder, TextureDtype};

/// Trims trailing whitespace from each line before comparing.
///
/// A `{{placeholder}}` substituted with an empty string (e.g. `switch_cases`
/// with zero channels) leaves its surrounding indentation behind as a
/// whitespace-only line; that trailing whitespace carries no semantic
/// meaning and is exactly the kind of thing editors/formatters routinely
/// strip from checked-in source, so comparing raw strings here would make
/// these regression tests fail on formatting noise rather than a real
/// template change. Mirrors `pluot`'s `check_svg_snapshot`/
/// `check_text_snapshot` normalization for the same reason.
fn normalize(s: &str) -> String {
    s.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n")
}

fn assert_wgsl_eq(actual: &str, expected: &str) {
    assert_eq!(normalize(actual), normalize(expected));
}

/// Mirrors the `switch_cases` construction in `BitmaskLayer::draw` exactly
/// (same format string, same join separator), so tests here build the
/// dispatch switch identically to production.
fn switch_cases(n: usize) -> String {
    (0..n)
        .map(|i| format!("case {i}u: {{ return get_channel_color_{i}(label_index); }}"))
        .collect::<Vec<_>>()
        .join("\n        ")
}

#[test]
fn uniform_rgb_template_matches_expected() {
    // Mirrors `prepare_channel_color`'s `None` / `ColorMode::UniformRgb` arm.
    let actual = ShaderBuilder::new(get_channel_color::UNIFORM_RGB)
        .define("ch", "0")
        .build();
    let expected = "\
// BitmaskLayer per-channel ColorMode::UniformRgb (and None) — every object in
// this channel shares the static color from the uniform.
fn get_channel_color_0(label_index: u32) -> vec3<f32> {
  return u.channels[0].static_color.rgb;
}
";
    assert_wgsl_eq(&actual, expected);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
}

#[test]
fn instanced_rgb_template_matches_expected() {
    // Mirrors `prepare_channel_color`'s `ColorMode::InstancedRgb` arm.
    let actual = ShaderBuilder::new(get_channel_color::INSTANCED_RGB)
        .define("ch", "1")
        .define_bidx("r", 5)
        .define_bidx("g", 6)
        .define_bidx("b", 7)
        .inject_texture_sample_type("r", TextureDtype::U8)
        .inject_texture_sample_type("g", TextureDtype::U8)
        .inject_texture_sample_type("b", TextureDtype::U8)
        .build();
    let expected = "\
// BitmaskLayer per-channel ColorMode::InstancedRgb — per-object RGB from
// three parallel value textures, indexed by object id (`label_index`).
// Depends on `flat_texel_coord` being injected.
@group(0) @binding(5) var channel_color_r_1: texture_2d<u32>;
@group(0) @binding(6) var channel_color_g_1: texture_2d<u32>;
@group(0) @binding(7) var channel_color_b_1: texture_2d<u32>;

fn get_channel_color_1(label_index: u32) -> vec3<f32> {
  let r = f32(textureLoad(channel_color_r_1, flat_texel_coord(label_index, textureDimensions(channel_color_r_1).x), 0).x) / 255.0;
  let g = f32(textureLoad(channel_color_g_1, flat_texel_coord(label_index, textureDimensions(channel_color_g_1).x), 0).x) / 255.0;
  let b = f32(textureLoad(channel_color_b_1, flat_texel_coord(label_index, textureDimensions(channel_color_b_1).x), 0).x) / 255.0;
  return vec3<f32>(r, g, b);
}
";
    assert_wgsl_eq(&actual, expected);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
}

#[test]
fn instanced_rgb_interleaved_template_matches_expected() {
    // Mirrors `prepare_channel_color`'s `ColorMode::InstancedRgbInterleaved` arm.
    let actual = ShaderBuilder::new(get_channel_color::INSTANCED_RGB_INTERLEAVED)
        .define("ch", "2")
        .define_bidx("rgb", 8)
        .inject_texture_sample_type("rgb", TextureDtype::U8)
        .build();
    let expected = "\
// BitmaskLayer per-channel ColorMode::InstancedRgbInterleaved — per-object
// RGB from one interleaved value texture, indexed by object id
// (`label_index`). Depends on `flat_texel_coord` being injected.
@group(0) @binding(8) var channel_color_rgb_2: texture_2d<u32>;

fn get_channel_color_2(label_index: u32) -> vec3<f32> {
  let w = textureDimensions(channel_color_rgb_2).x;
  let base = label_index * 3u;
  let r = f32(textureLoad(channel_color_rgb_2, flat_texel_coord(base, w), 0).x) / 255.0;
  let g = f32(textureLoad(channel_color_rgb_2, flat_texel_coord(base + 1u, w), 0).x) / 255.0;
  let b = f32(textureLoad(channel_color_rgb_2, flat_texel_coord(base + 2u, w), 0).x) / 255.0;
  return vec3<f32>(r, g, b);
}
";
    assert_wgsl_eq(&actual, expected);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
}

#[test]
fn categorical_template_matches_expected() {
    // Mirrors `prepare_channel_color`'s `ColorMode::Categorical` arm (and,
    // identically, its `CategoricalCustom` arm -- both share this template).
    let actual = ShaderBuilder::new(get_channel_color::CATEGORICAL)
        .define("ch", "3")
        .define_bidx("labels", 9)
        .define_bidx("palette", 10)
        .inject_texture_sample_type("labels", TextureDtype::U8)
        .build();
    let expected = "\
// BitmaskLayer per-channel ColorMode::Categorical / CategoricalCustom — an
// integer \"set color\" index per object, indexed by object id (`label_index`)
// against a palette uploaded as a 1-row RGBA texture. The index wraps around
// (modulo) the palette length, handling negative values. Depends on
// `flat_texel_coord` being injected.
@group(0) @binding(9) var channel_color_labels_3: texture_2d<u32>;
@group(0) @binding(10) var channel_color_palette_3: texture_2d<f32>;

fn get_channel_color_3(label_index: u32) -> vec3<f32> {
  let raw = i32(textureLoad(channel_color_labels_3, flat_texel_coord(label_index, textureDimensions(channel_color_labels_3).x), 0).x);
  let n = i32(textureDimensions(channel_color_palette_3).x);
  let idx = u32(((raw % n) + n) % n);
  return textureLoad(channel_color_palette_3, vec2<u32>(idx, 0u), 0).rgb;
}
";
    assert_wgsl_eq(&actual, expected);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
}

#[test]
fn quantitative_template_matches_expected() {
    // Mirrors `prepare_channel_color`'s `ColorMode::Quantitative` arm.
    let actual = ShaderBuilder::new(get_channel_color::QUANTITATIVE)
        .define("ch", "4")
        .define_bidx("values", 11)
        .inject_texture_sample_type("values", TextureDtype::F32)
        .define("colormap_fn_name", "viridis")
        .build();
    let expected = "\
// BitmaskLayer per-channel ColorMode::Quantitative — a per-object scalar
// feature value, indexed by object id (`label_index`), normalized into 0-1
// using the channel's (min, max) domain, then mapped through a continuous
// colormap. The colormap function's source and name are injected by
// ShaderBuilder as placeholders below (not spelled out here, to avoid the
// literal placeholder text itself being matched and substituted). Depends on
// `flat_texel_coord` being injected.
@group(0) @binding(11) var channel_color_values_4: texture_2d<f32>;

fn get_channel_color_4(label_index: u32) -> vec3<f32> {
  var x = f32(textureLoad(channel_color_values_4, flat_texel_coord(label_index, textureDimensions(channel_color_values_4).x), 0).x);
  let lo = u.channels[4].color_domain.x;
  let hi = u.channels[4].color_domain.y;
  x = clamp((x - lo) / max(hi - lo, 1e-20), 0.0, 1.0);
  if (u.channels[4].color_reverse == 1u) {
    x = 1.0 - x;
  }
  return viridis(x).rgb;
}
";
    assert_wgsl_eq(&actual, expected);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
}

#[test]
fn channel_color_dispatch_with_no_channels_matches_expected() {
    let actual = ShaderBuilder::new(bitmask_channel::CHANNEL_COLOR_DISPATCH)
        .define("switch_cases", &switch_cases(0))
        .build();
    let expected = "\
// Dispatches to the correct per-channel `get_channel_color_N`. Each channel
// may use a different `ColorMode` (and therefore a different generated
// function/set of texture bindings -- WGSL has no runtime function-pointer
// indirection), so this switch is generated once per draw call, sized to the
// actual channel count (see `crate::layers::bitmask_layer::draw`). Depends on
// the per-channel `get_channel_color_N` functions (see `get_channel_color`)
// also being injected. Template: the switch's case list below is substituted
// in with one \"case N: return get_channel_color_N(label_index);\" per channel.
fn get_channel_color(channel_index: u32, label_index: u32) -> vec3<f32> {
    switch (channel_index) {

        default: { return vec3<f32>(0.0, 0.0, 0.0); }
    }
}
";
    assert_wgsl_eq(&actual, expected);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
}

#[test]
fn channel_color_dispatch_with_three_channels_matches_expected() {
    let actual = ShaderBuilder::new(bitmask_channel::CHANNEL_COLOR_DISPATCH)
        .define("switch_cases", &switch_cases(3))
        .build();
    let expected = "\
// Dispatches to the correct per-channel `get_channel_color_N`. Each channel
// may use a different `ColorMode` (and therefore a different generated
// function/set of texture bindings -- WGSL has no runtime function-pointer
// indirection), so this switch is generated once per draw call, sized to the
// actual channel count (see `crate::layers::bitmask_layer::draw`). Depends on
// the per-channel `get_channel_color_N` functions (see `get_channel_color`)
// also being injected. Template: the switch's case list below is substituted
// in with one \"case N: return get_channel_color_N(label_index);\" per channel.
fn get_channel_color(channel_index: u32, label_index: u32) -> vec3<f32> {
    switch (channel_index) {
        case 0u: { return get_channel_color_0(label_index); }
        case 1u: { return get_channel_color_1(label_index); }
        case 2u: { return get_channel_color_2(label_index); }
        default: { return vec3<f32>(0.0, 0.0, 0.0); }
    }
}
";
    assert_wgsl_eq(&actual, expected);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
}

#[test]
fn channel_sample_and_is_edge_are_not_templated() {
    // Unlike `get_channel_color`'s per-`ColorMode` snippets, these two are
    // ordinary WGSL functions parameterized by `channel_index` -- injected
    // once via `ShaderBuilder::inject_function` regardless of channel count,
    // with no `{{...}}` placeholders (and no per-channel `ch` substitution)
    // of their own.
    assert!(!bitmask_channel::CHANNEL_SAMPLE.contains("{{"));
    assert!(!bitmask_channel::CHANNEL_IS_EDGE.contains("{{"));
    assert!(bitmask_channel::CHANNEL_SAMPLE.contains("fn bitmask_sample("));
    assert!(bitmask_channel::CHANNEL_IS_EDGE.contains("fn bitmask_is_edge("));
}

/// Full outer-shader assembly, replicating `BitmaskLayer::draw`'s
/// `ShaderBuilder` chain exactly (same injected functions, same `.define()`
/// calls) for a representative 2-channel configuration: channel 0 uses
/// `ColorMode::UniformRgb` (or `None`, which builds identically), channel 1
/// uses `ColorMode::Quantitative` with the `viridis` colormap. Compared
/// against a checked-in golden file (`tests/snaps/`) rather than an inline
/// literal, since the fully assembled shader is large; update that file
/// (after visually verifying the diff) if a deliberate template change
/// requires it.
#[test]
fn full_shader_assembly_matches_snapshot() {
    let channel_color_functions = format!(
        "{}\n{}",
        ShaderBuilder::new(get_channel_color::UNIFORM_RGB).define("ch", "0").build(),
        ShaderBuilder::new(get_channel_color::QUANTITATIVE)
            .define("ch", "1")
            .define_bidx("values", 2)
            .inject_texture_sample_type("values", TextureDtype::F32)
            .define("colormap_fn_name", "viridis")
            .build(),
    );
    let colormap_functions = pluot_core::shader_modules::colormaps::VIRIDIS.to_string();
    let channel_color_dispatch = ShaderBuilder::new(bitmask_channel::CHANNEL_COLOR_DISPATCH)
        .define("switch_cases", &switch_cases(2))
        .build();

    let shader_source = ShaderBuilder::new(include_str!("../src/layers/shaders/bitmask_layer.wgsl"))
        .inject_function("scale", common::SCALE)
        .inject_function("translate", common::TRANSLATE)
        .inject_function("get_aspect_ratio_mat", common::GET_ASPECT_RATIO_MAT)
        .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
        .inject_texture_sample_type("mask_data", TextureDtype::U32)
        .inject_function("bitmask_sample", bitmask_channel::CHANNEL_SAMPLE)
        .inject_function("bitmask_is_edge", bitmask_channel::CHANNEL_IS_EDGE)
        .define("colormap_functions", &colormap_functions)
        .define("channel_color_functions", &channel_color_functions)
        .define("channel_color_dispatch", &channel_color_dispatch)
        .build();

    assert!(!shader_source.contains("{{"), "no placeholder should remain unsubstituted");

    let expected = include_str!("snaps-blessed/bitmask_layer_shader_two_channels.wgsl");
    assert_eq!(
        normalize(&shader_source), normalize(expected),
        "Assembled bitmask_layer.wgsl shader source no longer matches the golden \
         snapshot at tests/snaps-blessed/bitmask_layer_shader_two_channels.wgsl. If this \
         change is intentional, review the diff carefully (e.g. by writing \
         `shader_source` to that file) and update the snapshot.",
    );
}
