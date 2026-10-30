//! Ports of a subset of d3-array: https://github.com/d3/d3-array

/// The `p`-quantile of ascending-sorted `values`, using the R-7 method (as in `d3.quantileSorted`).
/// Returns `None` for empty `values` or a NaN `p`.
pub fn quantile_sorted<T: Copy + Into<f64>>(values: &[T], p: f64) -> Option<f64> {
    let n = values.len();
    if n == 0 || p.is_nan() {
        return None;
    }
    if p <= 0.0 || n < 2 {
        return Some(values[0].into());
    }
    if p >= 1.0 {
        return Some(values[n - 1].into());
    }
    let i = (n - 1) as f64 * p;
    let i0 = i.floor() as usize;
    let value0: f64 = values[i0].into();
    let value1: f64 = values[i0 + 1].into();
    Some(value0 + (value1 - value0) * (i - i0 as f64))
}

pub fn mean<T: Copy + Into<f64>>(values: &[T]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().map(|&v| v.into()).sum::<f64>() / values.len() as f64)
}

/// The unbiased sample standard deviation (as in `d3.deviation`), computed with Welford's algorithm.
/// Returns `None` for fewer than two values.
pub fn deviation<T: Copy + Into<f64>>(values: &[T]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let (mut mean, mut sum_of_squares) = (0.0, 0.0);
    for (count, &value) in values.iter().enumerate() {
        let value: f64 = value.into();
        let delta = value - mean;
        mean += delta / (count + 1) as f64;
        sum_of_squares += delta * (value - mean);
    }
    Some((sum_of_squares / (values.len() - 1) as f64).sqrt())
}

/// One bin of [`bin`], covering `[x0, x1)`, or `[x0, x1]` for the last bin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bin {
    pub x0: f64,
    pub x1: f64,
    pub count: usize,
}

/// Counts `values` into the bins delimited by `thresholds` within `domain` (as in
/// `d3.bin().domain(domain).thresholds(thresholds)`). Values outside the domain are not counted.
/// `thresholds` must be ascending.
pub fn bin<T: Copy + Into<f64>>(values: &[T], domain: (f64, f64), thresholds: &[f64]) -> Vec<Bin> {
    let (x0, x1) = domain;
    let start = thresholds.partition_point(|&t| t <= x0);
    let end = thresholds.partition_point(|&t| t <= x1);
    let inner_thresholds = &thresholds[start..end.max(start)];
    let m = inner_thresholds.len();

    let mut bins: Vec<Bin> = (0..=m)
        .map(|i| Bin {
            x0: if i > 0 { inner_thresholds[i - 1] } else { x0 },
            x1: if i < m { inner_thresholds[i] } else { x1 },
            count: 0,
        })
        .collect();
    for &value in values {
        let x: f64 = value.into();
        if x0 <= x && x <= x1 {
            bins[inner_thresholds.partition_point(|&t| t <= x)].count += 1;
        }
    }
    bins
}
