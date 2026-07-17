import type { AsyncReadable } from "zarrita";
import { URW_FONT_MAP, loadUrwFont } from "./urw-fonts.js";

export type FontWeight = "Normal" | "Bold";
export type FontStyle = "Normal" | "Italic" | "Oblique";

// Optional explicit overrides: "family/style/weight" -> bytes or Promise<bytes|null>.
// These take priority over URW bundled fonts, stylesheets, and local fonts.
const fontOverrides: Record<string, Uint8Array | Promise<Uint8Array | null>> = {};

function _fontKey(family: string, weight: FontWeight, style: FontStyle): string {
  return `${family}/${style}/${weight}`;
}

/**
 * Parse the zarr store key emitted by Rust into {family, weight, style}.
 * Key format: "{family}/{style}/{weight}.ttf".
 * Falls back to treating the whole name as the family with normal weight/style
 * for keys that do not match the expected structure.
 */
function _parseKey(key: string): { family: string; weight: FontWeight; style: FontStyle } {
  let name = key.startsWith("/") ? key.slice(1) : key; // strip accidental leading slash
  if (name.toLowerCase().endsWith(".ttf") || name.toLowerCase().endsWith(".otf")) {
    name = name.slice(0, -4);
  }
  const parts = name.split("/");
  if (parts.length === 3) {
    return {
      family: parts[0],
      style: (["Italic", "Oblique"].includes(parts[1]) ? parts[1] : "Normal") as FontStyle,
      weight: (parts[2] === "Bold" ? "Bold" : "Normal") as FontWeight,
    };
  }
  return { family: name, weight: "Normal", style: "Normal" };
}

/**
 * Try to find a font by family name, weight, and style in the page's @font-face
 * stylesheet rules and fetch its bytes. Returns null if not found or on error.
 */
async function _tryFontInStylesheets(family: string, weight: FontWeight, style: FontStyle): Promise<Uint8Array | null> {
  if (typeof document === "undefined") return null;
  const familyLower = family.toLowerCase();
  for (const sheet of Array.from(document.styleSheets)) {
    let rules: CSSRuleList;
    try {
      rules = sheet.cssRules;
    } catch {
      continue; // Cross-origin stylesheet; skip.
    }
    for (const rule of Array.from(rules)) {
      if (!(rule instanceof CSSFontFaceRule)) continue;
      const ruleFamily = rule.style.getPropertyValue("font-family").replace(/['"]/g, "").trim();
      if (ruleFamily.toLowerCase() !== familyLower) continue;
      const ruleWeight = rule.style.getPropertyValue("font-weight").toLowerCase() || "normal";
      const ruleStyle = rule.style.getPropertyValue("font-style").toLowerCase() || "normal";
      if (ruleWeight !== weight.toLowerCase()) continue;
      if (ruleStyle !== style.toLowerCase()) continue;
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
 * AsyncReadable store for fonts. Registered as the "__fonts__" zarr store so that
 * Rust can request font bytes via zarr_get_status / zarr_get using store_name = "__fonts__"
 * and key = "{family}|{weight}|{style}.ttf". Status tracking and promise deduplication
 * are handled by the LruStore wrapper in core.ts, reusing the same infrastructure as
 * zarr data stores.
 *
 * Lookup order:
 *   1. Explicit override registered via setFont()
 *   2. URW Core 35 bundled fonts
 *   3. @font-face rules in page stylesheets
 *
 * Throws if the font cannot be found (so LruStore marks the key as "rejected",
 * causing Rust to fall back to the bundled default font).
 */
export class FontStore implements AsyncReadable {
  async get(key: string): Promise<Uint8Array | undefined> {
    const { family, weight, style } = _parseKey(key);
    const overrideKey = _fontKey(family, weight, style);

    // 1. Explicit override via setFont().
    if (overrideKey in fontOverrides) {
      const entry = fontOverrides[overrideKey];
      const result = entry instanceof Uint8Array ? entry : await entry;
      if (result === null) throw new Error(`Font unavailable: ${overrideKey}`);
      return result;
    }

    // 2. URW Core 35 bundled fonts.
    const urwFilename = URW_FONT_MAP[overrideKey];
    if (urwFilename !== undefined) {
      return loadUrwFont(urwFilename);
    }

    // 3. @font-face rules in page stylesheets.
    const fromStylesheets = await _tryFontInStylesheets(family, weight, style);
    if (fromStylesheets) return fromStylesheets;

    // Font not found. Throw so LruStore marks this key as "rejected".
    throw new Error(`Font not found: ${overrideKey}`);
  }
}

/**
 * Explicitly register font bytes or a source URL for a font in TextLayer.
 * This takes priority over URW bundled fonts and stylesheet detection.
 * - Pass a Uint8Array or Promise<Uint8Array|null> to supply bytes directly.
 * - Pass a URL string to fetch the font from that location at render time.
 * - Pass null to clear any override and fall back to automatic detection.
 */
export function setFont(
  fontFamily: string,
  data: string | Uint8Array | Promise<Uint8Array | null> | null,
  options?: { weight?: FontWeight; style?: FontStyle },
) {
  const key = _fontKey(fontFamily, options?.weight ?? "Normal", options?.style ?? "Normal");
  if (data === null) {
    delete fontOverrides[key];
  } else if (typeof data === "string") {
    // URL string: fetch lazily and cache the promise.
    fontOverrides[key] = fetch(data)
      .then(r => {
        if (!r.ok) throw new Error(`Failed to fetch font: ${data} (${r.status})`);
        return r.arrayBuffer();
      })
      .then(buf => new Uint8Array(buf))
      .catch((_err): null => null);
  } else {
    fontOverrides[key] = data;
  }
}

export { setUrwFontBaseUrl } from "./urw-fonts.js";
