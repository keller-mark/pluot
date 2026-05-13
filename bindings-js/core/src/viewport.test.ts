import { describe, it, expect } from 'vitest';
import { getBounds, getCameraMatrixFromBounds } from './viewport.js';
import type { AspectRatioMode, AspectRatioAlignmentMode, ViewportParams } from './viewport.js';

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

function zoomAndTranslateCamera(zoom: number, tx: number, ty: number): Float32Array {
  return new Float32Array([
    zoom, 0,    0, 0,
    0,    zoom, 0, 0,
    0,    0,    1, 0,
    tx,   ty,   0, 1,
  ]);
}

function makeViewport(
  width: number,
  height: number,
  aspectRatioMode: AspectRatioMode,
  aspectRatioAlignmentMode: AspectRatioAlignmentMode = 'Center',
): ViewportParams {
  return { width, height, aspectRatioMode, aspectRatioAlignmentMode };
}

function expectArrayCloseTo(actual: Float32Array, expected: Float32Array, numDigits = 5): void {
  expect(actual.length).toBe(expected.length);
  for (let i = 0; i < actual.length; i++) {
    expect(actual[i]).toBeCloseTo(expected[i], numDigits);
  }
}

// =================== getBounds ===================

describe('getBounds', () => {
  it('identity camera, square, Ignore → full [0, 1] range', () => {
    const b = getBounds(identityCamera(), makeViewport(100, 100, 'Ignore'));
    expect(b.xMin).toBeCloseTo(0);
    expect(b.xMax).toBeCloseTo(1);
    expect(b.yMin).toBeCloseTo(0);
    expect(b.yMax).toBeCloseTo(1);
  });

  it('2x zoom, square, Ignore → [0.25, 0.75]', () => {
    const b = getBounds(zoomCamera(2), makeViewport(100, 100, 'Ignore'));
    expect(b.xMin).toBeCloseTo(0.25);
    expect(b.xMax).toBeCloseTo(0.75);
    expect(b.yMin).toBeCloseTo(0.25);
    expect(b.yMax).toBeCloseTo(0.75);
  });

  it('0.5x zoom (zoomed out 2x), square, Ignore → [-0.5, 1.5]', () => {
    const b = getBounds(zoomCamera(0.5), makeViewport(100, 100, 'Ignore'));
    expect(b.xMin).toBeCloseTo(-0.5);
    expect(b.xMax).toBeCloseTo(1.5);
    expect(b.yMin).toBeCloseTo(-0.5);
    expect(b.yMax).toBeCloseTo(1.5);
  });

  it('wide (200×100), Contain → x extends to [-0.5, 1.5], y stays [0, 1]', () => {
    const b = getBounds(identityCamera(), makeViewport(200, 100, 'Contain'));
    expect(b.xMin).toBeCloseTo(-0.5);
    expect(b.xMax).toBeCloseTo(1.5);
    expect(b.yMin).toBeCloseTo(0);
    expect(b.yMax).toBeCloseTo(1);
  });

  it('tall (100×200), Contain → x stays [0, 1], y extends to [-0.5, 1.5]', () => {
    const b = getBounds(identityCamera(), makeViewport(100, 200, 'Contain'));
    expect(b.xMin).toBeCloseTo(0);
    expect(b.xMax).toBeCloseTo(1);
    expect(b.yMin).toBeCloseTo(-0.5);
    expect(b.yMax).toBeCloseTo(1.5);
  });

  it('wide (200×100), Cover → x stays [0, 1], y shrinks to [0.25, 0.75]', () => {
    const b = getBounds(identityCamera(), makeViewport(200, 100, 'Cover'));
    expect(b.xMin).toBeCloseTo(0);
    expect(b.xMax).toBeCloseTo(1);
    expect(b.yMin).toBeCloseTo(0.25);
    expect(b.yMax).toBeCloseTo(0.75);
  });

  it('with margins, square (100×100), Ignore → [0, 1] × [0, 1]', () => {
    const viewport: ViewportParams = {
      ...makeViewport(100, 100, 'Ignore'),
      margins: { marginLeft: 20, marginBottom: 20 },
    };
    const b = getBounds(identityCamera(), viewport);
    expect(b.xMin).toBeCloseTo(0);
    expect(b.xMax).toBeCloseTo(1);
    expect(b.yMin).toBeCloseTo(0);
    expect(b.yMax).toBeCloseTo(1);
  });
});

// =================== getCameraMatrixFromBounds ===================

