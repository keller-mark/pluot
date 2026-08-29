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
//! `` `{{c_idx}}` ``) literally gets matched and substituted too, since
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

/// Compare assembled WGSL against a checked-in golden file in
/// `tests/snaps-blessed/`.
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
fn switch_cases(stroke_or_fill_property: &str, n: usize) -> String {
    (0..n)
        .map(|i| {
            format!("case {i}u: {{ return get_channel_{stroke_or_fill_property}_{i}(label_index); }}")
        })
        .collect::<Vec<_>>()
        .join("\n        ")
}

/// Mirrors `channel_dispatch` in `BitmaskLayer`'s module.
fn channel_dispatch(template: &str, stroke_or_fill_property: &str, n: usize) -> String {
    ShaderBuilder::new(template)
        .define("stroke_or_fill_property", stroke_or_fill_property)
        .define("switch_cases", &switch_cases(stroke_or_fill_property, n))
        .build()
}

/// Mirrors the WGSL `crate::emphasis_mode::prepare_emphasis_criteria` emits
/// for an empty (no) criteria list: a predicate that always returns `true`,
/// with no texture bindings. Used to stand in for a channel's
/// `filtering_criteria`/`selection_criteria`, which default to empty.
fn empty_criteria_wgsl(fn_name: &str) -> String {
    format!("fn {fn_name}(instance_index: u32) -> bool {{\n    return true;\n}}\n")
}

#[test]
fn uniform_rgb_template_matches_expected() {
    // Mirrors `prepare_channel_color`'s `None` / `ColorMode::UniformRgb` arm.
    let actual = ShaderBuilder::new(get_channel_color::UNIFORM_RGB)
        .define("stroke_or_fill_property", "fill_color")
        .define("c_idx", "0")
        .build();
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "bitmask_layer_uniform_rgb_template.wgsl");
}

#[test]
fn instanced_rgb_template_matches_expected() {
    // Mirrors `prepare_channel_color`'s `ColorMode::InstancedRgb` arm.
    let actual = ShaderBuilder::new(get_channel_color::INSTANCED_RGB)
        .define("stroke_or_fill_property", "stroke_color")
        .define("c_idx", "1")
        .define_bidx("r", 5)
        .define_bidx("g", 6)
        .define_bidx("b", 7)
        .inject_texture_sample_type("r", TextureDtype::U8)
        .inject_texture_sample_type("g", TextureDtype::U8)
        .inject_texture_sample_type("b", TextureDtype::U8)
        .build();
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "bitmask_layer_instanced_rgb_template.wgsl");
}

#[test]
fn instanced_rgb_interleaved_template_matches_expected() {
    // Mirrors `prepare_channel_color`'s `ColorMode::InstancedRgbInterleaved` arm.
    let actual = ShaderBuilder::new(get_channel_color::INSTANCED_RGB_INTERLEAVED)
        .define("stroke_or_fill_property", "fill_color")
        .define("c_idx", "2")
        .define_bidx("rgb", 8)
        .inject_texture_sample_type("rgb", TextureDtype::U8)
        .build();
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "bitmask_layer_instanced_rgb_interleaved_template.wgsl");
}

#[test]
fn categorical_template_matches_expected() {
    // Mirrors `prepare_channel_color`'s `ColorMode::Categorical` arm (and,
    // identically, its `CategoricalCustom` arm -- both share this template).
    let actual = ShaderBuilder::new(get_channel_color::CATEGORICAL)
        .define("stroke_or_fill_property", "fill_color")
        .define("c_idx", "3")
        .define_bidx("labels", 9)
        .define_bidx("palette", 10)
        .inject_texture_sample_type("labels", TextureDtype::U8)
        .build();
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "bitmask_layer_categorical_template.wgsl");
}

#[test]
fn quantitative_template_matches_expected() {
    // Mirrors `prepare_channel_color`'s `ColorMode::Quantitative` arm.
    let actual = ShaderBuilder::new(get_channel_color::QUANTITATIVE)
        .define("stroke_or_fill_property", "stroke_color")
        .define("c_idx", "4")
        .define_bidx("values", 11)
        .inject_texture_sample_type("values", TextureDtype::F32)
        .define("colormap_fn_name", "viridis")
        .build();
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "bitmask_layer_quantitative_template.wgsl");
}

#[test]
fn uniform_scalar_template_matches_expected() {
    // Mirrors `prepare_channel_scalar`'s uniform (no instanced values) arm.
    let actual = ShaderBuilder::new(get_channel_scalar::UNIFORM)
        .define("stroke_or_fill_property", "stroke_width")
        .define("c_idx", "0")
        .build();
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "bitmask_layer_uniform_scalar_template.wgsl");
}

