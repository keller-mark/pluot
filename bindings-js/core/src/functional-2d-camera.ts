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


// --- Matrix operation helpers (pure, no mutation of input) ---

function applyTranslate(view: CameraMatrix, dx: number, dy: number): CameraMatrix {
  const t = mat4.create();
  mat4.fromTranslation(t, [dx, dy, 0]);
  const result = mat4.create();
  mat4.multiply(result, t, view);
  return result as CameraMatrix;
}

function applyScale(view: CameraMatrix, sx: number, sy: number, cx: number, cy: number): CameraMatrix {
  // Replicate camera-2d-simple's scale: view = a * s * inv(a) * view
  // where a = translate to scale center, s = scale matrix
  const s = mat4.create();
  mat4.fromScaling(s, [sx, sy, 1]);
  const a = mat4.create();
  mat4.fromTranslation(a, [cx, cy, 0]);
  const aInv = mat4.create();
  mat4.invert(aInv, a);

  const temp1 = mat4.create();
  mat4.multiply(temp1, aInv, view);   // inv(a) * view
  const temp2 = mat4.create();
  mat4.multiply(temp2, s, temp1);     // s * inv(a) * view
  const result = mat4.create();
  mat4.multiply(result, a, temp2);    // a * s * inv(a) * view
  return result as CameraMatrix;
}

function applyRotate(view: CameraMatrix, rad: number): CameraMatrix {
  const r = mat4.create();
  mat4.fromRotation(r, rad, [0, 0, 1]);
  const result = mat4.create();
  mat4.multiply(result, r, view);
  return result as CameraMatrix;
}


// --- Event handlers ---

export function onWheel(viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, event: WheelEvent): CameraMatrix {
  if (isFixed || (!isZoomX && !isZoomY)) return prevCameraMatrix;

  const { width, height } = viewportParams;
  const { xFactor, yFactor, xAlignTranslation, yAlignTranslation } = computeAspectRatioFactors(viewportParams);

  const scaleFactor = event.deltaMode === 1 ? 12 : 1;
  const scrollDist = scaleFactor * (event.deltaY || event.deltaX || 0);
  if (scrollDist === 0) return prevCameraMatrix;

  const dZ = zoomSpeed * Math.exp(scrollDist / height);

  // Transform mouse position to camera space
  const mouseRelX = event.offsetX;
  const mouseRelY = event.offsetY;
  const transformedX = isNdc
    ? ((-1 + (mouseRelX / width) * 2) - xAlignTranslation) * (1.0 / xFactor)
    : mouseRelX;
  const transformedY = isNdc
    ? ((1 - (mouseRelY / height) * 2) - yAlignTranslation) * (1.0 / yFactor)
    : mouseRelY;

  return applyScale(
    prevCameraMatrix,
    isZoomX ? 1 / dZ : 1,
    isZoomY ? 1 / dZ : 1,
    transformedX,
    transformedY,
  );
}

export function onMouseMove(viewportParams: ViewportParams, prevCameraMatrix: CameraMatrix, event: MouseEvent): CameraMatrix {
  if (isFixed) return prevCameraMatrix;

  const isLeftMousePressed = (event.buttons & 1) !== 0;
  if (!isLeftMousePressed) return prevCameraMatrix;

  const modKey = KEY_MAP[mouseDownMoveModKey];
  const isMouseDownMoveModActive = Boolean(modKey && (event as any)[modKey]);

  const { width, height } = viewportParams;
  const { xFactor, yFactor } = computeAspectRatioFactors(viewportParams);

  // Pan (uses event.movementX/Y for the pixel delta since last mousemove)
  if ((isPanX || isPanY) &&
    ((panOnMouseDownMove && !isMouseDownMoveModActive) ||
     (!panOnMouseDownMove && isMouseDownMoveModActive))) {

    const rawDX = isPanXInverted ? -event.movementX : event.movementX;
    const rawDY = isPanYInverted ? -event.movementY : event.movementY;

    const transformedPanX = isPanX
      ? (isNdc ? (rawDX / width) * 2 * (1.0 / xFactor) : rawDX) * panSpeed
      : 0;
    const transformedPanY = isPanY
      ? (isNdc ? (rawDY / height) * 2 * (1.0 / yFactor) : -rawDY) * panSpeed
      : 0;

    if (transformedPanX !== 0 || transformedPanY !== 0) {
      return applyTranslate(prevCameraMatrix, transformedPanX, transformedPanY);
    }
  }

  // Rotate
  if (isRotate &&
    ((panOnMouseDownMove && isMouseDownMoveModActive) ||
     (!panOnMouseDownMove && !isMouseDownMoveModActive))) {

    const wh = width / 2;
    const hh = height / 2;

    // Derive previous position from current + movementX/Y
    const currentX = event.offsetX;
    const currentY = event.offsetY;
    const prevX = currentX - event.movementX;
    const prevY = currentY - event.movementY;

    const x1 = prevX - wh;
    const y1 = hh - prevY;
    const x2 = currentX - wh;
    const y2 = hh - currentY;

    if (Math.abs(x1 - x2) + Math.abs(y1 - y2) > 0) {
      const radians = vec2.angle([x1, y1], [x2, y2]);
      const cross = x1 * y2 - x2 * y1;
      return applyRotate(prevCameraMatrix, rotateSpeed * radians * Math.sign(cross));
    }
  }

  return prevCameraMatrix;
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
