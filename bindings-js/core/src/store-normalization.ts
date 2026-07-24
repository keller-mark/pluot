import { FetchStore, type AsyncReadable } from "zarrita";
import { type ZarrStoreInfo, storeInstanceToMetadata, storeMetadataToInstance } from './store-metadata.js';
import { setStoreByName, getStore } from './core.js';
import { assert } from './assert.js';

// A store value is either a live zarrita store instance or already-derived
// `ZarrStoreInfo` metadata (see `@pluot/core`'s `store-metadata.ts`).
export function isZarrStoreInfo(value: unknown): boolean {
  return value != null && typeof value === 'object' && 'store_type' in value;
}

type StoreInput = string | AsyncReadable | ZarrStoreInfo;
type StoresInput = Record<string, StoreInput>;
type StoresOutput = Record<string, ZarrStoreInfo>

type NormalizeStoresParam = {
  stores?: StoresInput,
  store?: StoreInput,
  storeName?: string,
  plotId: string,
  register: boolean
};

export function normalizeStores({ stores, store, storeName, plotId, register = true }: NormalizeStoresParam): StoresOutput | undefined {
  const result: StoresOutput = {};

  if ((store || storeName) && stores) {
    throw new Error('store/storeName (singular) are mutually exclusive with stores (plural).');
  }
  const singleStoreName = storeName ?? plotId;

  if (typeof store === 'string') {
    // If store is a string, assume it is a URL and initialize a FetchStore here.
    const storeByUrl = new FetchStore(store);
    if (register) {
      setStoreByName(singleStoreName, storeByUrl);
    }
    result[singleStoreName] = storeInstanceToMetadata(storeByUrl);
  } else if (store && typeof store !== 'string' && !isZarrStoreInfo(store)) {
    // Assume `store` is a zarrita Store instance.
    assert("get" in store, "isZarrStoreInfo returned false, so store is expected to be a zarrita AsyncReadable instance.");

    if (register) {
      setStoreByName(singleStoreName, store);
    }
    result[singleStoreName] = storeInstanceToMetadata(store);
  } else if (store && typeof store !== 'string' && isZarrStoreInfo(store)) {
    // Assume `store` is a ZarrStoreInfo dict/JSON object.
    assert("store_type" in store, "isZarrStoreInfo returned true, so store is expected to be ZarrStoreInfo dict.");

    if (register) {
      const storeByInfo = storeMetadataToInstance(store);
      setStoreByName(singleStoreName, storeByInfo);
    }
    result[singleStoreName] = store;
  }

  // The plural `stores` prop: each value is either a live store instance
  // (registered by name) or an already-derived `ZarrStoreInfo` object.
  if (stores) {
    for (const [name, value] of Object.entries(stores)) {
      if (typeof value === 'string') {
        // If `value` is a string, assume it is a URL and initialize a FetchStore here.
        const storeByUrl = new FetchStore(value);
        if (register) {
          setStoreByName(name, storeByUrl);
        }
        result[name] = storeInstanceToMetadata(storeByUrl);
      } else if (typeof value !== 'string' && !isZarrStoreInfo(value)) {
        // Assume `value` is a zarrita Store instance.
        assert("get" in value, "isZarrStoreInfo returned false, so store is expected to be a zarrita AsyncReadable instance.");
        if (register) {
          setStoreByName(name, value);
        }
        result[name] = storeInstanceToMetadata(value);
      } else if (typeof value !== 'string' && isZarrStoreInfo(value)) {
        // Assume `value` is a ZarrStoreInfo dict/JSON object.
        assert("store_type" in value, "isZarrStoreInfo returned true, so store is expected to be ZarrStoreInfo dict.");
        if (register) {
          const storeByInfo = storeMetadataToInstance(value);
          setStoreByName(name, storeByInfo);
        }
        result[name] = value;
      }
    }
  }

  return Object.keys(result).length > 0 ? result : undefined;
}
