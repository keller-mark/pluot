//! Generalized Hilbert ("gilbert") space-filling curve over a grid of any
//! width and height. Iterative ports of `gilbert_d2xy` and `gilbert_xy2d` from
//! https://github.com/jakubcerveny/gilbert (BSD 2-Clause License, Copyright (c)
//! 2018, Jakub Červený), mirrored in WGSL by `wgsl_functions/gilbert_xy2d.wgsl`.

/// The start corner and the major (`a`) and minor (`b`) axis vectors of one
/// sub-rectangle of the recursion.
#[derive(Clone, Copy)]
struct Block {
    x: i64,
    y: i64,
    ax: i64,
    ay: i64,
    bx: i64,
    by: i64,
}

impl Block {
    fn root(width: u32, height: u32) -> Self {
        let (w, h) = (width as i64, height as i64);
        if w >= h {
            Block { x: 0, y: 0, ax: w, ay: 0, bx: 0, by: h }
        } else {
            Block { x: 0, y: 0, ax: 0, ay: h, bx: w, by: 0 }
        }
    }

    fn contains(&self, x: i64, y: i64) -> bool {
        let within = |p: i64, start: i64, extent: i64| if extent < 0 { p <= start && p > start + extent } else { p >= start && p < start + extent };
        within(x, self.x, self.ax + self.bx) && within(y, self.y, self.ay + self.by)
    }

    fn len(&self) -> i64 {
        ((self.ax + self.ay) * (self.bx + self.by)).abs()
    }

    /// The sub-blocks that the curve visits in order, or `None` when this block
    /// is a single row or column traversed directly.
    fn split(&self) -> Option<Vec<Block>> {
        let Block { x, y, ax, ay, bx, by } = *self;
        let (w, h) = ((ax + ay).abs(), (bx + by).abs());
        if w == 1 || h == 1 {
            return None;
        }
        let (dax, day, dbx, dby) = (ax.signum(), ay.signum(), bx.signum(), by.signum());
        let (mut ax2, mut ay2) = (ax.div_euclid(2), ay.div_euclid(2));
        let (mut bx2, mut by2) = (bx.div_euclid(2), by.div_euclid(2));
        let (w2, h2) = ((ax2 + ay2).abs(), (bx2 + by2).abs());

        if 2 * w > 3 * h {
            if w2 % 2 == 1 && w > 2 {
                (ax2, ay2) = (ax2 + dax, ay2 + day);
            }
            return Some(vec![
                Block { x, y, ax: ax2, ay: ay2, bx, by },
                Block { x: x + ax2, y: y + ay2, ax: ax - ax2, ay: ay - ay2, bx, by },
            ]);
        }

        if h2 % 2 == 1 && h > 2 {
            (bx2, by2) = (bx2 + dbx, by2 + dby);
        }
        Some(vec![
            Block { x, y, ax: bx2, ay: by2, bx: ax2, by: ay2 },
            Block { x: x + bx2, y: y + by2, ax, ay, bx: bx - bx2, by: by - by2 },
            Block {
                x: x + (ax - dax) + (bx2 - dbx),
                y: y + (ay - day) + (by2 - dby),
                ax: -bx2,
                ay: -by2,
                bx: -(ax - ax2),
                by: -(ay - ay2),
            },
        ])
    }

    /// The unit step along this single-row or single-column block.
    fn step(&self) -> (i64, i64) {
        if (self.bx + self.by).abs() == 1 { (self.ax.signum(), self.ay.signum()) } else { (self.bx.signum(), self.by.signum()) }
    }
}

/// The `(x, y)` cell at position `index` (below `width * height`) along the curve.
pub fn gilbert_d2xy(index: u32, width: u32, height: u32) -> (u32, u32) {
    assert!((index as u64) < width as u64 * height as u64, "gilbert_d2xy: index {index} is outside of the {width}x{height} grid");
    let mut block = Block::root(width, height);
    let mut start = 0i64;
    let index = index as i64;
    while let Some(sub_blocks) = block.split() {
        for sub_block in sub_blocks {
            if index < start + sub_block.len() {
                block = sub_block;
                break;
            }
            start += sub_block.len();
        }
    }
    let (dx, dy) = block.step();
    let offset = index - start;
    ((block.x + dx * offset) as u32, (block.y + dy * offset) as u32)
}

/// The position along the curve of cell `(x, y)`, which must be within the `width` x `height` grid.
pub fn gilbert_xy2d(x: u32, y: u32, width: u32, height: u32) -> u32 {
    assert!(x < width && y < height, "gilbert_xy2d: ({x}, {y}) is outside of the {width}x{height} grid");
    let (x, y) = (x as i64, y as i64);
    let mut block = Block::root(width, height);
    let mut start = 0i64;
    while let Some(sub_blocks) = block.split() {
        for sub_block in sub_blocks {
            if sub_block.contains(x, y) {
                block = sub_block;
                break;
            }
            start += sub_block.len();
        }
    }
    let (dx, dy) = block.step();
    (start + dx * (x - block.x) + dy * (y - block.y)) as u32
}

/// The item at curve position `position` when `num_items` items are spread
/// evenly, in order, along a curve of `curve_len` cells.
pub fn item_at_curve_position(position: u32, num_items: u32, curve_len: u32) -> u32 {
    (position as u64 * num_items as u64 / curve_len.max(1) as u64) as u32
}
