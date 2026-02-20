import QuickLRU from "quick-lru";

import type { RangeQuery, AsyncReadable, AbsolutePath } from 'zarrita';


function normalizeKey(key: string, range?: RangeQuery) {
  if (!range) return key;
  if ("suffixLength" in range) return `${key}:-${range.suffixLength}`;
  return `${key}:${range.offset}:${range.offset + range.length - 1}`;
}

// Provides a blanket implementation of getRange that can be used with any AsyncReadable store,
// even if it doesn't define a getRange method.
// If the store does have a native getRange method, we use that instead.
// Reference: https://github.com/vitessce/vitessce/blob/main/packages/utils/zarr-utils/src/base-getrange.ts
export function createGetRange<S extends AsyncReadable>(store: S) {
  // TODO: support options param for getRange?
  return async (...args: Parameters<NonNullable<S["getRange"]>>): Promise<Uint8Array | undefined> => {
    const [key, range, opts] = args;
    if (typeof store.getRange === 'function') {
      return store.getRange(key, range, opts);
    }
    // Store does not have a native getRange method; falling back to get. This may be inefficient for large data.
    const arr = await store.get(key, opts);
    if (!arr) return undefined;
    const { buffer } = arr;
    if ('suffixLength' in range) {
      const { suffixLength } = range;
      return new Uint8Array(buffer, buffer.byteLength - suffixLength, suffixLength);
    }
    if ('offset' in range && 'length' in range) {
      const { offset, length } = range;
      return new Uint8Array(buffer, offset, length);
    }
    throw new Error('Invalid rangeQuery value.');
  };
}

// A class-based version of the proxy-based lru() function from vizarr.
// Reference: https://github.com/hms-dbmi/vizarr/blob/862745c1c7c095748bbe97475da61807d5b49189/src/lru-store.ts
class LruStore<S extends AsyncReadable> implements AsyncReadable {
  #inner_store: S;
  #cache: QuickLRU<string, [Promise<Uint8Array | undefined>, AbortController]>;

  constructor(store: S, maxSize = 100) {
    this.#inner_store = store;
    this.#cache = new QuickLRU<string, [Promise<Uint8Array | undefined>, AbortController]>({ maxSize });
  }

  async get(...args: Parameters<S["get"]>): Promise<Uint8Array | undefined> {
    const [key, opts] = args;
    // console.log(`LRU get: ${key}`);
    const cacheKey = normalizeKey(key);
    const cached = this.#cache.get(cacheKey);
    if (cached) {
      return cached[0];
    }
    const controller = new AbortController();
    let getResult = this.#inner_store.get(key, {
      signal: controller.signal,
      ...(opts ?? {})
    });
    const result = Promise.resolve(getResult).catch((err) => {
      this.#cache.delete(cacheKey);
      throw err;
    });
    this.#cache.set(cacheKey, [result, controller]);
    return result;
  }

  async getRange(...args: Parameters<NonNullable<S["getRange"]>>): Promise<Uint8Array | undefined> {
    const [key, range, opts] = args;
    const cacheKey = normalizeKey(key, range);
    const cached = this.#cache.get(cacheKey);
    if (cached) {
      return cached[0];
    }

    const _getRange = typeof this.#inner_store.getRange === 'function'
      ? this.#inner_store.getRange.bind(this.#inner_store)
      : createGetRange(this.#inner_store);

    const controller = new AbortController();
    // @ts-expect-error
    let getRangeResult = _getRange(key, range, {
      signal: controller.signal,
      ...(opts ?? {})
    });
    const result = Promise.resolve(getRangeResult).catch((err) => {
      this.#cache.delete(cacheKey);
      throw err;
    });
    this.#cache.set(cacheKey, [result, controller]);
    return result;
  }

  clearCache() {
    // Use AbortSignal in clearCache for promises that have not yet been resolved.
    this.#cache.forEach(([promise, controller]) => {
      // TODO: check if promise is still pending before aborting? Or just always abort?
      // TODO: verify that this aborting is actually working
      controller.abort()
    });
    this.#cache.clear();
  }
}

export function lru(inner_store: AsyncReadable, maxSize = 100): AsyncReadable {
  return new LruStore(inner_store, maxSize);
}
