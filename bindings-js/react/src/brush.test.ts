import { describe, it, expect } from 'vitest';
import {
  clampToBrushRegion,
  describeWedgePath,
  getBrushGeometry,
  getClearButtonCenter,
  getEdgeDragCorners,
  getEdgeLine,
  getEditableEdges,
  getVerticesBoundingBox,
  isDegenerateBrush,
  isPointInBrush,
  pixelsFromVertex,
  rectVerticesFromCorners,
  reprojectBrushState,
  reprojectVertex,
  vertexFromPixels,
  type BrushGeometryParams,
} from './brush.js';
import type { BrushState } from './types.js';

function identityCamera(): Float32Array {
  return new Float32Array([
    1, 0, 0, 0,
    0, 1, 0, 0,
    0, 0, 1, 0,
    0, 0, 0, 1,
  ]);
}

function zoomCamera(zoom: number): Float32Array {
  return new Float32Array([
    zoom, 0,    0, 0,
    0,    zoom, 0, 0,
    0,    0,    1, 0,
    0,    0,    0, 1,
  ]);
}

// A 400x400 container with 50px margins all around, giving a square 300x300
// layer region, so that "Contain" mode introduces no aspect-ratio adjustment.
function baseParams(overrides: Partial<BrushGeometryParams> = {}): BrushGeometryParams {
  return {
    width: 400,
    height: 400,
    marginTop: 50,
    marginRight: 50,
    marginBottom: 50,
    marginLeft: 50,
    brushMarginTop: undefined,
    brushMarginRight: undefined,
    brushMarginBottom: undefined,
    brushMarginLeft: undefined,
    brushUnitsModeX: "Data",
    brushUnitsModeY: "Data",
    aspectRatioMode: "Contain",
    aspectRatioAlignmentMode: "Center",
    cameraMatrix: identityCamera(),
    ...overrides,
  };
}

describe('getBrushGeometry', () => {
  it('defaults the brushable region to the layer region', () => {
    const geom = getBrushGeometry(baseParams({ brushUnitsModeX: "Pixels", brushUnitsModeY: "Pixels" }));
    expect(geom.brushLeft).toBe(50);
    expect(geom.brushTop).toBe(50);
    expect(geom.brushRight).toBe(350);
    expect(geom.brushBottom).toBe(350);
  });

  it('applies the brush margins when the units mode is not Data', () => {
    const geom = getBrushGeometry(baseParams({
      brushUnitsModeX: "Pixels",
      brushUnitsModeY: "Normalized",
      brushMarginLeft: 0,
      brushMarginRight: 100,
      brushMarginTop: 10,
      brushMarginBottom: 20,
    }));
    expect(geom.brushLeft).toBe(0);
    expect(geom.brushRight).toBe(300);
    expect(geom.brushTop).toBe(10);
    expect(geom.brushBottom).toBe(380);
    // The layer region is unaffected by the brush margins.
    expect(geom.layerLeft).toBe(50);
    expect(geom.layerWidth).toBe(300);
  });

  it('ignores the brush margins for an axis in Data units mode', () => {
    const geom = getBrushGeometry(baseParams({
      brushUnitsModeX: "Data",
      brushUnitsModeY: "Pixels",
      brushMarginLeft: 0,
      brushMarginRight: 0,
      brushMarginTop: 0,
      brushMarginBottom: 0,
    }));
    // X falls back to the layer (camera) bounds...
    expect(geom.brushLeft).toBe(50);
    expect(geom.brushRight).toBe(350);
    // ...while Y honours the brush margins.
    expect(geom.brushTop).toBe(0);
    expect(geom.brushBottom).toBe(400);
  });
});

