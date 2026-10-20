// Compressed sparse (CSR or CSC) matrix values, in the `scipy.sparse` layout:
// the entries of major index `m` are at `indptr[m]..indptr[m + 1]` of
// `indices` and `data`, with ascending minor indices, so an entry is found by
// binary search. A minor index with no entry is an implicit zero.
@group(0) @binding({{sparse_indptr_bidx}}) var sparse_indptr: texture_2d<{{sparse_indptr_dtype}}>;
@group(0) @binding({{sparse_indices_bidx}}) var sparse_indices: texture_2d<{{sparse_indices_dtype}}>;
@group(0) @binding({{sparse_data_bidx}}) var sparse_data: texture_2d<{{sparse_data_dtype}}>;

fn load_sparse_indptr(i: u32) -> u32 {
  return u32(textureLoad(sparse_indptr, flat_texel_coord(i, textureDimensions(sparse_indptr).x), 0).x);
}

fn load_sparse_index(i: u32) -> u32 {
  return u32(textureLoad(sparse_indices, flat_texel_coord(i, textureDimensions(sparse_indices).x), 0).x);
}

fn get_compressed_value(major: u32, minor_position: u32) -> f32 {
  let minor_index = get_minor_index(minor_position);
  let end = load_sparse_indptr(major + 1u);
  var lo = load_sparse_indptr(major);
  var hi = end;
  while (lo < hi) {
    let mid = lo + (hi - lo) / 2u;
    if (load_sparse_index(mid) < minor_index) {
      lo = mid + 1u;
    } else {
      hi = mid;
    }
  }
  if (lo < end && load_sparse_index(lo) == minor_index) {
    return f32(textureLoad(sparse_data, flat_texel_coord(lo, textureDimensions(sparse_data).x), 0).x);
  }
  return 0.0;
}
