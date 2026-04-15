// License copied from https://github.com/flekschas/dom-2d-camera/blob/master/LICENSE.md
//
// This software is released under the MIT license:
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software is furnished to do so,
// subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
// FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
// COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
// IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
// CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

// Reference: https://github.com/flekschas/dom-2d-camera/blob/cd59ea035a0ea72c2c0535fa3721f8127946576c/src/index.js
// TODO: contribute modifications back upstream once things are working.

import { vec2 } from "gl-matrix";
import createCamera from "camera-2d-simple";

const MOUSE_DOWN_MOVE_ACTIONS = ["pan", "rotate"];
const KEY_MAP = {
  alt: "altKey",
  cmd: "metaKey",
  ctrl: "ctrlKey",
  meta: "metaKey",
  shift: "shiftKey"
};

const dom2dCamera = (
  element,
  {
    distance = 1.0,
    target = [0, 0],
    rotation = 0,
    isNdc = true,
    isFixed = false,
    isPan = true,
    isPanInverted = [false, true],
    panSpeed = 1,
    isRotate = true,
    rotateSpeed = 1,
    defaultMouseDownMoveAction = "pan",
    mouseDownMoveModKey = "alt",
    isZoom = true,
    zoomSpeed = 1,
    viewCenter,
    scaleBounds,
    translationBounds,
    onKeyDown = () => {},
    onKeyUp = () => {},
    onMouseDown = () => {},
    onMouseUp = () => {},
    onMouseMove = () => {},
    onWheel = () => {},
    aspectRatioMode = "Ignore",
    aspectRatioAlignmentMode = "Center",
  } = {}
) => {
  let camera = createCamera(
    target,
    distance,
    rotation,
    viewCenter,
    scaleBounds,
    translationBounds
  );
  let mouseX = 0;
  let mouseY = 0;
  let mouseRelX = 0;
  let mouseRelY = 0;
  let prevMouseX = 0;
  let prevMouseY = 0;
  let isLeftMousePressed = false;
  let scrollDist = 0;

  let width = 1;
  let height = 1;
  let aspectRatio = 1;

  let isInteractivelyChanged = false;
  let isProgrammaticallyChanged = false;
  let isMouseDownMoveModActive = false;

  let panOnMouseDownMove = defaultMouseDownMoveAction === "pan";

  let isPanX = isPan;
  let isPanY = isPan;
  let isPanXInverted = isPanInverted;
  let isPanYInverted = isPanInverted;
  let isZoomX = isZoom;
  let isZoomY = isZoom;

  const spreadXYSettings = () => {
    isPanX = Array.isArray(isPan) ? Boolean(isPan[0]) : isPan;
    isPanY = Array.isArray(isPan) ? Boolean(isPan[1]) : isPan;
    isPanXInverted = Array.isArray(isPanInverted)
      ? Boolean(isPanInverted[0])
      : isPanInverted;
    isPanYInverted = Array.isArray(isPanInverted)
      ? Boolean(isPanInverted[1])
      : isPanInverted;
    isZoomX = Array.isArray(isZoom) ? Boolean(isZoom[0]) : isZoom;
    isZoomY = Array.isArray(isZoom) ? Boolean(isZoom[1]) : isZoom;
  };

  spreadXYSettings();

  let xAspectRatioModeFactor = 1.0;
  let yAspectRatioModeFactor = 1.0;
  let xAlignmentTranslation = 0.0;
  let yAlignmentTranslation = 0.0;

  /*
    // Logic for aspect ratio handling in point_layer.wgsl
    if (aspect_ratio_mode == 1u) {
        // fit/contain
        if (layer_aspect_ratio > 1.0) {
            // Wide rectangle
            // Show more than (0, 1) in x direction. Show exactly (0, 1) in y direction.
            x_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
        } else if(layer_aspect_ratio < 1.0) {
            // Tall layer
            // Show exactly (0, 1) in x direction. Show more than (0, 1) in y direction.
            y_scale_for_aspect_ratio_mode = layer_aspect_ratio;
        } else {
            // Square layer; no change needed.
            // Show exactly (0, 1) in both directions.
        }
    } else if (aspect_ratio_mode == 2u) {
        // fill/cover
        if(layer_aspect_ratio > 1.0) {
            // Wide rectangle
            // Show exactly (0, 1) in x direction. Show less than (0, 1) in y direction.
            y_scale_for_aspect_ratio_mode = layer_aspect_ratio;
        } else if(layer_aspect_ratio < 1.0) {
            // Tall layer
            // Show less than (0, 1) in x direction. Show exactly (0, 1) in y direction.
            x_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
        } else {
            // Square layer; no change needed.
            // Show exactly (0, 1) in both directions.
        }
    }
  */
  const updateAspectRatioModeFactors = () => {
    xAspectRatioModeFactor = 1.0;
    yAspectRatioModeFactor = 1.0;
    if(aspectRatioMode === "Contain") {
      if(aspectRatio > 1.0) {
        xAspectRatioModeFactor = 1.0 / aspectRatio;
      } else if(aspectRatio < 1.0) {
        yAspectRatioModeFactor = aspectRatio;
      }
    } else if(aspectRatioMode === "Cover") {
      if(aspectRatio > 1.0) {
        yAspectRatioModeFactor = aspectRatio;
      } else if(aspectRatio < 1.0) {
        xAspectRatioModeFactor = 1.0 / aspectRatio;
      }
    }

    xAlignmentTranslation = 0.0;
    yAlignmentTranslation = 0.0;
    if(aspectRatioAlignmentMode === "Start") {
      xAlignmentTranslation = xAspectRatioModeFactor - 1.0;
      yAlignmentTranslation = yAspectRatioModeFactor - 1.0;
    } else if(aspectRatioAlignmentMode === "End") {
      xAlignmentTranslation = 1.0 - xAspectRatioModeFactor;
      yAlignmentTranslation = 1.0 - yAspectRatioModeFactor;
    }
  };
  updateAspectRatioModeFactors();

  const transformPanX = isNdc
    ? dX => (dX / width) * 2 * (1.0 / xAspectRatioModeFactor) // to normalized device coords
    : dX => dX;
  const transformPanY = isNdc
    ? dY => (dY / height) * 2 * (1.0 / yAspectRatioModeFactor) // to normalized device coords
    : dY => -dY;

  const transformScaleX = isNdc
    ? x => ((-1 + (x / width) * 2) - xAlignmentTranslation) * (1.0 / xAspectRatioModeFactor)
    : x => x;
  const transformScaleY = isNdc
    ? y => ((1 - (y / height) * 2) - yAlignmentTranslation) * (1.0 / yAspectRatioModeFactor)
    : y => y;

  const tick = () => {
    if (isFixed) {
      const isChanged = isProgrammaticallyChanged;
      isProgrammaticallyChanged = false;
      return isChanged;
    }

    isInteractivelyChanged = false;
    const currentMouseX = mouseX;
    const currentMouseY = mouseY;

    if (
      (isPanX || isPanY) &&
      isLeftMousePressed &&
      ((panOnMouseDownMove && !isMouseDownMoveModActive) ||
        (!panOnMouseDownMove && isMouseDownMoveModActive))
    ) {
      const dX = isPanXInverted
        ? prevMouseX - currentMouseX
        : currentMouseX - prevMouseX;

      const transformedPanX = isPanX ? transformPanX(panSpeed * dX) : 0;

      const dY = isPanYInverted
        ? prevMouseY - currentMouseY
        : currentMouseY - prevMouseY;

      const transformedPanY = isPanY ? transformPanY(panSpeed * dY) : 0;

      if (transformedPanX !== 0 || transformedPanY !== 0) {
        camera.pan([transformedPanX, transformedPanY]);
        isInteractivelyChanged = true;
      }
    }

    if ((isZoomX || isZoomY) && scrollDist) {
      const dZ = zoomSpeed * Math.exp(scrollDist / height);

      const transformedX = transformScaleX(mouseRelX);
      const transformedY = transformScaleY(mouseRelY);

      camera.scale(
        [isZoomX ? 1 / dZ : 1, isZoomY ? 1 / dZ : 1],
        [transformedX, transformedY]
      );

      isInteractivelyChanged = true;
    }

    if (
      isRotate &&
      isLeftMousePressed &&
      ((panOnMouseDownMove && isMouseDownMoveModActive) ||
        (!panOnMouseDownMove && !isMouseDownMoveModActive)) &&
      Math.abs(prevMouseX - currentMouseX) +
        Math.abs(prevMouseY - currentMouseY) >
        0
    ) {
      const wh = width / 2;
      const hh = height / 2;
      const x1 = prevMouseX - wh;
      const y1 = hh - prevMouseY;
      const x2 = currentMouseX - wh;
      const y2 = hh - currentMouseY;
      // Angle between the start and end mouse position with respect to the
      // viewport center
      const radians = vec2.angle([x1, y1], [x2, y2]);
      // Determine the orientation
      const cross = x1 * y2 - x2 * y1;

      camera.rotate(rotateSpeed * radians * Math.sign(cross));

      isInteractivelyChanged = true;
    }

    // Reset scroll delta and mouse position
    scrollDist = 0;
    prevMouseX = currentMouseX;
    prevMouseY = currentMouseY;

    const isChanged = isInteractivelyChanged || isProgrammaticallyChanged;

    isProgrammaticallyChanged = false;

    return isChanged;
  };

  const config = ({
    defaultMouseDownMoveAction: newDefaultMouseDownMoveAction = null,
    isFixed: newIsFixed = null,
    isPan: newIsPan = null,
    isPanInverted: newIsPanInverted = null,
    isRotate: newIsRotate = null,
    isZoom: newIsZoom = null,
    panSpeed: newPanSpeed = null,
    rotateSpeed: newRotateSpeed = null,
    zoomSpeed: newZoomSpeed = null,
    mouseDownMoveModKey: newMouseDownMoveModKey = null
  } = {}) => {
    defaultMouseDownMoveAction =
      newDefaultMouseDownMoveAction !== null &&
      MOUSE_DOWN_MOVE_ACTIONS.includes(newDefaultMouseDownMoveAction)
        ? newDefaultMouseDownMoveAction
        : defaultMouseDownMoveAction;

    panOnMouseDownMove = defaultMouseDownMoveAction === "pan";

    isFixed = newIsFixed !== null ? newIsFixed : isFixed;
    isPan = newIsPan !== null ? newIsPan : isPan;
    isPanInverted =
      newIsPanInverted !== null ? newIsPanInverted : isPanInverted;
    isRotate = newIsRotate !== null ? newIsRotate : isRotate;
    isZoom = newIsZoom !== null ? newIsZoom : isZoom;
    panSpeed = +newPanSpeed > 0 ? newPanSpeed : panSpeed;
    rotateSpeed = +newRotateSpeed > 0 ? newRotateSpeed : rotateSpeed;
    zoomSpeed = +newZoomSpeed > 0 ? newZoomSpeed : zoomSpeed;

    spreadXYSettings();

    mouseDownMoveModKey =
      newMouseDownMoveModKey !== null &&
      Object.keys(KEY_MAP).includes(newMouseDownMoveModKey)
        ? newMouseDownMoveModKey
        : mouseDownMoveModKey;
  };

  const refresh = () => {
    const bBox = element.getBoundingClientRect();
    width = bBox.width;
    height = bBox.height;
    aspectRatio = width / height;
    updateAspectRatioModeFactors();
  };

  const keyUpHandler = event => {
    isMouseDownMoveModActive = false;

    onKeyUp(event);
  };

  const keyDownHandler = event => {
    isMouseDownMoveModActive = event[KEY_MAP[mouseDownMoveModKey]];

    onKeyDown(event);
  };

  const mouseUpHandler = event => {
    isLeftMousePressed = false;

    onMouseUp(event);
  };

  const mouseDownHandler = event => {
    isLeftMousePressed = event.buttons === 1;

    onMouseDown(event);
  };

  const offsetXSupport =
    document.createEvent("MouseEvent").offsetX !== undefined;

  const updateMouseRelXY = offsetXSupport
    ? event => {
        mouseRelX = event.offsetX;
        mouseRelY = event.offsetY;
      }
    : event => {
        const bBox = element.getBoundingClientRect();
        mouseRelX = event.clientX - bBox.left;
        mouseRelY = event.clientY - bBox.top;
      };

  const updateMouseXY = event => {
    mouseX = event.clientX;
    mouseY = event.clientY;
  };

  const mouseMoveHandler = event => {
    updateMouseXY(event);
    onMouseMove(event);
  };

  const wheelHandler = event => {
    if ((isZoomX || isZoomY) && !isFixed) {
      event.preventDefault();

      updateMouseXY(event);
      updateMouseRelXY(event);

      const scale = event.deltaMode === 1 ? 12 : 1;

      scrollDist += scale * (event.deltaY || event.deltaX || 0);
    }

    onWheel(event);
  };

  const dispose = () => {
    camera = undefined;
    element.removeEventListener("keydown", keyDownHandler);
    element.removeEventListener("keyup", keyUpHandler);
    element.removeEventListener("mousedown", mouseDownHandler);
    element.removeEventListener("mouseup", mouseUpHandler);
    element.removeEventListener("mousemove", mouseMoveHandler);
    element.removeEventListener("wheel", wheelHandler);
  };

  element.addEventListener("keydown", keyDownHandler, { passive: true });
  element.addEventListener("keyup", keyUpHandler, { passive: true });
  element.addEventListener("mousedown", mouseDownHandler, { passive: true });
  element.addEventListener("mouseup", mouseUpHandler, { passive: true });
  element.addEventListener("mousemove", mouseMoveHandler, { passive: true });
  element.addEventListener("wheel", wheelHandler, { passive: false });

  camera.config = config;
  camera.dispose = dispose;
  camera.refresh = refresh;
  camera.tick = tick;

  const withProgrammaticChange = fn =>
    function() {
      fn.apply(null, arguments);
      isProgrammaticallyChanged = true;
    };

  camera.lookAt = withProgrammaticChange(camera.lookAt);
  camera.translate = withProgrammaticChange(camera.translate);
  camera.pan = withProgrammaticChange(camera.pan);
  camera.rotate = withProgrammaticChange(camera.rotate);
  camera.scale = withProgrammaticChange(camera.scale);
  camera.zoom = withProgrammaticChange(camera.zoom);
  camera.reset = withProgrammaticChange(camera.reset);
  camera.set = withProgrammaticChange(camera.set);
  camera.setScaleBounds = withProgrammaticChange(camera.setScaleBounds);
  camera.setTranslationBounds = withProgrammaticChange(
    camera.setTranslationBounds
  );
  camera.setView = withProgrammaticChange(camera.setView);
  camera.setViewCenter = withProgrammaticChange(camera.setViewCenter);

  refresh();

  return camera;
};

export default dom2dCamera;