describe('getCameraMatrixFromBounds', () => {
  it('full [0, 1] range → identity camera', () => {
    const camera = getCameraMatrixFromBounds(
      { xMin: 0, xMax: 1, yMin: 0, yMax: 1 },
      identityCamera(),
      makeViewport(100, 100, 'Ignore'),
    );
    expectArrayCloseTo(camera, identityCamera());
  });

  it('[0.25, 0.75] × [0.25, 0.75] → 2x zoom camera', () => {
    const camera = getCameraMatrixFromBounds(
      { xMin: 0.25, xMax: 0.75, yMin: 0.25, yMax: 0.75 },
      identityCamera(),
      makeViewport(100, 100, 'Ignore'),
    );
    expectArrayCloseTo(camera, zoomCamera(2));
  });

  it('[-0.5, 1.5] × [-0.5, 1.5] → 0.5x zoom (zoomed out 2x)', () => {
    const camera = getCameraMatrixFromBounds(
      { xMin: -0.5, xMax: 1.5, yMin: -0.5, yMax: 1.5 },
      identityCamera(),
      makeViewport(100, 100, 'Ignore'),
    );
    expectArrayCloseTo(camera, zoomCamera(0.5));
  });

  it('x-offset bounds → zoom=1, translateX=0.5, translateY=0', () => {
    const camera = getCameraMatrixFromBounds(
      { xMin: -0.25, xMax: 0.75, yMin: 0, yMax: 1 },
      identityCamera(),
      makeViewport(100, 100, 'Ignore'),
    );
    expectArrayCloseTo(camera, zoomAndTranslateCamera(1, 0.5, 0));
  });

  it('zoom + translation bounds → zoom=2, translateX=0.5, translateY=0.25', () => {
    const camera = getCameraMatrixFromBounds(
      { xMin: 0.125, xMax: 0.625, yMin: 0.1875, yMax: 0.6875 },
      identityCamera(),
      makeViewport(100, 100, 'Ignore'),
    );
    expectArrayCloseTo(camera, zoomAndTranslateCamera(2, 0.5, 0.25));
  });

  it('wide Contain bounds [-0.5, 1.5] × [0, 1] → identity camera', () => {
    const camera = getCameraMatrixFromBounds(
      { xMin: -0.5, xMax: 1.5, yMin: 0, yMax: 1 },
      identityCamera(),
      makeViewport(200, 100, 'Contain'),
    );
    expectArrayCloseTo(camera, identityCamera());
  });

  it('tall Contain bounds [0, 1] × [-0.5, 1.5] → identity camera', () => {
    const camera = getCameraMatrixFromBounds(
      { xMin: 0, xMax: 1, yMin: -0.5, yMax: 1.5 },
      identityCamera(),
      makeViewport(100, 200, 'Contain'),
    );
    expectArrayCloseTo(camera, identityCamera());
  });

  it('wide Cover bounds [0, 1] × [0.25, 0.75] → identity camera', () => {
    const camera = getCameraMatrixFromBounds(
      { xMin: 0, xMax: 1, yMin: 0.25, yMax: 0.75 },
      identityCamera(),
      makeViewport(200, 100, 'Cover'),
    );
    expectArrayCloseTo(camera, identityCamera());
  });

  it('asymmetric ranges, Ignore → independent x/y zoom', () => {
    const camera = getCameraMatrixFromBounds(
      { xMin: 0, xMax: 0.5, yMin: 0, yMax: 1 },
      identityCamera(),
      makeViewport(100, 100, 'Ignore'),
    );
    expectArrayCloseTo(camera, new Float32Array([
      2, 0, 0, 0,
      0, 1, 0, 0,
      0, 0, 1, 0,
      1, 0, 0, 1,
    ]));
  });

  it('partial bounds (x-axis only) preserves y from prevCameraMatrix', () => {
    const prevCamera = zoomAndTranslateCamera(2, 0.5, 0.25);
    const viewport = makeViewport(100, 100, 'Ignore');
    const prevBounds = getBounds(prevCamera, viewport);

    const camera = getCameraMatrixFromBounds(
      { xMin: 0, xMax: 1 },
      prevCamera,
      viewport,
    );
    const newBounds = getBounds(camera, viewport);

    expect(newBounds.xMin).toBeCloseTo(0);
    expect(newBounds.xMax).toBeCloseTo(1);
    expect(newBounds.yMin).toBeCloseTo(prevBounds.yMin);
    expect(newBounds.yMax).toBeCloseTo(prevBounds.yMax);
  });
});

// =================== getBounds / getCameraMatrixFromBounds roundtrip ===================

describe('getBounds / getCameraMatrixFromBounds roundtrip', () => {
  it('identity camera → bounds → identity camera', () => {
    const viewport = makeViewport(100, 100, 'Ignore');
    const bounds = getBounds(identityCamera(), viewport);
    const camera = getCameraMatrixFromBounds(bounds, identityCamera(), viewport);
    expectArrayCloseTo(camera, identityCamera());
  });

  it('2x zoom camera → bounds → 2x zoom camera', () => {
    const viewport = makeViewport(100, 100, 'Ignore');
    const camera0 = zoomCamera(2);
    const bounds = getBounds(camera0, viewport);
    const camera1 = getCameraMatrixFromBounds(bounds, identityCamera(), viewport);
    expectArrayCloseTo(camera1, camera0);
  });

  it('wide Contain viewport, identity camera → bounds → identity camera', () => {
    const viewport = makeViewport(200, 100, 'Contain');
    const bounds = getBounds(identityCamera(), viewport);
    const camera = getCameraMatrixFromBounds(bounds, identityCamera(), viewport);
    expectArrayCloseTo(camera, identityCamera());
  });

  it('zoom + translation → bounds → same camera', () => {
    const viewport = makeViewport(100, 100, 'Ignore');
    const camera0 = zoomAndTranslateCamera(2, 0.5, 0.25);
    const bounds = getBounds(camera0, viewport);
    const camera1 = getCameraMatrixFromBounds(bounds, identityCamera(), viewport);
    expectArrayCloseTo(camera1, camera0);
  });

  it('with margins, identity camera → bounds → identity camera', () => {
    const viewport: ViewportParams = {
      ...makeViewport(100, 100, 'Ignore'),
      margins: { marginLeft: 20, marginBottom: 20 },
    };
    const bounds = getBounds(identityCamera(), viewport);
    const camera = getCameraMatrixFromBounds(bounds, identityCamera(), viewport);
    expectArrayCloseTo(camera, identityCamera());
  });
});