describe('vertexFromPixels', () => {
  it('maps the layer corners onto the data bounds, with Y flipped', () => {
    const geom = getBrushGeometry(baseParams());
    const { xMin, xMax, yMin, yMax } = geom.dataBounds;

    // Top-left in pixels is (xMin, yMax) in data, since data Y increases upwards.
    const topLeft = vertexFromPixels(50, 50, geom);
    expect(topLeft.x_data).toBeCloseTo(xMin, 6);
    expect(topLeft.y_data).toBeCloseTo(yMax, 6);

    const bottomRight = vertexFromPixels(350, 350, geom);
    expect(bottomRight.x_data).toBeCloseTo(xMax, 6);
    expect(bottomRight.y_data).toBeCloseTo(yMin, 6);
  });

  it('maps the brushable corners onto normalized 0-to-1, with Y flipped', () => {
    const geom = getBrushGeometry(baseParams({
      brushUnitsModeX: "Normalized",
      brushUnitsModeY: "Normalized",
      brushMarginLeft: 100,
      brushMarginRight: 100,
      brushMarginTop: 100,
      brushMarginBottom: 100,
    }));
    const topLeft = vertexFromPixels(100, 100, geom);
    expect(topLeft.x_normalized).toBeCloseTo(0, 6);
    expect(topLeft.y_normalized).toBeCloseTo(1, 6);

    const center = vertexFromPixels(200, 200, geom);
    expect(center.x_normalized).toBeCloseTo(0.5, 6);
    expect(center.y_normalized).toBeCloseTo(0.5, 6);
  });

  it('returns 0 rather than NaN for a degenerate region', () => {
    const geom = getBrushGeometry(baseParams({ width: 100, marginLeft: 50, marginRight: 50 }));
    expect(geom.layerWidth).toBe(0);
    expect(vertexFromPixels(50, 200, geom).x_normalized).toBe(0);
    expect(Number.isNaN(vertexFromPixels(50, 200, geom).x_data)).toBe(false);
  });
});

describe('pixelsFromVertex', () => {
  it('round-trips through every units mode', () => {
    const geom = getBrushGeometry(baseParams({
      brushUnitsModeX: "Pixels",
      brushUnitsModeY: "Pixels",
      brushMarginLeft: 20,
      brushMarginRight: 30,
      brushMarginTop: 40,
      brushMarginBottom: 10,
    }));
    const vertex = vertexFromPixels(123, 234, geom);

    for (const modeX of ["Pixels", "Data", "Normalized"] as const) {
      for (const modeY of ["Pixels", "Data", "Normalized"] as const) {
        const [x, y] = pixelsFromVertex(vertex, geom, modeX, modeY);
        expect(x).toBeCloseTo(123, 6);
        expect(y).toBeCloseTo(234, 6);
      }
    }
  });
});

describe('reprojectVertex', () => {
  it('keeps a Data-mode vertex pinned to the data when the camera zooms', () => {
    const before = getBrushGeometry(baseParams());
    const vertex = vertexFromPixels(200, 200, before);

    // Zooming 2x about the origin halves the visible data range.
    const after = getBrushGeometry(baseParams({ cameraMatrix: zoomCamera(2) }));
    const reprojected = reprojectVertex(vertex, after, "Data", "Data");

    expect(reprojected.x_data).toBeCloseTo(vertex.x_data, 5);
    expect(reprojected.y_data).toBeCloseTo(vertex.y_data, 5);
    // The center of the data stays at the center of the layer under this camera,
    // but a point off-center would move; check a corner to prove the pixels track.
    const corner = vertexFromPixels(50, 50, before);
    const reprojectedCorner = reprojectVertex(corner, after, "Data", "Data");
    expect(reprojectedCorner.x_pixels).not.toBeCloseTo(corner.x_pixels, 1);
    expect(reprojectedCorner.x_data).toBeCloseTo(corner.x_data, 5);
  });

  it('keeps a Pixels-mode vertex fixed on screen when the camera zooms', () => {
    const before = getBrushGeometry(baseParams({ brushUnitsModeX: "Pixels", brushUnitsModeY: "Pixels" }));
    const vertex = vertexFromPixels(80, 300, before);

    const after = getBrushGeometry(baseParams({
      brushUnitsModeX: "Pixels",
      brushUnitsModeY: "Pixels",
      cameraMatrix: zoomCamera(2),
    }));
    const reprojected = reprojectVertex(vertex, after, "Pixels", "Pixels");

    expect(reprojected.x_pixels).toBeCloseTo(80, 6);
    expect(reprojected.y_pixels).toBeCloseTo(300, 6);
    // The derived data coordinate does change, since less data is now visible.
    expect(reprojected.x_data).not.toBeCloseTo(vertex.x_data, 3);
  });

  it('can pin one axis to the data and the other to the screen', () => {
    const params = { brushUnitsModeX: "Data", brushUnitsModeY: "Pixels" } as const;
    const before = getBrushGeometry(baseParams(params));
    const vertex = vertexFromPixels(80, 300, before);

    const after = getBrushGeometry(baseParams({ ...params, cameraMatrix: zoomCamera(2) }));
    const reprojected = reprojectVertex(vertex, after, "Data", "Pixels");

    expect(reprojected.y_pixels).toBeCloseTo(300, 6);
    expect(reprojected.x_data).toBeCloseTo(vertex.x_data, 5);
    expect(reprojected.x_pixels).not.toBeCloseTo(vertex.x_pixels, 1);
  });
});

