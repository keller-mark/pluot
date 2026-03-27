//! This module is a port of the D3.js `d3-scale` library, specifically focusing on linear scales.
//! Original source: <https://github.com/d3/d3-scale>

use std::borrow::Borrow;
use std::collections::HashMap;

// Reference: https://github.com/d3/d3-scale/blob/main/src/linear.js

/// A trait for scales that map a domain value to a range value.
pub trait Scaleable<D, R> {
    /// Given a value from the domain, returns the corresponding value in the range.
    fn scale(&self, value: &D) -> R;
}

/// A trait for scales that have a linear range like [start_px, end_px].
pub trait LinearRangeable<R> {
    /// Gets the scale's range.
    fn get_range(&self) -> (R, R);
}

/// A trait for scales that gets a vector of domain values to use as axis ticks.
pub trait Tickable<D> {
    // Note: not all D3 scales support .ticks.
    // In these cases, the .ticks implementation should just return the domain items.
    // Reference: https://github.com/d3/d3-axis/blob/20aef368e872a88e83b31a46df725e29c49908e6/src/axis.js#L44C46-L44C51
    fn ticks(&self, count: Option<usize>) -> Vec<D>;
}

/// A continuous scale.
#[derive(Debug, Clone)]
pub struct ScaleLinear {
    domain: (f64, f64),
    range: (f64, f64),
    clamp: bool,
}

impl Default for ScaleLinear {
    /// Creates a default continuous scale with a domain and range of `[0.0, 1.0]`.
    fn default() -> Self {
        Self {
            domain: (0.0, 1.0),
            range: (0.0, 1.0),
            clamp: false,
        }
    }
}

impl Scaleable<f64, f64> for ScaleLinear {
    fn scale(&self, x: &f64) -> f64 {
        let (d0, d1) = self.domain;
        let (r0, r1) = self.range;

        let x_clamped = if self.clamp {
            if d0 > d1 {
                x.max(d1).min(d0)
            } else {
                x.max(d0).min(d1)
            }
        } else {
            *x
        };

        if d1 < d0 {
            let normalize = normalize(d1, d0);
            let interpolate = interpolate_number(r1, r0);
            interpolate(normalize(x_clamped))
        } else {
            let normalize = normalize(d0, d1);
            let interpolate = interpolate_number(r0, r1);
            interpolate(normalize(x_clamped))
        }
    }
}

impl LinearRangeable<f64> for ScaleLinear {
    fn get_range(&self) -> (f64, f64) {
        self.range
    }
}

impl Tickable<f64> for ScaleLinear {
    /// Returns approximately `count` ticks from the scale's domain.
    fn ticks(&self, count: Option<usize>) -> Vec<f64> {
        let count = count.unwrap_or(10);
        let (start, stop) = self.get_domain();
        ticks(start, stop, count)
    }
}

impl ScaleLinear {
    /// Creates a new default linear scale.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_domain(&self) -> (f64, f64) {
        self.domain
    }

    pub fn set_domain(&mut self, domain: (f64, f64)) {
        self.domain = domain;
    }

    pub fn set_range(&mut self, range: (f64, f64)) {
        self.range = range;
    }

    /// Gets the scale's clamp status.
    pub fn get_clamp(&self) -> bool {
        self.clamp
    }

    /// Enables or disables clamping.
    pub fn set_clamp(&mut self, clamp: bool) {
        self.clamp = clamp;
    }

    /// Rounds the start and end of the domain to "nice" numbers.
    pub fn nice(&mut self, count: Option<usize>) {
        let count = count.unwrap_or(10);
        let (start, stop) = self.get_domain();
        let (new_start, new_stop) = nice(start, stop, count);
        self.set_domain((new_start, new_stop));
    }
}

// Reference: https://github.com/d3/d3-scale/blob/main/src/band.js

/// A band scale for mapping discrete domain values to continuous range values.
/// This is commonly used for bar charts and other categorical visualizations.
#[derive(Debug, Clone)]
pub struct ScaleBand {
    domain: Vec<String>,
    range: (f64, f64),
    range_map: HashMap<String, f64>,
    step: f64,
    bandwidth: f64,
    round: bool,
    padding_inner: f64,
    padding_outer: f64,
    align: f64,
}

