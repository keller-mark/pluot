import { useCallback, useEffect, useEffectEvent, useMemo, useRef, useState, type RefObject } from "react";
import { throttle } from "lodash-es";
import type { AspectRatioMode, AspectRatioAlignmentMode, CameraMatrix } from "@pluot/core";
import {
  clampToBrushRegion,
  getBrushGeometry,
  getClearButtonCenter,
  getEdgeDragCorners,
  getVerticesBoundingBox,
  isDegenerateBrush,
  isPointInBrush,
  rectVerticesFromCorners,
  reprojectBrushState,
  vertexFromPixels,
  type BrushEdge,
  type BrushGeometry,
} from "./brush.js";
import { NO_BRUSH } from "./types.js";
import type { BrushState, BrushVertex, PluotProps } from "./types.js";

/** Cursor movement (in pixels) that cancels a pending long-click, since the user is panning instead. */
const LONG_CLICK_CANCEL_PX = 4;

/** Caps how many vertices a lasso drag can produce, per the `brushMode: "Polygon"` contract. */
const POLYGON_VERTEX_THROTTLE_MS = 40;

/** Radius (in pixels) of the hover target around the clear button. */
export const CLEAR_BUTTON_RADIUS_PX = 9;

/** What the overlay needs to draw the long-click progress indicator. */
export type BrushPressProgress = {
  xPixels: number;
  yPixels: number;
  /** 0 to 1, reaching 1 exactly when `brushDelay` elapses. */
  fraction: number;
};

export type UseBrushParams = Pick<PluotProps,
  | "brushUnitsModeX" | "brushUnitsModeY"
  | "brushMarginTop" | "brushMarginRight" | "brushMarginBottom" | "brushMarginLeft"
  | "enableBrushCreate" | "enableBrushEdit" | "enableBrushClear"
  | "brushDelay" | "maybeBrushDelay" | "persistBrush" | "brushMode"
  | "brush" | "onBrush" | "onBrushEnd" | "onBrushClear"
> & {
  containerRef: RefObject<HTMLDivElement | null>;
  width: number;
  height: number;
  marginTop: number;
  marginRight: number;
  marginBottom: number;
  marginLeft: number;
  aspectRatioMode: AspectRatioMode;
  aspectRatioAlignmentMode: AspectRatioAlignmentMode;
  cameraMatrix: CameraMatrix;
};

export type UseBrushResult = {
  /** The brush to draw, already reprojected into the current geometry. */
  brushState: BrushState | undefined;
  geometry: BrushGeometry;
  /** Must be attached to the overlay SVG, so its handles are excluded from brush creation. */
  overlayRef: RefObject<SVGSVGElement | null>;
  pressProgress: BrushPressProgress | null;
  /** Whether the clear button should be shown (hovering the brush, `enableBrushClear`). */
  isBrushHovered: boolean;
  /** True while a create/edit drag is in flight, so the camera can stand down. */
  isBrushingRef: RefObject<boolean>;
  /** Set when a brush drag just ended, so the ensuing `click` does not also pick. */
  shouldSuppressClickRef: RefObject<boolean>;
  onVertexMouseDown: (vertexIndex: number, event: React.MouseEvent) => void;
  onEdgeMouseDown: (edge: BrushEdge, event: React.MouseEvent) => void;
  onClearClick: (event: React.MouseEvent) => void;
};

/** Tracks the in-flight drag. Kept in a ref, since it does not affect rendering on its own. */
type ActiveInteraction =
  | { kind: "Create"; anchorX: number; anchorY: number }
  // For a Rect, `fixedX`/`fixedY` is the diagonally opposite corner, which stays put.
  | { kind: "EditVertex"; vertexIndex: number; fixedX: number; fixedY: number }
  // Dragging a side: the cursor drives only `axis`, so the perpendicular extent
  // is carried over from `movingX`/`movingY` and the opposite side stays put.
  | ({ kind: "EditEdge" } & ReturnType<typeof getEdgeDragCorners>);

/**
 * Implements the brush interactions: long-click to create a rect/lasso, drag a
 * vertex to edit, and click the clear button to cancel.
 *
 * Brushing is controlled when `brush` is a `BrushState` or `undefined`, and
 * uncontrolled when `brush` is `null` (mirroring `cameraMatrix`/`setCameraMatrix`).
 * When controlled, nothing is stored here: updates are emitted via `onBrush`/
 * `onBrushEnd` and the parent is expected to feed them back through `brush`.
 */
