---
name: pluot-layer-parameter-principles
description: Use when adding fields/properties/members to layer parameter structs.
---

## Option types

Unless a property absolutely must be specified explicitly, prefer Option types in which `None` is the default value of the property, such that `None` is a sentinel value which is interpreted to mean "I want the default behavior for this property".

## TypeScript types

Layer params structs, and every type they reference, must derive `ts_rs::TS` (alongside `Serialize` and `Deserialize`), so that `@pluot/core` gets strongly-typed `plot_params.layers`.

- New layer params structs must also be added as a variant of the `LayerParams` enum in `crates/pluot/src/render_params.rs`.
- Struct-level `#[serde(default)]` is handled on the TS side, where `layer_params` is wrapped in `Partial`.
- On nested (non-layer-params) structs that have `Option` fields, add `#[ts(optional_fields = nullable)]`, since serde allows those fields to be omitted.
- On fields with field-level `#[serde(default)]` or `#[serde(default = "...")]`, add `#[ts(optional = nullable)]`.
- For types from external crates, or types with custom `Serialize`/`Deserialize` impls, use `#[ts(type = "...")]` to give the TypeScript type of their serialized form (e.g., `OmeDimensionOrder` serializes to a `string`).

After changing any of these types, run `pnpm run generate-types`.
