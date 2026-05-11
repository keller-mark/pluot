# 3D view-controls camera matrix update pseudocode

Each event handler calls `viewRotate` or `viewPan`, which fan out to three
parallel controllers — turntable, orbit, matrix — to keep them in sync.
All rotation angles and pan distances are in world-space units.

---

## onMouseMove

```
scale = 1.0 / element.clientHeight
dx    = scale * (mouseX - lastX)        // normalised [0, 1)
dy    = scale * (mouseY - lastY)

flipX = camera.flipX ? +1 : -1
flipY = camera.flipY ? +1 : -1
drot  = π * camera.rotateSpeed
t     = now()

if left-button (buttons & 1):
    if shift held:
        viewRotate(t,  0,  0,  -dx * drot)              // roll only
    else:
        viewRotate(t,  flipX * drot * dx,               // yaw
                       -flipY * drot * dy,              // pitch
                       0)

else if right-button (buttons & 2):
    viewPan(t, -translateSpeed * dx * distance,         // strafe left/right
               +translateSpeed * dy * distance,         // strafe up/down
               0)

else if middle-button (buttons & 4):
    kzoom = zoomSpeed * dy / innerHeight * (t - lastT) * 50
    viewPan(t, 0, 0,  distance * (exp(kzoom) - 1))     // dolly in/out
```

---

## onWheel

```
// Scale raw delta to pixels
wheelScale = 1 (DOM_DELTA_PIXEL) | lineHeight (DOM_DELTA_LINE) | innerHeight (DOM_DELTA_PAGE)
dx *= wheelScale
dy *= wheelScale

flipX = camera.flipX ? +1 : -1
flipY = camera.flipY ? +1 : -1
t     = now()

if |dx| > |dy|:
    // Horizontal scroll → roll
    viewRotate(t, 0, 0, -dx * flipX * π * rotateSpeed / innerWidth)
else:
    // Vertical scroll → dolly
    kzoom = zoomSpeed * flipY * dy / innerHeight * (t - lastT) / 100
    viewPan(t, 0, 0,  distance * (exp(kzoom) - 1))
```

---

## viewRotate / viewPan dispatch

```
viewRotate(t, a1, a2, a3):
    turntableRotate(controllers[0], t, a1, a2, a3)
    orbitRotate    (controllers[1], t, a1, a2, a3)
    matrixRotate   (controllers[2], t, a1, a2, a3)

viewPan(t, a1, a2, a3):
    turntablePan   (controllers[0], t, a1, a2, a3)
    orbitPan       (controllers[1], t, a1, a2, a3)
    matrixPan      (controllers[2], t, a1, a2, a3)
```

---

## Orbit controller

### orbitRotate(c, t, dx, dy, dz)

The orbit controller stores orientation as a quaternion. Rotation is derived
from the mouse delta projected onto the current camera frame.

```
// 1. Refresh computed matrix from filtered-vector state
recalcMatrix(c, t)
M = c.computedMatrix        // view matrix (upper-3x3 rows = camera axes)

r̂ = M row 0  (world right)
û = M row 1  (world up)
f̂ = M row 2  (world forward / into screen)

// 2. Map (dx, dy) to a world-space direction q in the right/up plane
q = dx * r̂ + dy * û

// 3. Derive rotation axis perpendicular to both q and f̂
b_axis = q × f̂   (= -(f̂ × q))

// 4. Build quaternion b from that axis;
//    scalar part chosen so |b| = 1 (half-angle encoding)
b_w = sqrt(max(0, 1 - |b_axis|²))
b̂   = normalize(b_axis, b_w)

// 5. Compose: new_rot = current_rot ⊗ b̂  (quaternion multiply)
c_quat = c.computedRotation ⊗ b̂

// 6. Optional roll about the forward axis
if dz:
    b_roll = (f̂ * sin(dz) / |f̂|,  cos(dx))   // roll quaternion
    c_quat = c_quat ⊗ b_roll

// 7. Normalise and push into the filtered-vector timeline
c_quat = normalize(c_quat)
fvSet(c.rotation, t, c_quat)
```

