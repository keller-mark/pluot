import { describe, it, expect } from 'vitest';
import {
  clampToBrushRegion,
  describeWedgePath,
  getBrushGeometry,
  getVerticesBoundingBox,
  isPointInBrush,
  pixelsFromVertex,
  rectVerticesFromCorners,
  reprojectVertex,
  vertexFromPixels,
  type BrushGeometryParams,
} from './brush.js';

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
