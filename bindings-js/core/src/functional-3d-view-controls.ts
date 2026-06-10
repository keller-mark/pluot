// Functional/stateless adaptation of 3d-view-controls.
//
// License copied from https://github.com/mikolalysenko/3d-view-controls/blob/master/LICENSE
//
// The MIT License (MIT)
//
// Copyright (c) 2013 Mikola Lysenko
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

import { mat4, vec3, quat } from 'gl-matrix';
import { ViewportParams } from './viewport.js';
import { type CameraMatrix } from './functional-dom-2d-camera.js';

// Camera settings matching 3d-view-controls.js defaults
const ROTATE_SPEED = 1.0;
const ZOOM_SPEED = 1.0;
const TRANSLATE_SPEED = 1.0;
const FLIP_X = false;
const FLIP_Y = false;

// Low-level helpers

function len3(x: number, y: number, z: number): number {
  return Math.sqrt(x * x + y * y + z * z);
}

// Extract eye position from a column-major view matrix.
// eye = -R^T * t  (R is upper-left 3x3, t is translation column)
function eyeFromMat(m: Float32Array): vec3 {
  const tx = m[12], ty = m[13], tz = m[14];
  return vec3.fromValues(
    -(m[0] * tx + m[1] * ty + m[2] * tz),
    -(m[4] * tx + m[5] * ty + m[6] * tz),
    -(m[8] * tx + m[9] * ty + m[10] * tz),
  );
}

// Rebuild a lookAt view matrix from eye, center, and up.
function buildLookAt(eye: vec3, center: vec3, up: vec3): Float32Array {
  const m = mat4.create();
  mat4.lookAt(m, eye, center, up);
  return m as Float32Array;
}

// Orbit controller
// State layout: Float32Array of 20 elements
//   [0-15]: view matrix (column-major)
//   [16-18]: center x, y, z
//   [19]: log(radius)

function orbitStateFrom(cam: CameraMatrix): [Float32Array, vec3, number] {
  const m = new Float32Array(cam.subarray ? cam.subarray(0, 16) : cam.slice(0, 16));
  const cx = (cam.length > 16 && isFinite(cam[16])) ? cam[16] : 0;
  const cy = (cam.length > 17 && isFinite(cam[17])) ? cam[17] : 0;
  const cz = (cam.length > 18 && isFinite(cam[18])) ? cam[18] : 0;
  const center = vec3.fromValues(cx, cy, cz);
  let logRadius: number;
  if (cam.length > 19 && isFinite(cam[19])) {
    logRadius = cam[19];
  } else {
    const eye = eyeFromMat(m);
    const radius = vec3.distance(eye, center);
    logRadius = Math.log(Math.max(1e-4, radius));
  }
  return [m, center, logRadius];
}

function packOrbit(m: Float32Array, center: vec3, logRadius: number): CameraMatrix {
  const result = new Float32Array(20);
  result.set(m, 0);
  result[16] = center[0];
  result[17] = center[1];
  result[18] = center[2];
  result[19] = logRadius;
  return result as CameraMatrix;
}

function orbitRotate(cam: CameraMatrix, dx: number, dy: number, dz: number): CameraMatrix {
  const [m, center, logRadius] = orbitStateFrom(cam);

  // Camera frame from view matrix rows (column-major: row i = mat[i], mat[4+i], mat[8+i])
  const rx = m[0], ry = m[4], rz = m[8];    // right
  const ux = m[1], uy = m[5], uz = m[9];    // up
  const fx = m[2], fy = m[6], fz = m[10];   // forward (center --> eye direction)

  // World-space direction from screen deltas
  const qx = dx * rx + dy * ux;
  const qy = dx * ry + dy * uy;
  const qz = dx * rz + dy * uz;

  // Rotation axis b_axis = -(forward x q)
  let bx = -(fy * qz - fz * qy);
  let by = -(fz * qx - fx * qz);
  let bz = -(fx * qy - fy * qx);
  let bw = Math.sqrt(Math.max(0.0, 1.0 - bx * bx - by * by - bz * bz));
  const bl = Math.sqrt(bx * bx + by * by + bz * bz + bw * bw);
  if (bl > 1e-6) { bx /= bl; by /= bl; bz /= bl; bw /= bl; }
  else { bx = by = bz = 0; bw = 1; }

  const bq = quat.fromValues(bx, by, bz, bw);

  // Rotate eye around center
  const eye = eyeFromMat(m);
  const relEye = vec3.fromValues(eye[0] - center[0], eye[1] - center[1], eye[2] - center[2]);
  vec3.transformQuat(relEye, relEye, bq);
  const newEye = vec3.fromValues(center[0] + relEye[0], center[1] + relEye[1], center[2] + relEye[2]);

  // Rotate up
  const upVec = vec3.fromValues(ux, uy, uz);
  vec3.transformQuat(upVec, upVec, bq);

  if (dz) {
    const fwd = vec3.fromValues(fx, fy, fz);
    const rollQ = quat.setAxisAngle(quat.create(), fwd, dz);
    vec3.transformQuat(upVec, upVec, rollQ);
  }

  const newMat = buildLookAt(newEye, center, upVec);
  return packOrbit(newMat, center, logRadius);
}

