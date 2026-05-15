
// TODO: auto-generate these types from the Rust side: https://github.com/keller-mark/pluot/issues/133
export type AspectRatioMode = "Ignore" | "Contain" | "Cover";
export type AspectRatioAlignmentMode = "Center" | "Start" | "End";

export type Margins = {
  marginTop?: number;
  marginRight?: number;
  marginBottom?: number;
  marginLeft?: number;
};

export type ViewportParams = {
  width: number;
  height: number;
  aspectRatioMode: AspectRatioMode;
  aspectRatioAlignmentMode: AspectRatioAlignmentMode;
  margins?: Margins;
};

export type Bounds = {
  // Each value is optional.
  // When an entire dimension is omitted (X or Y),
  // use the current camera settings for that dimension.
  // When a single value is omitted (e.g., xMin), ensure the resulting camera matrix keeps this boundary unchanged.
  xMin?: number;
  xMax?: number;
  yMin?: number;
  yMax?: number;
};

/**
 * Calculate the visible data range based on camera view and viewport parameters.
 *
 * Bounds are expressed in normalized data coordinates where 0.0 is the
 * left/top edge of the data and 1.0 is the right/bottom edge.
 *
 * @param cameraMatrix - The current camera matrix, typically from the renderer's
 *   current state.
 * @param viewportParams - Describes the canvas size, aspect-ratio handling, and
 *   margins.
 * @returns The visible data range as `{ xMin, xMax, yMin, yMax }` in normalized
 *   data coordinates.
 *
 * @example
 * ```ts
 * import { getBounds } from "@pluot/core";
 *
 * const viewport = {
 *   width: 800,
 *   height: 600,
 *   aspectRatioMode: "Contain",
 *   aspectRatioAlignmentMode: "Center",
 *   margins: { marginTop: 10, marginRight: 10, marginBottom: 10, marginLeft: 10 },
 * };
 *
 * // Identity camera matrix (full data range visible).
 * const identity = new Float32Array([
 *   1, 0, 0, 0,
 *   0, 1, 0, 0,
 *   0, 0, 1, 0,
 *   0, 0, 0, 1,
 * ]);
 *
 * const bounds = getBounds(identity, viewport);
 * // bounds.xMin, bounds.xMax, bounds.yMin, bounds.yMax are all in [0, 1]
 * // for the identity matrix with "Contain" mode.
 * ```
 */
export function getBounds(cameraMatrix: Float32Array, viewportParams: ViewportParams): Required<Bounds> {
  const zoomX = cameraMatrix[0];
  const zoomY = cameraMatrix[5];
  const translateX = cameraMatrix[12];
  const translateY = cameraMatrix[13];

  const marginTop = viewportParams.margins?.marginTop ?? 0;
  const marginRight = viewportParams.margins?.marginRight ?? 0;
  const marginBottom = viewportParams.margins?.marginBottom ?? 0;
  const marginLeft = viewportParams.margins?.marginLeft ?? 0;

  const layerW = viewportParams.width - marginLeft - marginRight;
  const layerH = viewportParams.height - marginTop - marginBottom;
  const layerAspectRatio = layerW / layerH;

  let xScale = 1.0;
  let yScale = 1.0;
  if (viewportParams.aspectRatioMode === "Contain") {
    if (layerAspectRatio > 1.0) xScale = layerAspectRatio;
    else if (layerAspectRatio < 1.0) yScale = 1.0 / layerAspectRatio;
  } else if (viewportParams.aspectRatioMode === "Cover") {
    if (layerAspectRatio > 1.0) yScale = 1.0 / layerAspectRatio;
    else if (layerAspectRatio < 1.0) xScale = layerAspectRatio;
  }

  let xAlignTranslation = 0.0;
  let yAlignTranslation = 0.0;
  if (viewportParams.aspectRatioAlignmentMode === "Start") {
    xAlignTranslation = xScale - 1.0;
    yAlignTranslation = yScale - 1.0;
  } else if (viewportParams.aspectRatioAlignmentMode === "End") {
    xAlignTranslation = 1.0 - xScale;
    yAlignTranslation = 1.0 - yScale;
  }

  const xAdj = xScale - 1.0;
  const yAdj = yScale - 1.0;

  const xMin = ((-translateX - 1.0 - xAdj + xAlignTranslation) / zoomX + 1.0) / 2.0;
  const xMax = ((-translateX + 1.0 + xAdj + xAlignTranslation) / zoomX + 1.0) / 2.0;
  const yMin = ((-translateY - 1.0 - yAdj + yAlignTranslation) / zoomY + 1.0) / 2.0;
  const yMax = ((-translateY + 1.0 + yAdj + yAlignTranslation) / zoomY + 1.0) / 2.0;

  return { xMin, xMax, yMin, yMax };
}

