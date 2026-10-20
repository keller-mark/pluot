import { type CameraMatrix } from "./functional-dom-2d-camera.js";
import { getBounds, getCameraMatrixFromBounds, type Bounds, type ViewportParams } from "./viewport.js";
// These are 2D camera "filter" functions which can be used to modify the camera behavior when used prior to calling setCameraMatrix.

export type CameraFilterContext = ViewportParams & {
  /** The union of the layer extents along each axis, as returned by extent_wasm. */
  xExtent: [number, number] | null;
  yExtent: [number, number] | null;
};

export type CameraFilterFunction = (prevCameraMatrix: CameraMatrix, nextCameraMatrix: CameraMatrix, context?: CameraFilterContext) => CameraMatrix;

function getLayerSize({ width, height, margins }: ViewportParams) {
  return {
    layerW: width - (margins?.marginLeft ?? 0) - (margins?.marginRight ?? 0),
    layerH: height - (margins?.marginTop ?? 0) - (margins?.marginBottom ?? 0),
  };
}

// The matrices are column-major (gl-matrix), so row 0 (output X) lives at indices 0, 4, 8, 12
// and row 1 (output Y) lives at indices 1, 5, 9, 13.
const X_ROW_INDICES = [0, 4, 8, 12];
const Y_ROW_INDICES = [1, 5, 9, 13];
const X_SCALE_INDEX = 0;
const Y_SCALE_INDEX = 5;
const X_TRANSLATION_INDEX = 12;
const Y_TRANSLATION_INDEX = 13;

function copyRow(prevCameraMatrix: CameraMatrix, nextCameraMatrix: CameraMatrix, rowIndices: number[]): CameraMatrix {
  const result = new Float32Array(nextCameraMatrix);
  for (const i of rowIndices) {
    result[i] = prevCameraMatrix[i];
  }
  return result;
}

function normToNdc(value: number): number {
  return value * 2 - 1;
}

export function fixX(prevCameraMatrix: CameraMatrix, nextCameraMatrix: CameraMatrix): CameraMatrix {
  return copyRow(prevCameraMatrix, nextCameraMatrix, X_ROW_INDICES);
}

export function fixY(prevCameraMatrix: CameraMatrix, nextCameraMatrix: CameraMatrix): CameraMatrix {
  return copyRow(prevCameraMatrix, nextCameraMatrix, Y_ROW_INDICES);
}

// The shaders convert data coordinates to NDC before applying the camera, so data 0 enters
// the camera at NDC -1 and the pinned translation must compensate for the current zoom.
// The target coordinate is prior to the aspect ratio transform, and rotation is ignored.
function pinAxis(nextCameraMatrix: CameraMatrix, coord: number, scaleIndex: number, translationIndex: number): CameraMatrix {
  const result = new Float32Array(nextCameraMatrix);
  result[translationIndex] = normToNdc(coord) - result[scaleIndex] * normToNdc(0);
  return result;
}

export function getFixXAxisAtYCoord(yCoord: number) {
  return (_prevCameraMatrix: CameraMatrix, nextCameraMatrix: CameraMatrix): CameraMatrix =>
    pinAxis(nextCameraMatrix, yCoord, Y_SCALE_INDEX, Y_TRANSLATION_INDEX);
}

export function getFixYAxisAtXCoord(xCoord: number) {
  return (_prevCameraMatrix: CameraMatrix, nextCameraMatrix: CameraMatrix): CameraMatrix =>
    pinAxis(nextCameraMatrix, xCoord, X_SCALE_INDEX, X_TRANSLATION_INDEX);
}

// Convenience exports which use the above functions.
export const fixXAxisAtYZero = getFixXAxisAtYCoord(0.0);
export const fixYAxisAtXZero = getFixYAxisAtXCoord(0.0);

export function fixXAndFixXAxisAtYZero(prev: CameraMatrix, next: CameraMatrix): CameraMatrix {
  return fixXAxisAtYZero(prev, fixX(prev, next));
}

export function fixYAndFixYAxisAtXZero(prev: CameraMatrix, next: CameraMatrix): CameraMatrix {
  return fixYAxisAtXZero(prev, fixY(prev, next));
}

type AxisBounds = { min: number, max: number };

function getAxes(bounds: Required<Bounds>): [AxisBounds, AxisBounds] {
  return [{ min: bounds.xMin, max: bounds.xMax }, { min: bounds.yMin, max: bounds.yMax }];
}

function span(axis: AxisBounds): number {
  return axis.max - axis.min;
}

// The data coordinate which stays fixed on screen between the two bounds (i.e., the zoom pivot),
// or null when the span did not change (i.e., a pan).
function getZoomAnchor(prev: AxisBounds, next: AxisBounds): number | null {
  const spanRatio = span(next) / span(prev);
  if (Math.abs(1.0 - spanRatio) < 1e-9) {
    return null;
  }
  return (next.min - spanRatio * prev.min) / (1.0 - spanRatio);
}

function zoomAxisAbout(axis: AxisBounds, anchor: number, nextSpan: number): AxisBounds {
  const spanRatio = nextSpan / span(axis);
  return {
    min: anchor + (axis.min - anchor) * spanRatio,
    max: anchor + (axis.max - anchor) * spanRatio,
  };
}