function orbitPan(cam: CameraMatrix, dx: number, dy: number, dz: number): CameraMatrix {
  const [m, center, logRadius] = orbitStateFrom(cam);

  // Normalized up
  let ux = m[1], uy = m[5], uz = m[9];
  const ul = len3(ux, uy, uz);
  ux /= ul; uy /= ul; uz /= ul;

  // Right orthogonalized to up
  let rx = m[0], ry = m[4], rz = m[8];
  const ru = rx * ux + ry * uy + rz * uz;
  rx -= ux * ru; ry -= uy * ru; rz -= uz * ru;
  const rl = len3(rx, ry, rz);
  rx /= rl; ry /= rl; rz /= rl;

  const newCenter = vec3.fromValues(
    center[0] + rx * dx + ux * dy,
    center[1] + ry * dx + uy * dy,
    center[2] + rz * dx + uz * dy,
  );

  const radius = Math.exp(logRadius);
  const newLogRadius = Math.log(Math.max(1e-4, radius + dz));
  const newRadius = Math.exp(newLogRadius);

  // Maintain eye direction (forward = row 2 of view matrix)
  const fx = m[2], fy = m[6], fz = m[10];
  const newEye = vec3.fromValues(
    newCenter[0] + fx * newRadius,
    newCenter[1] + fy * newRadius,
    newCenter[2] + fz * newRadius,
  );
  const upVec = vec3.fromValues(ux, uy, uz);

  const newMat = buildLookAt(newEye, newCenter, upVec);
  return packOrbit(newMat, newCenter, newLogRadius);
}

// Turntable controller
// State layout: Float32Array of 28 elements
//   [0-15]: view matrix
//   [16-18]: center
//   [19]: log(radius)
//   [20]: theta (azimuth)
//   [21]: phi (elevation)
//   [22-24]: up_base
//   [25-27]: right_base

// Default turntable base frame: Y-up, X-right, -Z-toward
const TT_UP_DEFAULT: [number, number, number] = [0, 1, 0];
const TT_RIGHT_DEFAULT: [number, number, number] = [1, 0, 0];

// Derive initial (theta, phi) from a view matrix assuming standard base frame.
function anglesFromMat(m: Float32Array): { theta: number; phi: number } {
  // forward row (row 2): mat[2], mat[6], mat[10] = outward direction (center-->eye)
  // With right_base=(1,0,0), toward_base=(0,0,-1), up_base=(0,1,0):
  //   wx = dot(fw, right_base) = fw.x
  //   wy = dot(fw, toward_base) = -fw.z
  //   wz = dot(fw, up_base) = fw.y
  //   wz = sin(phi)  -->  phi = asin(fw.y)
  //   wx = cos(phi)*cos(theta), wy = cos(phi)*sin(theta) --> theta = atan2(wy, wx) = atan2(-fw.z, fw.x)
  const fy = m[6];
  const phi = Math.asin(Math.max(-1, Math.min(1, fy)));
  const theta = Math.atan2(-m[10], m[2]);
  return { theta, phi };
}

type TurntableState = {
  m: Float32Array;
  center: vec3;
  logRadius: number;
  theta: number;
  phi: number;
  upBase: vec3;
  rightBase: vec3;
};

