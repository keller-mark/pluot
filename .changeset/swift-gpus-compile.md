---
"@pluot/core": patch
---

Reduce Rust compile times of `pluot_core` and `pluot_zarr` (from ~6 minutes to under a minute for a clean `cargo build`) by avoiding repeated `Send` proofs through wgpu types: add manual `Send`/`Sync` impls for `GpuContext`, add a `GpuRenderPass` wrapper that `DrawToRasterGpu::draw` now takes instead of `wgpu::RenderPass`, and box the futures returned by the GPU compute functions.
