---
name: pluot-shader-modules
description: Use when you need to write or modify a WGSL shader.
---

In Pluot, we use a lightweight shader module system (ShaderBuilder) defined in `crates/pluot_core/src/shader_modules.rs`.
Prefer using or extending ShaderBuilder, rather than performing direct string manipulation (e.g., concatenation or replacement).
Using ShaderBuilder is cleaner, and facilitates snapshot-based unit testing of the final generated WGSL strings.
