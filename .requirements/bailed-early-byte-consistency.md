# Consistent "Bailed Early" Signal Across Output Formats

- Raster and vector (SVG) rendered output must both signal whether rendering "bailed early" (didn't finish loading everything) via the same trailing marker, regardless of compression settings.
- Every consumer that reads raw rendered output (test utilities, Python/R bindings, the JS/React component) must correctly account for this marker.
