---
"@pluot/core": patch
"@pluot/react": patch
---

Remove the unused Rust-side brush query: `brush_wasm`, `brush_py`, the `BrushableLayer` trait, and the `BrushingResult`/`LayerBrushingResult`/`BrushingInfo`/`BrushResult` types. `onBrush` and `onBrushEnd` now receive only the brush state.
