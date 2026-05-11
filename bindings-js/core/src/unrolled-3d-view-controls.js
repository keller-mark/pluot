// Imports kept (analog of gl-matrix in the 2D version):
//   gl-mat4 / gl-vec3 — low-level matrix/vector math
//   filtered-vector  — smoothed state container used by every controller
//   binary-search-bounds, mat4-interpolate, cubic-hermite — utility primitives
import mat4FromQuat from "gl-mat4/fromQuat";
import mat4Invert from "gl-mat4/invert";
import mat4Rotate from "gl-mat4/rotate";
import mat4RotateX from "gl-mat4/rotateX";
import mat4RotateY from "gl-mat4/rotateY";
import mat4RotateZ from "gl-mat4/rotateZ";
import mat4Translate from "gl-mat4/translate";
import mat4Interpolate from "mat4-interpolate";
import vec3Cross from "gl-vec3/cross";
import vec3Normalize from "gl-vec3/normalize";
import bsearch from "binary-search-bounds";

// Closure state inherited from 3d-view-controls.js (initialized elsewhere):
//   element                              — DOM node the listeners are on
//   camera = { flipX, flipY, rotateSpeed, zoomSpeed, translateSpeed, ... }
//   distance                             — number, recomputed by tick()
//   view                                 — 3d-view ViewController:
//     view._controllerList = [turntable, orbit, matrix]   // sub-controllers
//     view._active                                        // one of the three
//     view.lastT() — Math.max(_active.lastT, ...)
//
// State owned by mouse-change/mouse-listen.js (one instance, captured in the
// closure of `mouseChange(element, handleInteraction)`):
//   buttonState, mcX, mcY                — last reported (buttons, x, y)
//   mcMods = { shift, alt, control, meta }
//
// State owned by 3d-view-controls.js itself:
//   lastX, lastY, lastMods               — previous (x, y, mods) for handleInteraction
//
// State owned by mouse-wheel/wheel.js:
//   lineHeight = toPX('ex', element)     — computed once at listener attach
//
// One-shot init:
//   const now = (performance && performance.now)
//     ? () => performance.now()
//     : (Date.now || (() => +new Date()));


// ---------------------------------------------------------------------------
// Helpers inlined from orbit-camera-controller/orbit.js
// (these are file-local helpers, not exports — duplicated here so the
// controller method bodies below can stay close to their originals).
// ---------------------------------------------------------------------------
function len3(x, y, z) {
  return Math.sqrt(x * x + y * y + z * z);
}
function len4(w, x, y, z) {
  return Math.sqrt(w * w + x * x + y * y + z * z);
}
function normalize4(out, a) {
  const ax = a[0], ay = a[1], az = a[2], aw = a[3];
  const al = len4(ax, ay, az, aw);
  if (al > 1e-6) {
    out[0] = ax / al;
    out[1] = ay / al;
    out[2] = az / al;
    out[3] = aw / al;
  } else {
    out[0] = out[1] = out[2] = 0.0;
    out[3] = 1.0;
  }
}


// ---------------------------------------------------------------------------
// Inlined from filtered-vector/fvec.js — methods used by the controllers.
// `cubicHermite` is kept as an import (analog of gl-matrix), called via
// the global helper below.
// ---------------------------------------------------------------------------
function fvSet(fv, t /* , ...components */) {
  // Inlined: FilteredVector.prototype.set
  const d = fv.dimension;
  if (t < fv._time[fv._time.length - 1] || arguments.length !== d + 2) return;
  const state = fv._state;
  const velocity = fv._velocity;
  const lo = fv.bounds[0];
  const hi = fv.bounds[1];
  fv._time.push(t);
  for (let i = d; i > 0; --i) {
    const x = arguments[i + 1];
    state.push(Math.min(hi[i - 1], Math.max(lo[i - 1], x)));
    velocity.push(0);
  }
}

function fvMove(fv, t /* , ...deltas */) {
  // Inlined: FilteredVector.prototype.move
  const t0 = fv._time[fv._time.length - 1];
  const d = fv.dimension;
  if (t <= t0 || arguments.length !== d + 2) return;
  const state = fv._state;
  const velocity = fv._velocity;
  let statePtr = state.length - d;
  const lo = fv.bounds[0];
  const hi = fv.bounds[1];
  const dt = t - t0;
  const sf = dt > 1e-6 ? 1 / dt : 0.0;
  fv._time.push(t);
  for (let i = d; i > 0; --i) {
    const dx = arguments[i + 1];
    const next = state[statePtr++] + dx;
    state.push(Math.min(hi[i - 1], Math.max(lo[i - 1], next)));
    velocity.push(dx * sf);
  }
}

