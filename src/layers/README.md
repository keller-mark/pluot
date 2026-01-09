Code inspired by DeckGL layer/view concepts, as well as [deck-to-svg](https://github.com/keller-mark/deck-to-svg).

Note: [At first](https://github.com/keller-mark/pluot/pull/107), I started to port the LumaGL `Model` implementation from deck.gl-native, but then backtracked to `Layer.draw` calls that directly set up the WebGPU buffers, bind groups, and render pipeline.

~~To support both raster and vector rendering, we need to implement layers that accept both Zarr arrays and Two element lists. The layers that accept Zarr arrays will need to internally create Two element arrays before performing vector rendering. The layers that accept Two element arrays will be helpful for testing and shader debugging (e.g., making it easy to create layer instances without first creating an on-disk Zarr store and serving it).~~

The "plain" layers will accept Rust numeric vectors as data.
They can optionally be passed getter functions (inspired by those from DeckGL) that transform this data before creating the GPU buffers (the transformed data used for the buffers should be cached, and only re-computed when necessary (we may need to implement something analogous to DeckGL updateTriggers)).
These plain layers will be helpful for testing, debugging, and customization (e.g., internal composite layers such as AxisLayer).

We will also implement "zarr" layer variants of each "plain" layer that accept a Zarr store and corresponding keys into arrays in this store (rather than Rust vectors directly).
The async "prepare" functions of these layers will read the Zarr data to obtain Rust vectors internally.
We should also be able to support the same getter functions and cacheing for these as well.
The Zarr layer variants should be able to reuse the "drawing" logic from the "plain" layers after obtaining the Rust vectors from Zarr.

Finally, we will implement "composite" layers that accept multiple "plain" or "zarr" layers as children, such as the TileLayer that composes multiple BitmapLayers.
