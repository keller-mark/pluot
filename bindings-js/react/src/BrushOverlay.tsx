import React, { useMemo } from "react";
import { describeWedgePath, getClearButtonCenter, getVerticesBoundingBox } from "./brush.js";
import type { BrushState } from "./types.js";
import { CLEAR_BUTTON_RADIUS_PX, type BrushPressProgress } from "./useBrush.js";

const VERTEX_HANDLE_RADIUS_PX = 4;
const PRESS_INDICATOR_RADIUS_PX = 10;

const BRUSH_STROKE = "#3b6ea5";
const BRUSH_FILL = "rgba(59, 110, 165, 0.15)";
const HANDLE_FILL = "#ffffff";
const CLEAR_FILL = "#b34040";

export type BrushOverlayProps = {
  width: number;
  height: number;
  brushState: BrushState | undefined;
  pressProgress: BrushPressProgress | null;
  /** Whether to draw the clear button (the pointer is over the brush and `enableBrushClear`). */
  isBrushHovered: boolean;
  enableBrushEdit: boolean;
  onVertexMouseDown: (vertexIndex: number, event: React.MouseEvent) => void;
  onClearClick: (event: React.MouseEvent) => void;
};

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
    brushState,
    pressProgress,
    isBrushHovered,
    enableBrushEdit,
    onVertexMouseDown,
    onClearClick,
  } = props;

  const vertices = brushState?.vertices ?? [];

  // A rect is always closed; a lasso is left open while the user is still
  // drawing it, and closed once the drag completes.
  const pathData = useMemo(() => {
    if (vertices.length === 0) {
      return null;
    }
    const points = vertices.map(v => `${v.x_pixels},${v.y_pixels}`).join(" L ");
    const shouldClose = brushState?.shape === "Rect" || brushState?.status === "Complete";
    return `M ${points}${shouldClose ? " Z" : ""}`;
  }, [vertices, brushState?.shape, brushState?.status]);

  const clearButtonCenter = useMemo(() => {
    const boundingBox = getVerticesBoundingBox(vertices);
    return boundingBox ? getClearButtonCenter(boundingBox, CLEAR_BUTTON_RADIUS_PX) : null;
  }, [vertices]);

  // While drawing a lasso, the intermediate vertices are too dense to be useful
  // as handles, and they are not editable until the drag completes.
  const shouldShowVertexHandles = brushState?.shape === "Rect" || brushState?.status === "Complete";

  return (
    <svg
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
          fill={brushState?.shape === "Rect" || brushState?.status === "Complete" ? BRUSH_FILL : "none"}
          stroke={BRUSH_STROKE}
          strokeWidth={1.5}
          strokeDasharray={brushState?.status === "Drawing" ? "4 3" : undefined}
        />
      ) : null}
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
            cursor: enableBrushEdit ? "grab" : "default",
          }}
          onMouseDown={enableBrushEdit ? (event => onVertexMouseDown(vertexIndex, event)) : undefined}
        />
      )) : null}
      {isBrushHovered && clearButtonCenter ? (
        <g
          style={{ pointerEvents: "auto", cursor: "pointer" }}
          onMouseDown={event => event.stopPropagation()}
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
