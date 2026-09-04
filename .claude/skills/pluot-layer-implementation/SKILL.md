---
name: pluot-layer-implementation
description: Use when implementing layers for Pluot in Rust.
---

- Avoid iterating over data arrays when plotting via the GPU. Upload as textures and perform logic such as filtering, coloring, sizing via WGSL on the GPU. Minimize GPU passes when possible.
- Avoid casting data arrays that have been loaded via zarr - preserve the original dtype by using NumericData unless you need to do a full pass over the data to transform it in some way. Implementing or using accessors which index into a large array and cast individual elements is a form of casting and therefore such workarounds should also be avoided.
