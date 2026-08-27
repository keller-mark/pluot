# Bitmask Rendering Layer

- Provide a mask-rendering layer and a corresponding multiscale mask layer, as counterparts to the existing bitmap/image layer and its multiscale version.
- Same configuration surface as the bitmap layer, except for channel-specific display settings.
- Mask data is provided as numeric arrays with channel, then row, then column ordering.
- Shader code should be assembled from shared, reusable building blocks and generated per-channel via templated repetition, following the same pattern as the existing bitmap layer's shader — not built via ad hoc string concatenation.
- Needs unit tests validating the assembled shader output, plus rendering tests covering all color modes, dimension orderings, the vector (SVG) output path, and an empty mask.
