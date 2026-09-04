//! Regression tests for the `ShaderBuilder` usage performed by
//! `crate::emphasis_mode::prepare_emphasis_criteria` (see
//! `crates/pluot_core/src/emphasis_mode.rs`): each `is_included` template
//! under `wgsl_functions/get_is_included/`, plus the AND-wrapper function
//! that combines a `Vec<EmphasisCriteria>` -- `filtering_criteria` and
//! `selection_criteria` are each a list of criteria AND-ed together. Also
//! covers `cpu_is_included`, whose comparisons must match the ones baked into
//! the WGSL here.
//!
//! These assert exact string equality against golden output, mirroring
//! `test_shader_modules_bitmask.rs`, to catch template/placeholder
//! corruption that a "does it compile" check would miss.
//! `prepare_emphasis_criteria` itself needs a `wgpu::Device`/`Queue` to
//! upload the codes/values textures for the categorical/quantitative
//! variants, so rather than pull in a GPU context here, these tests
//! replicate its `ShaderBuilder` calls and AND-wrapper construction
//! directly -- the same templates, the same `.define()`/`.define_bidx()`
//! calls, the same string joins -- minus the actual texture upload. The
//! quantitative variant is the exception: its template and comparison
//! operators are chosen by `emphasis_mode::quantitative_criteria_wgsl`,
//! which is GPU-independent and so called here directly rather than
//! mirrored.

use std::sync::Arc;

use pluot_core::emphasis_mode::{cpu_is_included, quantitative_criteria_wgsl};
use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{EmphasisCriteria, QuantitativeCriteriaParams};
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

/// A quantitative criteria with the given bounds and bound exclusivity.
/// `values` is never read by `quantitative_criteria_wgsl` (the per-element
/// column only reaches the shader as a texture, which these tests skip), so
/// an empty column keeps each case to just the part that shapes the WGSL.
fn quantitative(
    min: Option<f32>,
    max: Option<f32>,
    min_exclusive: Option<bool>,
    max_exclusive: Option<bool>,
) -> QuantitativeCriteriaParams {
    QuantitativeCriteriaParams {
        values: NumericData::Float32(Arc::new(vec![])),
        min,
        max,
        min_exclusive,
        max_exclusive,
    }
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
    // A single-item `selection_criteria` list with both `min` and `max` set,
    // both inclusive (the default): the two-sided template, `>=` and `<=`.
    let term = quantitative_criteria_wgsl(
        &quantitative(Some(1.0), Some(2.0), None, None),
        "is_selected_in_0",
        "select_data_0",
        7,
        TextureDtype::F32,
    )
    .expect("a bounded criteria emits a term function");
    let actual = [term, and_wrapper("is_selected_in", &["is_selected_in_0"])].join("\n");
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "emphasis_mode_quantitative_criteria_both_bounds.wgsl");
}

#[test]
fn quantitative_criteria_exclusive_bounds_matches_expected() {
    // Same two-sided range, but with both bounds marked exclusive, so the
    // baked-in operators become `>` and `<` -- i.e. an open interval
    // `(1, 2)`.
    let term = quantitative_criteria_wgsl(
        &quantitative(Some(1.0), Some(2.0), Some(true), Some(true)),
        "is_selected_in_0",
        "select_data_0",
        7,
        TextureDtype::F32,
    )
    .expect("a bounded criteria emits a term function");
    let actual = [term, and_wrapper("is_selected_in", &["is_selected_in_0"])].join("\n");
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "emphasis_mode_quantitative_criteria_exclusive_bounds.wgsl");
}

#[test]
fn quantitative_criteria_omitted_max_matches_expected() {
    // An omitted `max` (implicit +infinity): the one-sided template, which
    // compares against `min` alone rather than against a `f32::MAX` sentinel.
    let term = quantitative_criteria_wgsl(
        &quantitative(Some(2.0), None, None, None),
        "is_filtered_in_0",
        "filter_data_0",
        5,
        TextureDtype::F32,
    )
    .expect("a bounded criteria emits a term function");
    let actual = [term, and_wrapper("is_filtered_in", &["is_filtered_in_0"])].join("\n");
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "emphasis_mode_quantitative_criteria_omitted_max.wgsl");
}

#[test]
fn quantitative_criteria_half_open_upper_bound_matches_expected() {
    // An omitted `min` with an exclusive `max`: the same one-sided template,
    // now with `<`, e.g. the upper edge of a half-open histogram bin.
    let term = quantitative_criteria_wgsl(
        &quantitative(None, Some(10.0), None, Some(true)),
        "is_filtered_in_0",
        "filter_data_0",
        5,
        TextureDtype::F32,
    )
    .expect("a bounded criteria emits a term function");
    let actual = [term, and_wrapper("is_filtered_in", &["is_filtered_in_0"])].join("\n");
    assert!(!actual.contains("{{"), "no placeholder should remain unsubstituted");
    check_wgsl_snapshot(&actual, "emphasis_mode_quantitative_criteria_half_open_upper_bound.wgsl");
}

