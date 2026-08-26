//! Tests for the dtype-generic sparse-matrix helpers behind `adata_io`'s CSR and CSC column
//! readers, which resolve an AnnData `X` (or `layers` entry) column without widening `indptr` or
//! `indices` out of the dtype they were stored in.

use pluot_zarr::adata_io::{find_column_entries, rows_for_entries, scatter_column};

// The 3x4 matrix used throughout these tests:
//   row 0: [0, 5, 0, 7]
//   row 1: [0, 0, 0, 0]
//   row 2: [3, 0, 8, 0]
// CSR: indptr [0, 2, 2, 4], indices [1, 3, 0, 2], data [5, 7, 3, 8]
// CSC: indptr [0, 1, 2, 3, 4], indices [2, 0, 2, 0], data [3, 5, 8, 7]
const DENSE: [[i32; 4]; 3] = [[0, 5, 0, 7], [0, 0, 0, 0], [3, 0, 8, 0]];

/// Mirrors `read_csr_column_values` over already-read arrays, traversing them in the same nested
/// fixed-size steps so that the block/span boundary arithmetic is exercised. `budget` stands in for
/// `MAX_ELEMENTS_PER_READ`; values well below the matrix size force the multi-block path that a
/// realistic budget would only reach on a very large matrix.
fn csr_column<P, C>(indptr: &[P], indices: &[C], data: &[i32], col_index: u64, n_obs: usize, budget: u64) -> Vec<i32>
where
    P: Copy + Ord + TryFrom<u64> + TryInto<u64>,
    C: Copy + Eq + TryFrom<u64>,
{
    let offset = |row: usize| -> u64 { indptr[row].try_into().ok().unwrap() };
    let per_read = budget as usize;

    let mut column = vec![0i32; n_obs];
    for first_row in (0..n_obs).step_by(per_read) {
        let rows_in_block = per_read.min(n_obs - first_row);
        let indptr_block = &indptr[first_row..=first_row + rows_in_block];
        let (block_start, block_stop) = (offset(first_row), offset(first_row + rows_in_block));

        for span_start in (block_start..block_stop).step_by(per_read) {
            let span_stop = (span_start + budget).min(block_stop);
            let span = &indices[span_start as usize..span_stop as usize];
            let matches = find_column_entries(span, col_index);
            if matches.is_empty() {
                continue;
            }
            let positions: Vec<u64> = matches.iter().map(|&p| span_start + p as u64).collect();
            let rows = rows_for_entries(indptr_block, &positions);
            for (&position, &row) in matches.iter().zip(rows.iter()) {
                column[first_row + row] = data[span_start as usize + position];
            }
        }
    }
    column
}

#[test]
fn csr_extraction_matches_the_dense_matrix_at_every_budget() {
    let indptr: [i32; 4] = [0, 2, 2, 4];
    let indices: [i32; 4] = [1, 3, 0, 2];
    let data: [i32; 4] = [5, 7, 3, 8];

    // A budget of 1 reads a single row and a single entry at a time — every block and span
    // boundary the traversal can produce; 8 exceeds the matrix, so it reads everything at once.
    for budget in 1..=8u64 {
        for col in 0..4u64 {
            let expected: Vec<i32> = DENSE.iter().map(|row| row[col as usize]).collect();
            assert_eq!(csr_column(&indptr, &indices, &data, col, 3, budget), expected, "column {col}, budget {budget}");
        }
    }
}

#[test]
fn csr_extraction_is_dtype_agnostic() {
    // A `uint8` indices array alongside an `int64` indptr: the two are read and used at their
    // own widths, and neither is widened to match the other.
    let indptr: [i64; 4] = [0, 2, 2, 4];
    let indices: [u8; 4] = [1, 3, 0, 2];
    let data: [i32; 4] = [5, 7, 3, 8];

    assert_eq!(csr_column(&indptr, &indices, &data, 1, 3, 2), vec![5, 0, 0]);
    // A column index too large for the indices dtype simply matches nothing.
    assert_eq!(csr_column(&indptr, &indices, &data, 300, 3, 2), vec![0, 0, 0]);
}

#[test]
fn csr_extraction_handles_empty_rows_and_matrices() {
    // Row 1 is empty (indptr[1] == indptr[2]), so it stays zero in every column.
    let indptr: [i32; 4] = [0, 2, 2, 4];
    let indices: [i32; 4] = [1, 3, 0, 2];
    let data: [i32; 4] = [5, 7, 3, 8];
    for budget in 1..=8u64 {
        assert_eq!(csr_column(&indptr, &indices, &data, 0, 3, budget)[1], 0);
    }

    // A matrix with no non-zeros at all reads no spans whatsoever.
    assert_eq!(csr_column(&[0i32, 0, 0, 0], &[] as &[i32], &[], 2, 3, 2), vec![0, 0, 0]);
}

#[test]
fn csr_extraction_handles_a_row_larger_than_the_budget() {
    // A single row holding more entries than one read allows: its entries span several reads,
    // and each of those still has to resolve back to that same row.
    //   row 0: [0, 0, 0], row 1: [1, 2, 3], row 2: [0, 0, 0]
    let indptr: [i32; 4] = [0, 0, 3, 3];
    let indices: [i32; 3] = [0, 1, 2];
    let data: [i32; 3] = [1, 2, 3];

    for budget in 1..=4u64 {
        assert_eq!(csr_column(&indptr, &indices, &data, 0, 3, budget), vec![0, 1, 0], "budget {budget}");
        assert_eq!(csr_column(&indptr, &indices, &data, 2, 3, budget), vec![0, 3, 0], "budget {budget}");
    }
}

#[test]
fn find_column_entries_returns_ascending_positions() {
    let indices: [i32; 6] = [2, 0, 2, 1, 2, 0];
    assert_eq!(find_column_entries(&indices, 2), vec![0, 2, 4]);
    assert_eq!(find_column_entries(&indices, 0), vec![1, 5]);
    assert_eq!(find_column_entries(&indices, 3), Vec::<usize>::new());
}

#[test]
fn rows_for_entries_assigns_each_entry_to_its_row() {
    // Rows 1 and 3 are empty; row 0 owns entries 0..2, row 2 owns 2..3, row 4 owns 3..5.
    let indptr: [i32; 6] = [0, 2, 2, 3, 3, 5];
    assert_eq!(rows_for_entries(&indptr, &[0, 1, 2, 3, 4]), vec![0, 0, 2, 4, 4]);
}

#[test]
fn rows_for_entries_numbers_rows_relative_to_its_indptr_block() {
    // The tail of the same indptr, as a block covering rows 2..4: absolute positions resolve to
    // rows 0 and 2 of the block, which the caller then shifts back by the block's first row.
    let block: [i32; 3] = [2, 3, 3];
    assert_eq!(rows_for_entries(&block, &[2]), vec![0]);
    let block: [i32; 2] = [3, 5];
    assert_eq!(rows_for_entries(&block, &[3, 4]), vec![0, 0]);
}

#[test]
fn scatter_column_places_values_at_their_row_indices() {
    // The CSC form of column 0 of `DENSE`: a single non-zero at row 2.
    assert_eq!(scatter_column(&[2i32], &[3i32], 3), vec![0, 0, 3]);
    // Rows may be listed in any order, and an all-zero column scatters nothing.
    assert_eq!(scatter_column(&[2u16, 0], &[8.0f32, 5.0], 3), vec![5.0, 0.0, 8.0]);
    assert_eq!(scatter_column(&[] as &[i64], &[] as &[u8], 3), vec![0, 0, 0]);
}
