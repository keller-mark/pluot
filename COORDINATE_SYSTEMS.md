# Coordinate Systems

## 2D plotting

Our coordinate system for 2D plotting diverges from that used by DeckGL (at least its CARTESIAN mode, which I am most familiar with).
DeckGL uses a coordinate system where the origin (0,0) is in the center of the canvas, and then a viewState `{ zoom, target: [x, y]}` is used to handle the camera.

In our system, the origin (0,0) is in the bottom-left corner of the canvas, and the top-right corner is at (1m, 1m) in pixels.
The bitmap layer takes a `PHYSICAL_SIZE_X_UNIT_EXP` parameter to determine how many meters each pixel represents in the X direction (e.g., -6 for 1px = 1um physical size).
This, plus the aspect ratio modes described below, will enable us to intuitively plot bioimaging data.
Users will be able to specify arbitrary affine "model matrices" to transform their imaging data as needed (i.e., beyond these basic phyiscal-sizing parameters).


Assuming the `camera_view` matrix is the identity matrix, the coordinate system would show values from 0.0m to 1.0m in x and y (when the `_UNIT_EXP` values are `0` indicating multiplication by `*10^0 = 1`).
Users will be able to supply the following parameters to modify the behavior (TODO: these should be converted to an affine matrix to use for transformation internally):

```rs
const PHYSICAL_SIZE_X: f32 = 1.0; // Square aspect ratio
const PHYSICAL_SIZE_X_UNIT_EXP: f32 = -6.0; // Each pixel is 1 micrometer wide (1e-6 meters)

const PHYSICAL_SIZE_Y: f32 = 1.0; // Square aspect ratio
const PHYSICAL_SIZE_Y_UNIT_EXP: f32 = -6.0; // Each pixel is 1 micrometer tall (1e-6 meters)
```


If the DeckGL-like coordinate system is needed, users should be able to modify their `camera_view` matrix to achieve this functionality. (TODO: test/document this and provide helpers)

### Aspect ratio modes

- 0: ignore / squeeze: For example,  a 200 x 100 canvas would show values from 0 to 1 in x and y. The 0 to 1 square would be stretched in the X direction (and squeezed in the Y direction) since the canvas is wider than it is tall.

- 1: fit (contain): For example, a 200 x 100 canvas would range from 0 to 1 in the Y direction, and from 0 to 1+extra in the X direction. The 0 to 1 square would keep its square aspect ratio and would be fully visible inside the rectangle (with no part of this square clipped). The pixels would be aligned to the left.

- 2: fill (cover): For example, a 200 x 100 canvas would range from 0 to 1 in the X direction, and from 0 to 1-extra in the Y direction. The 0 to 1 square would keep its square aspect ratio but would be clipped in the Y direction (at the top) so that the entire canvas is filled/covered. The pixels would be aligned to the bottom.

## 3D plotting