// `fv.curve(t)` mutates fv._scratch[0] and returns it. Every controller's
// `computedX` field is aliased to that same Float array, so calling curve()
// is what refreshes the controller's `computedRadius`, `computedCenter`, etc.
// curve() depends on `cubic-hermite` which is kept as an import.
import cubicHermite from "cubic-hermite";
function fvCurve(fv, t) {
  const time = fv._time;
  const n = time.length;
  const idx = bsearch.le(time, t);
  const result = fv._scratch[0];
  const state = fv._state;
  const velocity = fv._velocity;
  const d = fv.dimension;
  const bounds = fv.bounds;
  if (idx < 0) {
    let ptr = d - 1;
    for (let i = 0; i < d; ++i, --ptr) result[i] = state[ptr];
  } else if (idx >= n - 1) {
    let ptr = state.length - 1;
    const tf = t - time[n - 1];
    for (let i = 0; i < d; ++i, --ptr) result[i] = state[ptr] + tf * velocity[ptr];
  } else {
    let ptr = d * (idx + 1) - 1;
    const t0 = time[idx];
    const t1 = time[idx + 1];
    const dt = t1 - t0 || 1.0;
    const x0 = fv._scratch[1];
    const x1 = fv._scratch[2];
    const v0 = fv._scratch[3];
    const v1 = fv._scratch[4];
    let steady = true;
    for (let i = 0; i < d; ++i, --ptr) {
      x0[i] = state[ptr];
      v0[i] = velocity[ptr] * dt;
      x1[i] = state[ptr + d];
      v1[i] = velocity[ptr + d] * dt;
      steady = steady && x0[i] === x1[i] && v0[i] === v1[i] && v0[i] === 0.0;
    }
    if (steady) {
      for (let i = 0; i < d; ++i) result[i] = x0[i];
    } else {
      cubicHermite(x0, v0, x1, v1, (t - t0) / dt, result);
    }
  }
  const lo = bounds[0];
  const hi = bounds[1];
  for (let i = 0; i < d; ++i) {
    result[i] = Math.min(hi[i], Math.max(lo[i], result[i]));
  }
  return result;
}


// ---------------------------------------------------------------------------
// Inlined from orbit-camera-controller/orbit.js
// ---------------------------------------------------------------------------
function orbitRecalcMatrix(c, t) {
  fvCurve(c.radius, t);
  fvCurve(c.center, t);
  fvCurve(c.rotation, t);

  const quat = c.computedRotation;
  normalize4(quat, quat);

  const mat = c.computedMatrix;
  mat4FromQuat(mat, quat);

  const center = c.computedCenter;
  const eye = c.computedEye;
  const up = c.computedUp;
  const radius = Math.exp(c.computedRadius[0]);

  eye[0] = center[0] + radius * mat[2];
  eye[1] = center[1] + radius * mat[6];
  eye[2] = center[2] + radius * mat[10];
  up[0] = mat[1];
  up[1] = mat[5];
  up[2] = mat[9];

  for (let i = 0; i < 3; ++i) {
    let rr = 0.0;
    for (let j = 0; j < 3; ++j) rr += mat[i + 4 * j] * eye[j];
    mat[12 + i] = -rr;
  }
}

