//! Regression tests for the `ShaderBuilder` usage performed by
//! `BitmaskLayer` (see `crate::layers::bitmask_layer`): each `ColorMode`
//! template under `wgsl_functions/get_channel_color/`, each
//! `SizeMode`/`OpacityMode` template under `wgsl_functions/get_channel_scalar/`,
//! the per-channel dispatch templates under `wgsl_functions/bitmask/`, and the
//! full outer `bitmask_layer.wgsl` assembly.
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

use pluot_core::shader_modules::{
    bitmask_channel, common, get_channel_color, get_channel_scalar, ShaderBuilder, TextureDtype,
};

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

/// Compare assembled WGSL against a checked-in golden file, for output too
/// large to keep inline as a literal.
///
/// Writes the current output to `tests/snaps-dirty/<name>` and compares it
/// against `tests/snaps-blessed/<name>`, panicking with blessing instructions
/// on mismatch -- the same pattern `pluot`'s raster/vector/script snapshot
/// tests use (see `crates/pluot/tests/test_utils/snapshot_utils.rs`).
fn check_wgsl_snapshot(actual: &str, name: &str) {
    let tests_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let blessed_path = tests_dir.join("snaps-blessed").join(name);
    let dirty_path = tests_dir.join("snaps-dirty").join(name);

    // Always write the current output so it can be inspected / blessed.
    std::fs::create_dir_all(dirty_path.parent().unwrap()).unwrap();
    std::fs::write(&dirty_path, actual).unwrap();

    let expected = std::fs::read_to_string(&blessed_path).unwrap_or_default();
    if normalize(actual) != normalize(&expected) {
        panic!(
            "Assembled WGSL no longer matches the golden snapshot for '{name}'.\n\
             Current output: {dirty}\n\
             Reference snapshot: {blessed}\n\
             If this change is intentional, review the diff carefully and accept it with:\n  \
             cp {dirty} {blessed}",
            dirty = dirty_path.display(),
            blessed = blessed_path.display(),
        );
    }
}

/// Mirrors the `switch_cases` construction in `channel_dispatch` exactly (same
/// format string, same join separator), so tests here build the dispatch switch
/// identically to production.
fn switch_cases(name: &str, n: usize) -> String {
    (0..n)
        .map(|i| format!("case {i}u: {{ return get_channel_{name}_{i}(label_index); }}"))
        .collect::<Vec<_>>()
        .join("\n        ")
}

/// Mirrors `channel_dispatch` in `BitmaskLayer`'s module.
fn channel_dispatch(template: &str, name: &str, n: usize) -> String {
    ShaderBuilder::new(template)
        .define("name", name)
        .define("switch_cases", &switch_cases(name, n))
        .build()
}

#[test]
fn uniform_rgb_template_matches_expected() {
    // Mirrors `prepare_channel_color`'s `None` / `ColorMode::UniformRgb` arm.
    let actual = ShaderBuilder::new(get_channel_color::UNIFORM_RGB)
        .define("name", "fill_color")
        .define("ch", "0")
        .build();
    let expected = "\
// BitmaskLayer per-channel ColorMode::UniformRgb (and None) — every object in
// this channel shares the static color from the uniform. Templated per
// (channel, fill/stroke) pair, hence the two-part function name.
fn get_channel_fill_color_0(label_index: u32) -> vec3<f32> {
  return u.channels[0].fill_color_static.rgb;
}
";
    assert_wgsl_eq(&actual, expected);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
}