/**
 * Given data bounds, compute the corresponding camera matrix.
 * Missing bound values are filled in from `prevCameraMatrix` so partial updates
 * (e.g. panning only the X axis) work without resetting the other axis.
 *
 * Bounds are expressed in normalized data coordinates where 0.0 is the
 * left/top edge of the data and 1.0 is the right/bottom edge.
 *
 * @param bounds - The desired visible data range. Any omitted fields are
 *   preserved from `prevCameraMatrix`.
 * @param prevCameraMatrix - The current camera matrix, used to fill in any
 *   omitted bound values. Typically the matrix returned by a previous call or
 *   the renderer's current state.
 * @param viewportParams - Describes the canvas size, aspect-ratio handling, and
 *   margins so the zoom level can be computed correctly.
 * @returns A column-major 4×4 `Float32Array` suitable for passing directly to
 *   the renderer as the camera matrix.
 *
 * @example
 * ```ts
 * import { getCameraMatrixFromBounds } from "@pluot/core";
 *
 * const viewport = {
 *   width: 800,
 *   height: 600,
 *   aspectRatioMode: "Contain",
 *   aspectRatioAlignmentMode: "Center",
 *   margins: { marginTop: 10, marginRight: 10, marginBottom: 10, marginLeft: 10 },
 * };
 *
 * // Identity camera matrix (full data range visible).
 * const identity = new Float32Array([
 *   1, 0, 0, 0,
 *   0, 1, 0, 0,
 *   0, 0, 1, 0,
 *   0, 0, 0, 1,
 * ]);
 *
 * // Zoom into the top-left quadrant of the data.
 * const cameraMatrix = getCameraMatrixFromBounds(
 *   { xMin: 0.0, xMax: 0.5, yMin: 0.0, yMax: 0.5 },
 *   identity,
 *   viewport,
 * );
 *
 * // Pan to shift only the X axis, keeping Y unchanged.
 * const pannedMatrix = getCameraMatrixFromBounds(
 *   { xMin: 0.1, xMax: 0.6 },
 *   cameraMatrix,
 *   viewport,
 * );
 * ```
 */
export function getCameraMatrixFromBounds(bounds: Bounds, prevCameraMatrix: Float32Array, viewportParams: ViewportParams): Float32Array {
  // Fill in missing bounds from the previous camera matrix.
  const currentBounds = getBounds(prevCameraMatrix, viewportParams);
  const xMin = bounds.xMin ?? currentBounds.xMin;
  const xMax = bounds.xMax ?? currentBounds.xMax;
  const yMin = bounds.yMin ?? currentBounds.yMin;
  const yMax = bounds.yMax ?? currentBounds.yMax;

  const marginTop = viewportParams.margins?.marginTop ?? 0;
  const marginRight = viewportParams.margins?.marginRight ?? 0;
  const marginBottom = viewportParams.margins?.marginBottom ?? 0;
  const marginLeft = viewportParams.margins?.marginLeft ?? 0;

  const layerW = viewportParams.width - marginLeft - marginRight;
  const layerH = viewportParams.height - marginTop - marginBottom;
  const layerAspectRatio = layerW / layerH;

  let xScale = 1.0;
  let yScale = 1.0;
  if (viewportParams.aspectRatioMode === "Contain") {
    if (layerAspectRatio > 1.0) xScale = layerAspectRatio;
    else if (layerAspectRatio < 1.0) yScale = 1.0 / layerAspectRatio;
  } else if (viewportParams.aspectRatioMode === "Cover") {
    if (layerAspectRatio > 1.0) yScale = 1.0 / layerAspectRatio;
    else if (layerAspectRatio < 1.0) xScale = layerAspectRatio;
  }

  let xAlignTranslation = 0.0;
  let yAlignTranslation = 0.0;
  if (viewportParams.aspectRatioAlignmentMode === "Start") {
    xAlignTranslation = xScale - 1.0;
    yAlignTranslation = yScale - 1.0;
  } else if (viewportParams.aspectRatioAlignmentMode === "End") {
    xAlignTranslation = 1.0 - xScale;
    yAlignTranslation = 1.0 - yScale;
  }

  const xAdj = xScale - 1.0;
  const yAdj = yScale - 1.0;

  const xRange = xMax - xMin;
  const yRange = yMax - yMin;

  let zoomX = (1.0 + xAdj) / xRange;
  let zoomY = (1.0 + yAdj) / yRange;

  // When aspect ratio is ignored, zoom each axis independently.
  // Otherwise take the minimum so all requested data fits within the viewport.
  if (viewportParams.aspectRatioMode !== "Ignore") {
    zoomX = zoomY = Math.min(zoomX, zoomY);
  }

  // Invert the getBounds translation equations:
  //   min + max = (-translate + align) / zoom + 1.0
  // So: translate = align - zoom * ((min + max) - 1.0)
  const translateX = xAlignTranslation - zoomX * ((xMin + xMax) - 1.0);
  const translateY = yAlignTranslation - zoomY * ((yMin + yMax) - 1.0);

  return new Float32Array([
    zoomX, 0.0,   0.0, 0.0,
    0.0,   zoomY, 0.0, 0.0,
    0.0,   0.0,   1.0, 0.0,
    translateX, translateY, 0.0, 1.0,
  ]);
}