impl Default for ScaleBand {
    /// Creates a default band scale with an empty domain and range of `[0.0, 1.0]`.
    fn default() -> Self {
        let mut scale = Self {
            domain: Vec::new(),
            range: (0.0, 1.0),
            range_map: HashMap::new(),
            step: 0.0,
            bandwidth: 0.0,
            round: false,
            padding_inner: 0.0,
            padding_outer: 0.0,
            align: 0.5,
        };
        scale.rescale();
        scale
    }
}

impl LinearRangeable<f64> for ScaleBand {
    /// Gets the scale's range.
    fn get_range(&self) -> (f64, f64) {
        self.range
    }
}

impl Tickable<String> for ScaleBand {
    /// Returns the domain values as "ticks" for axis rendering.
    fn ticks(&self, count: Option<usize>) -> Vec<String> {
        self.domain.clone()
    }
}

impl ScaleBand {
    /// Creates a new default band scale.
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets the scale's domain.
    pub fn get_domain(&self) -> &[String] {
        &self.domain
    }

    /// Sets the scale's domain.
    pub fn set_domain(&mut self, domain: Vec<String>) {
        self.domain = domain;
        self.rescale();
    }

    /// Sets the scale's range.
    pub fn set_range(&mut self, range: (f64, f64)) {
        self.range = range;
        self.rescale();
    }

    /// Sets the scale's range and enables rounding.
    pub fn set_range_round(&mut self, range: (f64, f64)) {
        self.range = range;
        self.round = true;
        self.rescale();
    }

    /// Gets the width of each band.
    pub fn bandwidth(&self) -> f64 {
        self.bandwidth
    }

    /// Gets the distance between the starts of adjacent bands.
    pub fn step(&self) -> f64 {
        self.step
    }

    /// Gets the scale's rounding status.
    pub fn get_round(&self) -> bool {
        self.round
    }

    /// Enables or disables rounding.
    pub fn set_round(&mut self, round: bool) {
        self.round = round;
        self.rescale();
    }

    /// Sets both inner and outer padding to the same value.
    /// The value should be in the range [0, 1].
    pub fn set_padding(&mut self, padding: f64) {
        self.padding_inner = padding.min(1.0);
        self.padding_outer = padding;
        self.rescale();
    }

    /// Gets the scale's inner padding.
    pub fn get_padding_inner(&self) -> f64 {
        self.padding_inner
    }

    /// Sets the inner padding between bands.
    /// The value should be in the range [0, 1].
    pub fn set_padding_inner(&mut self, padding: f64) {
        self.padding_inner = padding.min(1.0);
        self.rescale();
    }

    /// Gets the scale's outer padding.
    pub fn get_padding_outer(&self) -> f64 {
        self.padding_outer
    }

    /// Sets the outer padding before the first and after the last band.
    pub fn set_padding_outer(&mut self, padding: f64) {
        self.padding_outer = padding;
        self.rescale();
    }

    /// Gets the scale's alignment.
    pub fn get_align(&self) -> f64 {
        self.align
    }

    /// Sets the alignment of the bands within the range.
    /// The value should be in the range [0, 1], where 0.5 is centered.
    pub fn set_align(&mut self, align: f64) {
        self.align = align.max(0.0).min(1.0);
        self.rescale();
    }

    /// Recalculates the scale's internal state based on current settings.
    fn rescale(&mut self) {
        let n = self.domain.len();
        let (r0, r1) = self.range;
        let reverse = r1 < r0;
        let (start, stop) = if reverse { (r1, r0) } else { (r0, r1) };

        // Calculate step size
        let divisor = (n as f64 - self.padding_inner + self.padding_outer * 2.0).max(1.0);
        self.step = (stop - start) / divisor;

        if self.round {
            self.step = self.step.floor();
        }

        // Calculate adjusted start position
        let mut adjusted_start =
            start + (stop - start - self.step * (n as f64 - self.padding_inner)) * self.align;

        // Calculate bandwidth
        self.bandwidth = self.step * (1.0 - self.padding_inner);

        if self.round {
            adjusted_start = adjusted_start.round();
            self.bandwidth = self.bandwidth.round();
        }

        // Build the range map
        self.range_map.clear();
        for (i, key) in self.domain.iter().enumerate() {
            let value = adjusted_start + self.step * i as f64;
            let final_value = if reverse {
                // For reversed ranges, we need to reverse the positions
                stop - (value - start)
            } else {
                value
            };
            self.range_map.insert(key.clone(), final_value);
        }
    }
}

