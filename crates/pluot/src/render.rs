use pluot_core::{LayerParams as RawLayerParams, RenderParams as RawRenderParams, StoreMap};
use pluot_core::{render as raw_render, stores_from_params};
use pluot_core::params::{GraphicsFormat, PlotParams, LayeredPlotRenderParams as RawLayeredPlotRenderParams};
use crate::render_params::{LayerParams, RenderParams};

fn to_raw_layer_params(layers: &[LayerParams]) -> Vec<RawLayerParams> {
    layers.iter().map(|layer| {
        // LayerParams is tagged as { "layer_type": "...", "layer_params": {...} }
        // which matches the fields of RawLayerParams exactly.
        let value = serde_json::to_value(layer).expect("LayerParams serialization failed");
        let obj = value.as_object().expect("LayerParams must serialize to an object");
        RawLayerParams {
            layer_type: obj["layer_type"].as_str().expect("layer_type must be a string").to_string(),
            layer_params: obj["layer_params"].clone(),
        }
    }).collect()
}

fn to_raw_render_params(render_params: RenderParams) -> RawRenderParams {
    let raw_layers = to_raw_layer_params(&render_params.layers);
    RawRenderParams {
        width: render_params.width,
        height: render_params.height,
        format: render_params.format,
        device_pixel_ratio: render_params.device_pixel_ratio,
        camera_view: render_params.camera_view,
        aspect_ratio_mode: render_params.aspect_ratio_mode,
        aspect_ratio_alignment_mode: render_params.aspect_ratio_alignment_mode,
        view_mode: render_params.view_mode,
        plot_id: render_params.plot_id,
        stores: render_params.stores,
        wait_for_store_gets: render_params.wait_for_store_gets,
        timeout: render_params.timeout,
        cache_enabled: render_params.cache_enabled,
        svg_compression_enabled: render_params.svg_compression_enabled,
        svg_include_document: render_params.svg_include_document,
        margin_left: render_params.margin_left,
        margin_right: render_params.margin_right,
        margin_top: render_params.margin_top,
        margin_bottom: render_params.margin_bottom,
        pickable: render_params.pickable,
        render_backend: render_params.render_backend,
        compute_backend: render_params.compute_backend,
        plot_params: PlotParams::LayeredPlot(RawLayeredPlotRenderParams {
            layers: raw_layers,
        }),
    }
}

// TODO: nicer return type. wrap with raster/vector variants?

pub async fn render(render_params: RenderParams) -> Vec<u8> {
    let raw_params = to_raw_render_params(render_params);
    // Construct the store objects from the store metadata and pass them in,
    // rather than registering them in the global store registry.
    let stores = stores_from_params(&raw_params);
    raw_render(raw_params, stores).await
}

/// Like [`render`], but lets the caller pass the [`StoreMap`] of store objects
/// directly instead of having them constructed from `render_params.stores`
/// metadata. Useful for Rust callers that already have store objects on hand
/// (e.g. native zarrs stores) rather than the `zarr_*` binding functions that
/// [`stores_from_params`] dispatches to.
pub async fn render_with_stores(render_params: RenderParams, stores: Option<StoreMap>) -> Vec<u8> {
    let raw_params = to_raw_render_params(render_params);
    raw_render(raw_params, stores).await
}

/// Serialize `render_params` into code (source or JSON) in the language and
/// flavor implied by `format`, decoupled from `render_params.format`.
///
/// [`render`]/[`render_with_stores`] always pass `render_params.format` as both
/// the params and the code target, so a request for e.g. `ScriptPython` can
/// only ever describe raster output. Calling this function directly instead
/// lets `render_params.format` carry the *real* desired output (`Raster` or
/// `Vector`) while `format` independently selects the code target.
pub fn render_to_script(render_params: RenderParams, format: &GraphicsFormat) -> String {
    let raw_params = to_raw_render_params(render_params);
    pluot_core::render_to_script(&raw_params, format)
}