function orbitRotate(c, t, dx, dy, dz) {
  orbitRecalcMatrix(c, t);
  dx = dx || 0.0;
  dy = dy || 0.0;

  const mat = c.computedMatrix;
  const rx = mat[0], ry = mat[4], rz = mat[8];
  const ux = mat[1], uy = mat[5], uz = mat[9];
  const fx = mat[2], fy = mat[6], fz = mat[10];

  const qx = dx * rx + dy * ux;
  const qy = dx * ry + dy * uy;
  const qz = dx * rz + dy * uz;

  let bx = -(fy * qz - fz * qy);
  let by = -(fz * qx - fx * qz);
  let bz = -(fx * qy - fy * qx);
  let bw = Math.sqrt(Math.max(0.0, 1.0 - bx * bx - by * by - bz * bz));
  let bl = len4(bx, by, bz, bw);
  if (bl > 1e-6) {
    bx /= bl; by /= bl; bz /= bl; bw /= bl;
  } else {
    bx = by = bz = 0.0; bw = 1.0;
  }

  const rotation = c.computedRotation;
  const ax = rotation[0], ay = rotation[1], az = rotation[2], aw = rotation[3];

  let cx = ax * bw + aw * bx + ay * bz - az * by;
  let cy = ay * bw + aw * by + az * bx - ax * bz;
  let cz = az * bw + aw * bz + ax * by - ay * bx;
  let cw = aw * bw - ax * bx - ay * by - az * bz;

  // Apply roll
  if (dz) {
    bx = fx; by = fy; bz = fz;
    const s = Math.sin(dz) / len3(bx, by, bz);
    bx *= s; by *= s; bz *= s;
    bw = Math.cos(dx);
    cx = cx * bw + cw * bx + cy * bz - cz * by;
    cy = cy * bw + cw * by + cz * bx - cx * bz;
    cz = cz * bw + cw * bz + cx * by - cy * bx;
    cw = cw * bw - cx * bx - cy * by - cz * bz;
  }

  const cl = len4(cx, cy, cz, cw);
  if (cl > 1e-6) {
    cx /= cl; cy /= cl; cz /= cl; cw /= cl;
  } else {
    cx = cy = cz = 0.0; cw = 1.0;
  }

  fvSet(c.rotation, t, cx, cy, cz, cw);
}

function orbitPan(c, t, dx, dy, dz) {
  dx = dx || 0.0;
  dy = dy || 0.0;
  dz = dz || 0.0;

  orbitRecalcMatrix(c, t);
  const mat = c.computedMatrix;

  let ux = mat[1], uy = mat[5], uz = mat[9];
  const ul = len3(ux, uy, uz);
  ux /= ul; uy /= ul; uz /= ul;

  let rx = mat[0], ry = mat[4], rz = mat[8];
  const ru = rx * ux + ry * uy + rz * uz;
  rx -= ux * ru; ry -= uy * ru; rz -= uz * ru;
  const rl = len3(rx, ry, rz);
  rx /= rl; ry /= rl; rz /= rl;

  // fx,fy,fz computed in the original orbit.pan but never used after
  // ortho-projection — left out (and present in the original as dead code).

  const vx = rx * dx + ux * dy;
  const vy = ry * dx + uy * dy;
  const vz = rz * dx + uz * dy;

  fvMove(c.center, t, vx, vy, vz);

  // Update z-component of radius
  let radius = Math.exp(c.computedRadius[0]);
  radius = Math.max(1e-4, radius + dz);
  fvSet(c.radius, t, Math.log(radius));
}


// ---------------------------------------------------------------------------
// Inlined from turntable-camera-controller/turntable.js
// ---------------------------------------------------------------------------
const turntableZAxis = [0, 0, 0];

function turntableRecalcMatrix(c, t) {
  fvCurve(c.center, t);
  fvCurve(c.up, t);
  fvCurve(c.right, t);
  fvCurve(c.radius, t);
  fvCurve(c.angle, t);

  const up = c.computedUp;
  const right = c.computedRight;
  let uu = 0.0, ur = 0.0;
  for (let i = 0; i < 3; ++i) {
    ur += up[i] * right[i];
    uu += up[i] * up[i];
  }
  const ul = Math.sqrt(uu);
  let rr = 0.0;
  for (let i = 0; i < 3; ++i) {
    right[i] -= up[i] * ur / uu;
    rr += right[i] * right[i];
    up[i] /= ul;
  }
  const rl = Math.sqrt(rr);
  for (let i = 0; i < 3; ++i) right[i] /= rl;

  const toward = c.computedToward;
  vec3Cross(toward, up, right);
  vec3Normalize(toward, toward);

  const radius = Math.exp(c.computedRadius[0]);
  const theta = c.computedAngle[0];
  const phi = c.computedAngle[1];

  const ctheta = Math.cos(theta), stheta = Math.sin(theta);
  const cphi = Math.cos(phi), sphi = Math.sin(phi);

  const center = c.computedCenter;
  const wx = ctheta * cphi, wy = stheta * cphi, wz = sphi;
  const sx = -ctheta * sphi, sy = -stheta * sphi, sz = cphi;

  const eye = c.computedEye;
  const mat = c.computedMatrix;
  for (let i = 0; i < 3; ++i) {
    const x = wx * right[i] + wy * toward[i] + wz * up[i];
    mat[4 * i + 1] = sx * right[i] + sy * toward[i] + sz * up[i];
    mat[4 * i + 2] = x;
    mat[4 * i + 3] = 0.0;
  }

  const ax = mat[1], ay = mat[5], az = mat[9];
  const bx = mat[2], by = mat[6], bz = mat[10];
  let cx = ay * bz - az * by;
  let cy = az * bx - ax * bz;
  let cz = ax * by - ay * bx;
  const cl = len3(cx, cy, cz);
  cx /= cl; cy /= cl; cz /= cl;
  mat[0] = cx; mat[4] = cy; mat[8] = cz;

  for (let i = 0; i < 3; ++i) eye[i] = center[i] + mat[2 + 4 * i] * radius;
  for (let i = 0; i < 3; ++i) {
    let r = 0.0;
    for (let j = 0; j < 3; ++j) r += mat[i + 4 * j] * eye[j];
    mat[12 + i] = -r;
  }
  mat[15] = 1.0;
}

