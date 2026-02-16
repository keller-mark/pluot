// The OmeZarrMultiscaleLayer will be a layer that can be used to display multiscale OME-Zarr data.
// It will be a wrapper around the BitmapLayer from the pluot_core crate.
// We want to implement this as a composite layer that internally manages multiple BitmapLayer instances as sublayers.
// The logic from the current MultiscaleLayer should be refactored into pure functions, so that it can be reused by both the MultiscaleLayer and the OmeZarrMultiscaleLayer.

// The prepare() function will be responsible for instantiating BitmapLayer instances for the visible region at each level of resolution, up to the maximum resolution level that would saturate the viewport pixels
// (e.g., we do not care about loading higher-res tiles if they would be smaller than 1px on the screen).
// Once instantiated, we will check the PrepareResult value returned by each sublayer's prepare().
// We will use these PrepareResults to determine which BitmapLayer instances to draw.
// Ideally, we will draw BitmapLayers at a single resolution level that is best-without-going-over (does not exceed the resolution of the current viewport),
// but the tiles at this resolution may not be ready yet.
// Therefore, we will want to draw the next-coarsest level whose PrepareResult is `bailed_early: false`, to provide a fallback while loading higher-res tiles.
// We will need to ensure that drawing of sublayers occurs from coarser to finer, so that finer tiles are drawn on top of coarser tiles.
// This will ensure that we don't have visual holes while loading finer tiles, and that we get a sharper image as soon as finer tiles are ready.