function turntableStateFrom(cam: CameraMatrix): TurntableState {
  const m = new Float32Array(cam.subarray ? cam.subarray(0, 16) : cam.slice(0, 16));
  const cx = (cam.length > 16 && isFinite(cam[16])) ? cam[16] : 0;
  const cy = (cam.length > 17 && isFinite(cam[17])) ? cam[17] : 0;
  const cz = (cam.length > 18 && isFinite(cam[18])) ? cam[18] : 0;
  const center = vec3.fromValues(cx, cy, cz);

  let logRadius: number;
  if (cam.length > 19 && isFinite(cam[19])) {
    logRadius = cam[19];
  } else {
    const eye = eyeFromMat(m);
    const radius = vec3.distance(eye, center);
    logRadius = Math.log(Math.max(1e-4, radius));
  }

  let theta: number, phi: number;
  if (cam.length > 21 && isFinite(cam[20]) && isFinite(cam[21])) {
    theta = cam[20];
    phi = cam[21];
  } else {
    ({ theta, phi } = anglesFromMat(m));
  }

  const upBase = (cam.length > 24 && isFinite(cam[22]))
    ? vec3.fromValues(cam[22], cam[23], cam[24])
    : vec3.fromValues(...TT_UP_DEFAULT);
  const rightBase = (cam.length > 27 && isFinite(cam[25]))
    ? vec3.fromValues(cam[25], cam[26], cam[27])
    : vec3.fromValues(...TT_RIGHT_DEFAULT);

  return { m, center, logRadius, theta, phi, upBase, rightBase };
}

function turntableRecalc(
  center: vec3, logRadius: number, theta: number, phi: number, upBase: vec3, rightBase: vec3
): { m: Float32Array; upBase: vec3; rightBase: vec3 } {
  // Gram-Schmidt orthonormalize upBase and rightBase
  const up = vec3.clone(upBase);
  const right = vec3.clone(rightBase);

  const uu = vec3.dot(up, up);
  const ur = vec3.dot(up, right);
  vec3.normalize(up, up);
  // right = right - up * (dot(right, up) / dot(up, up))
  vec3.scaleAndAdd(right, right, up, -ur / uu);
  vec3.normalize(right, right);

  // toward = up x right
  const toward = vec3.create();
  vec3.cross(toward, up, right);
  vec3.normalize(toward, toward);

  const radius = Math.exp(logRadius);
  const ctheta = Math.cos(theta), stheta = Math.sin(theta);
  const cphi = Math.cos(phi), sphi = Math.sin(phi);

  // Outward direction (center --> eye) in (right, toward, up) frame
  const wx = ctheta * cphi, wy = stheta * cphi, wz = sphi;
  // Screen-up direction in same frame
  const sx = -ctheta * sphi, sy = -stheta * sphi, sz = cphi;

  // Fill rows 1 (screen-up) and 2 (forward/outward) of the view matrix.
  // Column-major: mat[4*col + row].
  const mat = new Float32Array(16);
  for (let i = 0; i < 3; ++i) {
    mat[4 * i + 1] = sx * right[i] + sy * toward[i] + sz * up[i]; // row 1
    mat[4 * i + 2] = wx * right[i] + wy * toward[i] + wz * up[i]; // row 2
    mat[4 * i + 3] = 0.0;
  }

  // Row 0 (right) = cross(row1, row2) normalized
  const a0 = mat[1], a1 = mat[5], a2 = mat[9];   // row 1
  const b0 = mat[2], b1 = mat[6], b2 = mat[10];  // row 2
  let c0 = a1 * b2 - a2 * b1;
  let c1 = a2 * b0 - a0 * b2;
  let c2 = a0 * b1 - a1 * b0;
  const cl = len3(c0, c1, c2);
  c0 /= cl; c1 /= cl; c2 /= cl;
  mat[0] = c0; mat[4] = c1; mat[8] = c2;

  // Eye = center + forward * radius (forward = row 2 = mat[2], mat[6], mat[10])
  const eyeArr = [
    center[0] + mat[2] * radius,
    center[1] + mat[6] * radius,
    center[2] + mat[10] * radius,
  ];

  // Translation column: mat[12+i] = -dot(row_i, eye)
  for (let i = 0; i < 3; ++i) {
    let r = 0;
    for (let j = 0; j < 3; ++j) r += mat[i + 4 * j] * eyeArr[j];
    mat[12 + i] = -r;
  }
  mat[15] = 1.0;

  return { m: mat, upBase: up, rightBase: right };
}

