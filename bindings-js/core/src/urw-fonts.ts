/**
 * Maps the 14 PDF Base font names to their URW Core 35 TTF filename stems.
 * Only these names are recognised automatically. Any other font name (including
 * direct URW names like "NimbusSans-Regular") must be registered explicitly via
 * setFont() to be usable.
 *
 * The TTF files are shipped alongside this package (vendor/urw-core35-fonts/).
 * For production deployments, call setUrwFontBaseUrl() to point to wherever
 * the font files are hosted (CDN, self-hosted, etc.).
 */
export const URW_FONT_MAP: Record<string, string> = {
  "Courier":               "NimbusMonoPS-Regular",
  "Courier-Bold":          "NimbusMonoPS-Bold",
  "Courier-Oblique":       "NimbusMonoPS-Italic",
  "Courier-BoldOblique":   "NimbusMonoPS-BoldItalic",
  "Helvetica":             "NimbusSans-Regular",
  "Helvetica-Bold":        "NimbusSans-Bold",
  "Helvetica-Oblique":     "NimbusSans-Oblique",
  "Helvetica-BoldOblique": "NimbusSans-BoldOblique",
  "Times-Roman":           "NimbusRoman-Regular",
  "Times-Bold":            "NimbusRoman-Bold",
  "Times-Italic":          "NimbusRoman-Italic",
  "Times-BoldItalic":      "NimbusRoman-BoldItalic",
  "Symbol":                "StandardSymbolsPS",
  "ZapfDingbats":          "D050000L",
};

// Base URL for URW font files. By default resolved relative to this module's location,
// which works when the package is bundled with Vite/webpack (font files are copied to dist).
// Override via setUrwFontBaseUrl() when fonts are hosted at a different location.
let _urwFontBaseUrl: string | null = null;

export function setUrwFontBaseUrl(url: string): void {
  _urwFontBaseUrl = url.endsWith("/") ? url : url + "/";
}

function base64Decode(encoded: string) {
  // We do not want to use Buffer.from(encoded, 'base64') because
  // Buffer is not available in the browser and we do not want
  // to add a dependency on a polyfill if we dont have to.
  // Reference: https://stackoverflow.com/a/41106346
  return Uint8Array.from(atob(encoded), c => c.charCodeAt(0));
}


export async function loadUrwFont(filename: string): Promise<Uint8Array> {
  const module = await import(`./vendored-fonts/${filename}.ttf.js`)
  return base64Decode(module.ttfBytes);
}