### orbitPan(c, t, dx, dy, dz)

```
recalcMatrix(c, t)
M = c.computedMatrix

û = normalize(M row 1)                          // world up
r̂ = normalize(M row 0 - û * (M_row0 · û))      // right, orthogonalised to û

// World-space pan vector
v = r̂ * dx + û * dy

fvMove(c.center, t, v)                          // translate focus point

// Dolly: update radius in log-space
radius = max(1e-4, exp(c.computedRadius[0]) + dz)
fvSet(c.radius, t, log(radius))
```

---

## Turntable controller

The turntable stores orientation as azimuth θ (theta) and elevation φ (phi)
plus an explicit `up` and `right` vector for the base frame.

### turntableRecalcMatrix(c, t)

```
// 1. Gram-Schmidt: orthonormalise up and right
û = normalize(up)
r̂ = normalize(right - û * (right · û))
toward = normalize(û × r̂)

// 2. Spherical coordinates for the eye direction
radius = exp(computedRadius[0])
θ = angle[0],  φ = angle[1]

// Eye direction in (r̂, toward, û) frame:
w = (cosθ·cosφ,  sinθ·cosφ,  sinφ)          // outward (center → eye)
s = (-cosθ·sinφ, -sinθ·sinφ,  cosφ)         // screen-up

// 3. Build upper-3x3 of view matrix
M col 2 = w projected into world: Σ_i w[i] * basis[i]    // forward column
M col 1 = s projected into world: Σ_i s[i] * basis[i]    // screen-up column
M col 0 = (M col 1) × (M col 2), normalised               // screen-right column

// 4. Eye position and view-matrix translation
eye = center + M_col2 * radius
M col 3 = -(upper-3x3 * eye)                              // standard view translation
```

### turntableRotate(c, t, dtheta, dphi, droll)

```
// Azimuth and elevation update
fvMove(c.angle, t, dtheta, dphi)

// Optional roll: rotate up and right about the forward axis
if droll:
    recalcMatrix(c, t)
    mat4Rotate(frame, frame, droll, forward_axis)
    fvSet(c.up,    t, new_up)
    fvSet(c.right, t, new_right)
```

### turntablePan(c, t, dx, dy, dz)

```
recalcMatrix(c, t)
// Same Gram-Schmidt extraction as orbitPan
û = normalize(M row 1)
r̂ = normalize(M row 0 - û * (M_row0 · û))

v = r̂ * dx + û * dy
fvMove(c.center, t, v)

radius = max(1e-4, exp(computedRadius[0]) + dz)
fvSet(c.radius, t, log(radius))
```

---

## Matrix controller

Stores a sequence of raw 4×4 view-matrix keyframes; interpolates between them.

### matrixRecalcMatrix(c, t)

```
// Interpolate stored keyframes
mat = lerp(prevMatrix, nextMatrix,  (t - t0) / (t1 - t0))   // mat4Interpolate

// Derive eye from inverse view matrix
imat = inverse(mat)
eye  = imat col 3 / imat[15]       // homogeneous divide

// Derive center from eye and forward column
center = eye - mat_col2 * exp(computedRadius[0])
```

### matrixRotate(c, t, yaw, pitch, roll)

```
// Apply rotations in the camera's inverse (world-from-camera) space
imat = computedInverse
imat ← R_Y(yaw) · imat
imat ← R_X(pitch) · imat
imat ← R_Z(roll)  · imat

matrixSetMatrix(c, t, inverse(imat))
```

### matrixPan(c, t, dx, dy, dz)

```
// Translate camera position in world space by negating (dx, dy, dz)
imat = computedInverse
imat ← T(-dx, -dy, -dz) · imat

matrixSetMatrix(c, t, inverse(imat))
```
