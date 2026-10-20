// The minor index shown at each minor position, i.e. a subset (and ordering)
// of the columns of a CSR matrix or the rows of a CSC matrix.
@group(0) @binding({{minor_subset_bidx}}) var minor_subset: texture_2d<{{minor_subset_dtype}}>;

fn get_minor_index(minor_position: u32) -> u32 {
  return u32(textureLoad(minor_subset, flat_texel_coord(minor_position, textureDimensions(minor_subset).x), 0).x);
}