export function useBrush(params: UseBrushParams): UseBrushResult {
  const {
    containerRef,
    width, height,
    marginTop, marginRight, marginBottom, marginLeft,
    aspectRatioMode, aspectRatioAlignmentMode, cameraMatrix,
    brushUnitsModeX = "Data",
    brushUnitsModeY = "Data",
    brushMarginTop,
    brushMarginRight,
    brushMarginBottom,
    brushMarginLeft,
    enableBrushCreate = false,
    enableBrushEdit = false,
    enableBrushClear = false,
    brushDelay = 1500,
    maybeBrushDelay = 250,
    persistBrush = false,
    brushMode = "Rect",
    brush: controlledBrush,
    onBrush,
    onBrushEnd,
    onBrushClear,
  } = params;

  // `null` (or an omitted prop) means uncontrolled; a BrushState or `NO_BRUSH`
  // means controlled, with `NO_BRUSH` standing for "controlled, nothing brushed".
  const isControlledBrush = controlledBrush !== null && controlledBrush !== undefined;
  const [uncontrolledBrush, setUncontrolledBrush] = useState<BrushState | undefined>(undefined);
  const rawBrush: BrushState | undefined = isControlledBrush
    ? (controlledBrush === NO_BRUSH ? undefined : controlledBrush)
    : uncontrolledBrush;

  // A parent may switch between controlled and uncontrolled at runtime. Whatever
  // was stored during an earlier uncontrolled phase is not the current selection,
  // so drop it rather than let it resurface if the brush ever goes back.
  useEffect(() => {
    if (isControlledBrush) {
      setUncontrolledBrush(undefined);
    }
  }, [isControlledBrush]);

  const [pressProgress, setPressProgress] = useState<BrushPressProgress | null>(null);
  const [isBrushHovered, setIsBrushHovered] = useState(false);

  const isBrushingRef = useRef(false);
  const shouldSuppressClickRef = useRef(false);
  // The overlay SVG, so that presses on its handles can be told apart from
  // presses on the plot itself.
  const overlayRef = useRef<SVGSVGElement | null>(null);
  const interactionRef = useRef<ActiveInteraction | null>(null);
  // The in-progress brush, so that incremental updates (e.g. appending lasso
  // vertices) do not depend on a controlled parent having fed state back yet.
  const draftRef = useRef<BrushState | null>(null);
  // Bookkeeping for the pending long-click, before any brush exists.
  const pendingPressRef = useRef<{ x: number, y: number, startTime: number, rafId: number } | null>(null);

  const geometry = useMemo(() => getBrushGeometry({
    width, height,
    marginTop, marginRight, marginBottom, marginLeft,
    brushMarginTop, brushMarginRight, brushMarginBottom, brushMarginLeft,
    brushUnitsModeX, brushUnitsModeY,
    aspectRatioMode, aspectRatioAlignmentMode, cameraMatrix,
  }), [
    width, height, marginTop, marginRight, marginBottom, marginLeft,
    brushMarginTop, brushMarginRight, brushMarginBottom, brushMarginLeft,
    brushUnitsModeX, brushUnitsModeY,
    aspectRatioMode, aspectRatioAlignmentMode, cameraMatrix,
  ]);

  // Re-derive the pixel positions under the current geometry, so that a brush
  // with a "Data" units mode follows the camera as the user zooms/pans.
  const brushState = useMemo(
    () => (rawBrush ? reprojectBrushState(rawBrush, geometry, brushUnitsModeX, brushUnitsModeY) : undefined),
    [rawBrush, geometry, brushUnitsModeX, brushUnitsModeY],
  );

  // Convert a mouse event to a position relative to the top-left of the
  // container, which is the coordinate space of the brush overlay SVG.
  // `offsetX`/`offsetY` are unusable here because a drag may travel over
  // several descendants (or leave the container entirely).
  const getContainerCoords = useCallback((event: MouseEvent | React.MouseEvent): [number, number] => {
    const containerEl = containerRef.current;
    if (!containerEl) {
      return [0, 0];
    }
    const rect = containerEl.getBoundingClientRect();
    return [event.clientX - rect.left, event.clientY - rect.top];
  }, [containerRef]);

  // Push a brush update out: internally when uncontrolled, and to the parent in
  // both cases. Until the Rust-side `Brushable` trait lands there is nothing to
  // snap to, so the snapped state is the state and the `BrushResult` is unused.
  const emitBrush = useEffectEvent((nextBrush: BrushState, isEnd: boolean) => {
    // The draft is always advanced, since the rest of the drag builds on it...
    draftRef.current = nextBrush;
    // ...but a brush that spans nothing is not a selection. Committing one would
    // strand a stray dot on screen (four coincident vertex handles) that the user
    // then has to clear, so hold it back until the drag gives it some extent.
    if (isDegenerateBrush(nextBrush)) {
      return;
    }
    if (!isControlledBrush) {
      setUncontrolledBrush(nextBrush);
    }
    if (isEnd) {
      onBrushEnd?.(nextBrush, nextBrush);
      // `persistBrush` only applies when uncontrolled; when controlled, the brush
      // persists for exactly as long as the parent keeps passing it.
      if (!isControlledBrush && !persistBrush) {
        setUncontrolledBrush(undefined);
        draftRef.current = null;
      }
    } else {
      onBrush?.(nextBrush, nextBrush);
    }
  });

  const clearBrush = useEffectEvent(() => {
    const cleared = draftRef.current ?? rawBrush;
    draftRef.current = null;
    if (!isControlledBrush) {
      setUncontrolledBrush(undefined);
    }
    setIsBrushHovered(false);
    if (cleared) {
      onBrushClear?.(cleared);
    }
  });

  // --- Drawing a new brush ---

  const startBrush = useEffectEvent((xPixels: number, yPixels: number) => {
    const [x, y] = clampToBrushRegion(xPixels, yPixels, geometry);
    isBrushingRef.current = true;
    interactionRef.current = { kind: "Create", anchorX: x, anchorY: y };
    const vertices: BrushVertex[] = brushMode === "Polygon"
      ? [vertexFromPixels(x, y, geometry)]
      : rectVerticesFromCorners(x, y, x, y, geometry, brushMode);
    emitBrush({ status: "Drawing", shape: brushMode, vertices }, false);
  });

  const appendPolygonVertex = useEffectEvent((xPixels: number, yPixels: number) => {
    const draft = draftRef.current;
    if (!draft || draft.shape !== "Polygon") {
      return;
    }
    const [x, y] = clampToBrushRegion(xPixels, yPixels, geometry);
    emitBrush({ ...draft, vertices: [...draft.vertices, vertexFromPixels(x, y, geometry)] }, false);
  });

  // The lasso samples the cursor on a timer rather than on every mousemove, to
  // keep the vertex count bounded regardless of how slowly the user drags.
  const throttledAppendPolygonVertex = useMemo(
    () => throttle(appendPolygonVertex, POLYGON_VERTEX_THROTTLE_MS, { leading: true, trailing: true }),
    [],
  );

  const updateRect = useEffectEvent((xPixels: number, yPixels: number) => {
    const interaction = interactionRef.current;
    const draft = draftRef.current;
    if (!interaction || !draft || draft.shape === "Polygon") {
      return;
    }
    const [x, y] = clampToBrushRegion(xPixels, yPixels, geometry);

    // Every rect-like drag reduces to a fixed corner plus a moving one. While
    // creating, the fixed corner is the press point; while dragging a corner, it
    // is the corner diagonally opposite; while dragging a side, it is a corner of
    // the opposite side, and the cursor drives only one axis of the moving corner.
    const fixedX = interaction.kind === "Create" ? interaction.anchorX : interaction.fixedX;
    const fixedY = interaction.kind === "Create" ? interaction.anchorY : interaction.fixedY;
    const movingX = interaction.kind === "EditEdge" && interaction.axis === "Y" ? interaction.movingX : x;
    const movingY = interaction.kind === "EditEdge" && interaction.axis === "X" ? interaction.movingY : y;

    emitBrush({
      ...draft,
      // For RangeX/RangeY this discards the cross-axis drag, so dragging any
      // corner only ever moves the selected edge.
      vertices: rectVerticesFromCorners(fixedX, fixedY, movingX, movingY, geometry, draft.shape),
    }, false);
  });

  const movePolygonVertex = useEffectEvent((vertexIndex: number, xPixels: number, yPixels: number) => {
    const draft = draftRef.current;
    if (!draft) {
      return;
    }
    const [x, y] = clampToBrushRegion(xPixels, yPixels, geometry);
    const vertices = draft.vertices.map(
      (vertex, i) => (i === vertexIndex ? vertexFromPixels(x, y, geometry) : vertex),
    );
    emitBrush({ ...draft, vertices }, false);
  });

  const endBrush = useEffectEvent(() => {
    throttledAppendPolygonVertex.cancel();
    const draft = draftRef.current;
    interactionRef.current = null;
    isBrushingRef.current = false;
    if (!draft) {
      return;
    }
    shouldSuppressClickRef.current = true;
    emitBrush({ ...draft, status: "Complete" }, true);
  });

  // --- Long-click detection ---

  const cancelPendingPress = useCallback(() => {
    if (pendingPressRef.current) {
      cancelAnimationFrame(pendingPressRef.current.rafId);
      pendingPressRef.current = null;
      setPressProgress(null);
    }
  }, []);

  // Runs once per frame while the button is held, showing the filling wedge from
  // `maybeBrushDelay` onwards and handing off to `startBrush` at `brushDelay`.
  const tickPendingPress = useEffectEvent(() => {
    const pending = pendingPressRef.current;
    if (!pending) {
      return;
    }
    const elapsed = performance.now() - pending.startTime;
    if (elapsed >= brushDelay) {
      const { x, y } = pending;
      cancelPendingPress();
      startBrush(x, y);
      return;
    }
    setPressProgress(elapsed >= maybeBrushDelay
      ? { xPixels: pending.x, yPixels: pending.y, fraction: elapsed / brushDelay }
      : null);
    pending.rafId = requestAnimationFrame(tickPendingPress);
  });

  const mouseDownHandler = useEffectEvent((event: MouseEvent) => {
    // Only a primary-button press inside the brushable region can start a brush.
    if (!enableBrushCreate || event.button !== 0 || interactionRef.current) {
      return;
    }
    // Presses on the overlay's own controls (the vertex handles and the clear
    // button) are not attempts to draw a new brush. Their React handlers cannot
    // prevent this: React dispatches from its root, by which point this native
    // listener on an ancestor has already run, so `stopPropagation` is too late.
    if (event.target instanceof Node && overlayRef.current?.contains(event.target)) {
      return;
    }
    const [x, y] = getContainerCoords(event);
    if (x < geometry.brushLeft || x > geometry.brushRight || y < geometry.brushTop || y > geometry.brushBottom) {
      return;
    }
    cancelPendingPress();
    pendingPressRef.current = { x, y, startTime: performance.now(), rafId: requestAnimationFrame(tickPendingPress) };
  });

  const mouseMoveHandler = useEffectEvent((event: MouseEvent) => {
    // This listener is on the window, so bail out before reading the container's
    // layout when there is nothing brush-related to track.
    const isTrackingHover = enableBrushClear && brushState !== undefined;
    if (!pendingPressRef.current && !interactionRef.current && !isTrackingHover) {
      return;
    }
    const [x, y] = getContainerCoords(event);

    // Moving before the long-click completes means the user is panning, not brushing.
    const pending = pendingPressRef.current;
    if (pending && Math.hypot(x - pending.x, y - pending.y) > LONG_CLICK_CANCEL_PX) {
      cancelPendingPress();
    }

    const interaction = interactionRef.current;
    if (interaction) {
      if (interaction.kind === "Create") {
        if (brushMode === "Polygon") {
          throttledAppendPolygonVertex(x, y);
        } else {
          updateRect(x, y);
        }
      } else if (interaction.kind === "EditVertex" && draftRef.current?.shape === "Polygon") {
        movePolygonVertex(interaction.vertexIndex, x, y);
      } else {
        updateRect(x, y);
      }
      return;
    }

    // Not dragging: track whether the pointer is over the brush, to decide
    // whether to reveal the clear button.
    if (isTrackingHover && brushState) {
      const boundingBox = getVerticesBoundingBox(brushState.vertices);
      const clearButtonCenter = boundingBox ? getClearButtonCenter(boundingBox, CLEAR_BUTTON_RADIUS_PX) : null;
      // The button sits outside the brush, so it needs its own hover target;
      // otherwise it would vanish as soon as the pointer moved towards it.
      const isOverClearButton = clearButtonCenter !== null
        && Math.hypot(x - clearButtonCenter[0], y - clearButtonCenter[1]) <= CLEAR_BUTTON_RADIUS_PX * 2;
      setIsBrushHovered(isOverClearButton || isPointInBrush(x, y, brushState.vertices));
    }
  });

  const mouseUpHandler = useEffectEvent(() => {
    cancelPendingPress();
    if (interactionRef.current) {
      endBrush();
    }
  });

  const mouseLeaveHandler = useEffectEvent(() => {
    // Only the hover affordance is reset here; an in-flight drag continues,
    // since its listeners are on the window.
    if (!interactionRef.current) {
      cancelPendingPress();
      setIsBrushHovered(false);
    }
  });

  useEffect(() => {
    const containerEl = containerRef.current;
    if (!containerEl) {
      return () => {};
    }
    containerEl.addEventListener("mousedown", mouseDownHandler);
    containerEl.addEventListener("mouseleave", mouseLeaveHandler);
    // Move/up go on the window so a drag that leaves the container still
    // updates (clamped to the brushable region) and still terminates.
    window.addEventListener("mousemove", mouseMoveHandler);
    window.addEventListener("mouseup", mouseUpHandler);
    return () => {
      containerEl.removeEventListener("mousedown", mouseDownHandler);
      containerEl.removeEventListener("mouseleave", mouseLeaveHandler);
      window.removeEventListener("mousemove", mouseMoveHandler);
      window.removeEventListener("mouseup", mouseUpHandler);
    };
  }, [containerRef]);

  useEffect(() => () => {
    cancelPendingPress();
    throttledAppendPolygonVertex.cancel();
  }, [cancelPendingPress, throttledAppendPolygonVertex]);

  // --- Handlers for the overlay's own SVG elements ---

  const vertexMouseDown = useEffectEvent((vertexIndex: number, event: React.MouseEvent) => {
    if (!enableBrushEdit || !brushState || event.button !== 0) {
      return;
    }
    // `mouseDownHandler` already ignored this press; preventDefault only stops
    // the browser's own text-selection/drag behaviour.
    event.preventDefault();
    cancelPendingPress();
    draftRef.current = brushState;
    isBrushingRef.current = true;
    const oppositeVertex = brushState.vertices[(vertexIndex + 2) % 4];
    interactionRef.current = {
      kind: "EditVertex",
      vertexIndex,
      // Only meaningful for a Rect, where the opposite corner is the pivot.
      fixedX: oppositeVertex?.x_pixels ?? 0,
      fixedY: oppositeVertex?.y_pixels ?? 0,
    };
  });

  const edgeMouseDown = useEffectEvent((edge: BrushEdge, event: React.MouseEvent) => {
    if (!enableBrushEdit || !brushState || event.button !== 0) {
      return;
    }
    const boundingBox = getVerticesBoundingBox(brushState.vertices);
    if (!boundingBox) {
      return;
    }
    // See `vertexMouseDown`: preventDefault only stops text selection here.
    event.preventDefault();
    cancelPendingPress();
    draftRef.current = brushState;
    isBrushingRef.current = true;
    interactionRef.current = { kind: "EditEdge", ...getEdgeDragCorners(edge, boundingBox) };
  });

  const clearClick = useEffectEvent((event: React.MouseEvent) => {
    // The overlay is a sibling of the camera element rather than an ancestor,
    // so this click never reaches the picking handler and needs no suppression.
    event.preventDefault();
    clearBrush();
  });

  // Wrap in plain callbacks, since useEffectEvent functions may not be handed to children.
  const onVertexMouseDown = useCallback(
    (vertexIndex: number, event: React.MouseEvent) => vertexMouseDown(vertexIndex, event),
    [],
  );
  const onEdgeMouseDown = useCallback(
    (edge: BrushEdge, event: React.MouseEvent) => edgeMouseDown(edge, event),
    [],
  );
  const onClearClick = useCallback((event: React.MouseEvent) => clearClick(event), []);

  return {
    brushState,
    geometry,
    overlayRef,
    pressProgress,
    isBrushHovered: isBrushHovered && enableBrushClear,
    isBrushingRef,
    shouldSuppressClickRef,
    onVertexMouseDown,
    onEdgeMouseDown,
    onClearClick,
  };
}