impl Scaleable<String, f64> for ScaleBand {
    /// Maps a domain value to its corresponding range position.
    /// Returns `None` if the value is not in the domain.
    fn scale(&self, value: &String) -> f64 {
        *self.range_map.get(value).unwrap()
    }
}

/// Returns approximately `count` ticks, i.e. suitable rounded values for display,
/// covering the given range `[start, stop]`.
pub fn ticks(start: f64, stop: f64, count: usize) -> Vec<f64> {
    let step = tick_step(start, stop, count);
    let start_ceil = (start / step).ceil();
    let stop_floor = (stop / step).floor();
    let n = (stop_floor - start_ceil + 1.0).ceil() as usize;

    let mut ticks = Vec::with_capacity(n);
    for i in 0..n {
        ticks.push((start_ceil + i as f64) * step);
    }
    ticks
}

/// Returns the step size for ticks.
pub fn tick_step(start: f64, stop: f64, count: usize) -> f64 {
    let step0 = (stop - start).abs() / (count as f64).max(0.0);
    let mut step1 = 10.0_f64.powf((step0.log10()).floor());
    let error = step0 / step1;

    let E10: f64 = 50.0_f64.sqrt();
    let E5: f64 = 10.0_f64.sqrt();
    let E2: f64 = 2.0_f64.sqrt();

    if error >= E10 {
        step1 *= 10.0;
    } else if error >= E5 {
        step1 *= 5.0;
    } else if error >= E2 {
        step1 *= 2.0;
    }

    if stop < start {
        -step1
    } else {
        step1
    }
}

/// Similar to `tick_step`, but guarantees that the returned value is an integer if the tick values are integers.
pub fn tick_increment(start: f64, stop: f64, count: usize) -> f64 {
    let step = (stop - start) / (count as f64).max(0.0);
    let power = (step.log10()).floor();
    let error = step / 10.0_f64.powf(power);

    let E10: f64 = 50.0_f64.sqrt();
    let E5: f64 = 10.0_f64.sqrt();
    let E2: f64 = 2.0_f64.sqrt();

    let increment = if power >= 0.0 {
        let factor = if error >= E10 {
            10.0
        } else if error >= E5 {
            5.0
        } else if error >= E2 {
            2.0
        } else {
            1.0
        };
        factor * 10.0_f64.powf(power)
    } else {
        -10.0_f64.powf(-power)
            / if error >= E10 {
                10.0
            } else if error >= E5 {
                5.0
            } else if error >= E2 {
                2.0
            } else {
                1.0
            }
    };
    increment
}

/// Extends the domain to start and end on nice round values.
pub fn nice(mut start: f64, mut stop: f64, count: usize) -> (f64, f64) {
    if start == stop {
        return (start, stop);
    }
    if stop < start {
        std::mem::swap(&mut start, &mut stop);
    }

    let mut pre_step = 0.0;
    for _ in 0..10 {
        let step = tick_increment(start, stop, count);
        if step == pre_step {
            break;
        }

        if step > 0.0 {
            start = (start / step).floor() * step;
            stop = (stop / step).ceil() * step;
        } else if step < 0.0 {
            start = (start * step).ceil() / step;
            stop = (stop * step).floor() / step;
        } else {
            break;
        }
        pre_step = step;
    }
    (start, stop)
}

/// Returns a number format function suitable for displaying a tick value.
pub fn tick_format(
    start: f64,
    stop: f64,
    count: usize,
    specifier: Option<&str>,
) -> impl Fn(f64) -> String {
    let step = tick_step(start, stop, count);
    let specifier = specifier.unwrap_or(",f");
    let precision = if let Some(idx) = specifier.find('.') {
        if let Some(end) = specifier[idx + 1..].chars().find(|c| !c.is_ascii_digit()) {
            specifier[idx + 1..specifier.len() - end.len_utf8()]
                .parse::<usize>()
                .unwrap_or(0)
        } else {
            specifier[idx + 1..].parse::<usize>().unwrap_or(0)
        }
    } else {
        (step.abs().log10().floor().max(0.0) * -1.0) as usize
    };

    move |d| format!("{:.prec$}", d, prec = precision)
}

/// Normalizes a value from a given domain to a value between 0 and 1.
fn normalize(a: f64, b: f64) -> impl Fn(f64) -> f64 {
    let diff = b - a;
    move |x| {
        if diff.abs() > 1e-6 {
            (x - a) / diff
        } else {
            0.5
        }
    }
}

