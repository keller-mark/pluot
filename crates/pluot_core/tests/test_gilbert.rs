use pluot_core::gilbert::{gilbert_d2xy, gilbert_xy2d, item_at_curve_position};

/// A literal port of the recursive `generate2d` of the reference implementation.
fn generate2d(x: i64, y: i64, ax: i64, ay: i64, bx: i64, by: i64, out: &mut Vec<(u32, u32)>) {
    let (w, h) = ((ax + ay).abs(), (bx + by).abs());
    let (dax, day, dbx, dby) = (ax.signum(), ay.signum(), bx.signum(), by.signum());
    if h == 1 {
        (0..w).for_each(|i| out.push(((x + dax * i) as u32, (y + day * i) as u32)));
        return;
    }
    if w == 1 {
        (0..h).for_each(|i| out.push(((x + dbx * i) as u32, (y + dby * i) as u32)));
        return;
    }
    let (mut ax2, mut ay2) = (ax.div_euclid(2), ay.div_euclid(2));
    let (mut bx2, mut by2) = (bx.div_euclid(2), by.div_euclid(2));
    let (w2, h2) = ((ax2 + ay2).abs(), (bx2 + by2).abs());
    if 2 * w > 3 * h {
        if w2 % 2 == 1 && w > 2 {
            (ax2, ay2) = (ax2 + dax, ay2 + day);
        }
        generate2d(x, y, ax2, ay2, bx, by, out);
        generate2d(x + ax2, y + ay2, ax - ax2, ay - ay2, bx, by, out);
    } else {
        if h2 % 2 == 1 && h > 2 {
            (bx2, by2) = (bx2 + dbx, by2 + dby);
        }
        generate2d(x, y, bx2, by2, ax2, ay2, out);
        generate2d(x + bx2, y + by2, ax, ay, bx - bx2, by - by2, out);
        generate2d(x + (ax - dax) + (bx2 - dbx), y + (ay - day) + (by2 - dby), -bx2, -by2, -(ax - ax2), -(ay - ay2), out);
    }
}

fn reference_curve(width: u32, height: u32) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let (w, h) = (width as i64, height as i64);
    if w >= h { generate2d(0, 0, w, 0, 0, h, &mut out) } else { generate2d(0, 0, 0, h, w, 0, &mut out) }
    out
}

const SIZES: [(u32, u32); 12] = [(1, 1), (1, 7), (7, 1), (2, 2), (3, 5), (5, 3), (8, 8), (10, 7), (16, 9), (33, 20), (64, 64), (100, 37)];

#[test]
fn d2xy_matches_the_reference_curve() {
    for (width, height) in SIZES {
        let expected = reference_curve(width, height);
        assert_eq!(expected.len(), (width * height) as usize);
        for (index, &cell) in expected.iter().enumerate() {
            assert_eq!(gilbert_d2xy(index as u32, width, height), cell, "index {index} of {width}x{height}");
        }
    }
}

#[test]
fn xy2d_inverts_d2xy() {
    for (width, height) in SIZES {
        for (index, (x, y)) in reference_curve(width, height).into_iter().enumerate() {
            assert_eq!(gilbert_xy2d(x, y, width, height), index as u32, "({x}, {y}) of {width}x{height}");
        }
    }
}

#[test]
fn consecutive_cells_touch() {
    for (width, height) in SIZES {
        let curve = reference_curve(width, height);
        for pair in curve.windows(2) {
            let (dx, dy) = (pair[0].0.abs_diff(pair[1].0), pair[0].1.abs_diff(pair[1].1));
            assert!(dx <= 1 && dy <= 1, "{:?} to {:?} of {width}x{height}", pair[0], pair[1]);
        }
    }
}

#[test]
fn items_are_spread_evenly_along_the_curve() {
    let items: Vec<u32> = (0..10).map(|position| item_at_curve_position(position, 4, 10)).collect();
    assert_eq!(items, vec![0, 0, 0, 1, 1, 2, 2, 2, 3, 3]);
    let items: Vec<u32> = (0..4).map(|position| item_at_curve_position(position, 10, 4)).collect();
    assert_eq!(items, vec![0, 2, 5, 7]);
    assert_eq!(item_at_curve_position(u32::MAX - 1, u32::MAX, u32::MAX), u32::MAX - 1);
}
