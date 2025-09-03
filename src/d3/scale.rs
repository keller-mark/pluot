//! This module is a port of the D3.js `d3-scale` library, specifically focusing on linear scales.
//! Original source: <https://github.com/d3/d3-scale>

use std::borrow::Borrow;

// Reference: https://github.com/d3/d3-scale/blob/main/src/linear.js

/// A trait for scales that map a domain to a range.
pub trait Scale<D, R> {
    /// Given a value from the domain, returns the corresponding value in the range.
    fn scale(&self, value: &D) -> R;

    /// Gets the scale's domain.
    fn get_domain(&self) -> (D, D);

    /// Sets the scale's domain.
    fn set_domain(self, domain: (D, D)) -> Self;

    /// Gets the scale's range.
    fn get_range(&self) -> (R, R);

    /// Sets the scale's range.
    fn set_range(self, range: (R, R)) -> Self;
}

/// A continuous scale.
#[derive(Debug, Clone)]
pub struct ScaleContinuous {
    domain: (f64, f64),
    range: (f64, f64),
    clamp: bool,
}

impl Default for ScaleContinuous {
    /// Creates a default continuous scale with a domain and range of `[0.0, 1.0]`.
    fn default() -> Self {
        Self {
            domain: (0.0, 1.0),
            range: (0.0, 1.0),
            clamp: false,
        }
    }
}

impl ScaleContinuous {
    /// Creates a new default continuous scale.
    pub fn new() -> Self {
        Self::default()
    }

    /// Given a value in the domain, returns the corresponding value in the range.
    pub fn scale(&self, x: &f64) -> f64 {
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

    /// Gets the scale's clamp status.
    pub fn get_clamp(&self) -> bool {
        self.clamp
    }

    /// Enables or disables clamping of the input value to the domain.
    pub fn set_clamp(mut self, clamp: bool) -> Self {
        self.clamp = clamp;
        self
    }

    fn get_domain(&self) -> (f64, f64) {
        self.domain
    }

    fn set_domain(mut self, domain: (f64, f64)) -> Self {
        self.domain = domain;
        self
    }

    fn get_range(&self) -> (f64, f64) {
        self.range
    }

    fn set_range(mut self, range: (f64, f64)) -> Self {
        self.range = range;
        self
    }
}

/// A linear scale. This is a continuous scale with a linear relationship.
#[derive(Debug, Clone)]
pub struct ScaleLinear {
    continuous: ScaleContinuous,
}

impl Default for ScaleLinear {
    fn default() -> Self {
        Self {
            continuous: ScaleContinuous::new(),
        }
    }
}

impl ScaleLinear {
    /// Creates a new default linear scale.
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets the scale's clamp status.
    pub fn get_clamp(&self) -> bool {
        self.continuous.get_clamp()
    }

    /// Enables or disables clamping.
    pub fn set_clamp(mut self, clamp: bool) -> Self {
        self.continuous = self.continuous.set_clamp(clamp);
        self
    }

    /// Returns approximately `count` ticks from the scale's domain.
    pub fn ticks(&self, count: Option<usize>) -> Vec<f64> {
        let count = count.unwrap_or(10);
        let (start, stop) = self.get_domain();
        ticks(start, stop, count)
    }

    /// Rounds the start and end of the domain to "nice" numbers.
    pub fn nice(mut self, count: Option<usize>) -> Self {
        let count = count.unwrap_or(10);
        let (start, stop) = self.get_domain();
        let (new_start, new_stop) = nice(start, stop, count);
        self.continuous = self.continuous.set_domain((new_start, new_stop));
        self
    }
}

impl Scale<f64, f64> for ScaleLinear {
    fn scale(&self, value: &f64) -> f64 {
        self.continuous.scale(value)
    }

    fn get_domain(&self) -> (f64, f64) {
        self.continuous.get_domain()
    }

    fn set_domain(mut self, domain: (f64, f64)) -> Self {
        self.continuous = self.continuous.set_domain(domain);
        self
    }

