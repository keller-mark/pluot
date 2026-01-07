The code in this directory contains a rough port of [deck.gl-native](https://github.com/UnfoldedInc/deck.gl-native).
I say "rough port" because it will diverge from the cpp implementation.
For instance, rather than row-based getters (e.g., ScatterplotLayer.getPosition), we want to use column-based getters.
Likewise, we want to use Zarr arrays for data, rather than Arrow Tables.

We do not need the animation loop logic, and we may not need all of the state management logic (at least initially).

We are most interested in its Model and Layer/LayerManager abstractions.

For a proof of concept, we want to implement ScatterplotLayer, BitmapLayer, CompositeLayer, and TileLayer.

If successful, we will then be able to use the layer abstraction to implement simultaneous raster and vector-based rendering (e.g., agnostic to the rendering backend).