describe('getClearButtonCenter', () => {
  // Brushable region spans 50..350 on both axes.
  const geom = getBrushGeometry(baseParams({ brushUnitsModeX: "Pixels", brushUnitsModeY: "Pixels" }));
  const radius = 9;
  const offset = radius + 3;

  function makeVertices(points: number[][]) {
    return points.map(([x, y]) => vertexFromPixels(x!, y!, geom));
  }

  it('returns null for an empty brush', () => {
    expect(getClearButtonCenter([], radius, geom)).toBeNull();
  });

  it("sits adjacent to a rect's first vertex, outside the rect", () => {
    // rectVerticesFromCorners orders corners clockwise from the top-left, so the
    // first vertex is the top-left one and the button goes up and to the left.
    const vertices = rectVerticesFromCorners(100, 120, 200, 220, geom);
    const center = getClearButtonCenter(vertices, radius, geom)!;
    expect(center[0]).toBeCloseTo(100 - offset * Math.SQRT1_2, 4);
    expect(center[1]).toBeCloseTo(120 - offset * Math.SQRT1_2, 4);
  });

  it("sits adjacent to a lasso's first vertex, where the drag started", () => {
    // Centroid is (200, 200); the first vertex is straight above it.
    const center = getClearButtonCenter(makeVertices([[200, 100], [100, 250], [300, 250]]), radius, geom)!;
    expect(center[0]).toBeCloseTo(200, 4);
    expect(center[1]).toBeCloseTo(100 - offset, 4);
  });

  it('stays pinned to the start as a lasso grows, rather than trailing the cursor', () => {
    // The centroid shifts as vertices are appended, so the outward direction
    // rotates, but the button never leaves the neighbourhood of the first vertex.
    const start: number[][] = [[200, 100], [100, 250]];
    for (const points of [start, [...start, [300, 250]], [...start, [300, 250], [260, 180]]]) {
      const center = getClearButtonCenter(makeVertices(points), radius, geom)!;
      expect(Math.hypot(center[0] - 200, center[1] - 100)).toBeCloseTo(offset, 4);
    }
  });

  it('falls back to a fixed diagonal when there is no interior to move away from', () => {
    const center = getClearButtonCenter(makeVertices([[200, 200], [200, 200]]), radius, geom)!;
    expect(center[0]).toBeCloseTo(200 + offset * Math.SQRT1_2, 4);
    expect(center[1]).toBeCloseTo(200 - offset * Math.SQRT1_2, 4);
  });

  it('stays inside the brushable region when the brush reaches its edges', () => {
    // Unclamped this would land beyond the region, where the clipped overlay
    // would make it both invisible and unclickable.
    const center = getClearButtonCenter(makeVertices([[200, 200], [350, 350]]), radius, geom)!;
    expect(center[0]).toBeLessThanOrEqual(geom.brushRight - radius);
    expect(center[1]).toBeLessThanOrEqual(geom.brushBottom - radius);
  });

  it('clamps a brush that has scrolled out of the region entirely', () => {
    const center = getClearButtonCenter(makeVertices([[-800, -800], [-900, -900]]), radius, geom)!;
    expect(center[0]).toBeGreaterThanOrEqual(geom.brushLeft + radius);
    expect(center[1]).toBeGreaterThanOrEqual(geom.brushTop + radius);
  });
});

