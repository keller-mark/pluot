import React, { useEffect, useEffectEvent, useMemo, useRef, useState } from "react";
import { throttle } from "lodash-es";

// Visual styling of the brush overlay.
const BRUSH_STROKE_COLOR = "#333333";
const BRUSH_FILL_COLOR = "rgba(51, 51, 51, 0.15)";
const BRUSH_STROKE_WIDTH_PX = 1.5;
const LASSO_VERTEX_RADIUS_PX = 3;
const MAYBE_BRUSH_RADIUS_PX = 14;

// Mouse movement (in pixels) beyond which a click-and-hold is considered a
// drag (e.g. panning) rather than the beginning of a long-press brush.
const LONG_PRESS_THRESHOLD_PX = 3;

// While drawing a lasso, the cursor position is only sampled this often, so that
// a slow drag does not produce a polygon with thousands of vertices.
const LASSO_THROTTLE_MS = 40;
// Additionally, consecutive lasso vertices must be at least this far apart.
const LASSO_MIN_VERTEX_DISTANCE_PX = 4;

/**
 * Compute the rectangle (in pixel coordinates relative to the top-left of the
 * full plot) that corresponds to a brushRegion value.
 * @param {string} brushRegion One of "full", "layer", "marginLeft", "marginRight", "marginTop", "marginBottom".
 * @param {object} dims The plot dimensions and margins.
 * @returns {{x: number, y: number, width: number, height: number}} The region rectangle.
 */
export function getBrushRegionRect(brushRegion, dims) {
  const { width, height, marginTop, marginRight, marginBottom, marginLeft } = dims;
  const layerWidth = Math.max(0, width - marginLeft - marginRight);
  const layerHeight = Math.max(0, height - marginTop - marginBottom);
  switch (brushRegion) {
    case "full":
      return { x: 0, y: 0, width: Math.max(0, width), height: Math.max(0, height) };
    case "marginLeft":
      return { x: 0, y: marginTop, width: Math.max(0, marginLeft), height: layerHeight };
    case "marginRight":
      return { x: width - marginRight, y: marginTop, width: Math.max(0, marginRight), height: layerHeight };
    case "marginTop":
      return { x: marginLeft, y: 0, width: layerWidth, height: Math.max(0, marginTop) };
    case "marginBottom":
      return { x: marginLeft, y: height - marginBottom, width: layerWidth, height: Math.max(0, marginBottom) };
    case "layer":
    default:
      return { x: marginLeft, y: marginTop, width: layerWidth, height: layerHeight };
  }
}

function isPointInRect(point, rect) {
  return (
    point.x >= rect.x && point.x <= rect.x + rect.width
    && point.y >= rect.y && point.y <= rect.y + rect.height
  );
}

function clampPointToRect(point, rect) {
  return {
    x: Math.min(Math.max(point.x, rect.x), rect.x + rect.width),
    y: Math.min(Math.max(point.y, rect.y), rect.y + rect.height),
  };
}

