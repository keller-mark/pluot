// Functional version of `3d-view-controls`.
// Each event handler accepts viewportParams, the previous camera matrix, and the event object,
// then returns the next camera matrix. No controllers / filtered-vector state required.
//
// Implements only the matrix controller's logic from the original
// (the only one whose state is derived purely from a 4x4 view matrix).

import { mat4 } from 'gl-matrix';
import { ViewportParams } from './viewport.js';
import { type CameraMatrix } from './functional-dom-2d-camera.js';

// Camera configuration constants (matching 3d-view-controls defaults)
const rotateSpeed = 1;
const zoomSpeed = 1;
const translateSpeed = 1;
const flipX = false;
const flipY = false;

// The original times mouse-zoom by `(t - view.lastT())`. Without prior-event
// state, assume a single animation frame (~16ms) of elapsed time.
const ASSUMED_DT_MS = 16;

// Pixel approximation for `deltaMode === 1` (DOM_DELTA_LINE).
// The original computes this from `toPX('ex', element)`; 12px matches typical defaults.
const LINE_HEIGHT_PX = 12;


// Distance from the eye to the focus point.
// The original tracks this via a filtered `radius`; we approximate by
// assuming the focus point is the world origin, so distance = |eye|.
function getDistance(view: CameraMatrix): number {
  const inv = mat4.create();
  mat4.invert(inv, view);
  const w = inv[15] || 1;
  const ex = inv[12] / w;
  const ey = inv[13] / w;
  const ez = inv[14] / w;
  return Math.sqrt(ex * ex + ey * ey + ez * ez);
}

// Inlined: matrixRotate from matrix-camera-controller.
// Rotations are right-multiplied onto the inverse (world-from-camera) matrix —
// equivalent to rotating about the camera's local axes.
function applyRotate(prevView: CameraMatrix, yaw: number, pitch: number, roll: number): CameraMatrix {
  const imat = mat4.create();
  mat4.invert(imat, prevView);
  if (yaw) mat4.rotateY(imat, imat, yaw);
  if (pitch) mat4.rotateX(imat, imat, pitch);
  if (roll) mat4.rotateZ(imat, imat, roll);
  const out = mat4.create();
  mat4.invert(out, imat);
  return out as unknown as CameraMatrix;
}

// Inlined: matrixPan from matrix-camera-controller.
// Translates the inverse (world-from-camera) matrix by -(dx,dy,dz) in local
// camera space — i.e., moves the camera by +(dx,dy,dz) in its own frame.
function applyPan(prevView: CameraMatrix, dx: number, dy: number, dz: number): CameraMatrix {
  const imat = mat4.create();
  mat4.invert(imat, prevView);
  mat4.translate(imat, imat, new Float32Array([-dx, -dy, -dz]));
  const out = mat4.create();
  mat4.invert(out, imat);
  return out as unknown as CameraMatrix;
}


// --- Event handlers ---

export function onMouseMove(viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, event: MouseEvent): CameraMatrix {
  const { height: plotHeight, margins } = viewportParams;
  const height = plotHeight - ((margins?.marginBottom ?? 0) + (margins?.marginTop ?? 0));

  // Normalise mouse movement by element height (matches `scale = 1/clientHeight`).
  const scale = 1.0 / height;
  const dx = scale * event.movementX;
  const dy = scale * event.movementY;

  if (dx === 0 && dy === 0) return prevCameraMatrix;

  const flipXSign = flipX ? 1 : -1;
  const flipYSign = flipY ? 1 : -1;
  const drot = Math.PI * rotateSpeed;

  if (event.buttons & 1) {
    // Left button — rotate (roll only if shift held)
    if (event.shiftKey) {
      return applyRotate(prevCameraMatrix, 0, 0, -dx * drot);
    }
    return applyRotate(prevCameraMatrix, flipXSign * drot * dx, -flipYSign * drot * dy, 0);
  }

  if (event.buttons & 2) {
    // Right button — pan in the camera's screen plane
    const distance = getDistance(prevCameraMatrix);
    return applyPan(
      prevCameraMatrix,
      -translateSpeed * dx * distance,
      translateSpeed * dy * distance,
      0,
    );
  }

  if (event.buttons & 4) {
    // Middle button — dolly along the camera's forward axis
    const distance = getDistance(prevCameraMatrix);
    const kzoom = zoomSpeed * dy / height * ASSUMED_DT_MS * 50.0;
    return applyPan(prevCameraMatrix, 0, 0, distance * (Math.exp(kzoom) - 1));
  }

  return prevCameraMatrix;
}

export function onWheel(viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, event: WheelEvent): CameraMatrix {
  const { width: plotWidth, height: plotHeight, margins } = viewportParams;
  const width = plotWidth - ((margins?.marginLeft ?? 0) + (margins?.marginRight ?? 0));
  const height = plotHeight - ((margins?.marginBottom ?? 0) + (margins?.marginTop ?? 0));

  let dx = event.deltaX || 0;
  let dy = event.deltaY || 0;

  let wheelScale = 1;
  if (event.deltaMode === 1) wheelScale = LINE_HEIGHT_PX;       // DOM_DELTA_LINE
  else if (event.deltaMode === 2) wheelScale = height;          // DOM_DELTA_PAGE
  dx *= wheelScale;
  dy *= wheelScale;

  if (dx === 0 && dy === 0) return prevCameraMatrix;

  const flipXSign = flipX ? 1 : -1;
  const flipYSign = flipY ? 1 : -1;

  if (Math.abs(dx) > Math.abs(dy)) {
    // Horizontal scroll — roll around the camera's forward axis
    return applyRotate(
      prevCameraMatrix,
      0,
      0,
      -dx * flipXSign * Math.PI * rotateSpeed / width,
    );
  }

  // Vertical scroll — dolly
  const distance = getDistance(prevCameraMatrix);
  const kzoom = zoomSpeed * flipYSign * dy / height * ASSUMED_DT_MS / 100.0;
  return applyPan(prevCameraMatrix, 0, 0, distance * (Math.exp(kzoom) - 1));
}
