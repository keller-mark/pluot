import { type CameraMatrix } from "./functional-dom-2d-camera.js";
// These are 2D camera "filter" functions which can be used to modify the camera behavior when used prior to calling setCameraMatrix.


export type CameraFilterFunction = (prevCameraMatrix: CameraMatrix, nextCameraMatrix: CameraMatrix) => CameraMatrix;

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
export const fixYAxisAtXZero = getFixXAxisAtYCoord(0.0);

export function fixXAndFixXAxisAtYZero(prev: CameraMatrix, next: CameraMatrix): CameraMatrix {
  return fixXAxisAtYZero(prev, fixX(prev, next));
}

export function fixYAndFixYAxisAtXZero(prev: CameraMatrix, next: CameraMatrix): CameraMatrix {
  return fixYAxisAtXZero(prev, fixY(prev, next));
}

// TODO: zoom-to-square function: given an aspect ratio, filter the camera so that we zoom along a single axis until the aspect ratio is a square,
// then zoom along both axes keeping the square aspect ratio.

// TODO: limit-extent function: given an xLim and/or yLim, prevent the camera from zooming or panning to show anything outside of the range(s) along this axis or these axes.