    fn get_range(&self) -> (f64, f64) {
        self.continuous.get_range()
    }

    fn set_range(mut self, range: (f64, f64)) -> Self {
        self.continuous = self.continuous.set_range(range);
        self
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
        if let Some(end) = specifier[idx + 1..].chars().find(|c| !c.is_digit(10)) {
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
        let s = ScaleLinear::new().set_range((1.0, 2.0));
        assert_eq!(s.get_domain(), (0.0, 1.0));
        assert_eq!(s.get_range(), (1.0, 2.0));
        assert_eq!(s.scale(&0.5), 1.5);
    }

    #[test]
    fn test_scale_linear_domain_range_sets_domain_and_range() {
        let s = ScaleLinear::new()
            .set_domain((1.0, 2.0))
            .set_range((3.0, 4.0));
        assert_eq!(s.get_domain(), (1.0, 2.0));
        assert_eq!(s.get_range(), (3.0, 4.0));
        assert_eq!(s.scale(&1.5), 3.5);
    }

    #[test]
    fn test_linear_maps_domain_to_range() {
        assert_eq!(ScaleLinear::new().set_range((1.0, 2.0)).scale(&0.5), 1.5);
    }

    #[test]
    fn test_linear_clamp_true_restricts_output_to_range() {
        let s = ScaleLinear::new().set_clamp(true).set_range((10.0, 20.0));
        assert_eq!(s.scale(&2.0), 20.0);
        assert_eq!(s.scale(&-1.0), 10.0);
    }

    #[test]
    fn test_linear_nice_extends_domain() {
        let s = ScaleLinear::new().set_domain((0.0, 0.96)).nice(None);
        assert_eq!(s.get_domain(), (0.0, 1.0));

        let s = ScaleLinear::new().set_domain((0.0, 96.0)).nice(None);
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

        let s = ScaleLinear::new().set_domain((-100.0, 100.0));
        let ticks = s.ticks(Some(10));
        let expected: Vec<f64> = vec![
            -100.0, -80.0, -60.0, -40.0, -20.0, 0.0, 20.0, 40.0, 60.0, 80.0, 100.0,
        ];
        assert_eq!(ticks, expected);
    }

    #[test]
    fn test_linear_copy_isolates_domain_changes() {
        let x = ScaleLinear::new();
        let mut y = x.clone();
        let x = x.set_domain((1.0, 2.0));
        assert_eq!(y.get_domain(), (0.0, 1.0));
        assert_eq!(x.scale(&1.0), 0.0);
        assert_eq!(y.scale(&1.0), 1.0);

        y = y.set_domain((2.0, 3.0));
        assert_eq!(x.scale(&2.0), 1.0);
        assert_eq!(y.scale(&2.0), 0.0);
        assert_eq!(x.get_domain(), (1.0, 2.0));
        assert_eq!(y.get_domain(), (2.0, 3.0));
    }

    #[test]
    fn test_linear_copy_isolates_range_changes() {
        let x = ScaleLinear::new();
        let mut y = x.clone();
        let x = x.set_range((1.0, 2.0));
        assert_eq!(y.get_range(), (0.0, 1.0));

        y = y.set_range((2.0, 3.0));
        assert_eq!(x.get_range(), (1.0, 2.0));
        assert_eq!(y.get_range(), (2.0, 3.0));
    }

    #[test]
    fn test_linear_copy_isolates_clamp_changes() {
        let x = ScaleLinear::new().set_clamp(true);
        let mut y = x.clone();
        let x = x.set_clamp(false);
        assert_eq!(x.scale(&2.0), 2.0);
        assert_eq!(y.scale(&2.0), 1.0);
        assert_eq!(y.get_clamp(), true);

        y = y.set_clamp(false);
        assert_eq!(x.scale(&2.0), 2.0);
        assert_eq!(y.scale(&2.0), 2.0);
        assert_eq!(x.get_clamp(), false);
    }
}