function turntableRotate(c, t, dtheta, dphi, droll) {
  fvMove(c.angle, t, dtheta, dphi);
  if (droll) {
    turntableRecalcMatrix(c, t);

    const mat = c.computedMatrix;
    turntableZAxis[0] = mat[2];
    turntableZAxis[1] = mat[6];
    turntableZAxis[2] = mat[10];

    const up = c.computedUp;
    const right = c.computedRight;
    const toward = c.computedToward;

    for (let i = 0; i < 3; ++i) {
      mat[4 * i] = up[i];
      mat[4 * i + 1] = right[i];
      mat[4 * i + 2] = toward[i];
    }
    mat4Rotate(mat, mat, droll, turntableZAxis);
    for (let i = 0; i < 3; ++i) {
      up[i] = mat[4 * i];
      right[i] = mat[4 * i + 1];
    }

    fvSet(c.up, t, up[0], up[1], up[2]);
    fvSet(c.right, t, right[0], right[1], right[2]);
  }
}

function turntablePan(c, t, dx, dy, dz) {
  dx = dx || 0.0;
  dy = dy || 0.0;
  dz = dz || 0.0;

  turntableRecalcMatrix(c, t);
  const mat = c.computedMatrix;

  let ux = mat[1], uy = mat[5], uz = mat[9];
  const ul = len3(ux, uy, uz);
  ux /= ul; uy /= ul; uz /= ul;

  let rx = mat[0], ry = mat[4], rz = mat[8];
  const ru = rx * ux + ry * uy + rz * uz;
  rx -= ux * ru; ry -= uy * ru; rz -= uz * ru;
  const rl = len3(rx, ry, rz);
  rx /= rl; ry /= rl; rz /= rl;

  const vx = rx * dx + ux * dy;
  const vy = ry * dx + uy * dy;
  const vz = rz * dx + uz * dy;
  fvMove(c.center, t, vx, vy, vz);

  let radius = Math.exp(c.computedRadius[0]);
  radius = Math.max(1e-4, radius + dz);
  fvSet(c.radius, t, Math.log(radius));
}


// ---------------------------------------------------------------------------
// Inlined from matrix-camera-controller/matrix.js
// ---------------------------------------------------------------------------
const matrixTVec = [0, 0, 0];

