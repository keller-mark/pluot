import { getBounds, type AspectRatioMode, type AspectRatioAlignmentMode, type Bounds, type CameraMatrix } from "@pluot/core";
import type { BrushMode, BrushState, BrushUnitsMode, BrushVertex, RectLikeBrushMode } from "./types.js";

/** An axis-aligned brush extent, in container pixels. */
export type BrushBoundingBox = {
  left: number;
  top: number;
  right: number;
  bottom: number;
};

/** One side of an axis-aligned brush, which the user can drag to extend it. */
export type BrushEdge = "Top" | "Right" | "Bottom" | "Left";

/**
 * Everything needed to convert a brush vertex between the three units modes.
 *
 * There are two rectangles involved, both expressed in *container* pixels
 * (relative to the top-left of the outer `width` x `height` element, Y down),
 * which is also the coordinate space of the brush overlay SVG:
 *
 * - The **layer** rect (the camera region, inside `margin*`), which anchors the
 *   `Data` units mode, since that is the region the camera matrix maps onto.
 * - The **brushable** rect (inside `brushMargin*`), which bounds where the user
 *   may draw and which anchors the `Normalized` units mode.
 *
 * Note that `Data` and `Normalized` are Y-up (matching Pluot's data coordinate
 * system, where `getBounds().yMin` is the bottom of the layer), whereas
 * `Pixels` is Y-down (matching the DOM/SVG convention).
 */
export type BrushGeometry = {
  layerLeft: number;
  layerTop: number;
  layerWidth: number;
  layerHeight: number;
  brushLeft: number;
  brushTop: number;
  brushRight: number;
  brushBottom: number;
  /** The visible data range of the layer rect, under the current camera. */
  dataBounds: Required<Bounds>;
};

export type BrushGeometryParams = {
  width: number;
  height: number;
  marginTop: number;
  marginRight: number;
  marginBottom: number;
  marginLeft: number;
  /** Each defaults to the corresponding layer margin when undefined. */
  brushMarginTop: number | undefined;
  brushMarginRight: number | undefined;
  brushMarginBottom: number | undefined;
  brushMarginLeft: number | undefined;
  brushUnitsModeX: BrushUnitsMode;
  brushUnitsModeY: BrushUnitsMode;
  aspectRatioMode: AspectRatioMode;
  aspectRatioAlignmentMode: AspectRatioAlignmentMode;
  cameraMatrix: CameraMatrix;
};

// Avoid dividing by zero for degenerate (zero-width or zero-height) regions.
function safeDivide(numerator: number, denominator: number): number {
  return denominator === 0 ? 0 : numerator / denominator;
}

export function getBrushGeometry(params: BrushGeometryParams): BrushGeometry {
  const {
    width, height,
    marginTop, marginRight, marginBottom, marginLeft,
    brushMarginTop, brushMarginRight, brushMarginBottom, brushMarginLeft,
    brushUnitsModeX, brushUnitsModeY,
    aspectRatioMode, aspectRatioAlignmentMode, cameraMatrix,
  } = params;

  const layerLeft = marginLeft;
  const layerTop = marginTop;
  const layerWidth = width - marginLeft - marginRight;
  const layerHeight = height - marginTop - marginBottom;

  // When an axis is in `Data` units mode, the brush margins for that axis are
  // ignored and the layer (i.e. camera) bounds take precedence, so that the
  // brushable region always coincides with the region the camera maps onto.
  const isDataX = brushUnitsModeX === "Data";
  const isDataY = brushUnitsModeY === "Data";

  const brushLeft = isDataX ? layerLeft : (brushMarginLeft ?? marginLeft);
  const brushRight = width - (isDataX ? marginRight : (brushMarginRight ?? marginRight));
  const brushTop = isDataY ? layerTop : (brushMarginTop ?? marginTop);
  const brushBottom = height - (isDataY ? marginBottom : (brushMarginBottom ?? marginBottom));

  const dataBounds = getBounds(cameraMatrix, {
    width,
    height,
    aspectRatioMode,
    aspectRatioAlignmentMode,
    margins: { marginTop, marginRight, marginBottom, marginLeft },
  });

  return {
    layerLeft, layerTop, layerWidth, layerHeight,
    brushLeft, brushTop, brushRight, brushBottom,
    dataBounds,
  };
}

/**
 * Build a full {@link BrushVertex} (all three units modes) from a position in
 * container pixels.
 */