function packTurntable(
  m: Float32Array, center: vec3, logRadius: number,
  theta: number, phi: number, upBase: vec3, rightBase: vec3
): CameraMatrix {
  const result = new Float32Array(28);
  result.set(m, 0);
  result[16] = center[0]; result[17] = center[1]; result[18] = center[2];
  result[19] = logRadius;
  result[20] = theta;
  result[21] = phi;
  result[22] = upBase[0]; result[23] = upBase[1]; result[24] = upBase[2];
  result[25] = rightBase[0]; result[26] = rightBase[1]; result[27] = rightBase[2];
  return result as CameraMatrix;
}

function turntableRotate(
  cam: CameraMatrix, dtheta: number, dphi: number, droll: number
): CameraMatrix {
  const { center, logRadius, theta: t0, phi: p0, upBase, rightBase } = turntableStateFrom(cam);

  const theta = t0 + dtheta;
  const phi = Math.max(-Math.PI / 2, Math.min(Math.PI / 2, p0 + dphi));

  let { m, upBase: newUp, rightBase: newRight } = turntableRecalc(center, logRadius, theta, phi, upBase, rightBase);

  if (droll) {
    const fwd = vec3.fromValues(m[2], m[6], m[10]);
    const rollQ = quat.setAxisAngle(quat.create(), fwd, droll);
    const rollUp = vec3.clone(newUp);
    const rollRight = vec3.clone(newRight);
    vec3.transformQuat(rollUp, rollUp, rollQ);
    vec3.transformQuat(rollRight, rollRight, rollQ);
    const recalc = turntableRecalc(center, logRadius, theta, phi, rollUp, rollRight);
    m = recalc.m;
    newUp = recalc.upBase;
    newRight = recalc.rightBase;
  }

  return packTurntable(m, center, logRadius, theta, phi, newUp, newRight);
}

function turntablePan(cam: CameraMatrix, dx: number, dy: number, dz: number): CameraMatrix {
  const { m, center, logRadius, theta, phi, upBase, rightBase } = turntableStateFrom(cam);

  let ux = m[1], uy = m[5], uz = m[9];
  const ul = len3(ux, uy, uz);
  ux /= ul; uy /= ul; uz /= ul;

  let rx = m[0], ry = m[4], rz = m[8];
  const ru = rx * ux + ry * uy + rz * uz;
  rx -= ux * ru; ry -= uy * ru; rz -= uz * ru;
  const rl = len3(rx, ry, rz);
  rx /= rl; ry /= rl; rz /= rl;

  const newCenter = vec3.fromValues(
    center[0] + rx * dx + ux * dy,
    center[1] + ry * dx + uy * dy,
    center[2] + rz * dx + uz * dy,
  );

  const radius = Math.exp(logRadius);
  const newLogRadius = Math.log(Math.max(1e-4, radius + dz));

  const { m: newMat, upBase: newUp, rightBase: newRight } = turntableRecalc(
    newCenter, newLogRadius, theta, phi, upBase, rightBase
  );

  return packTurntable(newMat, newCenter, newLogRadius, theta, phi, newUp, newRight);
}

// Matrix controller
// State: just the 16-element view matrix

function matrixRotate(cam: CameraMatrix, yaw: number, pitch: number, roll: number): CameraMatrix {
  const imat = mat4.create();
  mat4.invert(imat, cam.subarray ? cam.subarray(0, 16) as Float32Array : new Float32Array(cam.slice(0, 16)));
  if (yaw)   mat4.rotateY(imat, imat, yaw);
  if (pitch) mat4.rotateX(imat, imat, pitch);
  if (roll)  mat4.rotateZ(imat, imat, roll);
  const result = mat4.create();
  mat4.invert(result, imat);
  return result as CameraMatrix;
}

