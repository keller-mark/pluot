// HeatmapLayer quantitative domains with one (min, max) pair per row or per
// column, uploaded as two value textures indexed by that row or column.
@group(0) @binding({{domain_min_bidx}}) var domain_min: texture_2d<{{domain_min_dtype}}>;
@group(0) @binding({{domain_max_bidx}}) var domain_max: texture_2d<{{domain_max_dtype}}>;

fn get_cell_domain(row: u32, col: u32) -> vec2<f32> {
  let i = {{domain_index}};
  return vec2<f32>(
    f32(textureLoad(domain_min, flat_texel_coord(i, textureDimensions(domain_min).x), 0).x),
    f32(textureLoad(domain_max, flat_texel_coord(i, textureDimensions(domain_max).x), 0).x)
  );
}