#[test]
fn quantitative_criteria_unbounded_emits_no_term() {
    // Neither bound set means every item is included, so the criteria
    // contributes no term function at all -- and, in
    // `prepare_emphasis_criteria`, no value texture either. The wrapper is
    // then the same one an empty criteria list produces.
    assert!(
        quantitative_criteria_wgsl(
            &quantitative(None, None, None, None),
            "is_filtered_in_0",
            "filter_data_0",
            5,
            TextureDtype::F32,
        )
        .is_none(),
        "an unbounded quantitative criteria should emit no WGSL",
    );
    // Bound exclusivity is meaningless without a bound, and must not make an
    // unbounded criteria emit a comparison.
    assert!(
        quantitative_criteria_wgsl(
            &quantitative(None, None, Some(true), Some(true)),
            "is_filtered_in_0",
            "filter_data_0",
            5,
            TextureDtype::F32,
        )
        .is_none(),
        "exclusivity flags alone should not make an unbounded criteria emit WGSL",
    );
    check_wgsl_snapshot(
        &and_wrapper("is_filtered_in", &[]),
        "emphasis_mode_quantitative_criteria_unbounded.wgsl",
    );
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
    let term_1 = quantitative_criteria_wgsl(
        &quantitative(Some(10.0), None, None, None),
        "is_filtered_in_1",
        "filter_data_1",
        6,
        TextureDtype::F32,
    )
    .expect("a bounded criteria emits a term function");
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
    let term_2 = quantitative_criteria_wgsl(
        &quantitative(None, Some(100.0), None, None),
        "is_filtered_in_2",
        "filter_data_2",
        7,
        TextureDtype::F32,
    )
    .expect("a bounded criteria emits a term function");
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

/// `cpu_is_included` over a single quantitative criteria whose values are
/// `[0, 1, 2, 3]`, returning the indices it includes. The CPU path must agree
/// with the operators the snapshot tests above bake into WGSL, since a layer
/// picks/renders on the GPU but hit-tests and draws SVG on the CPU.
fn cpu_included_indices(
    min: Option<f32>,
    max: Option<f32>,
    min_exclusive: Option<bool>,
    max_exclusive: Option<bool>,
) -> Vec<usize> {
    let criteria = vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
        values: NumericData::Float32(Arc::new(vec![0.0, 1.0, 2.0, 3.0])),
        min,
        max,
        min_exclusive,
        max_exclusive,
    })];
    (0..4).filter(|&i| cpu_is_included(&criteria, i)).collect()
}

#[test]
fn cpu_quantitative_bounds_are_inclusive_by_default() {
    // `[1, 2]` includes both endpoints when neither exclusivity flag is set.
    assert_eq!(cpu_included_indices(Some(1.0), Some(2.0), None, None), vec![1, 2]);
    // Explicit `Some(false)` reads the same as the `None` default.
    assert_eq!(
        cpu_included_indices(Some(1.0), Some(2.0), Some(false), Some(false)),
        vec![1, 2],
    );
    // One-sided: `>= 2` and `<= 1`.
    assert_eq!(cpu_included_indices(Some(2.0), None, None, None), vec![2, 3]);
    assert_eq!(cpu_included_indices(None, Some(1.0), None, None), vec![0, 1]);
}

#[test]
fn cpu_quantitative_exclusive_bounds_drop_the_endpoints() {
    // `(1, 2)` excludes both endpoints, leaving nothing in between here.
    assert_eq!(cpu_included_indices(Some(1.0), Some(2.0), Some(true), Some(true)), Vec::<usize>::new());
    // Half-open `[1, 3)`, e.g. a histogram bin: keeps its lower edge, drops
    // the upper one (which belongs to the next bin).
    assert_eq!(cpu_included_indices(Some(1.0), Some(3.0), None, Some(true)), vec![1, 2]);
    // Half-open the other way, `(1, 3]`.
    assert_eq!(cpu_included_indices(Some(1.0), Some(3.0), Some(true), None), vec![2, 3]);
    // One-sided: `> 2` and `< 1`.
    assert_eq!(cpu_included_indices(Some(2.0), None, Some(true), None), vec![3]);
    assert_eq!(cpu_included_indices(None, Some(1.0), None, Some(true)), vec![0]);
}

#[test]
fn cpu_quantitative_unbounded_includes_everything() {
    // Matching the GPU path, where an unbounded criteria emits no test at
    // all: every item is included, and stray exclusivity flags on the absent
    // bounds change nothing.
    assert_eq!(cpu_included_indices(None, None, None, None), vec![0, 1, 2, 3]);
    assert_eq!(cpu_included_indices(None, None, Some(true), Some(true)), vec![0, 1, 2, 3]);
}
