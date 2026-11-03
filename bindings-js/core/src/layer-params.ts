import type { LayerParams as StrictLayerParams } from "./generated/LayerParams.js";

export type LayerType = StrictLayerParams["layer_type"];

/**
 * One entry of `plot_params.layers`, mirroring the adjacently-tagged Rust
 * `LayerParams` enum: `{ "layer_type": "PointLayer", "layer_params": { ... } }`.
 *
 * `layer_params` is `Partial` because most Rust layer params structs use
 * `#[serde(default)]`, which ts-rs does not translate into optional fields.
 */
export type LayerParams = {
  [V in StrictLayerParams as V["layer_type"]]: {
    layer_type: V["layer_type"];
    layer_params: Partial<V["layer_params"]>;
  };
}[LayerType];

export type LayerParamsOf<T extends LayerType> = Extract<LayerParams, { layer_type: T }>["layer_params"];

/** Mirrors the Rust `LayeredPlotRenderParams` struct. */
export type PlotParams = {
  layers: LayerParams[];
};
