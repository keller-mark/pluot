# Bitmask Stroke/Fill Styling

- Bitmask rendering must support an outlined ("stroked") style and a filled style, each with independently configurable color and opacity.
- Per-channel display settings should be specified as directly inlined properties, not nested under a separate settings object.
- Default channel display settings must be consistent between OME-Zarr-backed masks and non-OME-Zarr masks.
- Shader parameters/flags (e.g. a stroke-vs-fill mode, a channel index) must use descriptive names, not cryptic single-letter or abbreviated placeholders, consistently between the layer code and its shader code.
