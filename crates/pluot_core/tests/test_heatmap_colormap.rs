use pluot_core::layers::heatmap_layer::HeatmapColormap;

#[test]
fn categorical_cpu_color_wraps_codes() {
    let colormap = HeatmapColormap::CategoricalCustom(vec![(255, 0, 0), (0, 255, 0)]);
    assert_eq!(colormap.cpu_color(1.0, 0, 0), [0, 255, 0]);
    assert_eq!(colormap.cpu_color(-1.0, 0, 0), [0, 255, 0]);
    assert_eq!(colormap.cpu_color(2.0, 0, 0), [255, 0, 0]);
}