function matrixPan(cam: CameraMatrix, dx: number, dy: number, dz: number): CameraMatrix {
  const imat = mat4.create();
  mat4.invert(imat, cam.subarray ? cam.subarray(0, 16) as Float32Array : new Float32Array(cam.slice(0, 16)));
  mat4.translate(imat, imat, [-dx, -dy, -dz]);
  const result = mat4.create();
  mat4.invert(result, imat);
  return result as CameraMatrix;
}

// Event handler helpers

function getWheelDeltas(event: WheelEvent, viewportParams: ViewportParams): { dx: number; dy: number } {
  let dx = event.deltaX || 0;
  let dy = event.deltaY || 0;
  let wheelScale = 1;
  if (event.deltaMode === 1) wheelScale = 20;
  else if (event.deltaMode === 2) wheelScale = viewportParams.height;
  return { dx: dx * wheelScale, dy: dy * wheelScale };
}

// Turntable event handlers

export function onWheel3dTurntable(viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, event: WheelEvent): CameraMatrix {
  event.preventDefault();
  const { dx, dy } = getWheelDeltas(event, viewportParams);
  if (!dx && !dy) return prevCameraMatrix;

  const flipX = FLIP_X ? 1 : -1;
  const flipY = FLIP_Y ? 1 : -1;
  const { logRadius } = turntableStateFrom(prevCameraMatrix);
  const distance = Math.exp(logRadius);

  if (Math.abs(dx) > Math.abs(dy)) {
    return turntableRotate(prevCameraMatrix, 0, 0, -dx * flipX * Math.PI * ROTATE_SPEED / viewportParams.width);
  } else {
    const kzoom = ZOOM_SPEED * dy / viewportParams.height * 0.16;
    return turntablePan(prevCameraMatrix, 0, 0, distance * (Math.exp(kzoom) - 1));
  }
}

export function onMouseMove3dTurntable(viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, event: MouseEvent): CameraMatrix {
  const { height } = viewportParams;
  const scale = 1.0 / height;
  const dx = scale * event.movementX;
  const dy = scale * event.movementY;
  if (!dx && !dy) return prevCameraMatrix;

  const flipX = FLIP_X ? 1 : -1;
  const flipY = FLIP_Y ? 1 : -1;
  const drot = Math.PI * ROTATE_SPEED;
  const buttons = event.buttons;

  if (buttons & 1) {
    if (event.shiftKey) {
      return turntableRotate(prevCameraMatrix, 0, 0, -dx * drot);
    } else {
      return turntableRotate(prevCameraMatrix, -flipX * drot * dx, flipY * drot * dy, 0);
    }
  } else if (buttons & 2) {
    const { logRadius } = turntableStateFrom(prevCameraMatrix);
    const distance = Math.exp(logRadius);
    return turntablePan(prevCameraMatrix, TRANSLATE_SPEED * dx * distance, -TRANSLATE_SPEED * dy * distance, 0);
  } else if (buttons & 4) {
    const { logRadius } = turntableStateFrom(prevCameraMatrix);
    const distance = Math.exp(logRadius);
    const kzoom = ZOOM_SPEED * dy / height * 0.32;
    return turntablePan(prevCameraMatrix, 0, 0, distance * (Math.exp(kzoom) - 1));
  }

  return prevCameraMatrix;
}

// Orbit event handlers

export function onWheel3dOrbit(viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, event: WheelEvent): CameraMatrix {
  event.preventDefault();
  const { dx, dy } = getWheelDeltas(event, viewportParams);
  if (!dx && !dy) return prevCameraMatrix;

  const flipX = FLIP_X ? 1 : -1;
  const flipY = FLIP_Y ? 1 : -1;
  const [, , logRadius] = orbitStateFrom(prevCameraMatrix);
  const distance = Math.exp(logRadius);

  if (Math.abs(dx) > Math.abs(dy)) {
    return orbitRotate(prevCameraMatrix, 0, 0, -dx * flipX * Math.PI * ROTATE_SPEED / viewportParams.width);
  } else {
    const kzoom = ZOOM_SPEED * dy / viewportParams.height * 0.16;
    return orbitPan(prevCameraMatrix, 0, 0, distance * (Math.exp(kzoom) - 1));
  }
}