describe('getEditableEdges', () => {
  it('offers all four sides of a Rect', () => {
    expect(getEditableEdges('Rect')).toEqual(['Top', 'Right', 'Bottom', 'Left']);
  });

  it('offers only the sides a range brush can actually move', () => {
    expect(getEditableEdges('RangeX')).toEqual(['Left', 'Right']);
    expect(getEditableEdges('RangeY')).toEqual(['Top', 'Bottom']);
  });

  it('offers no sides for a lasso', () => {
    expect(getEditableEdges('Polygon')).toEqual([]);
  });
});

describe('getEdgeLine', () => {
  const boundingBox = { left: 100, top: 120, right: 300, bottom: 340 };

  it('spans each side of the bounding box', () => {
    expect(getEdgeLine('Top', boundingBox)).toEqual([100, 120, 300, 120]);
    expect(getEdgeLine('Bottom', boundingBox)).toEqual([100, 340, 300, 340]);
    expect(getEdgeLine('Left', boundingBox)).toEqual([100, 120, 100, 340]);
    expect(getEdgeLine('Right', boundingBox)).toEqual([300, 120, 300, 340]);
  });
});

describe('getEdgeDragCorners', () => {
  const geom = getBrushGeometry(baseParams({ brushUnitsModeX: "Pixels", brushUnitsModeY: "Pixels" }));
  const boundingBox = { left: 100, top: 120, right: 300, bottom: 340 };

  // Replays what `updateRect` does with the corners this returns.
  function dragEdge(edge: Parameters<typeof getEdgeDragCorners>[0], cursorX: number, cursorY: number) {
    const { axis, fixedX, fixedY, movingX, movingY } = getEdgeDragCorners(edge, boundingBox);
    return getVerticesBoundingBox(rectVerticesFromCorners(
      fixedX, fixedY,
      axis === "Y" ? movingX : cursorX,
      axis === "X" ? movingY : cursorY,
      geom,
    ))!;
  }

  it('moves only the dragged side, ignoring cursor movement along the other axis', () => {
    // The cursor wanders far off in Y, but dragging the left side must not change
    // the top or the bottom.
    expect(dragEdge('Left', 160, 999)).toEqual({ left: 160, top: 120, right: 300, bottom: 340 });
    expect(dragEdge('Right', 260, -999)).toEqual({ left: 100, top: 120, right: 260, bottom: 340 });
    expect(dragEdge('Top', 999, 200)).toEqual({ left: 100, top: 200, right: 300, bottom: 340 });
    expect(dragEdge('Bottom', -999, 300)).toEqual({ left: 100, top: 120, right: 300, bottom: 300 });
  });

  it('flips the brush when a side is dragged past its opposite', () => {
    // Dragging the left side to the right of the right side yields a rect that
    // extends from the old right edge, rather than an inside-out one.
    expect(dragEdge('Left', 400, 0)).toEqual({ left: 300, top: 120, right: 400, bottom: 340 });
    expect(dragEdge('Bottom', 0, 50)).toEqual({ left: 100, top: 50, right: 300, bottom: 120 });
  });

  it('keeps the opposite side fixed', () => {
    for (const [edge, key] of [['Left', 'right'], ['Right', 'left'], ['Top', 'bottom'], ['Bottom', 'top']] as const) {
      const dragged = dragEdge(edge, 175, 175);
      expect(dragged[key]).toBe(boundingBox[key]);
    }
  });
});

