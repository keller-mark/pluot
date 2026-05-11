# Camera matrix update pseudocode

## onMouseMove

### Pan

```
dX = isPanXInverted ? (prevMouseX - mouseX) : (mouseX - prevMouseX)
dY = isPanYInverted ? (prevMouseY - mouseY) : (mouseY - prevMouseY)

// NDC mode:
tx = isPanX ? (panSpeed * dX / width)  * 2 * (1 / xAspectRatioModeFactor) : 0
ty = isPanY ? (panSpeed * dY / height) * 2 * (1 / yAspectRatioModeFactor) : 0
// pixel mode:
tx = isPanX ? panSpeed * dX : 0
ty = isPanY ? -(panSpeed * dY) : 0

if tx != 0 or ty != 0:
    view ← T(tx, ty, 0) · view
```

### Rotate

```
// viewport half-dimensions
wh = width / 2
hh = height / 2

// previous and current mouse positions relative to viewport center
p1 = (prevMouseX - wh,  hh - prevMouseY)
p2 = (mouseX    - wh,  hh - mouseY)

radians = angle(p1, p2)               // unsigned angle between the two vectors
cross   = p1.x * p2.y - p2.x * p1.y  // z-component of cross product; sign gives CW/CCW

θ = rotateSpeed * radians * sign(cross)

view ← R_z(θ) · view
```

---

## onWheel

### Zoom

```
dZ = zoomSpeed * exp(scrollDist / height)

sx = isZoomX ? 1/dZ : 1
sy = isZoomY ? 1/dZ : 1

// clamp so the resulting scale stays within [scaleXBounds, scaleYBounds]
currentScale = getScaling(view)   // (currentScale.x, currentScale.y)
sx = clamp(currentScale.x * sx, scaleXBounds) / currentScale.x
sy = clamp(currentScale.y * sy, scaleYBounds) / currentScale.y

// mouse position in NDC (the scale pivot):
// NDC mode:
px = ((-1 + (mouseRelX / width)  * 2) - xAlignmentTranslation) * (1 / xAspectRatioModeFactor)
py = ((1  - (mouseRelY / height) * 2) - yAlignmentTranslation) * (1 / yAspectRatioModeFactor)
// pixel mode:
px = mouseRelX
py = mouseRelY

p = (px, py, 0)

view ← T(p) · S(sx, sy, 1) · T(-p) · view
```
