// We only run this test on non-WASM targets AND when the "lacks_gpu" feature is not enabled (e.g., CI).
#![cfg(all(not(target_arch = "wasm32"), not(feature="lacks_gpu")))]


use pluot::{render, RenderParams, PlotParams, LayeredPlotRenderParams, GraphicsFormat, AspectRatioMode, LayerParams, ScatterplotLayerParams, UnitsMode, ViewParams, MarginParams, PointShapeMode};

#[tokio::test]
async fn test_render_unit_square_raster() {
    let params = RenderParams {
        width: 100,
        height: 100,
        format: GraphicsFormat::Raster,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: vec![
                LayerParams::ScatterplotLayer(ScatterplotLayerParams {
                    layer_id: "my_scatterplot_layer".to_string(),
                    bounds: Some(MarginParams {
                        margin_left: Some(0.0),
                        margin_right: Some(0.0),
                        margin_top: Some(0.0),
                        margin_bottom: Some(0.0),
                    }),
                    data_unit_mode: UnitsMode::Data,
                    point_radius: 10.0,
                    point_radius_unit_mode: UnitsMode::Pixels,
                    point_shape_mode: PointShapeMode::Square,
                    x_vec: vec![0.0, 1.0, 1.0, 0.0],
                    y_vec: vec![0.0, 0.0, 1.0, 1.0],
                    labels_vec: vec![0, 1, 2, 3],
                }),
            ],
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    let result_vec = render(params).await;

    let NUM_EXTRA_BYTES = 1;

    assert_eq!(
        result_vec.len(),
        ((100 * 100 * 4) + NUM_EXTRA_BYTES) as usize
    );

    let is_not_all_zero = result_vec.iter().any(|&x| x != 0);
    assert!(
        is_not_all_zero,
        "The rendered image should not be all black."
    );



}

// TODO: performance tests with many elements, both raster and svg formats

// To compare svg to raster, render svg using resvg
// Reference: https://github.com/linebender/resvg/blob/9876cd45dd461ac3083f584cc83e66473a3061ef/crates/resvg/examples/minimal.rs#L27
// TODO: use dify for pixel-based diffing
// Reference: https://github.com/emilk/egui/blob/fa78d25564a5dbcb546ff6db0a9e14cb603ba03b/crates/egui_kittest/src/snapshot.rs#L484
