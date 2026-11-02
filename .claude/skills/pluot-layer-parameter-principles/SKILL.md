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

## Scverse data structure parameters: AnnData, MuData, SpatialData

When defining parameters for AnnData layers (layers prefixed with the name `Adata` or `adata_`), references to particular dataframe columns or items within key-value mappings such as `obsm`/`varm` should require the user to specify the entire path to the column or mapping, relative to the root of the AnnData object.
For example, rather than `var_column: "highly_variable"` with an implicit assumption that this column is within the `var` dataframe, the parameter should be `var_column_path: "var/highly_variable"`.
This ensures that the code is also compatible with MuData or SpatialData objects, where the path to such a column may be nested one or more levels deeper, such as `var_column_path: "mod/rna/var/highly_variable"` or `var_column_path: "tables/my_table/var/highly_variable"`.

For expression matrices, rather than `layer`, prefer `layer_path` which can then accept `layer_path: "X"`, `layer_path: "layers/log1p"`, `layer_path: "tables/my_table/layers/log1p"`, etc.

For var index values (e.g., gene names) and obs index values (e.g., cell barcodes), by default use the dataframe indices specified in the Zarr attrs metadata. However, always allow the user to specify the path to these index columns explicitly (in case the index stores identifiers using a different system such as ENSG gene IDs yet the user wants to refer to genes via Gene Symbols which are stored in a different dataframe column).

When defining `groupby` parameters, when possible, support nested groupbys defined in terms of column paths from coarsest to finest-grained groupings using a value type such as `Vec<String>`.
