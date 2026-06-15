import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { FontStore, setFont } from './fonts.js';

vi.mock('./urw-fonts.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./urw-fonts.js')>();
  return { ...actual, loadUrwFont: vi.fn().mockResolvedValue(undefined) };
});

const VENDOR_DIR = resolve(import.meta.dirname, '../../../vendor/urw-core35-fonts');

describe('FontStore – PDF Base-14 fonts resolved via URW map', () => {
  let store: FontStore;

  beforeEach(() => {
    store = new FontStore();
  });

  it.each([
    ['Helvetica/Normal/Normal',   'NimbusSans-Regular'],
    ['Helvetica/Normal/Bold',     'NimbusSans-Bold'],
    ['Courier/Normal/Normal',     'NimbusMonoPS-Regular'],
    ['Courier/Normal/Bold',       'NimbusMonoPS-Bold'],
    ['Times-Roman/Normal/Normal', 'NimbusRoman-Regular'],
    ['Times-Roman/Normal/Bold',   'NimbusRoman-Bold'],
    ['Times-Roman/Italic/Normal', 'NimbusRoman-Italic'],
    ['Times-Roman/Italic/Bold',   'NimbusRoman-BoldItalic'],
  ])('%s routes to %s via URW map', async (key, urwStem) => {
    const { loadUrwFont } = await import('./urw-fonts.js');
    await store.get(`${key}.ttf`);
    expect(loadUrwFont).toHaveBeenCalledWith(urwStem);
  });

  it('key with a leading slash strips correctly', async () => {
    const { loadUrwFont } = await import('./urw-fonts.js');
    await store.get('/Helvetica/Normal/Normal.ttf');
    expect(loadUrwFont).toHaveBeenCalledWith('NimbusSans-Regular');
  });

  it('unknown font name throws "Font not found"', async () => {
    // UnknownFont is not in URW_FONT_MAP; fetch is never called.
    await expect(store.get('UnknownFont/Normal/Normal.ttf')).rejects.toThrow(
      /Font not found: UnknownFont\/Normal\/Normal/,
    );
  });
});

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
    const result = await store.get('DirectBytes/Normal/Normal.ttf');
    expect(result).toBeInstanceOf(Uint8Array);
    expect(result).toEqual(bytes);
  });

  it('setFont with bytes read from a local TTF file (vendor dir)', async () => {
    // Simulates the pattern of loading a TTF from a fontsource package in
    // node_modules: read the file bytes once and register them explicitly.
    const ttfPath = resolve(VENDOR_DIR, 'NimbusRoman-Regular.ttf');
    const bytes = new Uint8Array(readFileSync(ttfPath));
    setFont('VendorTtf', bytes);
    const result = await store.get('VendorTtf/Normal/Normal.ttf');
    expect(result).toBeInstanceOf(Uint8Array);
    expect(result!.length).toBe(bytes.length);
    expect(result).toEqual(bytes);
  });

  it('setFont(null) clears the override so an unknown font throws', async () => {
    setFont('ClearedFont', new Uint8Array([1, 2, 3]));
    setFont('ClearedFont', null);
    await expect(store.get('ClearedFont/Normal/Normal.ttf')).rejects.toThrow(/Font not found/);
  });
});
