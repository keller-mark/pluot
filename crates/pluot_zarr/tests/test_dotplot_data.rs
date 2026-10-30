use std::sync::Arc;

use pluot_core::numeric_data::NumericData;
use pluot_zarr::dotplot_data::{bucket_rows_by_category, resolve_gene_columns, summarize_gene_category};

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
