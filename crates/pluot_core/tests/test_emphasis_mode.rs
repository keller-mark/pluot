//! Regression tests for the `ShaderBuilder` usage performed by
//! `crate::emphasis_mode::prepare_emphasis_criteria` (see
//! `crates/pluot_core/src/emphasis_mode.rs`): each `is_included` template
//! under `wgsl_functions/get_is_included/`, plus the AND-wrapper function
//! that combines a `Vec<EmphasisCriteria>` -- `filtering_criteria` and
//! `selection_criteria` are each a list of criteria AND-ed together.
//!
//! These assert exact string equality against golden output, mirroring
//! `test_shader_modules_bitmask.rs`, to catch template/placeholder
//! corruption that a "does it compile" check would miss.
//! `prepare_emphasis_criteria` itself needs a `wgpu::Device`/`Queue` to
//! upload the codes/values textures for the categorical/quantitative
//! variants, so rather than pull in a GPU context here, these tests
//! replicate its `ShaderBuilder` calls and AND-wrapper construction
//! directly -- the same templates, the same `.define()`/`.define_bidx()`
//! calls, the same string joins -- minus the actual texture upload.

use pluot_core::shader_modules::{is_included, ShaderBuilder, TextureDtype};

/// Trims trailing whitespace from each line before comparing. See
/// `test_shader_modules_bitmask.rs`'s `normalize` for why: a `{{placeholder}}`
/// substituted with an empty string can leave whitespace-only lines behind
/// that carry no semantic meaning.
fn normalize(s: &str) -> String {
    s.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n")
}

/// Compare assembled WGSL against a checked-in golden file in
/// `tests/snaps-blessed/`. See `test_shader_modules_bitmask.rs`'s
/// `check_wgsl_snapshot` for the full rationale; this is the same helper.
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

/// Mirrors `wgsl_float` in `emphasis_mode.rs`.
fn wgsl_float(value: f32) -> String {
    format!("{value:e}")
}

/// Mirrors the AND-wrapper construction at the end of
/// `prepare_emphasis_criteria`: a function named `fn_name` that ANDs
/// together every per-criteria `term_fn_names[i](instance_index)` call, or
/// vacuously returns `true` when `term_fn_names` is empty (no criteria means
/// every item is included).
fn and_wrapper(fn_name: &str, term_fn_names: &[&str]) -> String {
    let and_expr = if term_fn_names.is_empty() {
        "true".to_string()
    } else {
        term_fn_names.iter().map(|n| format!("{n}(instance_index)")).collect::<Vec<_>>().join(" && ")
    };
    format!("fn {fn_name}(instance_index: u32) -> bool {{\n    return {and_expr};\n}}\n")
}

#[test]
fn empty_criteria_list_matches_expected() {
    // Mirrors `prepare_emphasis_criteria` called with an empty `criteria`
    // slice (e.g. `filtering_criteria: vec![]`): no term functions, no
    // textures -- just the wrapper, which vacuously returns `true`.
    let actual = and_wrapper("is_filtered_in", &[]);
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "emphasis_mode_empty_criteria_list.wgsl");
}

#[test]
fn categorical_empty_included_codes_template_matches_expected() {
    // Mirrors a single-item `filtering_criteria` list whose
    // `CategoricalCriteriaParams::included_codes` is an explicit empty list:
    // texture-free, always false, so nothing is included.
    let term = ShaderBuilder::new(is_included::EMPTY)
        .define("criteria_fn_name", "is_filtered_in_0")
        .build();
    let actual = [term, and_wrapper("is_filtered_in", &["is_filtered_in_0"])].join("\n");
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "emphasis_mode_categorical_empty_included_codes.wgsl");
}

#[test]
fn categorical_criteria_template_matches_expected() {
    // Mirrors a single-item `filtering_criteria` list with a non-empty
    // `included_codes` list.
    let term = ShaderBuilder::new(is_included::CATEGORICAL)
        .define("criteria_fn_name", "is_filtered_in_0")
        .define("criteria_data_var", "filter_data_0")
        .define_bidx("criteria_data", 5)
        .inject_texture_sample_type("criteria_data", TextureDtype::I32)
        .define_u32("criteria_included_len", 2)
        .define("criteria_included_codes", "0, 2")
        .build();
    let actual = [term, and_wrapper("is_filtered_in", &["is_filtered_in_0"])].join("\n");
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "emphasis_mode_categorical_criteria.wgsl");
}

