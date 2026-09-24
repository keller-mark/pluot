---
"pluot_widget": patch
---

Refactor the Pluot python package into three packages: pluot (bindings to Rust), pluot_widget (WASM-based anywidget, without dependency on Rust bindings), and pluot_core (shared functions for both pluot and pluot_widget).
