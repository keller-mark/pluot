// The functions in this file are convenience / helper functions, primarily intended for static visualization.
import lzs from "lz-string";
import { initialize, render_wasm } from "./core.js";
import { normalizeStores, type NormalizeStoresParam } from "./store-normalization.js";

// Needed due to "SyntaxError: Named export 'decompressFromUint8Array' not found.
// The requested module 'lz-string' is a CommonJS module,
// which may not support all module.exports as named exports."
const { decompressFromUint8Array } = lzs;

type RenderOptions = {
  // A subset of Rust RenderParams that are relevant to the following functions.
  format: "Vector" | "Raster",
  svg_compression_enabled: boolean,
  svg_include_document: boolean,
  width: number,
  height: number,
  timeout?: number,
  wait_for_store_gets: boolean,
} & NormalizeStoresParam;

// Analogous to the python render_raw, render_to_image, render_to_svg.
// We want variants that simply return the RGBA array or svg string,
// but also variants which create dom elements (svg, img, or canvas),
// or variants which render to existing dom elements (svg, img, or canvas).
export async function renderRaw(params: RenderOptions): Promise<Uint8Array> {
  await initialize();

  const { store, storeName, stores, register, ...rest } = params;
  const storesMeta = normalizeStores({ store, storeName, stores, register, plotId: rest.plotId });

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
    bailedEarly,
  };
}

export async function renderToString(params: RenderOptions): Promise<{ plot: string, bailedEarly: boolean|null }> {
  // TODO: throw error if params.format is not "Vector"
  const arr = await renderRaw(params);

  const bailedEarly = arr.at(-1) === 1;
  const graphicsArr = arr.subarray(0, -1);

  let gContents;
  if (params.svg_compression_enabled) {
    gContents = decompressFromUint8Array(graphicsArr);
  } else {
    gContents = (new TextDecoder()).decode(graphicsArr);
  }
  return {
    plot: gContents,
    bailedEarly,
  };
}

type RenderToElementOptions = {
  el?: Element | string,
  asChild?: boolean,
  // Exponential backoff bounds for the untilDone re-rendering loop.
  // Defaults match the React <Pluot> component.
  minTimeout?: number,
  maxTimeout?: number,
};

function resolveBaseElement(el: Element | string): Element {
  if (typeof el === "string") {
    const found = document.getElementById(el);
    if (!found) {
      throw new Error(`renderToElement: no element found with id "${el}".`);
    }
    return found;
  }
  return el;
}

function createRenderTarget(tag: "svg" | "canvas", width: number, height: number): Element {
  const elNew = tag === "svg"
    ? document.createElementNS("http://www.w3.org/2000/svg", "svg")
    : document.createElement("canvas");
  elNew.setAttribute("width", String(width));
  elNew.setAttribute("height", String(height));
  return elNew;
}

// Renders params into the target element once, returning bailedEarly
// (null when the underlying format does not yet report it, e.g. Vector).
async function renderOnceIntoTarget(params: RenderOptions, target: Element): Promise<boolean | null> {
  if (params.format === "Vector") {
    // Override svg_include_document, so that we can reuse the parent SVG element.
    const { plot: gContents, bailedEarly } = await renderToString({ ...params, svg_include_document: false });
    target.innerHTML = gContents;
    return bailedEarly;
  }

  // Format: Raster (render to canvas)
  const { plot: imageData, bailedEarly } = await renderToImageData(params);
  const ctx = (target as HTMLCanvasElement).getContext("2d");
  if (!ctx) {
    throw new Error("renderToElement: failed to obtain a 2d rendering context from the canvas element.");
  }
  ctx.putImageData(imageData, 0, 0);
  return bailedEarly;
}

// Use params.format to determine (format: Vector -> SVG; format: Raster -> canvas)
// If domElementOrId is undefined, return a newly created dom element.
// If domElementOrId is a string, use document.getElementById
// If domElementOrId is an element, use it directly.
// If domElementOrId is string or element, use asChild to determine whether to render to a child element (e.g., the provided element can be the parent div, rather than an existing svg/canvas element).
// Take into account params.svg_include_document when rendering to svg.
// If untilDone is true and domElementOrId is not undefined, check bailedEarly, and continue re-rendering to the provided/specified dom element/child until bailedEarly is false. When asChild is true, attempt to reuse the same child element, otherwise error.
// Use exponential backoff when re-rendering, using the same logic as in the react component.
// This function should not handle other things like reactive params, as the user can opt to use the React component for a fully interactive experience.
export async function renderToElement(params: RenderOptions, options: RenderToElementOptions): Promise<Element> {
  const { el, asChild = false, minTimeout = 32, maxTimeout = 5000 } = options ?? {};

  // If wait_for_store_gets is true, then we just render once (and using the provided timeout).
  // If wait_for_store_gets is false, then we attempt to re-render multiple times, until bailedEarly is false.
  const untilDone = !params.wait_for_store_gets;
  const { format, width, height } = params;
  const tag = format === "Vector" ? "svg" : "canvas";

  let target: Element;
  let baseEl: Element | undefined;

  if (el === undefined) {
    target = createRenderTarget(tag, width, height);
  } else {
    baseEl = resolveBaseElement(el);
    if (asChild) {
      const existingChild = Array.from(baseEl.children).find(
        child => child.tagName.toLowerCase() === tag
      );
      target = existingChild ?? createRenderTarget(tag, width, height);
      if (!existingChild) {
        baseEl.appendChild(target);
      }
    } else {
      if (baseEl.tagName.toLowerCase() !== tag) {
        throw new Error(
          `renderToElement: expected a <${tag}> element for format "${format}", but received a <${baseEl.tagName.toLowerCase()}> element.`
        );
      }
      target = baseEl;
    }
  }

  if (!untilDone || el === undefined) {
    await renderOnceIntoTarget(params, target);
    return target;
  }

  let currentTimeout = minTimeout;
  let bailedEarly: boolean | null = true;
  while (bailedEarly) {
    if (asChild && baseEl && !baseEl.contains(target)) {
      throw new Error(
        "renderToElement: expected to reuse the same child element across re-renders, but it is no longer present in the parent element."
      );
    }
    bailedEarly = await renderOnceIntoTarget({ ...params, timeout: currentTimeout }, target);
    if (bailedEarly) {
      currentTimeout = Math.min(currentTimeout * 2, maxTimeout);
    }
  }

  return target;
}
