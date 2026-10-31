use std::sync::Arc;

use pluot_core::numeric_data::NumericData;
use pluot_zarr::dotplot_data::{category_strata, combine_codes, filter_genes, genes_among, resolve_gene_columns, NestedCategories};

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
fn category_strata_are_requested_codes_with_distinct_codes_for_unknown_categories() {
    let nested = nested(&[&["B", "T", "NK"]], vec![0, 0, 2, 1, 1, -1]);
    let requested = vec![strings(&["T"]), strings(&["Unknown"]), strings(&["B"]), strings(&["Other"])];
    let strata = category_strata(&requested, &nested);
    assert_eq!(strata.included_codes, vec![1, -2, 0, -3]);
}

#[test]
fn nested_combinations_are_ordered_by_the_coarsest_column_first() {
    let nested = nested(&[&["B", "T"], &["s1", "s2", "s3"]], vec![]);
    let combinations = nested.combinations();
    assert_eq!(combinations[..4], [strings(&["B", "s1"]), strings(&["B", "s2"]), strings(&["B", "s3"]), strings(&["T", "s1"])]);
    for (code, combination) in combinations.iter().enumerate() {
        assert_eq!(nested.code_of(combination), Some(code as i64));
    }
    assert_eq!(nested.code_of(&strings(&["T", "s4"])), None);
    assert_eq!(nested.code_of(&strings(&["T"])), None);
    assert_eq!(nested.first_column_code(&strings(&["T", "s2"])), Some(1));
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
