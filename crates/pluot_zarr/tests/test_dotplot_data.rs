use std::collections::HashMap;
use std::sync::Arc;

use pluot_core::numeric_data::NumericData;
use pluot_zarr::dotplot_data::{
    background_and_foreground_rows, bucket_rows_by_category, filter_genes, genes_among, resolve_gene_columns, summarize_gene_category,
};

#[test]
fn resolve_gene_columns_skips_genes_not_found() {
    let var_index = vec!["GeneA".to_string(), "GeneB".to_string(), "GeneC".to_string()];
    let requested = vec!["GeneC".to_string(), "GeneX".to_string(), "GeneA".to_string()];
    let resolved = resolve_gene_columns(&requested, &var_index);
    assert_eq!(resolved, vec![("GeneC".to_string(), 2), ("GeneA".to_string(), 0)]);
}

#[test]
fn bucket_rows_by_category_groups_by_requested_categories_only() {
    let obs_categories = vec!["B".to_string(), "T".to_string(), "NK".to_string()];
    // Cell 5 has a missing category (-1); cell 2 belongs to "NK", which is not requested.
    let obs_codes = NumericData::Int32(Arc::new(vec![0, 0, 2, 1, 1, -1]));
    let requested = vec!["T".to_string(), "B".to_string()];

    let buckets = bucket_rows_by_category(&requested, &obs_categories, &obs_codes);
    assert_eq!(buckets.get("B"), Some(&vec![0, 1]));
    assert_eq!(buckets.get("T"), Some(&vec![3, 4]));
    assert_eq!(buckets.len(), 2);
}

#[test]
fn bucket_rows_by_category_with_no_requested_categories_is_empty() {
    let obs_categories = vec!["B".to_string(), "T".to_string()];
    let obs_codes = NumericData::Int32(Arc::new(vec![0, 1]));
    let buckets = bucket_rows_by_category(&[], &obs_categories, &obs_codes);
    assert!(buckets.is_empty());
}

#[test]
fn bucket_rows_by_category_logs_and_skips_unknown_category() {
    let obs_categories = vec!["B".to_string()];
    let obs_codes = NumericData::Int32(Arc::new(vec![0, 0]));
    let requested = vec!["B".to_string(), "Unknown".to_string()];
    let buckets = bucket_rows_by_category(&requested, &obs_categories, &obs_codes);
    assert_eq!(buckets.get("B"), Some(&vec![0, 1]));
    assert_eq!(buckets.get("Unknown"), Some(&vec![]));
}

#[test]
fn summarize_gene_category_computes_sum_count_and_num_expressing() {
    let values = [0.0, 1.0, 2.0, 9.0];

    let summary = summarize_gene_category(&values, &[0, 1, 2], 0.0, "index", "GeneA", "celltype", "T");
    assert_eq!(summary.cell_count, 3);
    assert_eq!(summary.sum, 3.0);
    // The cutoff is exclusive, so the 0.0 value does not count as expressing.
    assert_eq!(summary.num_expressing, 2);
    assert_eq!(summary.var_name, "GeneA");
    assert_eq!(summary.obs_value, "T");

    // Only the given rows contribute, in any order.
    let summary = summarize_gene_category(&values, &[3, 1], 1.5, "index", "GeneA", "celltype", "T");
    assert_eq!(summary.sum, 10.0);
    assert_eq!(summary.num_expressing, 1);

    // An empty group is all zeros rather than NaN.
    let summary = summarize_gene_category(&values, &[], 0.0, "index", "GeneA", "celltype", "T");
    assert_eq!(summary.sum, 0.0);
    assert_eq!(summary.cell_count, 0);
    assert_eq!(summary.num_expressing, 0);
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
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
fn foreground_rows_are_the_selected_background_rows() {
    let rows_by_category = HashMap::from([("B".to_string(), vec![0, 1, 2]), ("T".to_string(), vec![3, 4])]);
    let (background, foreground) = background_and_foreground_rows(rows_by_category.clone(), Some(&[0, 2, 3]), Some(&[2, 3, 4]), 5);
    assert_eq!(background, HashMap::from([("B".to_string(), vec![0, 2]), ("T".to_string(), vec![3])]));
    assert_eq!(foreground, Some(HashMap::from([("B".to_string(), vec![2]), ("T".to_string(), vec![3])])));

    let (background, foreground) = background_and_foreground_rows(rows_by_category.clone(), None, None, 5);
    assert_eq!(background, rows_by_category);
    assert_eq!(foreground, None);
}
