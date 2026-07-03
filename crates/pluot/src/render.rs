use pluot_core::{LayerParams as RawLayerParams, RenderParams as RawRenderParams};
use pluot_core::{render as raw_render};
use pluot_core::params::{PlotParams, LayeredPlotRenderParams as RawLayeredPlotRenderParams};
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

// TODO: nicer return type. wrap with raster/vector variants?

pub async fn render(render_params: RenderParams) -> Vec<u8> {
    let raw_layers = to_raw_layer_params(&render_params.layers);
    let raw_params = RawRenderParams {
        width: render_params.width,
        height: render_params.height,
        format: render_params.format,
        device_pixel_ratio: render_params.device_pixel_ratio,
        camera_view: render_params.camera_view,
        aspect_ratio_mode: render_params.aspect_ratio_mode,
        aspect_ratio_alignment_mode: render_params.aspect_ratio_alignment_mode,
        view_mode: render_params.view_mode,
        plot_id: render_params.plot_id,
        store_name: render_params.store_name,
        wait_for_store_gets: render_params.wait_for_store_gets,
        wait_for_store_pushes: render_params.wait_for_store_pushes,
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
    };
    raw_render(raw_params).await
}
