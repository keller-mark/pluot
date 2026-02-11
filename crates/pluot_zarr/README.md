This is a crate that contains implementations of Pluot layers that load and render data from the Zarr format.

These layers will be registered via the [inventory](https://github.com/dtolnay/inventory) system, and will enable easily using features in the root crate to enable or disable the Zarr-specific features and whether or not to pull in the `zarrs` and its compression dependencies.
