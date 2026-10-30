// Summarizes and bins one gene's expression per obs category, for violin plots.
// Reference: https://github.com/vitessce/vitessce/blob/main/packages/view-types/statistical-plots/src/expr-hooks.js

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use pluot_core::d3::array::{bin, deviation, mean, quantile_sorted};
use pluot_core::d3::scale::ticks;

/// One group's expression values, summarized for drawing its violin.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViolinSummary {
    /// (first quartile, median, third quartile), or `None` when the group has no values.
    pub quartiles: Option<(f32, f32, f32)>,
    /// The values within three standard deviations of the mean (Chauvenet's criterion), in ascending order.
    pub non_outliers: Vec<f32>,
    /// The number of values summarized, including outliers.
    pub num_values: u32,
}

/// Per-bin counts of every group's non-outlier values, over bins shared by all groups.
#[derive(Clone, Debug, PartialEq)]
pub struct ViolinBins {
    pub value_domain: (f32, f32),
    pub bin_starts: Vec<f32>,
    /// One entry per group, holding one count per bin.
    pub bin_counts: Vec<Vec<u32>>,
}

thread_local! {
    static USE_MEMO_CACHE_VIOLIN_SUMMARY: RefCell<Option<HashMap<Vec<String>, Arc<ViolinSummary>>>> = const { RefCell::new(None) };
}

fn use_memo_violin_summary(initializer: impl FnOnce() -> ViolinSummary, keys: &[String], cache_enabled: bool) -> Arc<ViolinSummary> {
    if !cache_enabled {
        return Arc::new(initializer());
    }

    let cached = USE_MEMO_CACHE_VIOLIN_SUMMARY.with(|cache| cache.borrow().as_ref().and_then(|map| map.get(keys).cloned()));
    if let Some(summary) = cached {
        return summary;
    }

    let summary = Arc::new(initializer());
    USE_MEMO_CACHE_VIOLIN_SUMMARY.with(|cache| {
        cache.borrow_mut().get_or_insert_with(HashMap::new).insert(keys.to_vec(), summary.clone());
    });
    summary
}

/// Summarizes (and caches, under `keys`) the expression `values` of one group's observation `rows`.
pub fn load_violin_summary(values: &[f32], rows: &[u32], keep_zeros: bool, keys: &[String], cache_enabled: bool) -> Arc<ViolinSummary> {
    use_memo_violin_summary(|| summarize_violin(values, rows, keep_zeros), keys, cache_enabled)
}

/// Summarizes the expression `values` of one group's observation `rows`.
pub fn summarize_violin(values: &[f32], rows: &[u32], keep_zeros: bool) -> ViolinSummary {
    let mut sorted: Vec<f32> = rows.iter()
        .map(|&row| values[row as usize])
        .filter(|value| !value.is_nan() && (keep_zeros || *value != 0.0))
        .collect();
    sorted.sort_by(f32::total_cmp);

    let quartile = |p: f64| quantile_sorted(&sorted, p).map(|q| q as f32);
    let quartiles = quartile(0.25).zip(quartile(0.5)).zip(quartile(0.75)).map(|((q1, q2), q3)| (q1, q2, q3));

    // Unlike Vitessce, which drops every value of a single-value group (as its deviation is undefined), this keeps them.
    let non_outliers = match mean(&sorted).zip(deviation(&sorted)) {
        Some((mean, deviation)) => {
            let (lower, upper) = (mean - 3.0 * deviation, mean + 3.0 * deviation);
            sorted.iter().copied().filter(|&v| lower <= v as f64 && v as f64 <= upper).collect()
        }
        None => sorted.clone(),
    };

    ViolinSummary { quartiles, non_outliers, num_values: sorted.len() as u32 }
}

/// Bins every group's non-outliers into `num_bins` (approximately, as nice tick values are used
/// as thresholds) bins spanning from `value_min` (by default, the lesser of zero and the smallest
/// non-outlier) to the largest non-outlier.
pub fn bin_violins(summaries: &[&ViolinSummary], num_bins: usize, value_min: Option<f32>) -> ViolinBins {
    let all_values = summaries.iter().flat_map(|summary| summary.non_outliers.iter().copied());
    let (smallest, largest) = all_values.fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)));
    let domain_min = value_min.unwrap_or(if smallest.is_finite() { smallest.min(0.0) } else { 0.0 });
    let domain_max = if largest.is_finite() { largest } else { domain_min };
    let domain = (domain_min as f64, domain_max as f64);

    let thresholds = ticks(domain.0, domain.1, num_bins);
    let bins_per_group: Vec<_> = summaries.iter().map(|summary| bin(&summary.non_outliers, domain, &thresholds)).collect();
    let bin_starts = match bins_per_group.first() {
        Some(bins) => bins.iter().map(|b| b.x0 as f32).collect(),
        None => Vec::new(),
    };
    let bin_counts = bins_per_group.iter().map(|bins| bins.iter().map(|b| b.count as u32).collect()).collect();

    ViolinBins { value_domain: (domain_min, domain_max), bin_starts, bin_counts }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_only_the_given_rows() {
        let values = [9.0, 1.0, 2.0, 3.0, 4.0];
        let summary = summarize_violin(&values, &[4, 3, 2, 1], true);
        assert_eq!(summary.quartiles, Some((1.75, 2.5, 3.25)));
        assert_eq!(summary.non_outliers, vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(summary.num_values, 4);
    }

    #[test]
    fn zeros_are_dropped_unless_kept() {
        let values = [0.0, 0.0, 2.0, 4.0];
        assert_eq!(summarize_violin(&values, &[0, 1, 2, 3], false).non_outliers, vec![2.0, 4.0]);
        assert_eq!(summarize_violin(&values, &[0, 1, 2, 3], true).num_values, 4);
    }

    #[test]
    fn values_beyond_three_standard_deviations_are_outliers() {
        let mut values = vec![1.0f32; 20];
        values.push(1000.0);
        let rows: Vec<u32> = (0..values.len() as u32).collect();
        let summary = summarize_violin(&values, &rows, true);
        assert_eq!(summary.non_outliers.len(), 20);
        assert_eq!(summary.num_values, 21);
    }

    #[test]
    fn an_empty_group_has_no_quartiles() {
        let summary = summarize_violin(&[1.0], &[], true);
        assert_eq!(summary, ViolinSummary { quartiles: None, non_outliers: vec![], num_values: 0 });
    }

    #[test]
    fn groups_share_bins_from_zero_to_the_largest_value() {
        let a = ViolinSummary { non_outliers: vec![0.5, 1.5], ..Default::default() };
        let b = ViolinSummary { non_outliers: vec![2.0], ..Default::default() };
        let bins = bin_violins(&[&a, &b], 2, None);
        assert_eq!(bins.value_domain, (0.0, 2.0));
        assert_eq!(bins.bin_starts, vec![0.0, 1.0, 2.0]);
        assert_eq!(bins.bin_counts, vec![vec![1, 1, 0], vec![0, 0, 1]]);
    }

    #[test]
    fn value_min_overrides_the_domain_minimum() {
        let a = ViolinSummary { non_outliers: vec![1.0, 3.0], ..Default::default() };
        assert_eq!(bin_violins(&[&a], 2, Some(1.0)).value_domain, (1.0, 3.0));
    }
}
