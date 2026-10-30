//! Rust port of some functionality from `d3-hierarchy`.
// Reference: https://github.com/d3/d3-hierarchy

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A node of a hierarchy, analogous to the nodes returned by `d3.hierarchy`.
/// A node without children is a leaf.
#[derive(Debug, Clone, PartialEq)]
pub struct HierarchyNode<T> {
    pub data: T,
    pub children: Vec<HierarchyNode<T>>,
    /// Populated by [`HierarchyNode::sum`].
    pub value: f64,
    /// Populated by [`Treemap::layout`].
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl<T> HierarchyNode<T> {
    pub fn new(data: T, children: Vec<HierarchyNode<T>>) -> Self {
        Self { data, children, value: 0.0, x0: 0.0, y0: 0.0, x1: 0.0, y1: 0.0 }
    }

    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    /// Sets each node's value to its own `value(data)` plus the values of all of its descendants.
    /// Non-finite values are treated as zero.
    // Reference: https://github.com/d3/d3-hierarchy/blob/main/src/hierarchy/sum.js
    pub fn sum(&mut self, value: &impl Fn(&T) -> f64) -> &mut Self {
        let own_value = value(&self.data);
        let mut total = if own_value.is_finite() { own_value } else { 0.0 };
        for child in self.children.iter_mut() {
            total += child.sum(value).value;
        }
        self.value = total;
        self
    }

    /// Stably sorts the children of every node.
    // Reference: https://github.com/d3/d3-hierarchy/blob/main/src/hierarchy/sort.js
    pub fn sort_by(&mut self, compare: &impl Fn(&HierarchyNode<T>, &HierarchyNode<T>) -> Ordering) -> &mut Self {
        self.children.sort_by(compare);
        for child in self.children.iter_mut() {
            child.sort_by(compare);
        }
        self
    }

    /// Visits every node in pre-order.
    pub fn each_before<'a>(&'a self, callback: &mut impl FnMut(&'a HierarchyNode<T>)) {
        callback(self);
        for child in &self.children {
            child.each_before(callback);
        }
    }

    pub fn each_before_mut(&mut self, callback: &mut impl FnMut(&mut HierarchyNode<T>)) {
        callback(self);
        for child in self.children.iter_mut() {
            child.each_before_mut(callback);
        }
    }

    pub fn leaves(&self) -> Vec<&HierarchyNode<T>> {
        let mut leaves = Vec::new();
        self.each_before(&mut |node| {
            if node.is_leaf() {
                leaves.push(node);
            }
        });
        leaves
    }
}

/// The tiling method used by [`Treemap`] to subdivide a node among its children.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
pub enum TreemapTile {
    /// Recursively partitions the children into a balanced binary tree,
    /// splitting horizontally for wide rectangles and vertically for tall ones.
    #[default]
    Binary,
    /// Divides the rectangle horizontally, ordered left to right.
    Dice,
    /// Divides the rectangle vertically, ordered top to bottom.
    Slice,
}

impl TreemapTile {
    fn tile<T>(&self, parent: &mut HierarchyNode<T>, x0: f64, y0: f64, x1: f64, y1: f64) {
        match self {
            TreemapTile::Binary => tile_binary(parent, x0, y0, x1, y1),
            TreemapTile::Dice => tile_dice(parent, x0, y0, x1, y1),
            TreemapTile::Slice => tile_slice(parent, x0, y0, x1, y1),
        }
    }
}

// Reference: https://github.com/d3/d3-hierarchy/blob/main/src/treemap/dice.js
fn tile_dice<T>(parent: &mut HierarchyNode<T>, x0: f64, y0: f64, x1: f64, y1: f64) {
    let k = if parent.value != 0.0 { (x1 - x0) / parent.value } else { 0.0 };
    let mut x = x0;
    for node in parent.children.iter_mut() {
        node.y0 = y0;
        node.y1 = y1;
        node.x0 = x;
        x += node.value * k;
        node.x1 = x;
    }
}

// Reference: https://github.com/d3/d3-hierarchy/blob/main/src/treemap/slice.js
fn tile_slice<T>(parent: &mut HierarchyNode<T>, x0: f64, y0: f64, x1: f64, y1: f64) {
    let k = if parent.value != 0.0 { (y1 - y0) / parent.value } else { 0.0 };
    let mut y = y0;
    for node in parent.children.iter_mut() {
        node.x0 = x0;
        node.x1 = x1;
        node.y0 = y;
        y += node.value * k;
        node.y1 = y;
    }
}

// Reference: https://github.com/d3/d3-hierarchy/blob/main/src/treemap/binary.js
fn tile_binary<T>(parent: &mut HierarchyNode<T>, x0: f64, y0: f64, x1: f64, y1: f64) {
    let mut sums = Vec::with_capacity(parent.children.len() + 1);
    let mut sum = 0.0;
    sums.push(sum);
    for node in &parent.children {
        sum += node.value;
        sums.push(sum);
    }
    let n = parent.children.len();
    binary_partition(&mut parent.children, &sums, 0, n, parent.value, x0, y0, x1, y1);
}

fn binary_partition<T>(nodes: &mut [HierarchyNode<T>], sums: &[f64], i: usize, j: usize, value: f64, x0: f64, y0: f64, x1: f64, y1: f64) {
    if i + 1 >= j {
        let node = &mut nodes[i];
        node.x0 = x0;
        node.y0 = y0;
        node.x1 = x1;
        node.y1 = y1;
        return;
    }

    let value_offset = sums[i];
    let value_target = value / 2.0 + value_offset;
    let mut k = i + 1;
    let mut hi = j - 1;

    while k < hi {
        let mid = (k + hi) / 2;
        if sums[mid] < value_target {
            k = mid + 1;
        } else {
            hi = mid;
        }
    }

    if (value_target - sums[k - 1]) < (sums[k] - value_target) && i + 1 < k {
        k -= 1;
    }

    let value_left = sums[k] - value_offset;
    let value_right = value - value_left;

    if (x1 - x0) > (y1 - y0) {
        let xk = if value != 0.0 { (x0 * value_right + x1 * value_left) / value } else { x1 };
        binary_partition(nodes, sums, i, k, value_left, x0, y0, xk, y1);
        binary_partition(nodes, sums, k, j, value_right, xk, y0, x1, y1);
    } else {
        let yk = if value != 0.0 { (y0 * value_right + y1 * value_left) / value } else { y1 };
        binary_partition(nodes, sums, i, k, value_left, x0, y0, x1, yk);
        binary_partition(nodes, sums, k, j, value_right, x0, yk, x1, y1);
    }
}

/// Lays out a hierarchy as nested rectangles, with the root spanning `(0, 0)` to `size`.
/// Like D3, Y increases downwards.
// Reference: https://github.com/d3/d3-hierarchy/blob/main/src/treemap/index.js
#[derive(Debug, Clone)]
pub struct Treemap {
    tile: TreemapTile,
    size: (f64, f64),
    padding_inner: f64,
    padding_outer: f64,
    round: bool,
}

impl Default for Treemap {
    fn default() -> Self {
        Self {
            tile: TreemapTile::default(),
            size: (1.0, 1.0),
            padding_inner: 0.0,
            padding_outer: 0.0,
            round: false,
        }
    }
}

impl Treemap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_tile(&mut self, tile: TreemapTile) -> &mut Self {
        self.tile = tile;
        self
    }