export function vertexFromPixels(xPixels: number, yPixels: number, geom: BrushGeometry): BrushVertex {
  const { xMin, xMax, yMin, yMax } = geom.dataBounds;
  return {
    x_pixels: xPixels,
    y_pixels: yPixels,
    x_data: xMin + safeDivide(xPixels - geom.layerLeft, geom.layerWidth) * (xMax - xMin),
    // Y is flipped: data Y increases upwards, pixel Y increases downwards.
    y_data: yMin + safeDivide(geom.layerTop + geom.layerHeight - yPixels, geom.layerHeight) * (yMax - yMin),
    x_normalized: safeDivide(xPixels - geom.brushLeft, geom.brushRight - geom.brushLeft),
    y_normalized: safeDivide(geom.brushBottom - yPixels, geom.brushBottom - geom.brushTop),
  };
}

/**
 * Recover the container-pixel position of a vertex from whichever of its
 * representations is authoritative for each axis.
 *
 * Only the representation matching the units mode survives a camera or resize
 * change; the other two are derived, so they must be recomputed rather than
 * read back (see {@link reprojectVertex}).
 */
export function pixelsFromVertex(
  vertex: BrushVertex,
  geom: BrushGeometry,
  brushUnitsModeX: BrushUnitsMode,
  brushUnitsModeY: BrushUnitsMode,
): [number, number] {
  const { xMin, xMax, yMin, yMax } = geom.dataBounds;

  let xPixels: number;
  if (brushUnitsModeX === "Data") {
    xPixels = geom.layerLeft + safeDivide(vertex.x_data - xMin, xMax - xMin) * geom.layerWidth;
  } else if (brushUnitsModeX === "Normalized") {
    xPixels = geom.brushLeft + vertex.x_normalized * (geom.brushRight - geom.brushLeft);
  } else {
    xPixels = vertex.x_pixels;
  }

  let yPixels: number;
  if (brushUnitsModeY === "Data") {
    yPixels = geom.layerTop + geom.layerHeight - safeDivide(vertex.y_data - yMin, yMax - yMin) * geom.layerHeight;
  } else if (brushUnitsModeY === "Normalized") {
    yPixels = geom.brushBottom - vertex.y_normalized * (geom.brushBottom - geom.brushTop);
  } else {
    yPixels = vertex.y_pixels;
  }

  return [xPixels, yPixels];
}

/**
 * Re-derive the non-authoritative representations of a vertex under the current
 * geometry. This is what makes a `Data`-units brush track the camera as the user
 * zooms/pans: `x_data`/`y_data` stay fixed while the pixel positions move.
 */
export function reprojectVertex(
  vertex: BrushVertex,
  geom: BrushGeometry,
  brushUnitsModeX: BrushUnitsMode,
  brushUnitsModeY: BrushUnitsMode,
): BrushVertex {
  const [xPixels, yPixels] = pixelsFromVertex(vertex, geom, brushUnitsModeX, brushUnitsModeY);
  return vertexFromPixels(xPixels, yPixels, geom);
}

export function reprojectBrushState(
  state: BrushState,
  geom: BrushGeometry,
  brushUnitsModeX: BrushUnitsMode,
  brushUnitsModeY: BrushUnitsMode,
): BrushState {
  const vertices = state.vertices.map(v => reprojectVertex(v, geom, brushUnitsModeX, brushUnitsModeY));
  const boundingBox = state.shape === "Polygon" ? null : getVerticesBoundingBox(vertices);
  if (state.shape === "Polygon" || boundingBox === null) {
    return { ...state, vertices };
  }
  // Rebuild the corners from the reprojected extent, so that the unselected axis
  // of a RangeX/RangeY brush keeps spanning the whole brushable region even as
  // the camera, the container size, or the margins change. For a plain Rect this
  // is a no-op, since reprojection is axis-aligned and monotonic.
  return {
    ...state,
    vertices: rectVerticesFromCorners(
      boundingBox.left, boundingBox.top,
      boundingBox.right, boundingBox.bottom,
      geom, state.shape,
    ),
  };
}

/** Restrict a container-pixel position to the brushable region. */
export function clampToBrushRegion(xPixels: number, yPixels: number, geom: BrushGeometry): [number, number] {
  return [
    Math.min(Math.max(xPixels, geom.brushLeft), geom.brushRight),
    Math.min(Math.max(yPixels, geom.brushTop), geom.brushBottom),
  ];
}

