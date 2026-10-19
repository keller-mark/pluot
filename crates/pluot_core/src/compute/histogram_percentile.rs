/// The bin of `value` among `num_bins` equal-width bins spanning `[lo, hi)`,
/// clamping out-of-range values into the edge bins. Mirrors the binning of
/// the histogram reductions (`reduce.wgsl`, `matrix_axis_extents.wgsl`).
pub fn histogram_bin(value: f32, lo: f32, hi: f32, num_bins: u32) -> usize {
    let range = hi - lo;
    if range <= 0.0 {
        return 0;
    }
    (((value - lo) / range) * num_bins as f32).clamp(0.0, (num_bins - 1) as f32) as usize
}

/// Estimates a percentile of a distribution from successive histograms of it,
/// without sorting: each [`PercentileSearch::narrow`] zooms into the bin that
/// contains the percentile, so `k` histograms of `n` bins locate it to within
/// `1 / n^k` of the initial range, even when outliers stretch that range.
///
/// Every histogram must span [`PercentileSearch::range`], with values outside
/// of it clamped into the edge bins (as [`histogram_bin`] does).
#[derive(Clone, Copy, Debug)]
pub struct PercentileSearch {
    lo: f32,
    hi: f32,
    /// How many values lie below `lo` / above `hi`, which the edge bins of
    /// every histogram of a narrowed range also count.
    num_below: u64,
    num_above: u64,
    /// In `[0, 100]`.
    percentile: f32,
}

/// Where the percentile falls within the in-range counts of a histogram.
struct Located {
    bin: usize,
    num_before_bin: u64,
    num_in_bin: u64,
    rank_in_range: f64,
}

impl PercentileSearch {
    /// `extent` must contain every value, so that the first histogram has no out-of-range values.
    pub fn new(extent: (f32, f32), percentile: f32) -> Self {
        Self { lo: extent.0, hi: extent.1, num_below: 0, num_above: 0, percentile: percentile.clamp(0.0, 100.0) }
    }

    pub fn range(&self) -> (f32, f32) {
        (self.lo, self.hi)
    }

    fn locate(&self, counts: &[u32]) -> Option<Located> {
        if counts.is_empty() || self.hi <= self.lo {
            return None;
        }
        let mut in_range: Vec<u64> = counts.iter().map(|&c| c as u64).collect();
        let last = in_range.len() - 1;
        in_range[0] = in_range[0].saturating_sub(self.num_below);
        in_range[last] = in_range[last].saturating_sub(self.num_above);
        let num_in_range: u64 = in_range.iter().sum();
        if num_in_range == 0 {
            return None;
        }
        let num_total = self.num_below + num_in_range + self.num_above;
        let rank_in_range = (self.percentile as f64 / 100.0 * num_total as f64 - self.num_below as f64).clamp(0.0, num_in_range as f64);

        let mut num_before_bin = 0u64;
        let mut last_nonempty = None;
        for (bin, &num_in_bin) in in_range.iter().enumerate() {
            if num_in_bin == 0 {
                continue;
            }
            if (num_before_bin + num_in_bin) as f64 >= rank_in_range {
                return Some(Located { bin, num_before_bin, num_in_bin, rank_in_range });
            }
            last_nonempty = Some((bin, num_before_bin, num_in_bin));
            num_before_bin += num_in_bin;
        }
        last_nonempty.map(|(bin, num_before_bin, num_in_bin)| Located { bin, num_before_bin, num_in_bin, rank_in_range })
    }

    fn bin_width(&self, num_bins: usize) -> f32 {
        (self.hi - self.lo) / num_bins as f32
    }

    /// Narrows the range to the bin of `counts` (a histogram of `range()`) containing the percentile.
    pub fn narrow(&mut self, counts: &[u32]) {
        let Some(located) = self.locate(counts) else { return };
        let width = self.bin_width(counts.len());
        let num_in_range: u64 = self.num_in_range(counts);
        self.num_above += num_in_range - located.num_before_bin - located.num_in_bin;
        self.num_below += located.num_before_bin;
        self.lo += located.bin as f32 * width;
        self.hi = self.lo + width;
    }

    fn num_in_range(&self, counts: &[u32]) -> u64 {
        let total: u64 = counts.iter().map(|&c| c as u64).sum();
        total.saturating_sub(self.num_below + self.num_above)
    }

    /// The percentile, linearly interpolated within its bin of `counts` (a histogram of `range()`).
    pub fn estimate(&self, counts: &[u32]) -> f32 {
        let Some(located) = self.locate(counts) else { return self.lo };
        let width = self.bin_width(counts.len());
        let fraction = (located.rank_in_range - located.num_before_bin as f64) / located.num_in_bin as f64;
        (self.lo + (located.bin as f32 + fraction as f32) * width).clamp(self.lo, self.hi)
    }
}