    pub fn set_size(&mut self, size: (f64, f64)) -> &mut Self {
        self.size = size;
        self
    }

    /// Sets both the inner padding (between siblings) and the outer padding (between a parent and its children).
    pub fn set_padding(&mut self, padding: f64) -> &mut Self {
        self.padding_inner = padding;
        self.padding_outer = padding;
        self
    }

    pub fn set_padding_inner(&mut self, padding: f64) -> &mut Self {
        self.padding_inner = padding;
        self
    }

    pub fn set_padding_outer(&mut self, padding: f64) -> &mut Self {
        self.padding_outer = padding;
        self
    }

    pub fn set_round(&mut self, round: bool) -> &mut Self {
        self.round = round;
        self
    }

    /// Positions every node of `root`, whose values must already be populated via [`HierarchyNode::sum`].
    pub fn layout<T>(&self, root: &mut HierarchyNode<T>) {
        root.x0 = 0.0;
        root.y0 = 0.0;
        root.x1 = self.size.0;
        root.y1 = self.size.1;
        self.position_node(root, 0.0);
        if self.round {
            root.each_before_mut(&mut |node| {
                node.x0 = node.x0.round();
                node.y0 = node.y0.round();
                node.x1 = node.x1.round();
                node.y1 = node.y1.round();
            });
        }
    }

    fn position_node<T>(&self, node: &mut HierarchyNode<T>, padding: f64) {
        let (x0, x1) = collapse_if_inverted(node.x0 + padding, node.x1 - padding);
        let (y0, y1) = collapse_if_inverted(node.y0 + padding, node.y1 - padding);
        node.x0 = x0;
        node.y0 = y0;
        node.x1 = x1;
        node.y1 = y1;

        if node.is_leaf() {
            return;
        }
        let child_padding = self.padding_inner / 2.0;
        let inset = self.padding_outer - child_padding;
        let (x0, x1) = collapse_if_inverted(x0 + inset, x1 - inset);
        let (y0, y1) = collapse_if_inverted(y0 + inset, y1 - inset);
        self.tile.tile(node, x0, y0, x1, y1);
        for child in node.children.iter_mut() {
            self.position_node(child, child_padding);
        }
    }
}

fn collapse_if_inverted(lo: f64, hi: f64) -> (f64, f64) {
    if hi < lo {
        let mid = (lo + hi) / 2.0;
        (mid, mid)
    } else {
        (lo, hi)
    }
}
