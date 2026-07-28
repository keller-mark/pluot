//! Rust ports of a small subset of D3 functionality,
//! such as axes and scales.

// Note: There appear to be some other D3 porting attempts as well:
// - https://github.com/askanium/rustplotlib
// - https://github.com/martinfrances107/rust_d3_geo
// - https://github.com/pierreaubert/gpui-toolkit/tree/main/gpui-d3rs

// Side note: Due to the fact that D3 was developed prior to many modern
// JavaScript features that we take for granted (classes, for example),
// it can be unclear what form the ported implementation should take.

pub mod axis;
pub mod scale;
