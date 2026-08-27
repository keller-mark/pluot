# Skip Off-Screen Bitmask Rasterization

- When rendering a mask to vector (SVG) output, first check whether any part of the mask falls within the current view bounds; if the mask is entirely outside the view, skip rasterizing it entirely.
- Needs a dedicated test covering a mask positioned fully outside the view bounds, with an explanatory note on the expected (no-op) behavior.
