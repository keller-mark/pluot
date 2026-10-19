---
"@pluot/core": patch
"@pluot/react": patch
---

Add the `limitExtent` and `zoomToSquare` camera filters, pass a `CameraFilterContext` (viewport params plus the `extent_wasm` extent) as the third camera filter argument, and add the `limitExtent` prop and `"zoomToSquare"` camera filter string to the `Pluot` component. Add the `refitCameraOnAspectRatioChange` prop to opt out of refitting the camera when the aspect ratio settings change.