/// Creates a linear interpolator function between two numbers.
fn interpolate_number(a: f64, b: f64) -> impl Fn(f64) -> f64 {
    let diff = b - a;
    move |t| a + diff * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scale_linear_defaults() {
        let s = ScaleLinear::new();
        assert_eq!(s.get_domain(), (0.0, 1.0));
        assert_eq!(s.get_range(), (0.0, 1.0));
        assert_eq!(s.get_clamp(), false);
    }

    #[test]
    fn test_scale_linear_range_sets_range() {
        let mut s = ScaleLinear::new();
        s.set_range((1.0, 2.0));
        assert_eq!(s.get_domain(), (0.0, 1.0));
        assert_eq!(s.get_range(), (1.0, 2.0));
        assert_eq!(s.scale(&0.5), 1.5);
    }

    #[test]
    fn test_scale_linear_domain_range_sets_domain_and_range() {
        let mut s = ScaleLinear::new();
        s.set_domain((1.0, 2.0));
        s.set_range((3.0, 4.0));
        assert_eq!(s.get_domain(), (1.0, 2.0));
        assert_eq!(s.get_range(), (3.0, 4.0));
        assert_eq!(s.scale(&1.5), 3.5);
    }

    #[test]
    fn test_linear_maps_domain_to_range() {
        let mut s = ScaleLinear::new();
        s.set_range((1.0, 2.0));
        assert_eq!(s.scale(&0.5), 1.5);
    }

    #[test]
    fn test_linear_clamp_true_restricts_output_to_range() {
        let mut s = ScaleLinear::new();
        s.set_clamp(true);
        s.set_range((10.0, 20.0));
        assert_eq!(s.scale(&2.0), 20.0);
        assert_eq!(s.scale(&-1.0), 10.0);
    }

    #[test]
    fn test_linear_nice_extends_domain() {
        let mut s = ScaleLinear::new();
        s.set_domain((0.0, 0.96));
        s.nice(None);
        assert_eq!(s.get_domain(), (0.0, 1.0));

        let mut s = ScaleLinear::new();
        s.set_domain((0.0, 96.0));
        s.nice(None);
        assert_eq!(s.get_domain(), (0.0, 100.0));
    }

    #[test]
    fn test_linear_ticks_returns_expected_ticks() {
        let s = ScaleLinear::new();
        let ticks = s.ticks(Some(10));
        let expected: Vec<f64> = vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
        assert_eq!(ticks.len(), expected.len());
        for (a, b) in ticks.iter().zip(expected.iter()) {
            assert!((a - b).abs() < 1e-9);
        }

        let mut s = ScaleLinear::new();
        s.set_domain((-100.0, 100.0));
        let ticks = s.ticks(Some(10));
        let expected: Vec<f64> = vec![
            -100.0, -80.0, -60.0, -40.0, -20.0, 0.0, 20.0, 40.0, 60.0, 80.0, 100.0,
        ];
        assert_eq!(ticks, expected);
    }

    #[test]
    fn test_linear_copy_isolates_domain_changes() {
        let mut x = ScaleLinear::new();
        let mut y = x.clone();
        x.set_domain((1.0, 2.0));
        assert_eq!(y.get_domain(), (0.0, 1.0));
        assert_eq!(x.scale(&1.0), 0.0);
        assert_eq!(y.scale(&1.0), 1.0);

        y.set_domain((2.0, 3.0));
        assert_eq!(x.scale(&2.0), 1.0);
        assert_eq!(y.scale(&2.0), 0.0);
        assert_eq!(x.get_domain(), (1.0, 2.0));
        assert_eq!(y.get_domain(), (2.0, 3.0));
    }

    #[test]
    fn test_linear_copy_isolates_range_changes() {
        let mut x = ScaleLinear::new();
        let mut y = x.clone();
        x.set_range((1.0, 2.0));
        assert_eq!(y.get_range(), (0.0, 1.0));

        y.set_range((2.0, 3.0));
        assert_eq!(x.get_range(), (1.0, 2.0));
        assert_eq!(y.get_range(), (2.0, 3.0));
    }

    #[test]
    fn test_linear_copy_isolates_clamp_changes() {
        let mut x = ScaleLinear::new();
        x.set_clamp(true);
        let mut y = x.clone();
        x.set_clamp(false);
        assert_eq!(x.scale(&2.0), 2.0);
        assert_eq!(y.scale(&2.0), 1.0);
        assert_eq!(y.get_clamp(), true);

        y.set_clamp(false);
        assert_eq!(x.scale(&2.0), 2.0);
        assert_eq!(y.scale(&2.0), 2.0);
        assert_eq!(x.get_clamp(), false);
    }

    // ScaleBand tests

    #[test]
    fn test_scale_band_defaults() {
        let s = ScaleBand::new();
        assert_eq!(s.get_domain().len(), 0);
        assert_eq!(s.get_range(), (0.0, 1.0));
        assert_eq!(s.get_round(), false);
        assert_eq!(s.get_padding_inner(), 0.0);
        assert_eq!(s.get_padding_outer(), 0.0);
        assert_eq!(s.get_align(), 0.5);
    }

    #[test]
    fn test_scale_band_domain_sets_domain() {
        let mut s = ScaleBand::new();
        s.set_domain(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        assert_eq!(s.get_domain(), &["a", "b", "c"]);
    }

    #[test]
    fn test_scale_band_maps_domain_to_range() {
        let mut s = ScaleBand::new();
        s.set_domain(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        s.set_range((0.0, 960.0));

        assert_eq!(s.scale(&"a".to_string()), 0.0);
        assert_eq!(s.scale(&"b".to_string()), 320.0);
        assert_eq!(s.scale(&"c".to_string()), 640.0);
        assert_eq!(s.bandwidth(), 320.0);
    }

    #[test]
    fn test_scale_band_with_padding() {
        let mut s = ScaleBand::new();
        s.set_domain(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        s.set_range((0.0, 960.0));
        s.set_padding_inner(0.1);
        s.set_padding_outer(0.2);

        let step = s.step();
        let bandwidth = s.bandwidth();

        // With 3 bands, padding_inner = 0.1, padding_outer = 0.2:
        // step = 960 / (3 - 0.1 + 0.2 * 2) = 960 / 3.3 ≈ 290.909...
        assert!((step - 290.909090909).abs() < 1e-6);

        // bandwidth = step * (1 - padding_inner) = step * 0.9
        assert!((bandwidth - step * 0.9).abs() < 1e-6);
    }

    #[test]
    fn test_scale_band_with_round() {
        let mut s = ScaleBand::new();
        s.set_domain(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        s.set_range_round((0.0, 100.0));

        // With rounding, step and positions should be integers
        let step = s.step();
        assert_eq!(step, step.floor());

        let a_pos = s.scale(&"a".to_string());
        assert_eq!(a_pos, a_pos.round());
    }

    #[test]
    fn test_scale_band_unknown_value() {
        let mut s = ScaleBand::new();
        s.set_domain(vec!["a".to_string(), "b".to_string()]);

        assert_eq!(s.scale(&"a".to_string()), 0.0);
        // TODO: assert error for unknown value instead
        // assert_eq!(s.scale(&"unknown".to_string()), 0.0);
    }

    #[test]
    fn test_scale_band_ticks() {
        let mut s = ScaleBand::new();
        s.set_domain(vec!["a".to_string(), "b".to_string(), "c".to_string()]);

        let ticks = s.ticks(None);
        assert_eq!(ticks, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_scale_band_copy_isolates_changes() {
        let mut x = ScaleBand::new();
        x.set_domain(vec!["a".to_string(), "b".to_string()]);

        let mut y = x.clone();
        y.set_domain(vec!["c".to_string(), "d".to_string()]);

        assert_eq!(x.get_domain(), &["a", "b"]);
        assert_eq!(y.get_domain(), &["c", "d"]);
    }

    #[test]
    fn test_scale_band_align() {
        let mut s = ScaleBand::new();
        s.set_domain(vec!["a".to_string(), "b".to_string()]);
        s.set_range((0.0, 100.0));
        s.set_padding(0.2);

        // Default align is 0.5 (centered)
        let default_a = s.scale(&"a".to_string());

        // Align to start (0.0)
        s.set_align(0.0);
        let start_a = s.scale(&"a".to_string());
        assert!(start_a < default_a);

        // Align to end (1.0)
        s.set_align(1.0);
        let end_a = s.scale(&"a".to_string());
        assert!(end_a > default_a);
    }
}
