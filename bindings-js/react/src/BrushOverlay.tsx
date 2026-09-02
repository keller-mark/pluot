import React, { useId, useMemo, type RefObject } from "react";
import {
  describeWedgePath, getClearButtonCenter, getEdgeLine, getEditableEdges, getVerticesBoundingBox,
  type BrushEdge, type BrushGeometry,
} from "./brush.js";
import type { BrushState } from "./types.js";
import { CLEAR_BUTTON_RADIUS_PX, type BrushPressProgress } from "./use-brush.js";

const VERTEX_HANDLE_RADIUS_PX = 4;
const PRESS_INDICATOR_RADIUS_PX = 10;
/** How wide a side's invisible grab target is. Kept generous, since a side is 1.5px of ink. */
const EDGE_HANDLE_WIDTH_PX = 9;

/** Opacity of the brush's fill, relative to `color`; the stroke and handles stay fully opaque. */
const BRUSH_FILL_OPACITY = 0.15;
const HANDLE_FILL = "#ffffff";
const CLEAR_FILL = "#b34040";

// Note: we could alternatively render the brush overlay on the rust side, as a new layer type.
// This would solve the problem of syncing the overlay (on the JS side) with the rendered visualization (on the rust side) upon camera interactions.
// However this syncing problem only arises when the brush units mode is Data _and_ the brush overlay is persisted beyond the brush creation.

export type BrushOverlayProps = {
  width: number;
  height: number;
  /** From `useBrush`, so that presses on the handles below are not read as new brushes. */
  overlayRef: RefObject<SVGSVGElement | null>;
  /** Supplies the brushable region, which everything drawn here is clipped to. */
  geometry: BrushGeometry;
  /** Stroke color of the brush outline/handles; the fill uses the same color at reduced opacity. */
  color: string;
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
    geometry,
    color,
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

  const clearButtonCenter = getClearButtonCenter(vertices, CLEAR_BUTTON_RADIUS_PX, geometry);

  // `useId` emits colons, which are legal in an id but awkward inside `url(#...)`.
  const clipPathId = `pluot-brush-clip-${useId().replace(/:/g, "")}`;

  // While drawing a lasso, the intermediate vertices are too dense to be useful
  // as handles, and they are not editable until the drag completes.
  const shouldShowVertexHandles = isClosed;

  // Sides are draggable only once the shape is settled, and only for the
  // axis-aligned shapes; a lasso has no meaningful sides.
  const editableEdges = enableBrushEdit && isClosed && brushState
    ? getEditableEdges(brushState.shape)
    : [];

  // The side handles are the only thing here that needs the extent, so it is not
  // computed for a lasso or for a brush whose sides are not draggable.
  const edgeBoundingBox = editableEdges.length > 0 ? getVerticesBoundingBox(vertices) : null;

  return (
    <svg
      ref={overlayRef}
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        marginTop: 0,
        marginLeft: 0,
        marginRight: 0,
        marginBottom: 0,
        pointerEvents: "none",
        // Sit above the canvas/SVG plot and the camera element.
        zIndex: 1,
      }}
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <clipPath id={clipPathId}>
          <rect
            x={geometry.brushLeft}
            y={geometry.brushTop}
            width={Math.max(geometry.brushRight - geometry.brushLeft, 0)}
            height={Math.max(geometry.brushBottom - geometry.brushTop, 0)}
          />
        </clipPath>
      </defs>
      {/* Everything is clipped to the brushable region: a brush anchored in data
          units scrolls with the camera, so without this it would spill over the
          axes and the surrounding margins as the user pans or zooms out. Clipping
          also applies to hit-testing, so handles that have scrolled out of the
          region stop responding, matching what the user can see. */}
      <g clipPath={`url(#${clipPathId})`}>
        {pathData ? (
          <path
            d={pathData}
            fill={isClosed ? color : "none"}
            fillOpacity={isClosed ? BRUSH_FILL_OPACITY : undefined}
            stroke={color}
            strokeWidth={1.5}
            strokeDasharray={brushState?.status === "Drawing" ? "4 3" : undefined}
          />
        ) : null}
        {/* Drawn before the corner handles, so a press near a corner grabs the
            corner rather than one of the two sides meeting there. */}
        {edgeBoundingBox === null ? null : editableEdges.map(edge => {
          const [x1, y1, x2, y2] = getEdgeLine(edge, edgeBoundingBox);
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
            stroke={color}
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
              stroke={color}
              strokeWidth={1.5}
            />
            <path
              d={describeWedgePath(
                pressProgress.xPixels,
                pressProgress.yPixels,
                PRESS_INDICATOR_RADIUS_PX,
                pressProgress.fraction,
              )}
              fill={color}
            />
          </g>
        ) : null}
      </g>
    </svg>
  );
}
