import type { AsyncReadable } from "zarrita";
import { render_wasm, setStoreByName, getStore } from "./core.js";
import { storeInstanceToMetadata, type ZarrStoreInfo } from "./store-metadata.js";

/** A single store, given either as a live zarrita store instance or already-derived metadata. */
export type StoreValue = AsyncReadable | ZarrStoreInfo;

/**
 * The ergonomic `store`/`storeName`/`stores` convenience params accepted by
 * {@link render}, layered on top of the raw `RenderParams` JSON shape that
 * `render_wasm` expects.
 */
export type RenderOptions = Record<string, unknown> & {
  /** A single Zarr store (instance or `ZarrStoreInfo`), named via `storeName` (or `"default"`). */
  store?: StoreValue;
  /** Name for `store`, or (with no `store`/`stores`) a store already registered via `setStoreByName`. */
  storeName?: string;
  /** Named Zarr stores, each a live instance or already-derived `ZarrStoreInfo`. */
  stores?: Record<string, StoreValue>;
};

function isZarrStoreInfo(value: StoreValue): value is ZarrStoreInfo {
  return typeof value === "object" && value !== null && "store_type" in value;
}

/**
 * Build the top-level `stores` metadata map (store name -> `ZarrStoreInfo`)
 * that `render_wasm` expects, from the ergonomic `store`/`storeName`/`stores`
 * convenience params. Live store instances are registered by name (via
 * {@link setStoreByName}) so the `zarr_*` bound functions can reach them;
 * values that are already `ZarrStoreInfo`-shaped pass through as-is.
 */
function buildStoresMetadata(
  store: StoreValue | undefined,
  storeName: string | undefined,
  stores: Record<string, StoreValue> | undefined,
): Record<string, ZarrStoreInfo> | undefined {
  const result: Record<string, ZarrStoreInfo> = {};

  if (stores) {
    for (const [name, value] of Object.entries(stores)) {
      if (isZarrStoreInfo(value)) {
        result[name] = value;
      } else {
        setStoreByName(name, value);
        result[name] = storeInstanceToMetadata(value);
      }
    }
  }

  if (store) {
    const name = storeName ?? "default";
    if (isZarrStoreInfo(store)) {
      result[name] = store;
    } else {
      setStoreByName(name, store);
      result[name] = storeInstanceToMetadata(store);
    }
  } else if (storeName) {
    // No `store` given: treat `storeName` as referencing a store already
    // registered (e.g. via `setStoreByName`).
    const registered = getStore(storeName);
    if (registered) {
      result[storeName] = storeInstanceToMetadata(registered);
    }
  }

  return Object.keys(result).length > 0 ? result : undefined;
}

/**
 * Ergonomic wrapper over `render_wasm` that accepts `store`/`storeName`/
 * `stores` convenience params (mirroring `pluot.render()` on the Python side)
 * in addition to the raw `RenderParams` shape. Returns a `Uint8Array` of RGBA
 * bytes (plus one trailing status byte) for raster output, or a compressed SVG
 * document for vector output.
 */
export async function render(params: RenderOptions): Promise<Uint8Array> {
  const { store, storeName, stores, ...rest } = params;
  const storesMeta = buildStoresMetadata(store, storeName, stores);
  return render_wasm({
    ...rest,
    ...(storesMeta ? { stores: storesMeta } : {}),
  });
}