function distance(a, b) {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

/**
 * Compute the four corners of a rectangular brush.
 * In "x" mode the rectangle always spans the full height of the brush region,
 * and in "y" mode it always spans the full width.
 * @param {string} brushMode One of "xy", "x", "y".
 * @param {{x: number, y: number}} anchor The point at which the brush started.
 * @param {{x: number, y: number}} cursor The current cursor position.
 * @param {{x: number, y: number, width: number, height: number}} region The brush region rectangle.
 * @returns {Array<{x: number, y: number}>} The corners, clockwise from the top-left.
 */
function getRectBrushVertices(brushMode, anchor, cursor, region) {
  const spansX = brushMode === "y";
  const spansY = brushMode === "x";
  const x0 = spansX ? region.x : Math.min(anchor.x, cursor.x);
  const x1 = spansX ? region.x + region.width : Math.max(anchor.x, cursor.x);
  const y0 = spansY ? region.y : Math.min(anchor.y, cursor.y);
  const y1 = spansY ? region.y + region.height : Math.max(anchor.y, cursor.y);
  return [
    { x: x0, y: y0 },
    { x: x1, y: y0 },
    { x: x1, y: y1 },
    { x: x0, y: y1 },
  ];
}

/**
 * Build the object passed to the onBrush/onBrushEnd callbacks.
 * All coordinates are in pixels relative to the top-left of the full plot
 * (i.e., they are not relative to the brush region).
 */
function getBrushPayload(interaction, brushMode, brushRegion, region) {
  const isLasso = brushMode === "lasso";
  const vertices = isLasso
    ? interaction.vertices
    : getRectBrushVertices(brushMode, interaction.anchor, interaction.cursor, region);
  const [topLeft, , bottomRight] = vertices;
  return {
    mode: brushMode,
    region: brushRegion,
    vertices,
    // Convenience for the rectangular brush modes; null for the lasso.
    rect: isLasso ? null : {
      x: topLeft.x,
      y: topLeft.y,
      width: bottomRight.x - topLeft.x,
      height: bottomRight.y - topLeft.y,
    },
  };
}

/**
 * Build an SVG path for a wedge (slice of pie) that begins at 12 o'clock and
 * sweeps clockwise, used for the "maybe brushing" long-press indicator.
 */
function getWedgePath(cx, cy, r, progress) {
  const theta = 2 * Math.PI * Math.min(Math.max(progress, 0), 1);
  if (theta <= 0) {
    return "";
  }
  if (theta >= 2 * Math.PI) {
    // A single arc cannot represent a full circle, so use two half-circle arcs.
    return `M ${cx} ${cy - r} A ${r} ${r} 0 1 1 ${cx} ${cy + r} A ${r} ${r} 0 1 1 ${cx} ${cy - r} Z`;
  }
  const endX = cx + r * Math.sin(theta);
  const endY = cy - r * Math.cos(theta);
  const largeArc = theta > Math.PI ? 1 : 0;
  return `M ${cx} ${cy} L ${cx} ${cy - r} A ${r} ${r} 0 ${largeArc} 1 ${endX} ${endY} Z`;
}

/**
 * Hook that implements rect- and lasso-based brushing interactions.
 *
 * The in-progress brush is tracked in a ref (`interactionRef`) rather than in
 * state, so that the window-level mousemove/mouseup handlers always observe the
 * latest values; the ref contents are then mirrored into state (`brushView`)
 * for rendering purposes only.
 *
 * @returns {object} An object containing:
 *   - blocksEvent: whether the camera/click/hover handlers should ignore a mouse event.
 *   - consumeBrushedClick: whether the click that is about to be handled was the end of a brush.
 *   - overlayProps: props to pass to the BrushOverlay component.
 */
export function useBrush({
  containerRef,
  width,
  height,
  marginTop,
  marginRight,
  marginBottom,
  marginLeft,
  isBrushing,
  brushDelay,
  maybeBrushDelay,
  brushMode,
  brushRegion,
  onBrush: onBrushProp,
  onBrushEnd: onBrushEndProp,
}) {
  // Mirror of interactionRef.current, used only for rendering the overlay.
  const [brushView, setBrushView] = useState(null);

  // null when there is no interaction in progress, otherwise:
  // {
  //   phase: "maybe" | "active",
  //   anchor: {x, y},        // where the interaction started (full-plot pixels)
  //   cursor: {x, y},        // the latest cursor position (full-plot pixels)
  //   vertices: [{x, y}],    // the committed lasso vertices
  //   startTime: number,     // the mousedown timestamp, for the long-press animation
  //   progress: number,      // 0 to 1 during the "maybe" phase, for the wedge animation
  // }
  const interactionRef = useRef(null);
  const rafRef = useRef(null);
  // Set when a brush ends, so that the click event that the browser dispatches
  // after the final mouseup does not also trigger click-picking.
  const brushedClickRef = useRef(false);

  const region = useMemo(
    () => getBrushRegionRect(brushRegion, { width, height, marginTop, marginRight, marginBottom, marginLeft }),
    [brushRegion, width, height, marginTop, marginRight, marginBottom, marginLeft],
  );

  // Convert a mouse event to pixel coordinates relative to the top-left of the plot.
  const getPoint = useEffectEvent((event) => {
    const containerEl = containerRef.current;
    if (!containerEl) {
      return null;
    }
    const containerRect = containerEl.getBoundingClientRect();
    return { x: event.clientX - containerRect.left, y: event.clientY - containerRect.top };
  });

  const syncView = useEffectEvent(() => {
    // Shallow-copy so that React sees a new object and re-renders the overlay.
    setBrushView(interactionRef.current === null ? null : { ...interactionRef.current });
  });

  const emitBrush = useEffectEvent((isEnd) => {
    const interaction = interactionRef.current;
    if (interaction === null || interaction.phase !== "active") {
      return;
    }
    const callback = isEnd ? onBrushEndProp : onBrushProp;
    if (typeof callback === "function") {
      callback(getBrushPayload(interaction, brushMode, brushRegion, region));
    }
  });

  // Append the current cursor position to the lasso polygon.
  // Throttled below, so that a slow drag does not produce too many vertices.
  const appendLassoVertex = useEffectEvent(() => {
    const interaction = interactionRef.current;
    if (interaction === null || interaction.phase !== "active") {
      return;
    }
    const lastVertex = interaction.vertices.at(-1);
    if (lastVertex && distance(lastVertex, interaction.cursor) < LASSO_MIN_VERTEX_DISTANCE_PX) {
      return;
    }
    interaction.vertices = [...interaction.vertices, interaction.cursor];
    syncView();
    emitBrush(false);
  });

  const throttledAppendLassoVertex = useMemo(
    () => throttle(
      () => appendLassoVertex(),
      LASSO_THROTTLE_MS,
      { leading: true, trailing: true },
    ), []);

  const cancelInteraction = useEffectEvent(() => {
    if (rafRef.current !== null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
    throttledAppendLassoVertex.cancel();
    interactionRef.current = null;
    setBrushView(null);
  });

  // Transition from the "maybe" phase (or directly from mousedown, when
  // isBrushing is true) to an in-progress brush.
  const beginBrush = useEffectEvent((point) => {
    if (rafRef.current !== null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
    interactionRef.current = {
      phase: "active",
      anchor: point,
      cursor: point,
      vertices: brushMode === "lasso" ? [point] : [],
      startTime: 0,
      progress: 0,
    };
    syncView();
    emitBrush(false);
  });

  // Animation loop for the long-press ("maybe brushing") indicator.
  const tickMaybeBrush = useEffectEvent(() => {
    rafRef.current = null;
    const interaction = interactionRef.current;
    if (interaction === null || interaction.phase !== "maybe") {
      return;
    }
    const elapsed = performance.now() - interaction.startTime;
    if (elapsed >= brushDelay) {
      // The long press completed, so the brushing interaction begins.
      beginBrush(interaction.anchor);
      return;
    }
    if (elapsed >= maybeBrushDelay) {
      // Fill the wedge from empty (at maybeBrushDelay) to full (at brushDelay).
      const fillDuration = Math.max(brushDelay - maybeBrushDelay, 1);
      interaction.progress = Math.min(1, (elapsed - maybeBrushDelay) / fillDuration);
      syncView();
    }
    rafRef.current = requestAnimationFrame(() => tickMaybeBrush());
  });

  const endBrush = useEffectEvent(() => {
    const interaction = interactionRef.current;
    if (interaction === null || interaction.phase !== "active") {
      cancelInteraction();
      return;
    }
    throttledAppendLassoVertex.cancel();
    if (brushMode === "lasso") {
      // Ensure the final cursor position is part of the polygon.
      const lastVertex = interaction.vertices.at(-1);
      if (!lastVertex || distance(lastVertex, interaction.cursor) > 0) {
        interaction.vertices = [...interaction.vertices, interaction.cursor];
      }
    }
    emitBrush(true);
    // Ignore the click event that follows this mouseup.
    brushedClickRef.current = true;
    // The consumer now owns the final vertices (e.g. to render a selection),
    // so the in-progress overlay is cleared.
    cancelInteraction();
  });

  const handleWindowMouseMove = useEffectEvent((event) => {
    const interaction = interactionRef.current;
    if (interaction === null) {
      return;
    }
    const point = getPoint(event);
    if (point === null) {
      return;
    }
    if (interaction.phase === "maybe") {
      // Moving the mouse before the long press completes means that the user
      // intended to drag (e.g. to pan the camera) rather than to brush.
      if (distance(point, interaction.anchor) > LONG_PRESS_THRESHOLD_PX) {
        cancelInteraction();
      }
      return;
    }
    // The mouse button was released outside of the window, so end the brush.
    if ((event.buttons & 1) === 0) {
      endBrush();
      return;
    }
    // Prevent the brush from extending outside of the brush region.
    interaction.cursor = clampPointToRect(point, region);
    syncView();
    if (brushMode === "lasso") {
      throttledAppendLassoVertex();
    } else {
      emitBrush(false);
    }
  });

  const handleWindowMouseUp = useEffectEvent(() => {
    const interaction = interactionRef.current;
    if (interaction === null) {
      return;
    }
    if (interaction.phase === "maybe") {
      // The mouse was released before the long press completed: a plain click.
      cancelInteraction();
      return;
    }
    endBrush();
  });

  const handleWindowKeyDown = useEffectEvent((event) => {
    if (event.key === "Escape" && interactionRef.current !== null) {
      // Abort without calling onBrushEnd.
      cancelInteraction();
    }
  });

  const handleMouseDown = useEffectEvent((event) => {
    // Any new mousedown supersedes the click suppression from a previous brush.
    brushedClickRef.current = false;
    if (isBrushing === false || event.button !== 0) {
      return;
    }
    const point = getPoint(event);
    if (point === null || !isPointInRect(point, region)) {
      return;
    }
    if (isBrushing === true) {
      // Controlled: brushing begins immediately upon dragging (no long press).
      // Prevent the default to avoid text selection while dragging.
      event.preventDefault();
      beginBrush(point);
    } else {
      // Uncontrolled: begin a long press, which becomes a brushing interaction
      // if the mouse is held (without moving) for brushDelay milliseconds.
      interactionRef.current = {
        phase: "maybe",
        anchor: point,
        cursor: point,
        vertices: [],
        startTime: performance.now(),
        progress: 0,
      };
      rafRef.current = requestAnimationFrame(() => tickMaybeBrush());
    }
  });

  useEffect(() => {
    const containerEl = containerRef.current;
    if (!containerEl || isBrushing === false) {
      return () => { };
    }
    // The mousedown listener is attached to the container (rather than to the
    // overlay) so that the long press can be detected without the overlay
    // needing to swallow events while brushing is merely possible.
    containerEl.addEventListener("mousedown", handleMouseDown);
    window.addEventListener("mousemove", handleWindowMouseMove);
    window.addEventListener("mouseup", handleWindowMouseUp);
    window.addEventListener("keydown", handleWindowKeyDown);
    return () => {
      containerEl.removeEventListener("mousedown", handleMouseDown);
      window.removeEventListener("mousemove", handleWindowMouseMove);
      window.removeEventListener("mouseup", handleWindowMouseUp);
      window.removeEventListener("keydown", handleWindowKeyDown);
    };
  }, [containerRef, isBrushing]);

  // Abort any in-progress interaction when the brush configuration changes
  // (or when the component unmounts), since its result would be ambiguous.
  useEffect(() => {
    return () => cancelInteraction();
  }, [isBrushing, brushMode, brushRegion]);

  useEffect(() => {
    return () => throttledAppendLassoVertex.cancel();
  }, [throttledAppendLassoVertex]);

  // Whether the camera (zoom/pan), click, and hover/tooltip handlers should
  // ignore a mouse event because it belongs to (or is disabled by) brushing.
  const blocksEvent = useEffectEvent((event) => {
    if (interactionRef.current?.phase === "active") {
      // While brushing, all camera/click/hover interactions are disabled,
      // including those outside of the brush region.
      return true;
    }
    if (isBrushing !== true) {
      return false;
    }
    const point = getPoint(event);
    return point !== null && isPointInRect(point, region);
  });

  // Whether the click that is currently being handled ended a brush,
  // in which case it should not trigger click-picking.
  const consumeBrushedClick = useEffectEvent(() => {
    const wasBrushedClick = brushedClickRef.current;
    brushedClickRef.current = false;
    return wasBrushedClick;
  });

  return {
    blocksEvent,
    consumeBrushedClick,
    overlayProps: {
      region,
      brushMode,
      brushView,
      // When brushing is enabled (or in progress), the overlay swallows mouse
      // events within the brush region, which is what disables the camera,
      // click, and hover/tooltip interactions there.
      isInteractive: isBrushing === true || brushView?.phase === "active",
    },
  };
}

/**
 * SVG overlay that renders the in-progress brush (and the long-press
 * indicator) within the brush region.
 * Coordinates are converted from full-plot pixels to region-relative pixels.
 */
export function BrushOverlay(props) {
  const {
    region,
    brushMode,
    brushView,
    isInteractive,
  } = props;

  const toLocal = (point) => ({ x: point.x - region.x, y: point.y - region.y });

  let contents = null;
  if (brushView?.phase === "maybe" && brushView.progress > 0) {
    // The long-press indicator: a circle that fills up as the wedge angle grows.
    const { x: cx, y: cy } = toLocal(brushView.anchor);
    contents = (
      <g>
        <circle
          cx={cx}
          cy={cy}
          r={MAYBE_BRUSH_RADIUS_PX}
          fill="none"
          stroke={BRUSH_STROKE_COLOR}
          strokeWidth={BRUSH_STROKE_WIDTH_PX}
        />
        <path
          d={getWedgePath(cx, cy, MAYBE_BRUSH_RADIUS_PX, brushView.progress)}
          fill={BRUSH_FILL_COLOR}
        />
      </g>
    );
  } else if (brushView?.phase === "active" && brushMode === "lasso") {
    const vertices = brushView.vertices.map(toLocal);
    const cursor = toLocal(brushView.cursor);
    const firstVertex = vertices.at(0);
    contents = (
      <g>
        <polyline
          points={[...vertices, cursor].map(({ x, y }) => `${x},${y}`).join(" ")}
          fill={BRUSH_FILL_COLOR}
          stroke={BRUSH_STROKE_COLOR}
          strokeWidth={BRUSH_STROKE_WIDTH_PX}
          strokeLinejoin="round"
        />
        {vertices.length > 1 ? (
          // Hint at how the polygon will be closed once the drag ends.
          <line
            x1={cursor.x}
            y1={cursor.y}
            x2={firstVertex.x}
            y2={firstVertex.y}
            stroke={BRUSH_STROKE_COLOR}
            strokeWidth={BRUSH_STROKE_WIDTH_PX}
            strokeDasharray="4 4"
          />
        ) : null}
        {vertices.map(({ x, y }, i) => (
          <circle
            // The vertices are append-only, so the index is a stable key.
            key={i}
            cx={x}
            cy={y}
            r={LASSO_VERTEX_RADIUS_PX}
            fill={BRUSH_STROKE_COLOR}
          />
        ))}
      </g>
    );
  } else if (brushView?.phase === "active") {
    const [topLeft, , bottomRight] = getRectBrushVertices(
      brushMode, brushView.anchor, brushView.cursor, region,
    );
    const localTopLeft = toLocal(topLeft);
    contents = (
      <rect
        x={localTopLeft.x}
        y={localTopLeft.y}
        width={bottomRight.x - topLeft.x}
        height={bottomRight.y - topLeft.y}
        fill={BRUSH_FILL_COLOR}
        stroke={BRUSH_STROKE_COLOR}
        strokeWidth={BRUSH_STROKE_WIDTH_PX}
      />
    );
  }

  return (
    <div
      style={{
        position: "absolute",
        top: region.y,
        left: region.x,
        width: region.width,
        height: region.height,
        overflow: "hidden",
        userSelect: "none",
        pointerEvents: isInteractive ? "auto" : "none",
        ...(isInteractive ? { cursor: "crosshair" } : {}),
      }}
    >
      {contents === null ? null : (
        <svg
          width={region.width}
          height={region.height}
          viewBox={`0 0 ${region.width} ${region.height}`}
          xmlns="http://www.w3.org/2000/svg"
          style={{ display: "block", pointerEvents: "none" }}
        >
          {contents}
        </svg>
      )}
    </div>
  );
}