function matrixRecalcMatrix(c, t) {
  const time = c._time;
  const tidx = bsearch.le(time, t);
  const mat = c.computedMatrix;
  if (tidx < 0) return;
  const comps = c._components;
  if (tidx === time.length - 1) {
    let ptr = 16 * tidx;
    for (let i = 0; i < 16; ++i) mat[i] = comps[ptr++];
  } else {
    const dt = time[tidx + 1] - time[tidx];
    let ptr = 16 * tidx;
    const prev = c.prevMatrix;
    let allEqual = true;
    for (let i = 0; i < 16; ++i) prev[i] = comps[ptr++];
    const next = c.nextMatrix;
    for (let i = 0; i < 16; ++i) {
      next[i] = comps[ptr++];
      allEqual = allEqual && prev[i] === next[i];
    }
    if (dt < 1e-6 || allEqual) {
      for (let i = 0; i < 16; ++i) mat[i] = prev[i];
    } else {
      mat4Interpolate(mat, prev, next, (t - time[tidx]) / dt);
    }
  }

  const up = c.computedUp;
  up[0] = mat[1]; up[1] = mat[5]; up[2] = mat[9];
  vec3Normalize(up, up);

  const imat = c.computedInverse;
  mat4Invert(imat, mat);
  const eye = c.computedEye;
  const w = imat[15];
  eye[0] = imat[12] / w;
  eye[1] = imat[13] / w;
  eye[2] = imat[14] / w;

  const center = c.computedCenter;
  const radius = Math.exp(c.computedRadius[0]);
  for (let i = 0; i < 3; ++i) center[i] = eye[i] - mat[2 + 4 * i] * radius;
}

function matrixSetMatrix(c, t, mat) {
  // Inlined: proto.setMatrix
  if (t < c._time[c._time.length - 1]) return;
  c._time.push(t);
  for (let i = 0; i < 16; ++i) c._components.push(mat[i]);
}

function matrixRotate(c, t, yaw, pitch, roll) {
  matrixRecalcMatrix(c, t);
  const mat = c.computedInverse;
  if (yaw)   mat4RotateY(mat, mat, yaw);
  if (pitch) mat4RotateX(mat, mat, pitch);
  if (roll)  mat4RotateZ(mat, mat, roll);
  matrixSetMatrix(c, t, mat4Invert(c.computedMatrix, mat));
}

function matrixPan(c, t, dx, dy, dz) {
  matrixTVec[0] = -(dx || 0.0);
  matrixTVec[1] = -(dy || 0.0);
  matrixTVec[2] = -(dz || 0.0);
  matrixRecalcMatrix(c, t);
  const mat = c.computedInverse;
  mat4Translate(mat, mat, matrixTVec);
  matrixSetMatrix(c, t, mat4Invert(mat, mat));
}


// ---------------------------------------------------------------------------
// Inlined: 3d-view/view.js#rotate, #pan — dispatcher fan-out.
// _controllerList order matches the keys passed to ViewController:
// [turntable, orbit, matrix]. The dispatch loop calls the same method on
// each so all three stay in sync.
// ---------------------------------------------------------------------------
function viewRotate(t, a1, a2, a3) {
  turntableRotate(view._controllerList[0], t, a1, a2, a3);
  orbitRotate(view._controllerList[1], t, a1, a2, a3);
  matrixRotate(view._controllerList[2], t, a1, a2, a3);
}
function viewPan(t, a1, a2, a3) {
  turntablePan(view._controllerList[0], t, a1, a2, a3);
  orbitPan(view._controllerList[1], t, a1, a2, a3);
  matrixPan(view._controllerList[2], t, a1, a2, a3);
}


// ===========================================================================
//   Event handlers
// ===========================================================================

