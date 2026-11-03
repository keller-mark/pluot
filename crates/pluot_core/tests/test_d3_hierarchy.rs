use pluot_core::d3::hierarchy::{HierarchyNode, Treemap, TreemapTile};

fn leaf(value: f64) -> HierarchyNode<f64> {
    HierarchyNode::new(value, vec![])
}

fn bounds(node: &HierarchyNode<f64>) -> (f64, f64, f64, f64) {
    (node.x0, node.y0, node.x1, node.y1)
}

#[test]
fn sum_includes_descendants() {
    let mut root = HierarchyNode::new(0.0, vec![leaf(1.0), HierarchyNode::new(0.5, vec![leaf(2.0), leaf(f64::NAN)])]);
    root.sum(&|d| *d);
    assert_eq!(root.value, 3.5);
    assert_eq!(root.children[1].value, 2.5);
}

#[test]
fn sort_by_is_recursive() {
    let mut root = HierarchyNode::new(0.0, vec![leaf(1.0), HierarchyNode::new(0.0, vec![leaf(2.0), leaf(5.0)])]);
    root.sum(&|d| *d).sort_by(&|a, b| b.value.total_cmp(&a.value));
    assert_eq!(root.children[0].value, 7.0);
    assert_eq!(root.children[0].children[0].value, 5.0);
}

#[test]
fn dice_and_slice_divide_proportionally() {
    let mut root = HierarchyNode::new(0.0, vec![leaf(1.0), leaf(3.0)]);
    root.sum(&|d| *d);
    let mut treemap = Treemap::new();
    treemap.set_size((8.0, 4.0)).set_tile(TreemapTile::Dice);
    treemap.layout(&mut root);
    assert_eq!(bounds(&root.children[0]), (0.0, 0.0, 2.0, 4.0));
    assert_eq!(bounds(&root.children[1]), (2.0, 0.0, 8.0, 4.0));

    treemap.set_tile(TreemapTile::Slice);
    treemap.layout(&mut root);
    assert_eq!(bounds(&root.children[0]), (0.0, 0.0, 8.0, 1.0));
    assert_eq!(bounds(&root.children[1]), (0.0, 1.0, 8.0, 4.0));
}

// Expected values traced by hand through d3-hierarchy's binary.js for
// d3.treemap().tile(d3.treemapBinary).size([100, 50]) over values [6, 2, 1, 1].
#[test]
fn binary_matches_d3() {
    let mut root = HierarchyNode::new(0.0, vec![leaf(6.0), leaf(2.0), leaf(1.0), leaf(1.0)]);
    root.sum(&|d| *d);
    let mut treemap = Treemap::new();
    treemap.set_size((100.0, 50.0));
    treemap.layout(&mut root);
    assert_eq!(bounds(&root.children[0]), (0.0, 0.0, 60.0, 50.0));
    assert_eq!(bounds(&root.children[1]), (60.0, 0.0, 100.0, 25.0));
    assert_eq!(bounds(&root.children[2]), (60.0, 25.0, 80.0, 50.0));
    assert_eq!(bounds(&root.children[3]), (80.0, 25.0, 100.0, 50.0));
}

#[test]
fn padding_insets_children_and_round_snaps_to_integers() {
    let mut root = HierarchyNode::new(0.0, vec![HierarchyNode::new(0.0, vec![leaf(1.0), leaf(2.0)])]);
    root.sum(&|d| *d);
    let mut treemap = Treemap::new();
    treemap.set_size((30.0, 10.0)).set_padding(1.0).set_round(true);
    treemap.layout(&mut root);

    let group = &root.children[0];
    assert_eq!(bounds(group), (1.0, 1.0, 29.0, 9.0));
    for leaf in group.leaves() {
        assert!(leaf.x0 >= group.x0 + 1.0 && leaf.x1 <= group.x1 - 1.0);
        assert_eq!(leaf.x0.fract(), 0.0);
    }
}

#[test]
fn zero_valued_children_collapse() {
    let mut root = HierarchyNode::new(0.0, vec![leaf(0.0), leaf(0.0)]);
    root.sum(&|d| *d);
    let mut treemap = Treemap::new();
    treemap.set_size((10.0, 10.0));
    treemap.layout(&mut root);
    for leaf in root.leaves() {
        assert!(leaf.x0.is_finite() && leaf.y0.is_finite());
    }
}
