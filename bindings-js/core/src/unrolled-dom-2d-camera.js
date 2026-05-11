import { mat4, vec2 } from "gl-matrix";

// Closure state inherited from dom-2d-camera.js (initialized elsewhere):
//   element
//   isPanX, isPanY, isPanXInverted, isPanYInverted
//   isZoomX, isZoomY, isFixed, isNdc, isRotate
//   panSpeed, zoomSpeed, rotateSpeed
//   panOnMouseDownMove, isMouseDownMoveModActive, isLeftMousePressed
//   width, height
//   xAspectRatioModeFactor, yAspectRatioModeFactor
//   xAlignmentTranslation, yAlignmentTranslation
//   mouseX, mouseY, mouseRelX, mouseRelY, prevMouseX, prevMouseY
//   scrollDist
//   view (mat4), viewCenter (vec4)
//   scaleXBounds, scaleYBounds
//   scratch0, scratch1, scratch2 (Float32Array(16))
//   isInteractivelyChanged, isProgrammaticallyChanged
//   offsetXSupport = document.createEvent("MouseEvent").offsetX !== undefined
//   onMouseMove, onWheel (user callbacks)


function onMouseMove(event) {
  // ---- Inlined: mouseMoveHandler ---------------------------------------
  // Inlined: updateMouseXY(event)
  mouseX = event.clientX;
  mouseY = event.clientY;

  //onMouseMove(event); // user callback

  // ---- Inlined: tick() — pan + rotate branches -------------------------
  // (In the original, tick() runs on each animation frame and consumes
  // the mouseX/mouseY the mousemove handler just wrote. Inlining it here
  // shows the full path of work triggered by a mousemove event.)
  isInteractivelyChanged = false;
  const currentMouseX = mouseX;
  const currentMouseY = mouseY;

  // ---- pan branch ------------------------------------------------------
  if (
    (isPanX || isPanY) &&
    isLeftMousePressed &&
    ((panOnMouseDownMove && !isMouseDownMoveModActive) ||
      (!panOnMouseDownMove && isMouseDownMoveModActive))
  ) {
    const dX = isPanXInverted
      ? prevMouseX - currentMouseX
      : currentMouseX - prevMouseX;
    // Inlined: transformPanX(panSpeed * dX)
    const transformedPanX = isPanX
      ? (isNdc
          ? ((panSpeed * dX) / width) * 2 * (1.0 / xAspectRatioModeFactor)
          : panSpeed * dX)
      : 0;

    const dY = isPanYInverted
      ? prevMouseY - currentMouseY
      : currentMouseY - prevMouseY;
    // Inlined: transformPanY(panSpeed * dY)
    const transformedPanY = isPanY
      ? (isNdc
          ? ((panSpeed * dY) / height) * 2 * (1.0 / yAspectRatioModeFactor)
          : -(panSpeed * dY))
      : 0;

    if (transformedPanX !== 0 || transformedPanY !== 0) {
      // Inlined: camera.pan([transformedPanX, transformedPanY])
      // — `pan` is an alias for `translate` in camera-2d-simple.
      {
        const x = transformedPanX;
        const y = transformedPanY;
        scratch0[0] = x;
        scratch0[1] = y;
        scratch0[2] = 0;

        const t = mat4.fromTranslation(scratch1, scratch0);

        // Translate about the viewport center
        // (identical to `i * t * i * view` where `i` is the identity).
        mat4.multiply(view, t, view);

        // Inlined: withProgrammaticChange wrapper around camera.pan.
        isProgrammaticallyChanged = true;
      }
      isInteractivelyChanged = true;
    }
  }

  // ---- rotate branch ---------------------------------------------------
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
    // viewport center.
    const radians = vec2.angle([x1, y1], [x2, y2]);
    // Determine the orientation.
    const cross = x1 * y2 - x2 * y1;

    // Inlined: camera.rotate(rotateSpeed * radians * Math.sign(cross))
    // — from camera-2d-simple/dist/camera-2d.esm.js
    {
      const rad = rotateSpeed * radians * Math.sign(cross);
      const r = mat4.create();
      mat4.fromRotation(r, rad, [0, 0, 1]);

      // Rotate about the viewport center
      // (identical to `i * r * i * view` where `i` is the identity).
      mat4.multiply(view, r, view);

      // Inlined: withProgrammaticChange wrapper around camera.rotate.
      isProgrammaticallyChanged = true;
    }

    isInteractivelyChanged = true;
  }

  // Inlined: tick()'s tail — advance prev mouse position, report change.
  prevMouseX = currentMouseX;
  prevMouseY = currentMouseY;
  const isChanged = isInteractivelyChanged || isProgrammaticallyChanged;
  isProgrammaticallyChanged = false;
  return isChanged;
}