describe('isDegenerateBrush', () => {
  const geom = getBrushGeometry(baseParams({ brushUnitsModeX: "Pixels", brushUnitsModeY: "Pixels" }));

  function state(shape: BrushState['shape'], vertices: BrushState['vertices']): BrushState {
    return { status: 'Drawing', shape, vertices };
  }

  it('rejects the zero-area rect that a long-click with no drag produces', () => {
    expect(isDegenerateBrush(state('Rect', rectVerticesFromCorners(200, 200, 200, 200, geom)))).toBe(true);
  });

  it('rejects a rect that collapses along either axis', () => {
    expect(isDegenerateBrush(state('Rect', rectVerticesFromCorners(100, 200, 300, 200, geom)))).toBe(true);
    expect(isDegenerateBrush(state('Rect', rectVerticesFromCorners(200, 100, 200, 300, geom)))).toBe(true);
  });

  it('accepts a rect with extent on both axes', () => {
    expect(isDegenerateBrush(state('Rect', rectVerticesFromCorners(100, 100, 300, 300, geom)))).toBe(false);
  });

  it('judges a range brush only on the axis it selects', () => {
    // Full brush height, but no width: nothing was selected.
    expect(isDegenerateBrush(state('RangeX', rectVerticesFromCorners(200, 0, 200, 0, geom, 'RangeX')))).toBe(true);
    expect(isDegenerateBrush(state('RangeX', rectVerticesFromCorners(100, 0, 300, 0, geom, 'RangeX')))).toBe(false);
    expect(isDegenerateBrush(state('RangeY', rectVerticesFromCorners(0, 200, 0, 200, geom, 'RangeY')))).toBe(true);
    expect(isDegenerateBrush(state('RangeY', rectVerticesFromCorners(0, 100, 0, 300, geom, 'RangeY')))).toBe(false);
  });

  it('requires a lasso to have at least three vertices', () => {
    const points = [[100, 100], [200, 100], [200, 200]].map(([x, y]) => vertexFromPixels(x!, y!, geom));
    expect(isDegenerateBrush(state('Polygon', []))).toBe(true);
    expect(isDegenerateBrush(state('Polygon', points.slice(0, 2)))).toBe(true);
    expect(isDegenerateBrush(state('Polygon', points))).toBe(false);
  });
});

