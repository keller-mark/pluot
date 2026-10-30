use std::sync::Arc;

use pluot_core::numeric_data::NumericData;
use pluot_zarr::dotplot_data::{category_strata, filter_genes, genes_among, resolve_gene_columns};

#[test]
fn resolve_gene_columns_skips_genes_not_found() {
    let var_index = vec!["GeneA".to_string(), "GeneB".to_string(), "GeneC".to_string()];
    let requested = vec!["GeneC".to_string(), "GeneX".to_string(), "GeneA".to_string()];
    let resolved = resolve_gene_columns(&requested, &var_index);
    assert_eq!(resolved, vec![("GeneC".to_string(), 2), ("GeneA".to_string(), 0)]);
}

#[test]
fn category_strata_are_requested_codes_with_distinct_codes_for_unknown_categories() {
    let obs_categories = vec!["B".to_string(), "T".to_string(), "NK".to_string()];
    let obs_codes = NumericData::Int32(Arc::new(vec![0, 0, 2, 1, 1, -1]));
    let requested = vec!["T".to_string(), "Unknown".to_string(), "B".to_string(), "Other".to_string()];
    let strata = category_strata(&requested, &obs_categories, &obs_codes);
    assert_eq!(strata.included_codes, vec![1, -2, 0, -3]);
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
