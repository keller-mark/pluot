//! Compressed sparse (CSR and CSC) matrices, kept sparse all the way to the
//! GPU rather than densified.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::numeric_data::{NumericData, NumericValue};
use crate::shader_modules::{matrix_value, ShaderBuilder, TextureDtype};
use crate::wgpu;

/// The axis that a [`CompressedMatrix`]'s `indptr` is indexed by.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum CompressedAxis {
    /// CSR: one `indptr` offset per row, with column indices.
    Rows,
    /// CSC: one `indptr` offset per column, with row indices.
    Cols,
}

/// The stored entries of a CSR or CSC matrix, in the `scipy.sparse` layout:
/// the entries of major index `m` (a row of a CSR matrix, a column of a CSC
/// matrix) are `indices[indptr[m]..indptr[m + 1]]` and `data[indptr[m]..indptr[m + 1]]`.
/// Minor indices with no entry are implicit zeros.
#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[ts(optional_fields = nullable)]
pub struct CompressedMatrix {
    /// One offset per major index, plus a final offset equal to the number of entries.
    pub indptr: NumericData,
    /// The minor index of each entry, ascending within each major index (as
    /// in a canonical `scipy.sparse` matrix), since entries are found by binary search.
    pub indices: NumericData,
    /// The value of each entry.
    pub data: NumericData,
    /// The minor index shown at each minor position: the subset, in display
    /// order, of the columns of a CSR matrix or the rows of a CSC matrix.
    /// `None` shows every minor index in stored order.
    pub minor_subset: Option<NumericData>,
}

impl Default for CompressedMatrix {
    fn default() -> Self {
        Self {
            indptr: NumericData::Uint32(Arc::new(vec![0])),
            indices: NumericData::Uint32(Arc::new(vec![])),
            data: NumericData::Float32(Arc::new(vec![])),
            minor_subset: None,
        }
    }
}

/// The textures and WGSL [`CompressedMatrix::prepare_gpu`] produces.
pub struct PreparedCompressedMatrix {
    /// Bound consecutively, starting at the `first_binding` that was passed.
    pub textures: Vec<(wgpu::TextureView, TextureDtype)>,
    /// Defines `fn get_matrix_value(row: u32, col: u32) -> f32`.
    pub wgsl: String,
}

impl CompressedMatrix {
    pub fn num_major(&self) -> usize {
        self.indptr.len().saturating_sub(1)
    }

    /// Panics unless this is a well-formed `num_rows x num_cols` matrix
    /// compressed along `axis`.
    pub fn validate(&self, axis: CompressedAxis, num_rows: usize, num_cols: usize) {
        let (num_major, num_minor) = match axis {
            CompressedAxis::Rows => (num_rows, num_cols),
            CompressedAxis::Cols => (num_cols, num_rows),
        };
        assert_eq!(self.indptr.len(), num_major + 1, "CompressedMatrix indptr must have one more entry than the {num_major} major indices");
        assert_eq!(self.indices.len(), self.data.len(), "CompressedMatrix indices and data must have one entry each per stored value");
        let num_entries = self.indptr.get_f64(num_major) as usize;
        assert_eq!(num_entries, self.data.len(), "CompressedMatrix indptr ends at {num_entries} but there are {} entries", self.data.len());
        if let Some(subset) = &self.minor_subset {
            assert_eq!(subset.len(), num_minor, "CompressedMatrix minor_subset must have one entry per each of the {num_minor} minor positions");
        }
    }

    /// The position in `indices`/`data` of the entry at (`major`,
    /// `minor_position`), or `None` for an implicit zero. CPU mirror of
    /// `get_compressed_value` in `compressed_lookup.wgsl`.
    pub fn entry_at(&self, major: usize, minor_position: usize) -> Option<usize> {
        let target = match &self.minor_subset {
            Some(subset) => subset.get_f64(minor_position),
            None => minor_position as f64,
        };
        let end = self.indptr.get_f64(major + 1) as usize;
        let (mut lo, mut hi) = (self.indptr.get_f64(major) as usize, end);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.indices.get_f64(mid) < target { lo = mid + 1 } else { hi = mid }
        }
        (lo < end && self.indices.get_f64(lo) == target).then_some(lo)
    }

    /// The value at (`row`, `col`) of this matrix compressed along `axis`.
    pub fn value_at(&self, axis: CompressedAxis, row: usize, col: usize) -> f32 {
        let (major, minor_position) = if axis == CompressedAxis::Rows { (row, col) } else { (col, row) };
        self.entry_at(major, minor_position).map_or(0.0, |entry| self.data.get_f32(entry))
    }

    /// Like [`value_at`](Self::value_at), in the stored dtype.
    pub fn native_value_at(&self, axis: CompressedAxis, row: usize, col: usize) -> NumericValue {
        let (major, minor_position) = if axis == CompressedAxis::Rows { (row, col) } else { (col, row) };
        match self.entry_at(major, minor_position) {
            Some(entry) => self.data.value(entry),
            None => self.data.zero_value(),
        }
    }

    /// Uploads this matrix, compressed along `axis`, as textures bound from
    /// `first_binding` on, and assembles the WGSL that reads them.
    pub fn prepare_gpu(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        axis: CompressedAxis,
        first_binding: u32,
    ) -> PreparedCompressedMatrix {
        let (indptr_view, indptr_dtype) = self.indptr.create_data_texture(device, queue, "Sparse Matrix Indptr Texture");
        let (indices_view, indices_dtype) = self.indices.create_data_texture(device, queue, "Sparse Matrix Indices Texture");
        let (data_view, data_dtype) = self.data.create_data_texture(device, queue, "Sparse Matrix Data Texture");
        let mut textures = vec![(indptr_view, indptr_dtype), (indices_view, indices_dtype), (data_view, data_dtype)];

        let minor_subset_wgsl = match &self.minor_subset {
            Some(subset) => {
                let (view, dtype) = subset.create_data_texture(device, queue, "Sparse Matrix Minor Subset Texture");
                textures.push((view, dtype));
                ShaderBuilder::new(matrix_value::MINOR_SUBSET_TEXTURE)
                    .define_bidx("minor_subset", first_binding + 3)
                    .inject_texture_sample_type("minor_subset", dtype)
                    .build()
            }
            None => matrix_value::MINOR_SUBSET_IDENTITY.to_string(),
        };
        let lookup_wgsl = ShaderBuilder::new(matrix_value::COMPRESSED_LOOKUP)
            .define_bidx("sparse_indptr", first_binding)
            .define_bidx("sparse_indices", first_binding + 1)
            .define_bidx("sparse_data", first_binding + 2)
            .inject_texture_sample_type("sparse_indptr", indptr_dtype)
            .inject_texture_sample_type("sparse_indices", indices_dtype)
            .inject_texture_sample_type("sparse_data", data_dtype)
            .build();
        let value_wgsl = match axis {
            CompressedAxis::Rows => matrix_value::CSR,
            CompressedAxis::Cols => matrix_value::CSC,
        };
        PreparedCompressedMatrix { textures, wgsl: [minor_subset_wgsl, lookup_wgsl, value_wgsl.to_string()].join("\n") }
    }
}
