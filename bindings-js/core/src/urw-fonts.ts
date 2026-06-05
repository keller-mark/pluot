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

export async function loadUrwFont(filename: string): Promise<Uint8Array> {
  const base = _urwFontBaseUrl
    ?? new URL("../../../vendor/urw-core35-fonts/", import.meta.url).href;
  const url = `${base}${filename}.ttf`;
  const resp = await fetch(url);
  if (!resp.ok) throw new Error(`Failed to fetch URW font: ${url} (${resp.status})`);
  return new Uint8Array(await resp.arrayBuffer());
}
