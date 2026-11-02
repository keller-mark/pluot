use std::sync::Arc;

use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{CategoricalCriteriaParams, EmphasisCriteria};
use pluot_zarr::dotplot_data::{combine_codes, filter_genes, genes_among, groupby_column_orders, resolve_gene_columns, NestedCategories};
use pluot_zarr::heatmap_data::AxisCriteria;
use pluot_zarr::zarr_emphasis_criteria::{IncludedCategories, ZarrCategoricalCriteriaParams, ZarrEmphasisCriteria};

#[test]
fn resolve_gene_columns_skips_genes_not_found() {
    let var_index = vec!["GeneA".to_string(), "GeneB".to_string(), "GeneC".to_string()];
    let requested = vec!["GeneC".to_string(), "GeneX".to_string(), "GeneA".to_string()];
    let resolved = resolve_gene_columns(&requested, &var_index);
    assert_eq!(resolved, vec![("GeneC".to_string(), 2), ("GeneA".to_string(), 0)]);
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn nested(categories_per_column: &[&[&str]], codes: Vec<i32>) -> NestedCategories {
    NestedCategories {
        categories_per_column: categories_per_column.iter().map(|categories| Arc::new(strings(categories))).collect(),
        codes: Arc::new(NumericData::Int32(Arc::new(codes))),
    }
}

#[test]
fn nested_combinations_are_ordered_by_the_coarsest_column_first() {
    let nested = nested(&[&["B", "T"], &["s1", "s2", "s3"]], vec![]);
    let codes = nested.combination_codes();
    assert_eq!(codes, vec![0, 1, 2, 3, 4, 5]);
    let combinations: Vec<Vec<String>> = codes[..4].iter().map(|&code| nested.combination(code)).collect();
    assert_eq!(combinations, [strings(&["B", "s1"]), strings(&["B", "s2"]), strings(&["B", "s3"]), strings(&["T", "s1"])]);
    assert_eq!(nested.first_column_code(4), Some(1));
    assert_eq!(nested.strata(&[4, 1]).included_codes, vec![4, 1]);
}

#[test]
fn combined_codes_are_missing_when_any_column_is() {
    let cell_type = NumericData::Int8(Arc::new(vec![0, 1, 1, -1]));
    let sample = NumericData::Int8(Arc::new(vec![2, 0, -1, 1]));
    assert_eq!(combine_codes(&[&cell_type, &sample], &[2, 3]), vec![2, 3, -1, -1]);
}

#[test]
fn genes_among_looks_up_each_gene_column() {
    let var_index = strings(&["GeneA", "GeneB", "GeneC"]);
    assert_eq!(genes_among(&strings(&["GeneC", "GeneX", "GeneA"]), &var_index, &[2, 1]), vec![true, false, false]);
}

#[test]
fn filter_genes_keeps_the_included_genes_in_order() {
    let var_index = strings(&["GeneA", "GeneB", "GeneC"]);
    let requested = strings(&["GeneC", "GeneX", "GeneA", "GeneB"]);
    assert_eq!(filter_genes(&requested, &var_index, Some(&[0, 2])), strings(&["GeneC", "GeneA"]));
    assert_eq!(filter_genes(&requested, &var_index, None), requested);
}

#[test]
fn combinations_follow_each_column_order_then_nested_order() {
    let nested = nested(&[&["B", "T", "NK"], &["s1", "s2"]], vec![]);
    let mut codes = nested.combination_codes();
    nested.sort_by_column_orders(&mut codes, &[Some(&[2, 0]), None]);
    let combinations: Vec<Vec<String>> = codes.iter().map(|&code| nested.combination(code)).collect();
    assert_eq!(
        combinations,
        [
            strings(&["NK", "s1"]),
            strings(&["NK", "s2"]),
            strings(&["B", "s1"]),
            strings(&["B", "s2"]),
            strings(&["T", "s1"]),
            strings(&["T", "s2"]),
        ]
    );
}

#[test]
fn groupby_column_orders_come_from_categorical_filtering_on_each_groupby_column() {
    let criterion = |codes_key: &str, included: Vec<i64>| {
        ZarrEmphasisCriteria::Categorical(ZarrCategoricalCriteriaParams {
            codes_key: codes_key.to_string(),
            categories_key: None,
            included_categories: IncludedCategories::Codes(included),
        })
    };
    let resolved = |included: Vec<i64>| EmphasisCriteria::Categorical(CategoricalCriteriaParams { codes: NumericData::from(vec![0i32]), included_codes: included });
    let filtering = AxisCriteria::Criteria(vec![criterion("/obs/batch/codes", vec![1]), criterion("obs/sample/codes", vec![2, 0])]);
    let filtering_criteria = [resolved(vec![1]), resolved(vec![2, 0])];
    let groupby = strings(&["obs/cell_type", "obs/sample"]);
    assert_eq!(groupby_column_orders(&groupby, Some(&filtering), &filtering_criteria), vec![None, Some(&[2i64, 0][..])]);
    assert_eq!(groupby_column_orders(&groupby, None, &[]), vec![None, None]);
}
