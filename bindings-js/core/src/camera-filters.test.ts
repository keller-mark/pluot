import { describe, it, expect } from 'vitest';
import { limitExtent, zoomToSquare, type CameraFilterContext } from './camera-filters.js';
import { getBounds, getCameraMatrixFromBounds, type AspectRatioMode, type Bounds } from './viewport.js';

const IDENTITY = new Float32Array([
  1, 0, 0, 0,
  0, 1, 0, 0,
  0, 0, 1, 0,
  0, 0, 0, 1,
]);

function makeContext(
  width: number,
  height: number,
  aspectRatioMode: AspectRatioMode,
  xExtent: [number, number] | null,
  yExtent: [number, number] | null,
): CameraFilterContext {
  return { width, height, aspectRatioMode, aspectRatioAlignmentMode: 'Center', xExtent, yExtent };
}

function cameraFor(bounds: Required<Bounds>, context: CameraFilterContext): Float32Array {
  return getCameraMatrixFromBounds(bounds, IDENTITY, context);
}

function expectBounds(actual: Required<Bounds>, expected: Required<Bounds>) {
  expect(actual.xMin).toBeCloseTo(expected.xMin, 4);
  expect(actual.xMax).toBeCloseTo(expected.xMax, 4);
  expect(actual.yMin).toBeCloseTo(expected.yMin, 4);
  expect(actual.yMax).toBeCloseTo(expected.yMax, 4);
}

describe('limitExtent', () => {
  it('passes through when no extent is known', () => {
    const context = makeContext(400, 400, 'Ignore', null, null);
    const next = cameraFor({ xMin: -5, xMax: 20, yMin: -5, yMax: 20 }, context);
    expect(limitExtent(next, next, context)).toBe(next);
  });

  it('clamps panning beyond the extent', () => {
    const context = makeContext(400, 400, 'Ignore', [0, 10], [0, 100]);
    const prev = cameraFor({ xMin: 0, xMax: 5, yMin: 0, yMax: 50 }, context);
    const next = cameraFor({ xMin: -2, xMax: 3, yMin: 60, yMax: 110 }, context);
    const result = limitExtent(prev, next, context);
    expectBounds(getBounds(result, context), { xMin: 0, xMax: 5, yMin: 50, yMax: 100 });
  });

  it('clamps zooming out per axis under Ignore', () => {
    const context = makeContext(400, 400, 'Ignore', [0, 10], [0, 100]);
    const prev = cameraFor({ xMin: 0, xMax: 10, yMin: 0, yMax: 50 }, context);
    const next = cameraFor({ xMin: -5, xMax: 15, yMin: -25, yMax: 75 }, context);
    const result = limitExtent(prev, next, context);
    expectBounds(getBounds(result, context), { xMin: 0, xMax: 10, yMin: 0, yMax: 100 });
  });

  it('stops zooming out under Contain once one axis fits, keeping the extent in view', () => {
    const context = makeContext(800, 400, 'Contain', [0, 10], [0, 10]);
    const fitted = cameraFor({ xMin: 0, xMax: 10, yMin: 0, yMax: 10 }, context);
    const zoomedOut = cameraFor({ xMin: -15, xMax: 25, yMin: -5, yMax: 15 }, context);
    const result = limitExtent(fitted, zoomedOut, context);
    const bounds = getBounds(result, context);
    expectBounds(bounds, getBounds(fitted, context));
    expect(result[0]).toBeCloseTo(result[5], 6);
  });
});

describe('zoomToSquare', () => {
  // 11 genes by 700 cells, stretched into a 400x400 layer.
  const context = makeContext(400, 400, 'Ignore', [0, 11], [0, 700]);
  const fitted = cameraFor({ xMin: 0, xMax: 11, yMin: 0, yMax: 700 }, context);

  function zoomAboutCenter(camera: Float32Array, factor: number): Float32Array {
    const bounds = getBounds(camera, context);
    const cx = (bounds.xMin + bounds.xMax) / 2;
    const cy = (bounds.yMin + bounds.yMax) / 2;
    const hx = (bounds.xMax - bounds.xMin) / 2 / factor;
    const hy = (bounds.yMax - bounds.yMin) / 2 / factor;
    return cameraFor({ xMin: cx - hx, xMax: cx + hx, yMin: cy - hy, yMax: cy + hy }, context);
  }

  function spans(camera: Float32Array): [number, number] {
    const bounds = getBounds(camera, context);
    return [bounds.xMax - bounds.xMin, bounds.yMax - bounds.yMin];
  }

  it('zooms only the compressed axis before cells are square', () => {
    const result = zoomToSquare(fitted, zoomAboutCenter(fitted, 2), context);
    const [xSpan, ySpan] = spans(result);
    expect(xSpan).toBeCloseTo(11, 4);
    expect(ySpan).toBeCloseTo(350, 4);
  });

  it('stops at square then zooms both axes', () => {
    const result = zoomToSquare(fitted, zoomAboutCenter(fitted, 100), context);
    const [xSpan, ySpan] = spans(result);
    // Square at a y span of 11, after a 700 / 11 zoom; the remaining factor zooms both axes.
    const remainingFactor = 100 / (700 / 11);
    expect(xSpan).toBeCloseTo(11 / remainingFactor, 4);
    expect(ySpan).toBeCloseTo(11 / remainingFactor, 4);
  });

  it('reverses when zooming out, stopping each axis at its full extent', () => {
    const square = cameraFor({ xMin: 2, xMax: 7, yMin: 300, yMax: 305 }, context);
    const uniform = zoomToSquare(square, zoomAboutCenter(square, 0.5), context);
    expect(spans(uniform)[0]).toBeCloseTo(10, 4);
    expect(spans(uniform)[1]).toBeCloseTo(10, 4);

    const pastXExtent = zoomToSquare(uniform, zoomAboutCenter(uniform, 0.5), context);
    expect(spans(pastXExtent)[0]).toBeCloseTo(11, 4);
    expect(spans(pastXExtent)[1]).toBeCloseTo(20, 4);
  });

  it('passes through pans and non-Ignore modes', () => {
    const panned = cameraFor({ xMin: 1, xMax: 12, yMin: 10, yMax: 710 }, context);
    expect(zoomToSquare(fitted, panned, context)).toBe(panned);

    const containContext = { ...context, aspectRatioMode: 'Contain' as const };
    const next = zoomAboutCenter(fitted, 2);
    expect(zoomToSquare(fitted, next, containContext)).toBe(next);
  });
});
