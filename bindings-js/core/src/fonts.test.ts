import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { FontStore, setFont } from './fonts.js';

const VENDOR_DIR = resolve(import.meta.dirname, '../../../vendor/urw-core35-fonts');

// ---------------------------------------------------------------------------
// PDF Base-14 font names via URW map
// ---------------------------------------------------------------------------

describe('FontStore – PDF Base-14 fonts resolved via URW map', () => {
  let store: FontStore;
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    store = new FontStore();
    // Stub fetch to return a minimal non-empty buffer; the exact bytes don't
    // matter here — we're testing the store routing, not font file correctness.
    fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      arrayBuffer: async () => new Uint8Array([0, 1, 2, 3]).buffer,
    });
    vi.stubGlobal('fetch', fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it.each([
    ['Helvetica',             'NimbusSans-Regular'],
    ['Helvetica-Bold',        'NimbusSans-Bold'],
    ['Courier',               'NimbusMonoPS-Regular'],
    ['Courier-Bold',          'NimbusMonoPS-Bold'],
    ['Times-Roman',           'NimbusRoman-Regular'],
    ['Times-Bold',            'NimbusRoman-Bold'],
    ['Times-Italic',          'NimbusRoman-Italic'],
    ['Times-BoldItalic',      'NimbusRoman-BoldItalic'],
  ])('%s → fetches %s.ttf', async (pdfName, urwStem) => {
    const result = await store.get(`${pdfName}.ttf`);
    expect(result).toBeInstanceOf(Uint8Array);
    expect(result!.length).toBeGreaterThan(0);
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining(`${urwStem}.ttf`),
    );
  });

  it('key with a leading slash strips correctly', async () => {
    const result = await store.get('/Helvetica.ttf');
    expect(result).toBeInstanceOf(Uint8Array);
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining('NimbusSans-Regular.ttf'),
    );
  });

  it('unknown font name throws "Font not found"', async () => {
    // UnknownFont is not in URW_FONT_MAP; fetch is never called.
    await expect(store.get('UnknownFont.ttf')).rejects.toThrow(
      /Font not found: UnknownFont/,
    );
  });
});

// ---------------------------------------------------------------------------
// Custom TTF font file supplied via setFont()
// ---------------------------------------------------------------------------

describe('FontStore – setFont() with custom TTF bytes', () => {
  let store: FontStore;

  beforeEach(() => {
    store = new FontStore();
  });

  afterEach(() => {
    // Clear any overrides registered during the test.
    setFont('DirectBytes', null);
    setFont('VendorTtf', null);
    setFont('ClearedFont', null);
  });

  it('setFont with a Uint8Array returns the exact bytes', async () => {
    const bytes = new Uint8Array([10, 20, 30, 40, 50]);
    setFont('DirectBytes', bytes);
    const result = await store.get('DirectBytes.ttf');
    expect(result).toBeInstanceOf(Uint8Array);
    expect(result).toEqual(bytes);
  });

  it('setFont with bytes read from a local TTF file (vendor dir)', async () => {
    // Simulates the pattern of loading a TTF from a fontsource package in
    // node_modules: read the file bytes once and register them explicitly.
    const ttfPath = resolve(VENDOR_DIR, 'NimbusRoman-Regular.ttf');
    const bytes = new Uint8Array(readFileSync(ttfPath));
    setFont('VendorTtf', bytes);
    const result = await store.get('VendorTtf.ttf');
    expect(result).toBeInstanceOf(Uint8Array);
    expect(result!.length).toBe(bytes.length);
    expect(result).toEqual(bytes);
  });

  it('setFont(null) clears the override so an unknown font throws', async () => {
    setFont('ClearedFont', new Uint8Array([1, 2, 3]));
    setFont('ClearedFont', null);
    await expect(store.get('ClearedFont.ttf')).rejects.toThrow(/Font not found/);
  });
});
