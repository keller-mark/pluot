// Reference: https://github.com/hms-dbmi/vizarr/blob/862745c1c7c095748bbe97475da61807d5b49189/src/lru-store.ts
import QuickLRU from "quick-lru";

import type * as zarr from "zarrita";

type RangeQuery =
  | {
      offset: number;
      length: number;
    }
  | {
      suffixLength: number;
    };

function normalizeKey(key: string, range?: RangeQuery) {
  if (!range) return key;
  if ("suffixLength" in range) return `${key}:-${range.suffixLength}`;
  return `${key}:${range.offset}:${range.offset + range.length - 1}`;
}

// TODO: switch implementation from proxy to Store wrapper class.
// TODO: add a clearCache method to clear all cached entries for a given store.
export function lru<S extends zarr.AsyncReadable>(store: S, maxSize = 100) {
  const cache = new QuickLRU<string, Promise<Uint8Array | undefined>>({ maxSize });
  let getRange = store.getRange ? store.getRange.bind(store) : undefined;
  function get(...args: Parameters<S["get"]>) {
    const [key, opts] = args;
    // console.log(`LRU get: ${key}`);
    const cacheKey = normalizeKey(key);
    const cached = cache.get(cacheKey);
    if (cached) return cached;

    let getResult = store.get(key, opts);
    if (getResult !== undefined) {
      getResult = getResult.then(d => {
        // Delete immediately after
        //cache.delete(cacheKey);
        return d;
      });
    }
    const result = Promise.resolve(getResult).catch((err) => {
      cache.delete(cacheKey);
      throw err;
    });
    cache.set(cacheKey, result);
    return result;
  }
  if (getRange) {
    const _getRange = getRange;
    getRange = (...args: Parameters<NonNullable<S["getRange"]>>) => {
      const [key, range, opts] = args;
      const cacheKey = normalizeKey(key, range);
      const cached = cache.get(cacheKey);
      if (cached) return cached;

      let getRangeResult = _getRange(key, range, opts);
      if (getRangeResult !== undefined) {
        getRangeResult = getRangeResult.then(d => {
          //cache.delete(cacheKey);
          return d;
        });
      }
      const result = Promise.resolve(getRangeResult).catch((err) => {
        cache.delete(cacheKey);
        throw err;
      });
      cache.set(cacheKey, result);
      return result;
    };
  }
  return new Proxy(store, {
    get(target, prop, receiver) {
      if (prop === "get") return get;
      if (prop === "getRange" && getRange) return getRange;
      return Reflect.get(target, prop, receiver);
    },
  });
}
