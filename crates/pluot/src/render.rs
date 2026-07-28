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

/// Given plotting parameters as input, render a graphical (vector or bitmap) output.
pub async fn render(render_params: RenderParams) -> Vec<u8> {
    let raw_params = to_raw_render_params(render_params);
    // Construct the store objects from the store metadata and pass them in,
    // rather than registering them in the global store registry.
    let stores = stores_from_params(&raw_params);
    raw_render(raw_params, stores).await
}

/// Similar to [`render`], but also lets the caller pass Zarr store instances
/// via a [`StoreMap`].
pub async fn render_with_stores(render_params: RenderParams, stores: Option<StoreMap>) -> Vec<u8> {
    let raw_params = to_raw_render_params(render_params);
    raw_render(raw_params, stores).await
}

/// Given plotting parameters as input, "render" them to code which can be used to reproduce the plot.
pub fn render_to_script(render_params: RenderParams, format: &GraphicsFormat) -> String {
    let raw_params = to_raw_render_params(render_params);
    pluot_core::render_to_script(&raw_params, format)
}
