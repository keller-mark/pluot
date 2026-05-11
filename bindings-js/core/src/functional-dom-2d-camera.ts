// Functional version of `dom-2d-camera`.
// Each event handler accepts viewportParams, the previous camera matrix, and the event object,
// then returns the next camera matrix. No global camera object or tick function required.

import { mat4, vec2 } from 'gl-matrix';
import { ViewportParams } from './viewport.js';


export type CameraMatrix = Float32Array;

// Camera configuration constants (matching dom-2d-camera defaults)
const isFixed = false;
const isNdc = true;
const isPan = true;
const isPanInverted = [false, true];
const panSpeed = 1;
const isRotate = true;
const rotateSpeed = 1;
const defaultMouseDownMoveAction = "pan";
const mouseDownMoveModKey = "alt";
const isZoom = true;
const zoomSpeed = 1;

// Derived settings
const isPanX = Array.isArray(isPan) ? Boolean(isPan[0]) : Boolean(isPan);
const isPanY = Array.isArray(isPan) ? Boolean(isPan[1]) : Boolean(isPan);
const isPanXInverted = Array.isArray(isPanInverted) ? Boolean(isPanInverted[0]) : Boolean(isPanInverted);
const isPanYInverted = Array.isArray(isPanInverted) ? Boolean(isPanInverted[1]) : Boolean(isPanInverted);
const isZoomX = Array.isArray(isZoom) ? Boolean(isZoom[0]) : Boolean(isZoom);
const isZoomY = Array.isArray(isZoom) ? Boolean(isZoom[1]) : Boolean(isZoom);
const panOnMouseDownMove = defaultMouseDownMoveAction === "pan";

const KEY_MAP: Record<string, string> = {
  alt: "altKey",
  cmd: "metaKey",
  ctrl: "ctrlKey",
  meta: "metaKey",
  shift: "shiftKey",
};


// --- Aspect ratio helpers ---

function computeAspectRatioFactors(vp: ViewportParams) {
  const aspectRatio = vp.width / vp.height;
  let xFactor = 1.0;
  let yFactor = 1.0;

  if (vp.aspectRatioMode === "Contain") {
    if (aspectRatio > 1.0) xFactor = 1.0 / aspectRatio;
    else if (aspectRatio < 1.0) yFactor = aspectRatio;
  } else if (vp.aspectRatioMode === "Cover") {
    if (aspectRatio > 1.0) yFactor = aspectRatio;
    else if (aspectRatio < 1.0) xFactor = 1.0 / aspectRatio;
  }

  let xAlignTranslation = 0.0;
  let yAlignTranslation = 0.0;
  if (vp.aspectRatioAlignmentMode === "Start") {
    xAlignTranslation = xFactor - 1.0;
    yAlignTranslation = yFactor - 1.0;
  } else if (vp.aspectRatioAlignmentMode === "End") {
    xAlignTranslation = 1.0 - xFactor;
    yAlignTranslation = 1.0 - yFactor;
  }

  return { xFactor, yFactor, xAlignTranslation, yAlignTranslation };
}



// --- Event handlers ---

export function onWheel(viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, event: WheelEvent): CameraMatrix {
  if ((!isZoomX && !isZoomY) || isFixed) return prevCameraMatrix;

  const { width: plotWidth, height: plotHeight, margins } = viewportParams;
  const width = plotWidth - ((margins?.marginLeft ?? 0) + (margins?.marginRight ?? 0));
  const height = plotHeight - ((margins?.marginBottom ?? 0) + (margins?.marginTop ?? 0));
  const { xFactor, yFactor, xAlignTranslation, yAlignTranslation } = computeAspectRatioFactors(viewportParams);

  const deltaModeScale = event.deltaMode === 1 ? 12 : 1;
  const scrollDist = deltaModeScale * (event.deltaY || event.deltaX || 0);
  if (!scrollDist) return prevCameraMatrix;

  const dZ = zoomSpeed * Math.exp(scrollDist / height);

  const px = isNdc
    ? ((-1 + (event.offsetX / width) * 2) - xAlignTranslation) * (1.0 / xFactor)
    : event.offsetX;
  const py = isNdc
    ? ((1 - (event.offsetY / height) * 2) - yAlignTranslation) * (1.0 / yFactor)
    : event.offsetY;

  let dx = isZoomX ? 1 / dZ : 1;
  let dy = isZoomY ? 1 / dZ : 1;

  if (dx <= 0 || dy <= 0 || (dx === 1 && dy === 1)) return prevCameraMatrix;

  const view = mat4.clone(prevCameraMatrix);
  const s = mat4.fromScaling(mat4.create(), new Float32Array([dx, dy, 1]));
  const p = new Float32Array([px, py, 0]);
  const a = mat4.fromTranslation(mat4.create(), p);
  const aInv = mat4.invert(mat4.create(), a)!;

  // view = a * s * aInv * prevCameraMatrix  (scale about mouse pivot)
  mat4.multiply(view, aInv, view);
  mat4.multiply(view, s, view);
  mat4.multiply(view, a, view);

  return view as unknown as CameraMatrix;
}