function onWheel(event) {
  // ---- Inlined: wheelHandler -------------------------------------------
  if ((isZoomX || isZoomY) && !isFixed) {
    event.preventDefault();

    // Inlined: updateMouseXY(event)
    mouseX = event.clientX;
    mouseY = event.clientY;

    // Inlined: updateMouseRelXY(event)
    if (offsetXSupport) {
      mouseRelX = event.offsetX;
      mouseRelY = event.offsetY;
    } else {
      const bBox = element.getBoundingClientRect();
      mouseRelX = event.clientX - bBox.left;
      mouseRelY = event.clientY - bBox.top;
    }

    const deltaModeScale = event.deltaMode === 1 ? 12 : 1;
    scrollDist += deltaModeScale * (event.deltaY || event.deltaX || 0);
  }

  //onWheel(event); // user callback

  // ---- Inlined: tick() — zoom branch only ------------------------------
  // (In the original, tick() runs on each animation frame and consumes
  // the scrollDist that the wheel handler accumulated. Inlining it here
  // shows the full path of work triggered by a wheel event.)
  isInteractivelyChanged = false;

  if ((isZoomX || isZoomY) && scrollDist) {
    const dZ = zoomSpeed * Math.exp(scrollDist / height);

    // Inlined: transformScaleX(mouseRelX)
    const transformedX = isNdc
      ? ((-1 + (mouseRelX / width) * 2) - xAlignmentTranslation) * (1.0 / xAspectRatioModeFactor)
      : mouseRelX;
    // Inlined: transformScaleY(mouseRelY)
    const transformedY = isNdc
      ? ((1 - (mouseRelY / height) * 2) - yAlignmentTranslation) * (1.0 / yAspectRatioModeFactor)
      : mouseRelY;

    // Inlined: camera.scale(
    //   [isZoomX ? 1 / dZ : 1, isZoomY ? 1 / dZ : 1],
    //   [transformedX, transformedY],
    // )
    // — from camera-2d-simple/dist/camera-2d.esm.js
    {
      const d = [isZoomX ? 1 / dZ : 1, isZoomY ? 1 / dZ : 1];
      const mousePos = [transformedX, transformedY];

      // const isArray = Array.isArray(d); // always true here
      let dx = d[0];
      let dy = d[1];

      if (!(dx <= 0 || dy <= 0 || (dx === 1 && dy === 1))) {
        // Inlined: getScaling() => mat4.getScaling(scratch0, view).slice(0, 2)
        const scaling = mat4.getScaling(scratch0, view).slice(0, 2);
        const newXScale = scaling[0] * dx;
        const newYScale = scaling[1] * dy;

        dx =
          Math.max(scaleXBounds[0], Math.min(newXScale, scaleXBounds[1])) /
          scaling[0];
        dy =
          Math.max(scaleYBounds[0], Math.min(newYScale, scaleYBounds[1])) /
          scaling[1];

        if (!(dx === 1 && dy === 1)) {
          scratch0[0] = dx;
          scratch0[1] = dy;
          scratch0[2] = 1;

          const s = mat4.fromScaling(scratch1, scratch0);

          const scaleCenter = mousePos ? [...mousePos, 0] : viewCenter;
          const a = mat4.fromTranslation(scratch0, scaleCenter);

          // Translate about the scale center (mouse position).
          mat4.multiply(
            view,
            a,
            mat4.multiply(
              view,
              s,
              mat4.multiply(view, mat4.invert(scratch2, a), view),
            ),
          );

          // Inlined: withProgrammaticChange wrapper around camera.scale.
          isProgrammaticallyChanged = true;
        }
      }
    }

    isInteractivelyChanged = true;
  }

  // Inlined: tick()'s tail — reset scroll delta and report change.
  scrollDist = 0;
  const isChanged = isInteractivelyChanged || isProgrammaticallyChanged;
  isProgrammaticallyChanged = false;
  return isChanged;
}
