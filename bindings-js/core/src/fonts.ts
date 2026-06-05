import type { AsyncReadable } from "zarrita";

// Optional explicit overrides: font_name -> bytes or Promise<bytes|null>.
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
 * Throws if the font cannot be found (so LruStore marks the key as "rejected").
 */
export class FontStore implements AsyncReadable {
  async get(key: string): Promise<Uint8Array | undefined> {
    const fontName = _keyToFontName(key);

    // 1. Explicit override via setFont.
    if (fontName in fontOverrides) {
      const entry = fontOverrides[fontName];
      const result = entry instanceof Uint8Array ? entry : await entry;
      if (result === null) throw new Error(`Font unavailable: ${fontName}`);
      return result;
    }

    // 2. @font-face rules in page stylesheets.
    const fromStylesheets = await _tryFontInStylesheets(fontName);
    if (fromStylesheets) return fromStylesheets;

    // 3. Local Font Access API (Chrome 103+, requires permission).
    const fromLocal = await _tryQueryLocalFonts(fontName);
    if (fromLocal) return fromLocal;

    // Font not found — throw so LruStore marks this key as "rejected".
    throw new Error(`Font not found: ${fontName}`);
  }
}

/**
 * Override the bytes used for a named font in TextLayer.
 * This takes priority over auto-detection from stylesheets or local fonts.
 * Pass null to clear any override and fall back to auto-detection.
 */
export function setFont(fontName: string, data: Uint8Array | Promise<Uint8Array | null> | null) {
  if (data === null) {
    delete fontOverrides[fontName];
  } else {
    fontOverrides[fontName] = data;
  }
}