#[test]
fn quantitative_criteria_both_bounds_matches_expected() {
    // Mirrors a single-item `selection_criteria` list with both `min` and
    // `max` set.
    let term = ShaderBuilder::new(is_included::QUANTITATIVE)
        .define("criteria_fn_name", "is_selected_in_0")
        .define("criteria_data_var", "select_data_0")
        .define_bidx("criteria_data", 7)
        .inject_texture_sample_type("criteria_data", TextureDtype::F32)
        .define("criteria_min_value", &wgsl_float(1.0))
        .define("criteria_max_value", &wgsl_float(2.0))
        .build();
    let actual = [term, and_wrapper("is_selected_in", &["is_selected_in_0"])].join("\n");
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "emphasis_mode_quantitative_criteria_both_bounds.wgsl");
}

#[test]
fn quantitative_criteria_omitted_max_matches_expected() {
    // Mirrors an omitted `max` (implicit +infinity, baked in as `f32::MAX`).
    let term = ShaderBuilder::new(is_included::QUANTITATIVE)
        .define("criteria_fn_name", "is_filtered_in_0")
        .define("criteria_data_var", "filter_data_0")
        .define_bidx("criteria_data", 5)
        .inject_texture_sample_type("criteria_data", TextureDtype::F32)
        .define("criteria_min_value", &wgsl_float(2.0))
        .define("criteria_max_value", &wgsl_float(f32::MAX))
        .build();
    let actual = [term, and_wrapper("is_filtered_in", &["is_filtered_in_0"])].join("\n");
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "emphasis_mode_quantitative_criteria_omitted_max.wgsl");
}

#[test]
fn multiple_criteria_and_wrapper_matches_expected() {
    // Mirrors `prepare_emphasis_criteria` called with a two-item
    // `filtering_criteria` list -- a categorical column AND-ed with a
    // quantitative column, e.g. "cell type is one of {T cell, B cell,
    // Myeloid} AND expression >= 10". Each criteria gets its own
    // `is_filtered_in_{i}` term function and its own texture binding
    // (5, then 6); the wrapper ANDs both terms together.
    let term_0 = ShaderBuilder::new(is_included::CATEGORICAL)
        .define("criteria_fn_name", "is_filtered_in_0")
        .define("criteria_data_var", "filter_data_0")
        .define_bidx("criteria_data", 5)
        .inject_texture_sample_type("criteria_data", TextureDtype::I32)
        .define_u32("criteria_included_len", 3)
        .define("criteria_included_codes", "0, 1, 2")
        .build();
    let term_1 = ShaderBuilder::new(is_included::QUANTITATIVE)
        .define("criteria_fn_name", "is_filtered_in_1")
        .define("criteria_data_var", "filter_data_1")
        .define_bidx("criteria_data", 6)
        .inject_texture_sample_type("criteria_data", TextureDtype::F32)
        .define("criteria_min_value", &wgsl_float(10.0))
        .define("criteria_max_value", &wgsl_float(f32::MAX))
        .build();
    let actual = [
        term_0,
        term_1,
        and_wrapper("is_filtered_in", &["is_filtered_in_0", "is_filtered_in_1"]),
    ]
    .join("\n");
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "emphasis_mode_multiple_criteria_and.wgsl");
}

#[test]
fn three_criteria_and_wrapper_matches_expected() {
    // Same as `multiple_criteria_and_wrapper_matches_expected`, but with
    // three criteria to confirm the AND-join scales past two terms (e.g.
    // filtering by two categorical columns and a quantitative one).
    let term_0 = ShaderBuilder::new(is_included::CATEGORICAL)
        .define("criteria_fn_name", "is_filtered_in_0")
        .define("criteria_data_var", "filter_data_0")
        .define_bidx("criteria_data", 5)
        .inject_texture_sample_type("criteria_data", TextureDtype::I32)
        .define_u32("criteria_included_len", 1)
        .define("criteria_included_codes", "0")
        .build();
    let term_1 = ShaderBuilder::new(is_included::CATEGORICAL)
        .define("criteria_fn_name", "is_filtered_in_1")
        .define("criteria_data_var", "filter_data_1")
        .define_bidx("criteria_data", 6)
        .inject_texture_sample_type("criteria_data", TextureDtype::U8)
        .define_u32("criteria_included_len", 1)
        .define("criteria_included_codes", "1")
        .build();
    let term_2 = ShaderBuilder::new(is_included::QUANTITATIVE)
        .define("criteria_fn_name", "is_filtered_in_2")
        .define("criteria_data_var", "filter_data_2")
        .define_bidx("criteria_data", 7)
        .inject_texture_sample_type("criteria_data", TextureDtype::F32)
        .define("criteria_min_value", &wgsl_float(f32::MIN))
        .define("criteria_max_value", &wgsl_float(100.0))
        .build();
    let actual = [
        term_0,
        term_1,
        term_2,
        and_wrapper("is_filtered_in", &["is_filtered_in_0", "is_filtered_in_1", "is_filtered_in_2"]),
    ]
    .join("\n");
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "emphasis_mode_three_criteria_and.wgsl");
}
