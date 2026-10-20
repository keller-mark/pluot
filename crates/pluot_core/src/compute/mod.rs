//! Module where functions that perform GPGPU compute operations (and their CPU fallbacks) are defined.
pub mod reduce;
pub mod reduce_stratified;

/// Paired background/foreground result of a compute operation run with
/// filtering/selection criteria.
///
/// `background` is computed over the filter-included set; `foreground` over
/// the filter-included *and* selection-included subset (a subset of
/// `background`, since selection narrows the filtered set further). When both
/// criteria lists passed to the compute function are empty, every item is
/// both filter-included and selected, so `background` and `foreground` are
/// identical.
#[derive(Debug, Clone, PartialEq)]
pub struct ForegroundBackground<T> {
    pub background: T,
    pub foreground: T,
}
