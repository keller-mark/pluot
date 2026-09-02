import { getBounds, type AspectRatioMode, type AspectRatioAlignmentMode, type Bounds, type CameraMatrix } from "@pluot/core";
import type { BrushState, BrushUnitsMode, BrushVertex } from "./types.js";

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
  return {
    ...state,
    vertices: state.vertices.map(v => reprojectVertex(v, geom, brushUnitsModeX, brushUnitsModeY)),
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
 */
export function rectVerticesFromCorners(
  x0: number, y0: number,
  x1: number, y1: number,
  geom: BrushGeometry,
): BrushVertex[] {
  const left = Math.min(x0, x1);
  const right = Math.max(x0, x1);
  const top = Math.min(y0, y1);
  const bottom = Math.max(y0, y1);
  return [
    vertexFromPixels(left, top, geom),
    vertexFromPixels(right, top, geom),
    vertexFromPixels(right, bottom, geom),
    vertexFromPixels(left, bottom, geom),
  ];
}

/** The bounding box, in container pixels, of a list of already-reprojected vertices. */
export function getVerticesBoundingBox(vertices: BrushVertex[]): { left: number, top: number, right: number, bottom: number } | null {
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

/**
 * Where the clear button sits: just outside the top-right corner of the brush's
 * bounding box, so that it does not obscure the brushed content.
 */
export function getClearButtonCenter(
  boundingBox: { left: number, top: number, right: number, bottom: number },
  radius: number,
): [number, number] {
  return [boundingBox.right + radius, boundingBox.top - radius];
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