export function onMouseMove3dOrbit(viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, event: MouseEvent): CameraMatrix {
  const { height } = viewportParams;
  const scale = 1.0 / height;
  const dx = scale * event.movementX;
  const dy = scale * event.movementY;
  if (!dx && !dy) return prevCameraMatrix;

  const flipX = FLIP_X ? 1 : -1;
  const flipY = FLIP_Y ? 1 : -1;
  const drot = Math.PI * ROTATE_SPEED;
  const buttons = event.buttons;

  if (buttons & 1) {
    if (event.shiftKey) {
      return orbitRotate(prevCameraMatrix, 0, 0, -dx * drot);
    } else {
      return orbitRotate(prevCameraMatrix, -flipX * drot * dx, flipY * drot * dy, 0);
    }
  } else if (buttons & 2) {
    const [, , logRadius] = orbitStateFrom(prevCameraMatrix);
    const distance = Math.exp(logRadius);
    return orbitPan(prevCameraMatrix, TRANSLATE_SPEED * dx * distance, -TRANSLATE_SPEED * dy * distance, 0);
  } else if (buttons & 4) {
    const [, , logRadius] = orbitStateFrom(prevCameraMatrix);
    const distance = Math.exp(logRadius);
    const kzoom = ZOOM_SPEED * dy / height * 0.32;
    return orbitPan(prevCameraMatrix, 0, 0, distance * (Math.exp(kzoom) - 1));
  }

  return prevCameraMatrix;
}

// Matrix event handlers

export function onWheel3dMatrix(viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, event: WheelEvent): CameraMatrix {
  event.preventDefault();
  const { dx, dy } = getWheelDeltas(event, viewportParams);
  if (!dx && !dy) return prevCameraMatrix;

  const flipX = FLIP_X ? 1 : -1;
  const flipY = FLIP_Y ? 1 : -1;
  const eye = eyeFromMat(prevCameraMatrix as Float32Array);
  const distance = vec3.length(eye);

  if (Math.abs(dx) > Math.abs(dy)) {
    return matrixRotate(prevCameraMatrix, 0, 0, -dx * flipX * Math.PI * ROTATE_SPEED / viewportParams.width);
  } else {
    const kzoom = ZOOM_SPEED * dy / viewportParams.height * 0.16;
    return matrixPan(prevCameraMatrix, 0, 0, distance * (Math.exp(kzoom) - 1));
  }
}

export function onMouseMove3dMatrix(viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, event: MouseEvent): CameraMatrix {
  const { height } = viewportParams;
  const scale = 1.0 / height;
  const dx = scale * event.movementX;
  const dy = scale * event.movementY;
  if (!dx && !dy) return prevCameraMatrix;

  const flipX = FLIP_X ? 1 : -1;
  const flipY = FLIP_Y ? 1 : -1;
  const drot = Math.PI * ROTATE_SPEED;
  const buttons = event.buttons;
  const eye = eyeFromMat(prevCameraMatrix as Float32Array);
  const distance = vec3.length(eye);

  if (buttons & 1) {
    if (event.shiftKey) {
      return matrixRotate(prevCameraMatrix, 0, 0, -dx * drot);
    } else {
      return matrixRotate(prevCameraMatrix, -flipX * drot * dx, flipY * drot * dy, 0);
    }
  } else if (buttons & 2) {
    return matrixPan(prevCameraMatrix, TRANSLATE_SPEED * dx * distance, -TRANSLATE_SPEED * dy * distance, 0);
  } else if (buttons & 4) {
    const kzoom = ZOOM_SPEED * dy / height * 0.32;
    return matrixPan(prevCameraMatrix, 0, 0, distance * (Math.exp(kzoom) - 1));
  }

  return prevCameraMatrix;
}

// Default 3D handlers (orbit controller)

export function onWheel(viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, event: WheelEvent): CameraMatrix {
  return onWheel3dOrbit(viewportParams, prevCameraMatrix, event);
}

export function onMouseMove(viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, event: MouseEvent): CameraMatrix {
  return onMouseMove3dOrbit(viewportParams, prevCameraMatrix, event);
}