function withAxes(nextCameraMatrix: CameraMatrix, [x, y]: [AxisBounds, AxisBounds], context: CameraFilterContext): CameraMatrix {
  const fitted = getCameraMatrixFromBounds({ xMin: x.min, xMax: x.max, yMin: y.min, yMax: y.max }, nextCameraMatrix, context);
  const result = new Float32Array(nextCameraMatrix);
  for (const i of [X_SCALE_INDEX, Y_SCALE_INDEX, X_TRANSLATION_INDEX, Y_TRANSLATION_INDEX]) {
    result[i] = fitted[i];
  }
  return result;
}

// When the visible span is wider than the extent, nothing outside the extent can be hidden,
// so the extent is instead kept entirely in view.
function clampAxisPan(axis: AxisBounds, [lo, hi]: [number, number]): AxisBounds {
  const isWiderThanExtent = span(axis) > hi - lo;
  let shift = 0.0;
  if (isWiderThanExtent ? axis.min > lo : axis.min < lo) {
    shift = lo - axis.min;
  } else if (isWiderThanExtent ? axis.max < hi : axis.max > hi) {
    shift = hi - axis.max;
  }
  return { min: axis.min + shift, max: axis.max + shift };
}

/**
 * Prevents zooming or panning beyond `context.xExtent`/`context.yExtent`.
 * Under "Contain"/"Cover", zooming out stops once the whole extent fits along one axis,
 * so that both axes stay zoomed equally.
 */
export function limitExtent(prevCameraMatrix: CameraMatrix, nextCameraMatrix: CameraMatrix, context?: CameraFilterContext): CameraMatrix {
  if (!context || (!context.xExtent && !context.yExtent)) {
    return nextCameraMatrix;
  }
  const extents = [context.xExtent, context.yExtent];
  const prevAxes = getAxes(getBounds(prevCameraMatrix, context));
  let axes = getAxes(getBounds(nextCameraMatrix, context));

  const spanRatios = axes.map((axis, i) => {
    const extent = extents[i];
    return extent ? (extent[1] - extent[0]) / span(axis) : null;
  });
  const minUniformSpanRatio = Math.max(...spanRatios.filter((r): r is number => r !== null));
  axes = axes.map((axis, i) => {
    const spanRatio = context.aspectRatioMode === "Ignore" ? spanRatios[i] : minUniformSpanRatio;
    if (spanRatio === null || spanRatio >= 1.0) {
      return axis;
    }
    const anchor = getZoomAnchor(prevAxes[i], axis) ?? (axis.min + axis.max) / 2.0;
    return zoomAxisAbout(axis, anchor, span(axis) * spanRatio);
  }) as [AxisBounds, AxisBounds];

  axes = axes.map((axis, i) => {
    const extent = extents[i];
    return extent ? clampAxisPan(axis, extent) : axis;
  }) as [AxisBounds, AxisBounds];

  return withAxes(nextCameraMatrix, axes, context);
}

/**
 * Under "Ignore", the data is initially stretched to fill the viewport.
 * Zooming in first zooms along the more-compressed axis only, until one data unit spans
 * the same number of pixels along both axes, and then zooms along both axes.
 * Zooming out reverses this, stopping each axis once it shows its full extent.
 */
export function zoomToSquare(prevCameraMatrix: CameraMatrix, nextCameraMatrix: CameraMatrix, context?: CameraFilterContext): CameraMatrix {
  if (!context || context.aspectRatioMode !== "Ignore") {
    return nextCameraMatrix;
  }
  const prevAxes = getAxes(getBounds(prevCameraMatrix, context));
  const nextAxes = getAxes(getBounds(nextCameraMatrix, context));
  const anchors = prevAxes.map((prev, i) => getZoomAnchor(prev, nextAxes[i]));
  const zoomFactors = prevAxes.map((prev, i) => span(prev) / span(nextAxes[i]));
  if (anchors[0] === null || anchors[1] === null || Math.abs(zoomFactors[0] - zoomFactors[1]) > 1e-6) {
    return nextCameraMatrix;
  }
  const zoomFactor = zoomFactors[0];

  const { layerW, layerH } = getLayerSize(context);
  const pixelsPerUnit = [layerW / span(prevAxes[0]), layerH / span(prevAxes[1])];
  const extents = [context.xExtent, context.yExtent];

  const axisZoomFactors = pixelsPerUnit.map((ppu, i) => {
    if (zoomFactor > 1.0) {
      const otherPpu = pixelsPerUnit[1 - i];
      return ppu <= otherPpu ? zoomFactor : Math.max(1.0, zoomFactor * otherPpu / ppu);
    }
    const extent = extents[i];
    if (!extent) {
      return zoomFactor;
    }
    const fullExtentZoomFactor = span(prevAxes[i]) / (extent[1] - extent[0]);
    return Math.max(zoomFactor, Math.min(1.0, fullExtentZoomFactor));
  });

  const axes = prevAxes.map((prev, i) => (
    zoomAxisAbout(prev, anchors[i] as number, span(prev) / axisZoomFactors[i])
  )) as [AxisBounds, AxisBounds];
  return withAxes(nextCameraMatrix, axes, context);
}
