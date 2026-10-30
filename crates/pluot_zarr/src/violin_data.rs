// Summarizes and bins one gene's expression per obs category, for violin plots: on the GPU
// when a GPU context is available (see `pluot_core::compute::stratified_violin`), and
// otherwise exactly, on the CPU.
// Reference: https://github.com/vitessce/vitessce/blob/main/packages/view-types/statistical-plots/src/expr-hooks.js

use std::sync::Arc;

use pluot_core::compute::stratified_indices::{compute_stratified_indices, CompactionMethod, RowSource, StratifiedIndices};
use pluot_core::compute::stratified_violin::{compute_stratified_violin_bins, compute_stratified_violin_stats, StratifiedViolinInput, ViolinStratumStats};
use pluot_core::compute::ForegroundBackground;
use pluot_core::d3::array::{bin, deviation, mean, quantile_sorted};
use pluot_core::d3::scale::ticks;
use pluot_core::render_types::GpuContext;

use crate::memo::use_memo_shared;

/// Each category's violin stats, as background (every filter-included value) and foreground
/// (the selected ones among them).
pub type ViolinStats = ForegroundBackground<Vec<ViolinStratumStats>>;
/// Each category's bin counts, as background and foreground.
pub type ViolinBinCounts = ForegroundBackground<Vec<Vec<u32>>>;

/// The counted `values` of `rows`: those that are not NaN, and nonzero unless `keep_zeros`.
fn counted_values<'a>(values: &'a [f32], rows: &'a [u32], keep_zeros: bool) -> impl Iterator<Item = f32> + 'a {
    rows.iter().map(|&row| values[row as usize]).filter(move |value| !value.is_nan() && (keep_zeros || *value != 0.0))
}

/// Summarizes the expression `values` of one group's observation `rows`.
pub fn summarize_violin(values: &[f32], rows: &[u32], keep_zeros: bool) -> ViolinStratumStats {
    let mut sorted: Vec<f32> = counted_values(values, rows, keep_zeros).collect();
    sorted.sort_by(f32::total_cmp);

    let quartile = |p: f64| quantile_sorted(&sorted, p).map(|q| q as f32);
    let quartiles = quartile(0.25).zip(quartile(0.5)).zip(quartile(0.75)).map(|((q1, q2), q3)| (q1, q2, q3));

    // Unlike Vitessce, which drops every value of a single-value group (as its deviation is undefined), this keeps them.
    let non_outlier_bounds = match mean(&sorted).zip(deviation(&sorted)) {
        Some((mean, deviation)) => ((mean - 3.0 * deviation) as f32, (mean + 3.0 * deviation) as f32),
        None => (f32::NEG_INFINITY, f32::INFINITY),
    };
    let mut non_outliers = sorted.iter().copied().filter(|&v| non_outlier_bounds.0 <= v && v <= non_outlier_bounds.1);
    let first = non_outliers.next();
    let non_outlier_extent = first.map(|first| (first, non_outliers.next_back().unwrap_or(first)));

    ViolinStratumStats { num_values: sorted.len() as u32, quartiles, non_outlier_bounds, non_outlier_extent }
}

/// The stats of `num_strata` strata without any values (e.g., of a gene that is not found).
pub fn empty_violin_stats(num_strata: usize) -> ViolinStats {
    let empty = ViolinStratumStats { num_values: 0, quartiles: None, non_outlier_bounds: (f32::NEG_INFINITY, f32::INFINITY), non_outlier_extent: None };
    ForegroundBackground { background: vec![empty; num_strata], foreground: vec![empty; num_strata] }
}

/// Counts the non-outliers (per `non_outlier_bounds`) among the counted `values` of `rows` into
/// the bins that `inner_thresholds` delimit within `domain` (see [`violin_bin_edges`]).
pub fn bin_violin(values: &[f32], rows: &[u32], keep_zeros: bool, non_outlier_bounds: (f32, f32), domain: (f32, f32), inner_thresholds: &[f64]) -> Vec<u32> {
    let non_outliers: Vec<f32> = counted_values(values, rows, keep_zeros).filter(|&v| non_outlier_bounds.0 <= v && v <= non_outlier_bounds.1).collect();
    bin(&non_outliers, (domain.0 as f64, domain.1 as f64), inner_thresholds).iter().map(|b| b.count as u32).collect()
}

