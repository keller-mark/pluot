import * as wasm from "pluot";
import { lru, type LruStore, type PromiseState } from "./lru-store.js";
import type { AsyncReadable } from "zarrita";
import { FontStore } from "./fonts.js";

export const { render_wasm, pick_wasm } = wasm;

// Global stores singleton.
const stores: Record<string, LruStore<AsyncReadable>> = {};

let isInitializedPromise: Promise<any> | undefined = undefined;
let isWasmReady = false;

// Create an LruStore that "pushes" its Promise state changes into the Rust-side
// HashMap (see zarr.rs), so that Rust status checks with `wait_for_store_pushes: true`
// do not need a JS roundtrip on every re-render.
// Pushes are dropped while the wasm module is not ready yet; this is safe because the
// Rust-side HashMap is only populated by status checks during rendering, which
// cannot have happened before the wasm module is ready.
function makeLruStore(storeName: string, store: AsyncReadable): LruStore<AsyncReadable> {
  return lru(
    store,
    undefined,
    (cacheKey: string, state: PromiseState | undefined) => {
      if (!isWasmReady) return;
      if (state === undefined) {
        console.log("Removing promise state", storeName, cacheKey);
        wasm.zarr_remove_promise_status(storeName, cacheKey);
      } else if (state !== 'pending') {
        // Only push settled states (fulfilled/rejected). Rust records the pending
        // state itself from the initial status-check roundtrip that kicks off the Promise.
        console.log("Pushing promise state", storeName, cacheKey, state);
        wasm.zarr_push_promise_status(storeName, cacheKey, state);
      }
    },
    () => {
      if (!isWasmReady) return;
      wasm.zarr_clear_promise_statuses(storeName);
    },
  );
}

async function _initialize() {
  // IMPORTANT: This function should only be executed ONCE.
  await wasm.default();

  // This is a hack that allows avoiding putting these functions on `window` or `globalThis`.
  // It is a workaround for https://github.com/wasm-bindgen/wasm-bindgen/issues/3041
  // See corresponding code in `bindings.rs`.
  wasm.set_zarr_imports({
    zarr_get: async (store_name: string, key: string) => {
      console.log(`zarr_get: store_name=${store_name}, key=${key}`);
      return stores[store_name].get(`/${key}`);
    },

    zarr_get_status: (store_name: string, key: string) => {
      return stores[store_name].getPeek(`/${key}`);
    },

    zarr_has: async (store_name: string, key: string) => {
      // console.log(`zarr_has: store_name=${store_name}, key=${key}`);
      return stores[store_name].get(`/${key}`) !== undefined;
    },

    zarr_has_status: (store_name: string, key: string) => {
      return stores[store_name].getPeek(`/${key}`);
    },

    zarr_get_range_from_offset: async (store_name: string, key: string, offset: number, length: number) => {
      // console.log(`zarr_get_range_from_offset: store_name=${store_name}, key=${key}, offset=${offset}, length=${length}`);
      return stores[store_name].getRange(`/${key}`, { offset, length });
    },

    zarr_get_range_from_offset_status: (store_name: string, key: string, offset: number, length: number) => {
      // console.log(`zarr_get_range_from_offset: store_name=${store_name}, key=${key}, offset=${offset}, length=${length}`);
      return stores[store_name].getRangePeek(`/${key}`, { offset, length });
    },

    zarr_get_range_from_end: async (store_name: string, key: string, suffix_length: number) => {
      // console.log(`zarr_get_range_from_end: store_name=${store_name}, key=${key}, suffix_length=${suffix_length}`);
      return stores[store_name].getRange(`/${key}`, { suffixLength: suffix_length });
    },

    zarr_get_range_from_end_status: (store_name: string, key: string, suffix_length: number) => {
      // console.log(`zarr_get_range_from_end: store_name=${store_name}, key=${key}, suffix_length=${suffix_length}`);
      return stores[store_name].getRangePeek(`/${key}`, { suffixLength: suffix_length });
    },
  });

  // Register the font store so Rust can request fonts via zarr_get("__fonts__", "{name}.ttf").
  stores["__fonts__"] = makeLruStore("__fonts__", new FontStore());

  // Opt-in to better error messages.
  wasm.set_panic_hook();
}

export async function initialize() {
  // This function is safe to execute multiple times.
  if(!isInitializedPromise) {
    isInitializedPromise = _initialize().then(() => { isWasmReady = true; });
  } else {
    isInitializedPromise.then(() => { isWasmReady = true; });
  }
  return isInitializedPromise;
}

export function getIsWasmReady(): boolean {
  return isWasmReady;
}

export function setStore(store: AsyncReadable, plotId: string): string {
  setStoreByName(plotId + "_store", store);
  return plotId + "_store";
}

export function setStoreByName(storeName: string, store: AsyncReadable) {
  // Forget the Promise statuses pushed by any store previously registered under this name.
  if (isWasmReady && stores[storeName]) {
    wasm.zarr_clear_promise_statuses(storeName);
  }
  stores[storeName] = makeLruStore(storeName, store);
}

export function getStore(storeName: string) {
  return stores[storeName];
}

export { setFont } from "./fonts.js";
