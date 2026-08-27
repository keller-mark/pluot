# Bitmask Stroke Width Units

- Stroke width for outlined masks must support data-space, pixel-space, and normalized unit modes, matching the pattern already used for outlined polygon/curve rendering — but evaluated per-fragment against the mask data itself, since masks don't have discrete stroked geometry.
- Stroke width resolution must be independent of the underlying data texture's resolution — it should be resolved using the current camera/viewport state.
- Vector (SVG) output must visually match the GPU-rendered stroke width: rasterize at the output's pixel resolution unless the source mask data is higher-resolution, in which case rasterize at the data's native resolution; when necessary, up-sample the mask data so the target stroke width (rounded to the nearest pixel) is achievable. Vector output quality should be as high as achievable.