/**
 * The four corners of the rect spanned by two opposite corners, ordered
 * clockwise in pixel space starting from the top-left, so that corner `i` is
 * always diagonally opposite corner `(i + 2) % 4`.
 *
 * `RangeX` and `RangeY` select along a single axis, so the other axis is
 * discarded and pinned to the full extent of the brushable region.
 */
export function rectVerticesFromCorners(
  x0: number, y0: number,
  x1: number, y1: number,
  geom: BrushGeometry,
  shape: RectLikeBrushMode = "Rect",
): BrushVertex[] {
  const left = shape === "RangeY" ? geom.brushLeft : Math.min(x0, x1);
  const right = shape === "RangeY" ? geom.brushRight : Math.max(x0, x1);
  const top = shape === "RangeX" ? geom.brushTop : Math.min(y0, y1);
  const bottom = shape === "RangeX" ? geom.brushBottom : Math.max(y0, y1);
  return [
    vertexFromPixels(left, top, geom),
    vertexFromPixels(right, top, geom),
    vertexFromPixels(right, bottom, geom),
    vertexFromPixels(left, bottom, geom),
  ];
}

/** The bounding box, in container pixels, of a list of already-reprojected vertices. */
export function getVerticesBoundingBox(vertices: BrushVertex[]): BrushBoundingBox | null {
  if (vertices.length === 0) {
    return null;
  }
  const xs = vertices.map(v => v.x_pixels);
  const ys = vertices.map(v => v.y_pixels);
  return {
    left: Math.min(...xs),
    top: Math.min(...ys),
    right: Math.max(...xs),
    bottom: Math.max(...ys),
  };
}

/** The smallest extent, in pixels, that a brush must span along a selected axis. */
const MIN_BRUSH_EXTENT_PX = 2;

/**
 * Whether a brush is too small to be a selection.
 *
 * A long-click that never turns into a drag produces a rect whose four corners
 * coincide, which draws as a stray dot rather than as nothing, so these states
 * are held back instead of being committed.
 */
export function isDegenerateBrush(state: BrushState): boolean {
  if (state.shape === "Polygon") {
    return state.vertices.length < 3;
  }
  const boundingBox = getVerticesBoundingBox(state.vertices);
  if (boundingBox === null) {
    return true;
  }
  const brushWidth = boundingBox.right - boundingBox.left;
  const brushHeight = boundingBox.bottom - boundingBox.top;
  // A range brush only selects along one axis; the other always spans the whole
  // brushable region, so it is not evidence that the user drew anything.
  if (state.shape === "RangeX") {
    return brushWidth < MIN_BRUSH_EXTENT_PX;
  }
  if (state.shape === "RangeY") {
    return brushHeight < MIN_BRUSH_EXTENT_PX;
  }
  return brushWidth < MIN_BRUSH_EXTENT_PX || brushHeight < MIN_BRUSH_EXTENT_PX;
}

/** How much clear air to leave between the brush's last vertex and the clear button. */
const CLEAR_BUTTON_GAP_PX = 3;

/**
 * Where the clear button sits: adjacent to the brush's first vertex — the
 * top-left corner of a rect, or the point a lasso was started from.
 *
 * Anchoring to the first vertex keeps the button in one place while a lasso is
 * being drawn, rather than trailing the cursor around the shape. It is pushed
 * outwards along the ray from the centroid through that vertex, so it lands
 * outside the brush and does not obscure the brushed content. Returns `null` for
 * an empty brush.
 *
 * The result is kept within the brushable region, since the overlay is clipped to
 * that region and a button pushed outside it would be invisible and unclickable.
 */
export function getClearButtonCenter(
  vertices: BrushVertex[],
  radius: number,
  geom: BrushGeometry,
): [number, number] | null {
  const firstVertex = vertices[0];
  if (firstVertex === undefined) {
    return null;
  }

  const centroidX = vertices.reduce((sum, v) => sum + v.x_pixels, 0) / vertices.length;
  const centroidY = vertices.reduce((sum, v) => sum + v.y_pixels, 0) / vertices.length;
  let directionX = firstVertex.x_pixels - centroidX;
  let directionY = firstVertex.y_pixels - centroidY;
  const length = Math.hypot(directionX, directionY);
  if (length === 0) {
    // No interior to move away from, so fall back to a fixed up-and-right diagonal.
    directionX = Math.SQRT1_2;
    directionY = -Math.SQRT1_2;
  } else {
    directionX /= length;
    directionY /= length;
  }

  const offset = radius + CLEAR_BUTTON_GAP_PX;
  return [
    Math.min(Math.max(firstVertex.x_pixels + directionX * offset, geom.brushLeft + radius), geom.brushRight - radius),
    Math.min(Math.max(firstVertex.y_pixels + directionY * offset, geom.brushTop + radius), geom.brushBottom - radius),
  ];
}

