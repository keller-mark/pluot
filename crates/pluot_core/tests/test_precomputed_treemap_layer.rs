use pluot_core::composite_layers::precomputed_treemap_layer::{build_hierarchy, format_value, label_lines, layout_leaves, LeafRect};
use pluot_core::d3::hierarchy::TreemapTile;

fn paths(paths: &[&[&str]]) -> Vec<Vec<String>> {
    paths.iter().map(|path| path.iter().map(|label| label.to_string()).collect()).collect()
}

#[test]
fn hierarchy_nests_leaves_by_shared_label_prefix() {
    let root = build_hierarchy(&paths(&[&["T", "s1"], &["B", "s1"], &["T", "s2"]]));
    assert_eq!(root.children.len(), 2);
    let t_cells = &root.children[0];
    assert_eq!(t_cells.data, None);
    assert_eq!(t_cells.children.iter().map(|c| c.data).collect::<Vec<_>>(), vec![Some(0), Some(2)]);
    assert_eq!(root.children[1].children[0].data, Some(1));
}

#[test]
fn single_level_paths_are_direct_children_of_the_root() {
    let root = build_hierarchy(&paths(&[&["T"], &["B"]]));
    assert_eq!(root.children.iter().map(|c| c.data).collect::<Vec<_>>(), vec![Some(0), Some(1)]);
}

#[test]
fn leaves_are_returned_in_input_order_with_areas_proportional_to_values() {
    let rects = layout_leaves(&paths(&[&["small"], &["large"]]), &[1.0, 3.0], (400.0, 100.0), TreemapTile::Binary, 0.0);
    let area = |r: &LeafRect| (r.x1 - r.x0) * (r.y1 - r.y0);
    assert_eq!(area(&rects[0]) * 3.0, area(&rects[1]));
    // Sorted by descending value, so the larger leaf is laid out first.
    assert_eq!(rects[1].x0, 0.0);
}

#[test]
fn non_positive_values_get_empty_rects() {
    let rects = layout_leaves(&paths(&[&["a"], &["b"]]), &[2.0, -1.0], (10.0, 10.0), TreemapTile::Binary, 0.0);
    assert!(rects[1].x1 <= rects[1].x0 || rects[1].y1 <= rects[1].y0);
    assert!(!rects[1].contains(rects[1].x0, rects[1].y0));
}

#[test]
fn values_are_formatted_with_thousands_separators() {
    assert_eq!(format_value(0.0), "0");
    assert_eq!(format_value(999.0), "999");
    assert_eq!(format_value(1234567.0), "1,234,567");
    assert_eq!(format_value(-1000.0), "-1,000");
    assert_eq!(format_value(1.5), "1.5");
}

#[test]
fn label_lines_list_innermost_label_first_then_the_value() {
    assert_eq!(label_lines(&["T".to_string(), "s1".to_string()], 1200.0), vec!["s1", "T", "1,200"]);
}
