// The functions in this file are convenience / helper functions, primarily intended for static visualization.
import lzs from "lz-string";
import { initialize, render_wasm } from "./core.js";
import { normalizeStores, type NormalizeStoresParam } from "./store-normalization.js";

// Needed due to "SyntaxError: Named export 'decompressFromUint8Array' not found.
// The requested module 'lz-string' is a CommonJS module,
// which may not support all module.exports as named exports."
const { decompressFromUint8Array } = lzs;

type RenderOptions = {
  format: "Vector" | "Raster",
  svg_compression_enabled: boolean,
  svg_include_document: boolean,
  width: number,
  height: number,
} & NormalizeStoresParam;

// Analogous to the python render_raw, render_to_image, render_to_svg.
// We want variants that simply return the RGBA array or svg string,
// but also variants which create dom elements (svg, img, or canvas),
// or variants which render to existing dom elements (svg, img, or canvas).
export async function renderRaw(params: RenderOptions): Promise<Uint8Array> {
  await initialize();

  const { store, storeName, stores, register, ...rest } = params;
  const storesMeta = normalizeStores({ store, storeName, stores, register });

  return render_wasm({
    ...rest,
    ...(storesMeta ? { stores: storesMeta } : {}),
  });
}


export async function renderToArray(params: RenderOptions): Promise<{ plot: Uint8ClampedArray, bailedEarly: boolean }> {
  // TODO: throw error if params.format is not "Raster"
  const arr = await renderRaw(params);
  const bailedEarly = arr.at(-1) === 1;

  return {
    plot: new Uint8ClampedArray(arr.subarray(0, -1)),
    bailedEarly,
  };
}

export async function renderToImageData(params: RenderOptions): Promise<{ plot: ImageData, bailedEarly: boolean }> {
  // TODO: throw error if params.format is not "Raster"
  const { width, height } = params;
  const { plot: arr, bailedEarly } = await renderToArray(params);
  return {
    plot: new ImageData(
      arr as ImageDataArray,
      width,
      height,
    ),
    bailedEarly
  };
}

export async function renderToString(params: RenderOptions): Promise<{ plot: String, bailedEarly: boolean|null }> {
  // TODO: throw error if params.format is not "Vector"
  const arr = await renderRaw(params);

  let gContents;
  if (params.svg_compression_enabled) {
    gContents = decompressFromUint8Array(arr);
  } else {
    gContents = (new TextDecoder()).decode(arr);
  }
  return {
    plot: gContents,
    // TODO: handle bailed early byte once present in vector mode.
    bailedEarly: null
  };
}

type RenderToElementOptions = {
  el?: Element | string,
  asChild?: boolean,
  untilDone?: boolean,
};

// Use params.format to determine (format: Vector -> SVG; format: Raster -> canvas)
// If domElementOrId is undefined, return a newly created dom element.
// If domElementOrId is a string, use document.getElementById
// If domElementOrId is an element, use it directly.
// If domElementOrId is string or element, use asChild to determine whether to render to a child element (e.g., the provided element can be the parent div, rather than an existing svg/canvas element).
// Take into account params.svg_include_document when rendering to svg.
// If untilDone is true and domElementOrId is not undefined, check bailedEarly, and continue re-rendering to the provided/specified dom element/child until bailedEarly is false. When asChild is true, attempt to reuse the same child element, otherwise error.
// Use exponential backoff when re-rendering, using the same logic as in the react component.
// This function should not handle other things like reactive params, as the user can opt to use the React component for a fully interactive experience.
export async function renderToElement(params: RenderOptions, options: RenderToElementOptions) {
  const { el, asChild = false, untilDone = false } = options ?? {};
  // TODO: implement
}