/**
 * Which sides of a brush the user may drag to extend it.
 *
 * A range brush pins its unselected axis to the whole brushable region, so
 * dragging those two sides could not change anything and they are left out.
 */
export function getEditableEdges(shape: BrushMode): BrushEdge[] {
  switch (shape) {
    case "Rect":
      return ["Top", "Right", "Bottom", "Left"];
    case "RangeX":
      return ["Left", "Right"];
    case "RangeY":
      return ["Top", "Bottom"];
    default:
      return [];
  }
}

/** The endpoints `[x1, y1, x2, y2]` of an edge, in container pixels. */
export function getEdgeLine(edge: BrushEdge, boundingBox: BrushBoundingBox): [number, number, number, number] {
  const { left, top, right, bottom } = boundingBox;
  switch (edge) {
    case "Top":
      return [left, top, right, top];
    case "Bottom":
      return [left, bottom, right, bottom];
    case "Left":
      return [left, top, left, bottom];
    case "Right":
      return [right, top, right, bottom];
  }
}

/**
 * The two opposite corners that dragging `edge` spans: the corner that stays
 * put, and the corner that follows the cursor along `axis` only.
 *
 * Expressing an edge drag as a pair of corners lets it reuse
 * {@link rectVerticesFromCorners}, which also means dragging a side past its
 * opposite side flips the brush rather than inverting it.
 */
export function getEdgeDragCorners(edge: BrushEdge, boundingBox: BrushBoundingBox): {
  axis: "X" | "Y";
  fixedX: number;
  fixedY: number;
  movingX: number;
  movingY: number;
} {
  const { left, top, right, bottom } = boundingBox;
  switch (edge) {
    case "Left":
      return { axis: "X", fixedX: right, fixedY: top, movingX: left, movingY: bottom };
    case "Right":
      return { axis: "X", fixedX: left, fixedY: top, movingX: right, movingY: bottom };
    case "Top":
      return { axis: "Y", fixedX: left, fixedY: bottom, movingX: right, movingY: top };
    case "Bottom":
      return { axis: "Y", fixedX: left, fixedY: top, movingX: right, movingY: bottom };
  }
}

/**
 * Whether a container-pixel position lies inside a brush, used to decide when to
 * reveal the clear button. Hit-testing is done here rather than with SVG pointer
 * events so that the overlay never swallows camera pan/zoom interactions.
 */
export function isPointInBrush(xPixels: number, yPixels: number, vertices: BrushVertex[]): boolean {
  if (vertices.length < 3) {
    return false;
  }
  // Ray casting: count the polygon edges crossed by a ray heading in +X.
  let isInside = false;
  for (let i = 0, j = vertices.length - 1; i < vertices.length; j = i++) {
    const xi = vertices[i]!.x_pixels;
    const yi = vertices[i]!.y_pixels;
    const xj = vertices[j]!.x_pixels;
    const yj = vertices[j]!.y_pixels;
    const doesEdgeStraddleRay = (yi > yPixels) !== (yj > yPixels);
    if (doesEdgeStraddleRay && xPixels < xi + ((yPixels - yi) / (yj - yi)) * (xj - xi)) {
      isInside = !isInside;
    }
  }
  return isInside;
}

/**
 * An SVG path for a pie wedge filled clockwise from 12 o'clock, used to
 * visualize progress towards the long-click that starts a brush.
 */
export function describeWedgePath(cx: number, cy: number, radius: number, fraction: number): string {
  const clamped = Math.min(Math.max(fraction, 0), 1);
  if (clamped >= 1) {
    // A single arc cannot express a full circle, so use two half-circle arcs.
    return `M ${cx} ${cy - radius} A ${radius} ${radius} 0 1 1 ${cx} ${cy + radius} A ${radius} ${radius} 0 1 1 ${cx} ${cy - radius} Z`;
  }
  const angle = clamped * 2 * Math.PI;
  const endX = cx + radius * Math.sin(angle);
  const endY = cy - radius * Math.cos(angle);
  const largeArcFlag = clamped > 0.5 ? 1 : 0;
  return `M ${cx} ${cy} L ${cx} ${cy - radius} A ${radius} ${radius} 0 ${largeArcFlag} 1 ${endX} ${endY} Z`;
}
