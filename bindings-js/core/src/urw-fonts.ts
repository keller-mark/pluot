/**
 * Maps "family/style/weight" keys to their URW Core 35 TTF filename stems.
 * Keys use capitalized weight ("Normal"|"Bold") and style ("Normal"|"Italic"|"Oblique").
 * Helvetica uses oblique variants; Courier and Times use italic variants.
 *
 * The TTF files are shipped alongside this package (vendor/urw-core35-fonts/).
 * For production deployments, call setUrwFontBaseUrl() to point to wherever
 * the font files are hosted (CDN, self-hosted, etc.).
 */
export const URW_FONT_MAP: Record<string, string> = {
  "Courier/Normal/Normal":         "NimbusMonoPS-Regular",
  "Courier/Normal/Bold":           "NimbusMonoPS-Bold",
  "Courier/Italic/Normal":         "NimbusMonoPS-Italic",
  "Courier/Oblique/Normal":        "NimbusMonoPS-Italic",
  "Courier/Italic/Bold":           "NimbusMonoPS-BoldItalic",
  "Courier/Oblique/Bold":          "NimbusMonoPS-BoldItalic",
  "Helvetica/Normal/Normal":       "NimbusSans-Regular",
  "Helvetica/Normal/Bold":         "NimbusSans-Bold",
  "Helvetica/Oblique/Normal":      "NimbusSans-Oblique",
  "Helvetica/Italic/Normal":       "NimbusSans-Oblique",
  "Helvetica/Oblique/Bold":        "NimbusSans-BoldOblique",
  "Helvetica/Italic/Bold":         "NimbusSans-BoldOblique",
  "Times-Roman/Normal/Normal":     "NimbusRoman-Regular",
  "Times-Roman/Normal/Bold":       "NimbusRoman-Bold",
  "Times-Roman/Italic/Normal":     "NimbusRoman-Italic",
  "Times-Roman/Oblique/Normal":    "NimbusRoman-Italic",
  "Times-Roman/Italic/Bold":       "NimbusRoman-BoldItalic",
  "Times-Roman/Oblique/Bold":      "NimbusRoman-BoldItalic",
  "Symbol/Normal/Normal":          "StandardSymbolsPS",
  "ZapfDingbats/Normal/Normal":    "D050000L",
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