describe('reprojectBrushState', () => {
  function completeState(shape: BrushState['shape'], vertices: BrushState['vertices']): BrushState {
    return { status: 'Complete', shape, vertices };
  }

  // Reprojection round-trips through the Float32Array camera matrix, so the
  // recovered pixel positions carry float32-sized error.
  function expectBoundingBoxCloseTo(
    vertices: BrushState['vertices'],
    expected: { left: number, top: number, right: number, bottom: number },
  ) {
    const actual = getVerticesBoundingBox(vertices)!;
    expect(actual.left).toBeCloseTo(expected.left, 4);
    expect(actual.top).toBeCloseTo(expected.top, 4);
    expect(actual.right).toBeCloseTo(expected.right, 4);
    expect(actual.bottom).toBeCloseTo(expected.bottom, 4);
  }

  it('re-pins the unselected axis of a RangeX brush when the margins change', () => {
    // "Ignore" keeps the data bounds at 0..1 regardless of aspect ratio, so the
    // only thing moving in this test is the brushable extent.
    const params = { aspectRatioMode: "Ignore" } as const;
    const before = getBrushGeometry(baseParams(params));
    const state = completeState('RangeX', rectVerticesFromCorners(100, 0, 200, 0, before, 'RangeX'));
    expect(getVerticesBoundingBox(state.vertices)).toEqual({ left: 100, top: 50, right: 200, bottom: 350 });

    const after = getBrushGeometry(baseParams({ ...params, marginBottom: 100 }));
    const reprojected = reprojectBrushState(state, after, "Data", "Data");

    // The selected X extent is untouched, while Y shrinks to the new full height.
    expectBoundingBoxCloseTo(reprojected.vertices, { left: 100, top: 50, right: 200, bottom: 300 });
  });

  it('lets a Data-mode RangeX brush scroll with the camera while staying full-height', () => {
    const before = getBrushGeometry(baseParams());
    const state = completeState('RangeX', rectVerticesFromCorners(100, 0, 200, 0, before, 'RangeX'));

    const after = getBrushGeometry(baseParams({ cameraMatrix: zoomCamera(2) }));
    const reprojected = reprojectBrushState(state, after, "Data", "Data");
    const boundingBox = getVerticesBoundingBox(reprojected.vertices)!;

    // Zooming in spreads the selected range out across (and past) the viewport...
    expect(boundingBox.left).toBeCloseTo(0, 4);
    expect(boundingBox.right).toBeCloseTo(200, 4);
    // ...but the unselected axis still spans the full brush height.
    expect(boundingBox.top).toBe(50);
    expect(boundingBox.bottom).toBe(350);
  });

  it('re-pins the unselected axis of a RangeY brush when the container is resized', () => {
    // Both axes are screen-anchored, so the resize is the only thing in play.
    const params = {
      brushUnitsModeX: "Pixels", brushUnitsModeY: "Pixels",
      brushMarginLeft: 0, brushMarginRight: 0,
    } as const;
    const before = getBrushGeometry(baseParams(params));
    const state = completeState('RangeY', rectVerticesFromCorners(0, 100, 0, 200, before, 'RangeY'));
    expect(getVerticesBoundingBox(state.vertices)).toEqual({ left: 0, top: 100, right: 400, bottom: 200 });

    const after = getBrushGeometry(baseParams({ ...params, width: 200 }));
    const reprojected = reprojectBrushState(state, after, "Pixels", "Pixels");

    // The selected Y extent is untouched, while X shrinks to the new full width.
    expectBoundingBoxCloseTo(reprojected.vertices, { left: 0, top: 100, right: 200, bottom: 200 });
  });

  it('leaves a Rect unchanged when nothing about the geometry changed', () => {
    const geom = getBrushGeometry(baseParams());
    const state = completeState('Rect', rectVerticesFromCorners(100, 120, 300, 340, geom));
    const reprojected = reprojectBrushState(state, geom, "Data", "Data");
    expectBoundingBoxCloseTo(reprojected.vertices, { left: 100, top: 120, right: 300, bottom: 340 });
  });

  it('does not re-pin a Polygon to its bounding box', () => {
    const geom = getBrushGeometry(baseParams());
    const vertices = [[100, 100], [300, 120], [180, 300]].map(([x, y]) => vertexFromPixels(x!, y!, geom));
    const reprojected = reprojectBrushState(completeState('Polygon', vertices), geom, "Data", "Data");
    expect(reprojected.vertices).toHaveLength(3);
    // A rebuild from the bounding box would have moved every vertex to a corner.
    reprojected.vertices.forEach((vertex, i) => {
      expect(vertex.x_pixels).toBeCloseTo(vertices[i]!.x_pixels, 4);
      expect(vertex.y_pixels).toBeCloseTo(vertices[i]!.y_pixels, 4);
    });
  });
});

describe('clampToBrushRegion', () => {
  it('restricts positions to the brushable region', () => {
    const geom = getBrushGeometry(baseParams({ brushUnitsModeX: "Pixels", brushUnitsModeY: "Pixels" }));
    expect(clampToBrushRegion(-10, 500, geom)).toEqual([50, 350]);
    expect(clampToBrushRegion(200, 200, geom)).toEqual([200, 200]);
  });
});

