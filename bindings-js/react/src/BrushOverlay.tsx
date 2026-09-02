import React, { useMemo, type RefObject } from "react";
import {
  describeWedgePath, getClearButtonCenter, getEdgeLine, getEditableEdges, getVerticesBoundingBox,
  type BrushEdge,
} from "./brush.js";
import type { BrushState } from "./types.js";
import { CLEAR_BUTTON_RADIUS_PX, type BrushPressProgress } from "./useBrush.js";

const VERTEX_HANDLE_RADIUS_PX = 4;
const PRESS_INDICATOR_RADIUS_PX = 10;
/** How wide a side's invisible grab target is. Kept generous, since a side is 1.5px of ink. */
const EDGE_HANDLE_WIDTH_PX = 9;

const BRUSH_STROKE = "#3b6ea5";
const BRUSH_FILL = "rgba(59, 110, 165, 0.15)";
const HANDLE_FILL = "#ffffff";
const CLEAR_FILL = "#b34040";

export type BrushOverlayProps = {
  width: number;
  height: number;
  /** From `useBrush`, so that presses on the handles below are not read as new brushes. */
  overlayRef: RefObject<SVGSVGElement | null>;
  brushState: BrushState | undefined;
  pressProgress: BrushPressProgress | null;
  /** Whether to draw the clear button (the pointer is over the brush and `enableBrushClear`). */
  isBrushHovered: boolean;
  enableBrushEdit: boolean;
  onVertexMouseDown: (vertexIndex: number, event: React.MouseEvent) => void;
  onEdgeMouseDown: (edge: BrushEdge, event: React.MouseEvent) => void;
  onClearClick: (event: React.MouseEvent) => void;
};

/** A side is dragged along its perpendicular, so it takes the matching resize cursor. */
function getEdgeCursor(edge: BrushEdge): string {
  return edge === "Left" || edge === "Right" ? "ew-resize" : "ns-resize";
}

/**
 * The cursor for corner `vertexIndex`, which advertises the axes that corner can
 * actually move: a range brush only resizes along the axis it selects, and a rect
 * corner resizes along the diagonal it sits on.
 */
function getVertexCursor(shape: BrushState['shape'] | undefined, vertexIndex: number): string {
  if (shape === "RangeX") {
    return "ew-resize";
  }
  if (shape === "RangeY") {
    return "ns-resize";
  }
  if (shape === "Rect") {
    // Corners are ordered clockwise from the top-left.
    return vertexIndex % 2 === 0 ? "nwse-resize" : "nesw-resize";
  }
  return "grab";
}

/**
 * Draws the brush as an SVG above the plot: a rectangle (or lasso polygon) with
 * a circle at each vertex, plus the long-click progress wedge and the clear button.
 *
 * The SVG root is `pointerEvents: none` so that it never intercepts the camera's
 * pan/zoom; only the vertex handles and the clear button opt back in.
 */
