We need to port some D3 functionality to Rust. (Obviously not all of D3, but essential things like scales and axes.)

There appear to be some other D3 porting attempts as well:
- https://github.com/askanium/rustplotlib
- https://github.com/martinfrances107/rust_d3_geo

Side note: Due to the fact that D3 was developed prior to many modern JavaScript features that we take for granted (classes, for example), it is not as straightforward of an exercise as may be expected to directly port the code from JS to Rust.
