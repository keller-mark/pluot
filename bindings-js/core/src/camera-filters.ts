import { type CameraMatrix } from "./functional-dom-2d-camera.js";
// These are 2D camera "filter" functions which can be used to modify the camera behavior when used prior to calling setCameraMatrix.

export function fixX(prevCameraMatrix: CameraMatrix, nextCameraMatrix: CameraMatrix): CameraMatrix {
  // We want to fix the X-axis values (zoom and pan; taking them from the previous matrix),
  // while we allow the Y-axis values to vary.

}

export function fixY(prevCameraMatrix: CameraMatrix, nextCameraMatrix: CameraMatrix): CameraMatrix {
  // We want to fix the Y-axis values (zoom and pan; taking them from the previous matrix),
  // while we allow the X-axis values to vary.
}

export function getFixXAxisAtYCoord(yCoord: number) {
  // Given a normalized value (between 0 and 1) along the Y axis,
  // ensure that Y=0 (the X axis) is fixed at this coordinate upon camera updates.
  return (prevCameraMatrix: CameraMatrix, nextCameraMatrix: CameraMatrix): CameraMatrix => {

  };
}

export function getFixYAxisAtXCoord(xCoord: number) {
  // Given a normalized value (between 0 and 1) along the X axis,
  // ensure that X=0 (the Y axis) is fixed at this coordinate upon camera updates.
  return (prevCameraMatrix: CameraMatrix, nextCameraMatrix: CameraMatrix): CameraMatrix => {

  };
}