describe('rectVerticesFromCorners', () => {
  it('orders corners clockwise from the top-left, regardless of drag direction', () => {
    const geom = getBrushGeometry(baseParams());
    // Drag from bottom-right to top-left.
    const vertices = rectVerticesFromCorners(300, 300, 100, 100, geom);
    expect(vertices.map(v => [v.x_pixels, v.y_pixels])).toEqual([
      [100, 100],
      [300, 100],
      [300, 300],
      [100, 300],
    ]);
  });

  it('pins RangeX to the full brush height, keeping the dragged X extent', () => {
    const geom = getBrushGeometry(baseParams({
      brushUnitsModeY: "Pixels",
      brushMarginTop: 10,
      brushMarginBottom: 20,
    }));
    const vertices = rectVerticesFromCorners(120, 200, 260, 210, geom, "RangeX");
    expect(vertices.map(v => [v.x_pixels, v.y_pixels])).toEqual([
      [120, 10],
      [260, 10],
      [260, 380],
      [120, 380],
    ]);
  });

  it('pins RangeY to the full brush width, keeping the dragged Y extent', () => {
    const geom = getBrushGeometry(baseParams({
      brushUnitsModeX: "Pixels",
      brushMarginLeft: 30,
      brushMarginRight: 40,
    }));
    const vertices = rectVerticesFromCorners(200, 260, 210, 120, geom, "RangeY");
    expect(vertices.map(v => [v.x_pixels, v.y_pixels])).toEqual([
      [30, 120],
      [360, 120],
      [360, 260],
      [30, 260],
    ]);
  });

  it('puts each corner diagonally opposite the one two steps away', () => {
    const geom = getBrushGeometry(baseParams());
    const vertices = rectVerticesFromCorners(100, 120, 300, 340, geom);
    for (let i = 0; i < 4; i++) {
      const opposite = vertices[(i + 2) % 4]!;
      expect(opposite.x_pixels).not.toBe(vertices[i]!.x_pixels);
      expect(opposite.y_pixels).not.toBe(vertices[i]!.y_pixels);
    }
  });
});

describe('isPointInBrush', () => {
  const geom = getBrushGeometry(baseParams());
  const square = rectVerticesFromCorners(100, 100, 300, 300, geom);

  it('detects points inside and outside a rect', () => {
    expect(isPointInBrush(200, 200, square)).toBe(true);
    expect(isPointInBrush(99, 200, square)).toBe(false);
    expect(isPointInBrush(200, 301, square)).toBe(false);
  });

  it('detects points inside a concave polygon', () => {
    // An L shape occupying the top-left, bottom-left and bottom-right quadrants.
    const lShape = [
      [100, 100], [200, 100], [200, 200], [300, 200], [300, 300], [100, 300],
    ].map(([x, y]) => vertexFromPixels(x!, y!, geom));
    expect(isPointInBrush(150, 150, lShape)).toBe(true);
    expect(isPointInBrush(150, 250, lShape)).toBe(true);
    expect(isPointInBrush(250, 250, lShape)).toBe(true);
    // Inside the bounding box, but in the notch that the L excludes.
    expect(isPointInBrush(250, 150, lShape)).toBe(false);
  });

  it('rejects degenerate brushes', () => {
    expect(isPointInBrush(200, 200, [])).toBe(false);
    expect(isPointInBrush(200, 200, square.slice(0, 2))).toBe(false);
  });
});

describe('getVerticesBoundingBox', () => {
  it('returns null for an empty brush', () => {
    expect(getVerticesBoundingBox([])).toBeNull();
  });

  it('spans the extreme vertices', () => {
    const geom = getBrushGeometry(baseParams());
    const vertices = [[120, 340], [300, 90], [200, 200]].map(([x, y]) => vertexFromPixels(x!, y!, geom));
    expect(getVerticesBoundingBox(vertices)).toEqual({ left: 120, top: 90, right: 300, bottom: 340 });
  });
});

describe('describeWedgePath', () => {
  it('draws nothing but a degenerate wedge at zero progress', () => {
    expect(describeWedgePath(10, 10, 5, 0)).toContain('M 10 10');
  });

  it('uses the large-arc flag only past the halfway point', () => {
    expect(describeWedgePath(0, 0, 10, 0.25)).toContain('A 10 10 0 0 1');
    expect(describeWedgePath(0, 0, 10, 0.75)).toContain('A 10 10 0 1 1');
  });

  it('closes a full circle with two arcs, since one arc cannot express it', () => {
    const path = describeWedgePath(0, 0, 10, 1);
    expect(path.match(/A /g)).toHaveLength(2);
  });

  it('clamps out-of-range fractions', () => {
    expect(describeWedgePath(0, 0, 10, 2)).toBe(describeWedgePath(0, 0, 10, 1));
    expect(describeWedgePath(0, 0, 10, -1)).toBe(describeWedgePath(0, 0, 10, 0));
  });
});