#[test]
fn instanced_scalar_template_matches_expected() {
    // Mirrors `prepare_channel_scalar`'s instanced arm.
    let actual = ShaderBuilder::new(get_channel_scalar::INSTANCED)
        .define("stroke_or_fill_property", "fill_opacity")
        .define("c_idx", "1")
        .define_bidx("values", 12)
        .inject_texture_sample_type("values", TextureDtype::F32)
        .build();
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "bitmask_layer_instanced_scalar_template.wgsl");
}

#[test]
fn channel_color_dispatch_with_no_channels_matches_expected() {
    let actual = channel_dispatch(bitmask_channel::CHANNEL_COLOR_DISPATCH, "fill_color", 0);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "bitmask_layer_channel_color_dispatch_with_no_channels.wgsl");
}

#[test]
fn channel_color_dispatch_with_three_channels_matches_expected() {
    let actual = channel_dispatch(bitmask_channel::CHANNEL_COLOR_DISPATCH, "stroke_color", 3);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "bitmask_layer_channel_color_dispatch_with_three_channels.wgsl");
}

#[test]
fn channel_scalar_dispatch_with_two_channels_matches_expected() {
    let actual = channel_dispatch(bitmask_channel::CHANNEL_SCALAR_DISPATCH, "stroke_opacity", 2);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "bitmask_layer_channel_scalar_dispatch_with_two_channels.wgsl");
}

#[test]
fn channel_sample_and_is_edge_are_not_templated() {
    // Unlike `get_channel_color`'s per-`ColorMode` snippets, these two are
    // ordinary WGSL functions parameterized by `channel_index` -- injected
    // once via `ShaderBuilder::inject_function` regardless of channel count,
    // with no `{{...}}` placeholders (and no per-channel `c_idx` substitution)
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
            .define("stroke_or_fill_property", "fill_color")
            .define("c_idx", "0")
            .build(),
        ShaderBuilder::new(get_channel_color::UNIFORM_RGB)
            .define("stroke_or_fill_property", "stroke_color")
            .define("c_idx", "0")
            .build(),
        ShaderBuilder::new(get_channel_scalar::UNIFORM)
            .define("stroke_or_fill_property", "fill_opacity")
            .define("c_idx", "0")
            .build(),
        ShaderBuilder::new(get_channel_scalar::UNIFORM)
            .define("stroke_or_fill_property", "stroke_opacity")
            .define("c_idx", "0")
            .build(),
        ShaderBuilder::new(get_channel_scalar::UNIFORM)
            .define("stroke_or_fill_property", "stroke_width")
            .define("c_idx", "0")
            .build(),
        empty_criteria_wgsl("get_channel_is_filtered_in_0"),
        empty_criteria_wgsl("get_channel_is_selected_in_0"),
    ]
    .join("\n");
    let channel_1 = [
        ShaderBuilder::new(get_channel_color::UNIFORM_RGB)
            .define("stroke_or_fill_property", "fill_color")
            .define("c_idx", "1")
            .build(),
        ShaderBuilder::new(get_channel_color::QUANTITATIVE)
            .define("stroke_or_fill_property", "stroke_color")
            .define("c_idx", "1")
            .define_bidx("values", 2)
            .inject_texture_sample_type("values", TextureDtype::F32)
            .define("colormap_fn_name", "viridis")
            .build(),
        ShaderBuilder::new(get_channel_scalar::UNIFORM)
            .define("stroke_or_fill_property", "fill_opacity")
            .define("c_idx", "1")
            .build(),
        ShaderBuilder::new(get_channel_scalar::UNIFORM)
            .define("stroke_or_fill_property", "stroke_opacity")
            .define("c_idx", "1")
            .build(),
        ShaderBuilder::new(get_channel_scalar::INSTANCED)
            .define("stroke_or_fill_property", "stroke_width")
            .define("c_idx", "1")
            .define_bidx("values", 3)
            .inject_texture_sample_type("values", TextureDtype::F32)
            .build(),
        empty_criteria_wgsl("get_channel_is_filtered_in_1"),
        empty_criteria_wgsl("get_channel_is_selected_in_1"),
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
        channel_dispatch(bitmask_channel::CHANNEL_BOOL_DISPATCH, "is_filtered_in", 2),
        channel_dispatch(bitmask_channel::CHANNEL_BOOL_DISPATCH, "is_selected_in", 2),
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