function onMouseMove(event) {
  // ---- Inlined: mouse-change/mouse-listen.js#handleMouseMove -----------
  // Inlined: mouse-event/mouse.js#buttons(ev) — modern path returns ev.buttons.
  const mouseEventButtons = ("buttons" in event)
    ? event.buttons
    : (("which" in event)
        ? (event.which === 2 ? 4
          : event.which === 3 ? 2
          : event.which > 0 ? (1 << (event.which - 1))
          : 0)
        : (("button" in event)
            ? (event.button === 1 ? 4
              : event.button === 2 ? 2
              : event.button >= 0 ? (1 << event.button)
              : 0)
            : 0));

  const dispatchButtons = mouseEventButtons === 0 ? 0 : buttonState;

  // ---- Inlined: handleEvent(dispatchButtons, event) --------------------
  // Inlined: mouse-event/mouse.js#x, #y
  let nextX, nextY;
  if ("offsetX" in event) {
    nextX = event.offsetX;
  } else {
    const target = event.target || event.srcElement || window;
    nextX = event.clientX - target.getBoundingClientRect().left;
  }
  if ("offsetY" in event) {
    nextY = event.offsetY;
  } else {
    const target = event.target || event.srcElement || window;
    nextY = event.clientY - target.getBoundingClientRect().top;
  }

  let nextButtons = dispatchButtons;
  if ("buttons" in event) nextButtons = event.buttons | 0;

  // Inlined: updateMods(ev)
  let modsChanged = false;
  if ("altKey" in event) {
    modsChanged = modsChanged || event.altKey !== mcMods.alt;
    mcMods.alt = !!event.altKey;
  }
  if ("shiftKey" in event) {
    modsChanged = modsChanged || event.shiftKey !== mcMods.shift;
    mcMods.shift = !!event.shiftKey;
  }
  if ("ctrlKey" in event) {
    modsChanged = modsChanged || event.ctrlKey !== mcMods.control;
    mcMods.control = !!event.ctrlKey;
  }
  if ("metaKey" in event) {
    modsChanged = modsChanged || event.metaKey !== mcMods.meta;
    mcMods.meta = !!event.metaKey;
  }

  if (!(nextButtons !== buttonState || nextX !== mcX || nextY !== mcY || modsChanged)) {
    return;
  }
  buttonState = nextButtons | 0;
  mcX = nextX || 0;
  mcY = nextY || 0;

  const buttons = buttonState;
  const x = mcX;
  const y = mcY;
  const mods = mcMods;

  // ---- Inlined: handleInteraction(buttons, x, y, mods) -----------------
  const scale = 1.0 / element.clientHeight;
  const dx = scale * (x - lastX);
  const dy = scale * (y - lastY);

  const flipX = camera.flipX ? 1 : -1;
  const flipY = camera.flipY ? 1 : -1;

  const drot = Math.PI * camera.rotateSpeed;

  // Inlined: now()
  const t = performance.now();

  if (buttons & 1) {
    if (mods.shift) {
      // Inlined: view.rotate(t, 0, 0, -dx * drot)
      viewRotate(t, 0, 0, -dx * drot);
    } else {
      // Inlined: view.rotate(t, flipX*drot*dx, -flipY*drot*dy, 0)
      viewRotate(t, flipX * drot * dx, -flipY * drot * dy, 0);
    }
  } else if (buttons & 2) {
    // Inlined: view.pan(t, -translateSpeed*dx*distance, translateSpeed*dy*distance, 0)
    viewPan(
      t,
      -camera.translateSpeed * dx * distance,
      camera.translateSpeed * dy * distance,
      0,
    );
  } else if (buttons & 4) {
    // Inlined: view.pan(t, 0, 0, distance * (Math.exp(kzoom) - 1))
    const kzoom =
      camera.zoomSpeed * dy / window.innerHeight * (t - view.lastT()) * 50.0;
    viewPan(t, 0, 0, distance * (Math.exp(kzoom) - 1));
  }

  lastX = x;
  lastY = y;
  lastMods = mods;
}


function onWheel(event) {
  // ---- Inlined: mouse-wheel/wheel.js#listener --------------------------
  // The third arg `noScroll = true` was passed in 3d-view-controls.js, so
  // the listener always calls preventDefault().
  event.preventDefault();

  let dx = event.deltaX || 0;
  let dy = event.deltaY || 0;
  let dz = event.deltaZ || 0;
  let wheelScale = 1;
  switch (event.deltaMode) {
    case 1: wheelScale = lineHeight; break;          // DOM_DELTA_LINE
    case 2: wheelScale = window.innerHeight; break;  // DOM_DELTA_PAGE
  }
  dx *= wheelScale;
  dy *= wheelScale;
  dz *= wheelScale;
  if (!(dx || dy || dz)) return;

  // ---- Inlined: wheel callback from 3d-view-controls.js ----------------
  const flipX = camera.flipX ? 1 : -1;
  const flipY = camera.flipY ? 1 : -1;

  // Inlined: now()
  const t = performance.now();

  if (Math.abs(dx) > Math.abs(dy)) {
    // Inlined: view.rotate(t, 0, 0, -dx*flipX*PI*rotateSpeed / window.innerWidth)
    viewRotate(
      t,
      0,
      0,
      -dx * flipX * Math.PI * camera.rotateSpeed / window.innerWidth,
    );
  } else {
    // Inlined: view.pan(t, 0, 0, distance * (Math.exp(kzoom) - 1))
    const kzoom =
      camera.zoomSpeed * flipY * dy / window.innerHeight * (t - view.lastT()) / 100.0;
    viewPan(t, 0, 0, distance * (Math.exp(kzoom) - 1));
  }
}