/// The value domain and the thresholds within it of bins shared by every violin: approximately
/// `num_bins` (as nice tick values are used as thresholds), spanning from `value_min` (by default,
/// the lesser of zero and the smallest non-outlier) to the largest non-outlier.
pub fn violin_bin_edges(non_outlier_extents: impl IntoIterator<Item = (f32, f32)>, num_bins: usize, value_min: Option<f32>) -> ((f32, f32), Vec<f64>) {
    let (smallest, largest) = non_outlier_extents.into_iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), (min, max)| (lo.min(min), hi.max(max)));
    let domain_min = value_min.unwrap_or(if smallest.is_finite() { smallest.min(0.0) } else { 0.0 });
    let domain_max = if largest.is_finite() { largest } else { domain_min };
    let (x0, x1) = (domain_min as f64, domain_max as f64);
    let thresholds = ticks(x0, x1, num_bins);
    let start = thresholds.partition_point(|&t| t <= x0);
    let end = thresholds.partition_point(|&t| t <= x1).max(start);
    ((domain_min, domain_max), thresholds[start..end].to_vec())
}

/// The lower edge of each bin made by [`violin_bin_edges`].
pub fn violin_bin_starts(domain: (f32, f32), inner_thresholds: &[f64]) -> Vec<f32> {
    std::iter::once(domain.0).chain(inner_thresholds.iter().map(|&t| t as f32)).collect()
}

/// Each stratum's rows of `input`: background, then foreground.
async fn stratum_rows(input: &StratifiedViolinInput<'_>) -> (StratifiedIndices, StratifiedIndices) {
    let rows = RowSource::All(input.values.len());
    let stratify_by = Some(input.stratify_by);
    let background = compute_stratified_indices(None, rows, stratify_by, input.filtering_criteria, CompactionMethod::default()).await;
    let foreground_criteria = [input.filtering_criteria, input.selection_criteria].concat();
    let foreground = compute_stratified_indices(None, rows, stratify_by, &foreground_criteria, CompactionMethod::default()).await;
    (background, foreground)
}

/// Loads (and caches, under `keys`) each stratum's violin stats.
pub async fn load_violin_stats(gpu_context: Option<&GpuContext<'_>>, input: &StratifiedViolinInput<'_>, keys: &[String], cache_enabled: bool) -> Arc<ViolinStats> {
    let mut keys = keys.to_vec();
    keys.insert(0, "violin_stats".to_string());
    use_memo_shared(
        async || match gpu_context {
            Some(gpu_context) => compute_stratified_violin_stats(gpu_context, input).await,
            None => {
                let values = input.values.as_f32();
                let (background, foreground) = stratum_rows(input).await;
                let summarize = |rows: &StratifiedIndices| (0..rows.num_strata()).map(|s| summarize_violin(&values, rows.stratum(s), input.keep_zeros)).collect();
                ForegroundBackground { background: summarize(&background), foreground: summarize(&foreground) }
            }
        },
        &keys,
        cache_enabled,
    )
    .await
}

/// Loads (and caches, under `keys` and the bins) each stratum's bin counts, given its `stats`.
pub async fn load_violin_bins(
    gpu_context: Option<&GpuContext<'_>>,
    input: &StratifiedViolinInput<'_>,
    stats: &ViolinStats,
    domain: (f32, f32),
    inner_thresholds: &[f64],
    keys: &[String],
    cache_enabled: bool,
) -> Arc<ViolinBinCounts> {
    let mut keys = keys.to_vec();
    keys.insert(0, "violin_bins".to_string());
    keys.extend([domain.0, domain.1].into_iter().chain(inner_thresholds.iter().map(|&t| t as f32)).map(|v| v.to_bits().to_string()));
    let bounds = |stats: &[ViolinStratumStats]| stats.iter().map(|stats| stats.non_outlier_bounds).collect::<Vec<_>>();
    use_memo_shared(
        async || match gpu_context {
            Some(gpu_context) => {
                let bounds = ForegroundBackground { background: bounds(&stats.background), foreground: bounds(&stats.foreground) };
                let thresholds: Vec<f32> = inner_thresholds.iter().map(|&t| t as f32).collect();
                compute_stratified_violin_bins(gpu_context, input, &bounds, domain, &thresholds).await
            }
            None => {
                let values = input.values.as_f32();
                let (background, foreground) = stratum_rows(input).await;
                let bin_each = |rows: &StratifiedIndices, stats: &[ViolinStratumStats]| {
                    (0..rows.num_strata())
                        .map(|s| bin_violin(&values, rows.stratum(s), input.keep_zeros, stats[s].non_outlier_bounds, domain, inner_thresholds))
                        .collect()
                };
                ForegroundBackground { background: bin_each(&background, &stats.background), foreground: bin_each(&foreground, &stats.foreground) }
            }
        },
        &keys,
        cache_enabled,
    )
    .await
}