export function onMouseMove(viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, event: MouseEvent): CameraMatrix {
  const { width: plotWidth, height: plotHeight, margins } = viewportParams;
  const width = plotWidth - ((margins?.marginLeft ?? 0) + (margins?.marginRight ?? 0));
  const height = plotHeight - ((margins?.marginBottom ?? 0) + (margins?.marginTop ?? 0));
  const { xFactor, yFactor } = computeAspectRatioFactors(viewportParams);

  const isLeftMousePressed = (event.buttons & 1) !== 0;
  const isMouseDownMoveModActive = Boolean((event as unknown as Record<string, boolean>)[KEY_MAP[mouseDownMoveModKey]]);

  const view = mat4.clone(prevCameraMatrix);
  let changed = false;

  // Pan
  if (
    (isPanX || isPanY) &&
    isLeftMousePressed &&
    ((panOnMouseDownMove && !isMouseDownMoveModActive) ||
      (!panOnMouseDownMove && isMouseDownMoveModActive))
  ) {
    const dX = isPanXInverted ? -event.movementX : event.movementX;
    const dY = isPanYInverted ? -event.movementY : event.movementY;
    const tx = isPanX ? (isNdc ? ((panSpeed * dX) / width) * 2 * (1.0 / xFactor) : panSpeed * dX) : 0;
    const ty = isPanY ? (isNdc ? ((panSpeed * dY) / height) * 2 * (1.0 / yFactor) : -(panSpeed * dY)) : 0;

    if (tx !== 0 || ty !== 0) {
      const t = mat4.fromTranslation(mat4.create(), new Float32Array([tx, ty, 0]));
      mat4.multiply(view, t, view);
      changed = true;
    }
  }

  // Rotate
  if (
    isRotate &&
    isLeftMousePressed &&
    ((panOnMouseDownMove && isMouseDownMoveModActive) ||
      (!panOnMouseDownMove && !isMouseDownMoveModActive)) &&
    (Math.abs(event.movementX) + Math.abs(event.movementY)) > 0
  ) {
    const wh = width / 2;
    const hh = height / 2;
    const x1 = (event.offsetX - event.movementX) - wh;
    const y1 = hh - (event.offsetY - event.movementY);
    const x2 = event.offsetX - wh;
    const y2 = hh - event.offsetY;

    if (x1 * x1 + y1 * y1 > 0 && x2 * x2 + y2 * y2 > 0) {
      const radians = vec2.angle([x1, y1], [x2, y2]);
      const cross = x1 * y2 - x2 * y1;
      const rad = rotateSpeed * radians * Math.sign(cross);
      if (rad !== 0) {
        const r = mat4.fromRotation(mat4.create(), rad, new Float32Array([0, 0, 1]));
        mat4.multiply(view, r, view);
        changed = true;
      }
    }
  }

  return changed ? view as unknown as CameraMatrix : prevCameraMatrix;
}

// These handlers don't modify the camera matrix in the functional approach.
// Button state is read from event.buttons in onMouseMove; modifier key state
// is read from event.altKey/etc. in onMouseMove.

export function onMouseDown(_viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, _event: MouseEvent): CameraMatrix {
  return prevCameraMatrix;
}

export function onMouseUp(_viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, _event: MouseEvent): CameraMatrix {
  return prevCameraMatrix;
}

export function onKeyDown(_viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, _event: KeyboardEvent): CameraMatrix {
  return prevCameraMatrix;
}

export function onKeyUp(_viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, _event: KeyboardEvent): CameraMatrix {
  return prevCameraMatrix;
}
