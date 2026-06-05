import type { AsyncReadable } from "zarrita";
import { URW_FONT_MAP, loadUrwFont } from "./urw-fonts.js";

// Optional explicit overrides: font_name -> bytes or Promise<bytes|null>.
// These take priority over URW bundled fonts, stylesheets, and local fonts.
const fontOverrides: Record<string, Uint8Array | Promise<Uint8Array | null>> = {};

/**
 * Try to find a font by family name in the page's @font-face stylesheet rules
 * and fetch its bytes. Returns null if not found or on error.
 */
async function _tryFontInStylesheets(fontName: string): Promise<Uint8Array | null> {
  if (typeof document === "undefined") return null;
  const nameLower = fontName.toLowerCase();
  for (const sheet of Array.from(document.styleSheets)) {
    let rules: CSSRuleList;
    try {
      rules = sheet.cssRules;
    } catch {
      continue; // Cross-origin stylesheet — skip.
    }
    for (const rule of Array.from(rules)) {
      if (!(rule instanceof CSSFontFaceRule)) continue;
      const family = rule.style.getPropertyValue("font-family").replace(/['"]/g, "").trim();
      if (family.toLowerCase() !== nameLower) continue;
      // Extract all url(...) references from src and try each.
      const src = rule.style.getPropertyValue("src");
      for (const match of src.matchAll(/url\(['"]?([^'")\s]+)['"]?\)/g)) {
        try {
          const resp = await fetch(match[1]);
          if (resp.ok) return new Uint8Array(await resp.arrayBuffer());
        } catch { /* try next */ }
      }
    }
  }
  return null;
}

/**
 * Try to find a font via the Local Font Access API (Chrome 103+).
 * Requires user permission; silently returns null if unavailable or denied.
 */
async function _tryQueryLocalFonts(fontName: string): Promise<Uint8Array | null> {
  if (typeof window === "undefined" || !("queryLocalFonts" in window)) return null;
  try {
    const localFonts: any[] = await (window as any).queryLocalFonts();
    for (const font of localFonts) {
      if (font.family.toLowerCase() === fontName.toLowerCase()) {
        const blob: Blob = await font.blob();
        return new Uint8Array(await blob.arrayBuffer());
      }
    }
  } catch { /* permission denied or API not supported */ }
  return null;
}

function _keyToFontName(key: string): string {
  let name = key.startsWith("/") ? key.slice(1) : key;
  if (name.toLowerCase().endsWith(".ttf") || name.toLowerCase().endsWith(".otf")) {
    name = name.slice(0, -4);
  }
  return name;
}

/**
 * AsyncReadable store for fonts. Registered as the "__fonts__" zarr store so that
 * Rust can request font bytes via zarr_get_status / zarr_get using store_name = "__fonts__"
 * and key = "{font_name}.ttf". Status tracking and promise deduplication are handled by
 * the LruStore wrapper in core.ts, reusing the same infrastructure as zarr data stores.
 *
 * Lookup order:
 *   1. Explicit override registered via setFont()
 *   2. URW Core 35 bundled fonts (by direct name or PDF Base-14 alias)
 *   3. @font-face rules in page stylesheets
 *   4. Local Font Access API (Chrome 103+)
 *
 * Throws if the font cannot be found (so LruStore marks the key as "rejected",
 * causing Rust to fall back to the bundled Inter-Bold default).
 */
export class FontStore implements AsyncReadable {
  async get(key: string): Promise<Uint8Array | undefined> {
    const fontName = _keyToFontName(key);

    // 1. Explicit override via setFont().
    if (fontName in fontOverrides) {
      const entry = fontOverrides[fontName];
      const result = entry instanceof Uint8Array ? entry : await entry;
      if (result === null) throw new Error(`Font unavailable: ${fontName}`);
      return result;
    }

    // 2. URW Core 35 bundled fonts.
    const urwFilename = URW_FONT_MAP[fontName];
    if (urwFilename !== undefined) {
      return loadUrwFont(urwFilename);
    }

    // 3. @font-face rules in page stylesheets.
    const fromStylesheets = await _tryFontInStylesheets(fontName);
    if (fromStylesheets) return fromStylesheets;

    // 4. Local Font Access API (Chrome 103+, requires permission).
    const fromLocal = await _tryQueryLocalFonts(fontName);
    if (fromLocal) return fromLocal;

    // Font not found — throw so LruStore marks this key as "rejected".
    throw new Error(`Font not found: ${fontName}`);
  }
}

/**
 * Explicitly register font bytes or a source URL for a named font in TextLayer.
 * This takes priority over URW bundled fonts, stylesheet detection, and local fonts.
 * - Pass a Uint8Array or Promise<Uint8Array|null> to supply bytes directly.
 * - Pass a URL string to fetch the font from that location at render time.
 * - Pass null to clear any override and fall back to automatic detection.
 */
export function setFont(
  fontName: string,
  data: string | Uint8Array | Promise<Uint8Array | null> | null,
) {
  if (data === null) {
    delete fontOverrides[fontName];
  } else if (typeof data === "string") {
    // URL string: fetch lazily and cache the promise.
    fontOverrides[fontName] = fetch(data)
      .then(r => {
        if (!r.ok) throw new Error(`Failed to fetch font: ${data} (${r.status})`);
        return r.arrayBuffer();
      })
      .then(buf => new Uint8Array(buf))
      .catch((_err): null => null);
  } else {
    fontOverrides[fontName] = data;
  }
}

export { setUrwFontBaseUrl } from "./urw-fonts.js";
