import * as wasm from "pluot";
import { lru, type LruStore } from "./lru-store.js";
import type { AsyncReadable } from "zarrita";

export const { render_wasm, pick_wasm } = wasm;

// Global stores singleton.
const stores: Record<string, LruStore<AsyncReadable>> = {};

let isInitializedPromise: Promise<any> | undefined = undefined;
let isWasmReady = false;

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
  stores[plotId + "_store"] = lru(store);
  return plotId + "_store";
}

export function setStoreByName(storeName: string, store: AsyncReadable) {
  stores[storeName] = lru(store);
}

export function getStore(storeName: string) {
  return stores[storeName];
}