export function BrushOverlay(props: BrushOverlayProps) {
  const {
    width, height,
    overlayRef,
    brushState,
    pressProgress,
    isBrushHovered,
    enableBrushEdit,
    onVertexMouseDown,
    onEdgeMouseDown,
    onClearClick,
  } = props;

  const vertices = brushState?.vertices ?? [];

  // Every shape but the lasso is a closed rectangle throughout the drag; a lasso
  // is left open while the user is still drawing it, and closed once the drag completes.
  const isClosed = (brushState !== undefined && brushState.shape !== "Polygon")
    || brushState?.status === "Complete";

  const pathData = useMemo(() => {
    if (vertices.length === 0) {
      return null;
    }
    const points = vertices.map(v => `${v.x_pixels},${v.y_pixels}`).join(" L ");
    return `M ${points}${isClosed ? " Z" : ""}`;
  }, [vertices, isClosed]);

  const boundingBox = useMemo(() => getVerticesBoundingBox(vertices), [vertices]);

  const clearButtonCenter = boundingBox
    ? getClearButtonCenter(boundingBox, CLEAR_BUTTON_RADIUS_PX)
    : null;

  // While drawing a lasso, the intermediate vertices are too dense to be useful
  // as handles, and they are not editable until the drag completes.
  const shouldShowVertexHandles = isClosed;

  // Sides are draggable only once the shape is settled, and only for the
  // axis-aligned shapes; a lasso has no meaningful sides.
  const editableEdges = enableBrushEdit && isClosed && brushState && boundingBox
    ? getEditableEdges(brushState.shape)
    : [];

  return (
    <svg
      ref={overlayRef}
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        pointerEvents: "none",
        // Sit above the canvas/SVG plot and the camera element.
        zIndex: 1,
      }}
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      xmlns="http://www.w3.org/2000/svg"
    >
      {pathData ? (
        <path
          d={pathData}
          fill={isClosed ? BRUSH_FILL : "none"}
          stroke={BRUSH_STROKE}
          strokeWidth={1.5}
          strokeDasharray={brushState?.status === "Drawing" ? "4 3" : undefined}
        />
      ) : null}
      {/* Drawn before the corner handles, so a press near a corner grabs the
          corner rather than one of the two sides meeting there. */}
      {editableEdges.map(edge => {
        const [x1, y1, x2, y2] = getEdgeLine(edge, boundingBox!);
        return (
          <line
            key={edge}
            x1={x1}
            y1={y1}
            x2={x2}
            y2={y2}
            // Invisible ink, but a wide grab target.
            stroke="transparent"
            strokeWidth={EDGE_HANDLE_WIDTH_PX}
            strokeLinecap="butt"
            style={{ pointerEvents: "stroke", cursor: getEdgeCursor(edge) }}
            onMouseDown={event => onEdgeMouseDown(edge, event)}
          />
        );
      })}
      {shouldShowVertexHandles ? vertices.map((vertex, vertexIndex) => (
        <circle
          // Vertices have no identity beyond their position in the ring, and the
          // list is rebuilt on every update, so the index is the only stable key.
          key={vertexIndex}
          cx={vertex.x_pixels}
          cy={vertex.y_pixels}
          r={VERTEX_HANDLE_RADIUS_PX}
          fill={HANDLE_FILL}
          stroke={BRUSH_STROKE}
          strokeWidth={1.5}
          style={{
            pointerEvents: enableBrushEdit ? "auto" : "none",
            cursor: enableBrushEdit ? getVertexCursor(brushState?.shape, vertexIndex) : "default",
          }}
          onMouseDown={enableBrushEdit ? (event => onVertexMouseDown(vertexIndex, event)) : undefined}
        />
      )) : null}
      {isBrushHovered && clearButtonCenter ? (
        <g
          style={{ pointerEvents: "auto", cursor: "pointer" }}
          onClick={onClearClick}
          role="button"
          aria-label="Clear brush"
        >
          <circle
            cx={clearButtonCenter[0]}
            cy={clearButtonCenter[1]}
            r={CLEAR_BUTTON_RADIUS_PX}
            fill={CLEAR_FILL}
          />
          <path
            d={
              `M ${clearButtonCenter[0] - 4} ${clearButtonCenter[1] - 4} L ${clearButtonCenter[0] + 4} ${clearButtonCenter[1] + 4} `
              + `M ${clearButtonCenter[0] + 4} ${clearButtonCenter[1] - 4} L ${clearButtonCenter[0] - 4} ${clearButtonCenter[1] + 4}`
            }
            stroke="#ffffff"
            strokeWidth={1.5}
            strokeLinecap="round"
          />
        </g>
      ) : null}
      {pressProgress ? (
        <g>
          <circle
            cx={pressProgress.xPixels}
            cy={pressProgress.yPixels}
            r={PRESS_INDICATOR_RADIUS_PX}
            fill="rgba(255, 255, 255, 0.6)"
            stroke={BRUSH_STROKE}
            strokeWidth={1.5}
          />
          <path
            d={describeWedgePath(
              pressProgress.xPixels,
              pressProgress.yPixels,
              PRESS_INDICATOR_RADIUS_PX,
              pressProgress.fraction,
            )}
            fill={BRUSH_STROKE}
          />
        </g>
      ) : null}
    </svg>
  );
}