#[test]
fn instanced_rgb_template_matches_expected() {
    // Mirrors `prepare_channel_color`'s `ColorMode::InstancedRgb` arm.
    let actual = ShaderBuilder::new(get_channel_color::INSTANCED_RGB)
        .define("name", "stroke_color")
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
@group(0) @binding(5) var channel_stroke_color_r_1: texture_2d<u32>;
@group(0) @binding(6) var channel_stroke_color_g_1: texture_2d<u32>;
@group(0) @binding(7) var channel_stroke_color_b_1: texture_2d<u32>;

fn get_channel_stroke_color_1(label_index: u32) -> vec3<f32> {
  let r = f32(textureLoad(channel_stroke_color_r_1, flat_texel_coord(label_index, textureDimensions(channel_stroke_color_r_1).x), 0).x) / 255.0;
  let g = f32(textureLoad(channel_stroke_color_g_1, flat_texel_coord(label_index, textureDimensions(channel_stroke_color_g_1).x), 0).x) / 255.0;
  let b = f32(textureLoad(channel_stroke_color_b_1, flat_texel_coord(label_index, textureDimensions(channel_stroke_color_b_1).x), 0).x) / 255.0;
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
        .define("name", "fill_color")
        .define("ch", "2")
        .define_bidx("rgb", 8)
        .inject_texture_sample_type("rgb", TextureDtype::U8)
        .build();
    let expected = "\
// BitmaskLayer per-channel ColorMode::InstancedRgbInterleaved — per-object
// RGB from one interleaved value texture, indexed by object id
// (`label_index`). Depends on `flat_texel_coord` being injected.
@group(0) @binding(8) var channel_fill_color_rgb_2: texture_2d<u32>;

fn get_channel_fill_color_2(label_index: u32) -> vec3<f32> {
  let w = textureDimensions(channel_fill_color_rgb_2).x;
  let base = label_index * 3u;
  let r = f32(textureLoad(channel_fill_color_rgb_2, flat_texel_coord(base, w), 0).x) / 255.0;
  let g = f32(textureLoad(channel_fill_color_rgb_2, flat_texel_coord(base + 1u, w), 0).x) / 255.0;
  let b = f32(textureLoad(channel_fill_color_rgb_2, flat_texel_coord(base + 2u, w), 0).x) / 255.0;
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
        .define("name", "fill_color")
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
@group(0) @binding(9) var channel_fill_color_labels_3: texture_2d<u32>;
@group(0) @binding(10) var channel_fill_color_palette_3: texture_2d<f32>;

fn get_channel_fill_color_3(label_index: u32) -> vec3<f32> {
  let raw = i32(textureLoad(channel_fill_color_labels_3, flat_texel_coord(label_index, textureDimensions(channel_fill_color_labels_3).x), 0).x);
  let n = i32(textureDimensions(channel_fill_color_palette_3).x);
  let idx = u32(((raw % n) + n) % n);
  return textureLoad(channel_fill_color_palette_3, vec2<u32>(idx, 0u), 0).rgb;
}
";
    assert_wgsl_eq(&actual, expected);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
}

#[test]
fn quantitative_template_matches_expected() {
    // Mirrors `prepare_channel_color`'s `ColorMode::Quantitative` arm.
    let actual = ShaderBuilder::new(get_channel_color::QUANTITATIVE)
        .define("name", "stroke_color")
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
@group(0) @binding(11) var channel_stroke_color_values_4: texture_2d<f32>;

fn get_channel_stroke_color_4(label_index: u32) -> vec3<f32> {
  var x = f32(textureLoad(channel_stroke_color_values_4, flat_texel_coord(label_index, textureDimensions(channel_stroke_color_values_4).x), 0).x);
  let lo = u.channels[4].stroke_color_domain.x;
  let hi = u.channels[4].stroke_color_domain.y;
  x = clamp((x - lo) / max(hi - lo, 1e-20), 0.0, 1.0);
  if (u.channels[4].stroke_color_reverse == 1u) {
    x = 1.0 - x;
  }
  return viridis(x).rgb;
}
";
    assert_wgsl_eq(&actual, expected);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
}

#[test]
fn uniform_scalar_template_matches_expected() {
    // Mirrors `prepare_channel_scalar`'s uniform (no instanced values) arm.
    let actual = ShaderBuilder::new(get_channel_scalar::UNIFORM)
        .define("name", "stroke_width")
        .define("ch", "0")
        .build();
    let expected = "\
// BitmaskLayer per-channel SizeMode::UniformSize / OpacityMode::UniformOpacity
// (and None) — every object in this channel shares the static value from the
// uniform, whose field is named after the property being resolved. Templated
// per (channel, property) pair, hence the two-part function name.
fn get_channel_stroke_width_0(label_index: u32) -> f32 {
  return u.channels[0].stroke_width;
}
";
    assert_wgsl_eq(&actual, expected);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
}

#[test]
fn instanced_scalar_template_matches_expected() {
    // Mirrors `prepare_channel_scalar`'s instanced arm.
    let actual = ShaderBuilder::new(get_channel_scalar::INSTANCED)
        .define("name", "fill_opacity")
        .define("ch", "1")
        .define_bidx("values", 12)
        .inject_texture_sample_type("values", TextureDtype::F32)
        .build();
    let expected = "\
// BitmaskLayer per-channel SizeMode::InstancedSize /
// OpacityMode::InstancedOpacity — one value per object, read from a value
// texture indexed by object id (`label_index`). Objects past the end of the
// array read the texture's zero padding, i.e. no stroke / full transparency.
// Depends on `flat_texel_coord` being injected.
@group(0) @binding(12) var channel_fill_opacity_values_1: texture_2d<f32>;

fn get_channel_fill_opacity_1(label_index: u32) -> f32 {
  return f32(textureLoad(channel_fill_opacity_values_1, flat_texel_coord(label_index, textureDimensions(channel_fill_opacity_values_1).x), 0).x);
}
";
    assert_wgsl_eq(&actual, expected);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
}

#[test]
fn channel_color_dispatch_with_no_channels_matches_expected() {
    let actual = channel_dispatch(bitmask_channel::CHANNEL_COLOR_DISPATCH, "fill_color", 0);
    let expected = "\
// Dispatches one of the per-channel color getters (the channel's fill color or
// its stroke color) to the function matching `channel_index`. Each channel may
// use a different `ColorMode` (and therefore a different generated function/set
// of texture bindings -- WGSL has no runtime function-pointer indirection), so
// this switch is generated once per draw call, sized to the actual channel
// count (see `crate::layers::bitmask_layer::draw`). Depends on the matching
// per-channel functions (see `get_channel_color`) also being injected.
// Template: the switch's case list below is substituted in with one case per
// channel, returning that channel's generated getter.
fn get_channel_fill_color(channel_index: u32, label_index: u32) -> vec3<f32> {
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
    let actual = channel_dispatch(bitmask_channel::CHANNEL_COLOR_DISPATCH, "stroke_color", 3);
    let expected = "\
// Dispatches one of the per-channel color getters (the channel's fill color or
// its stroke color) to the function matching `channel_index`. Each channel may
// use a different `ColorMode` (and therefore a different generated function/set
// of texture bindings -- WGSL has no runtime function-pointer indirection), so
// this switch is generated once per draw call, sized to the actual channel
// count (see `crate::layers::bitmask_layer::draw`). Depends on the matching
// per-channel functions (see `get_channel_color`) also being injected.
// Template: the switch's case list below is substituted in with one case per
// channel, returning that channel's generated getter.
fn get_channel_stroke_color(channel_index: u32, label_index: u32) -> vec3<f32> {
    switch (channel_index) {
        case 0u: { return get_channel_stroke_color_0(label_index); }
        case 1u: { return get_channel_stroke_color_1(label_index); }
        case 2u: { return get_channel_stroke_color_2(label_index); }
        default: { return vec3<f32>(0.0, 0.0, 0.0); }
    }
}
";
    assert_wgsl_eq(&actual, expected);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
}

#[test]
fn channel_scalar_dispatch_with_two_channels_matches_expected() {
    let actual = channel_dispatch(bitmask_channel::CHANNEL_SCALAR_DISPATCH, "stroke_opacity", 2);
    let expected = "\
// Scalar counterpart of `channel_color_dispatch`: dispatches one of the
// per-channel scalar getters (fill opacity, stroke opacity or stroke width) to
// the function matching `channel_index`. Generated once per draw call per
// property, sized to the actual channel count, because each channel resolves
// the property through its own `SizeMode`/`OpacityMode` (see
// `crate::layers::bitmask_layer::draw`). Depends on the matching per-channel
// functions (see `get_channel_scalar`) also being injected. Template: the
// switch's case list below is substituted in with one case per channel,
// returning that channel's generated getter.
fn get_channel_stroke_opacity(channel_index: u32, label_index: u32) -> f32 {
    switch (channel_index) {
        case 0u: { return get_channel_stroke_opacity_0(label_index); }
        case 1u: { return get_channel_stroke_opacity_1(label_index); }
        default: { return 0.0; }
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
/// calls) for a representative 2-channel configuration: channel 0 is filled
/// with `ColorMode::UniformRgb` (or `None`, which builds identically) and
/// uniform opacity; channel 1 is stroked with `ColorMode::Quantitative` via the
/// `viridis` colormap and an instanced stroke width. Compared against a
/// checked-in golden file rather than an inline literal, since the fully
/// assembled shader is large; see [`check_wgsl_snapshot`] for how to bless a
/// deliberate template change.
#[test]
fn full_shader_assembly_matches_snapshot() {
    // Channel 0: fill color at binding 2 is uniform (no texture), so channel
    // 1's quantitative stroke color takes the first texture binding.
    let channel_0 = [
        ShaderBuilder::new(get_channel_color::UNIFORM_RGB)
            .define("name", "fill_color")
            .define("ch", "0")
            .build(),
        ShaderBuilder::new(get_channel_color::UNIFORM_RGB)
            .define("name", "stroke_color")
            .define("ch", "0")
            .build(),
        ShaderBuilder::new(get_channel_scalar::UNIFORM)
            .define("name", "fill_opacity")
            .define("ch", "0")
            .build(),
        ShaderBuilder::new(get_channel_scalar::UNIFORM)
            .define("name", "stroke_opacity")
            .define("ch", "0")
            .build(),
        ShaderBuilder::new(get_channel_scalar::UNIFORM)
            .define("name", "stroke_width")
            .define("ch", "0")
            .build(),
    ]
    .join("\n");
    let channel_1 = [
        ShaderBuilder::new(get_channel_color::UNIFORM_RGB)
            .define("name", "fill_color")
            .define("ch", "1")
            .build(),
        ShaderBuilder::new(get_channel_color::QUANTITATIVE)
            .define("name", "stroke_color")
            .define("ch", "1")
            .define_bidx("values", 2)
            .inject_texture_sample_type("values", TextureDtype::F32)
            .define("colormap_fn_name", "viridis")
            .build(),
        ShaderBuilder::new(get_channel_scalar::UNIFORM)
            .define("name", "fill_opacity")
            .define("ch", "1")
            .build(),
        ShaderBuilder::new(get_channel_scalar::UNIFORM)
            .define("name", "stroke_opacity")
            .define("ch", "1")
            .build(),
        ShaderBuilder::new(get_channel_scalar::INSTANCED)
            .define("name", "stroke_width")
            .define("ch", "1")
            .define_bidx("values", 3)
            .inject_texture_sample_type("values", TextureDtype::F32)
            .build(),
    ]
    .join("\n");
    let channel_functions = format!("{channel_0}\n{channel_1}");

    let colormap_functions = pluot_core::shader_modules::colormaps::VIRIDIS.to_string();

    let channel_dispatchers = [
        channel_dispatch(bitmask_channel::CHANNEL_COLOR_DISPATCH, "fill_color", 2),
        channel_dispatch(bitmask_channel::CHANNEL_COLOR_DISPATCH, "stroke_color", 2),
        channel_dispatch(bitmask_channel::CHANNEL_SCALAR_DISPATCH, "fill_opacity", 2),
        channel_dispatch(bitmask_channel::CHANNEL_SCALAR_DISPATCH, "stroke_opacity", 2),
        channel_dispatch(bitmask_channel::CHANNEL_SCALAR_DISPATCH, "stroke_width", 2),
    ]
    .join("\n");

    let shader_source = ShaderBuilder::new(include_str!("../src/layers/shaders/bitmask_layer.wgsl"))
        .inject_function("scale", common::SCALE)
        .inject_function("translate", common::TRANSLATE)
        .inject_function("get_aspect_ratio_mat", common::GET_ASPECT_RATIO_MAT)
        .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
        .inject_texture_sample_type("mask_data", TextureDtype::U32)
        .inject_function("bitmask_sample", bitmask_channel::CHANNEL_SAMPLE)
        .inject_function("bitmask_is_edge", bitmask_channel::CHANNEL_IS_EDGE)
        .inject_function("bitmask_stroke_width_texels", bitmask_channel::CHANNEL_STROKE_WIDTH)
        .define("colormap_functions", &colormap_functions)
        .define("channel_functions", &channel_functions)
        .define("channel_dispatchers", &channel_dispatchers)
        .build();

    assert!(!shader_source.contains("{{"), "no placeholder should remain unsubstituted");

    check_wgsl_snapshot(&shader_source, "bitmask_layer_shader_two_channels.wgsl");
}
