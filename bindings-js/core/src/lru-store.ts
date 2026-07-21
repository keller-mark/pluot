import * as zarr from "zarrita";
import type { RangeQuery, AsyncReadable, AbsolutePath } from 'zarrita';
import QuickLRU from "quick-lru";
import { assert } from './assert.js';


function normalizeKey(key: string, range?: RangeQuery): string {
  if (!range) return key;
  if ("suffixLength" in range) return `${key}:-${range.suffixLength}`;
  return `${key}:${range.offset}:${range.offset + range.length - 1}`;
}

// Provides a blanket implementation of getRange that can be used with any AsyncReadable store,
// even if it doesn't define a getRange method.
// If the store does have a native getRange method, we use that instead.
// Reference: https://github.com/vitessce/vitessce/blob/main/packages/utils/zarr-utils/src/base-getrange.ts
//
// Reference for defineStoreExtension:
// https://zarrita.dev/migration/v0.7.html
const withGetRange = zarr.defineStoreExtension(
  (innerStore) => {
    return {
      async getRange(...args: Parameters<NonNullable<typeof innerStore["getRange"]>>): Promise<Uint8Array | undefined> {
        const [key, range, opts] = args;
        if (typeof innerStore.getRange === 'function') {
          return innerStore.getRange(key, range, opts);
        }
        // Store does not have a native getRange method; falling back to get. This may be inefficient for large data.
        const arr = await innerStore.get(key, opts);
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
      }
    }
  },
)


// An adaptation of the proxy-based lru() function from vizarr.
// Reference: https://github.com/hms-dbmi/vizarr/blob/862745c1c7c095748bbe97475da61807d5b49189/src/lru-store.ts
const withLruCache = zarr.defineStoreExtension(
  (innerStore, opts: { maxSize?: number } = {}) => {
    const { maxSize = 100 } = opts;

    assert(typeof innerStore.getRange === 'function', 'The innerStore passed to withLruCache requires does not have a getRange method. Try using withGetRange.');

    // We need a way to synchronously peek at the promise state (a-la Bun's peek or Effect's Deferred.poll).
    // We can probably do something more sophisticated but will try this first.
    // TODO: should this map be stored on the Rust side instead, so that the peeking can be performed without
    // the JS function call? Instead, JS would "push" the promise states by calling a Rust function upon any
    // promise state change, via a new function exposed from Rust such as `wasm.push_promise_state(key, 'fulfilled')`
    //
    let promiseStates: Map<string, 'pending' | 'fulfilled' | 'rejected'> = new Map();
    const cache: QuickLRU<string, [Promise<Uint8Array | undefined>, AbortController]> = new QuickLRU({
      maxSize,
      onEviction: (key, _value) => {
        promiseStates.delete(key);
      },
    });

    return {
      // Return the replaced or extended store methods.

      async get(...args: Parameters<typeof innerStore["get"]>): Promise<Uint8Array | undefined> {
        const [key, opts] = args;
        // console.log(`LRU get: ${key}`);
        const cacheKey = normalizeKey(key);
        const cached = cache.get(cacheKey);
        if (cached) {
          return cached[0];
        }
        const controller = new AbortController();
        let getResult = innerStore.get(key, {
          signal: controller.signal,
          ...(opts ?? {})
        });

        const getResultPromise = Promise.resolve(getResult);
        promiseStates.set(cacheKey, 'pending');

        const result = getResultPromise.then((val) => {
          promiseStates.set(cacheKey, 'fulfilled');
          return val;
        }).catch((err) => {
          promiseStates.set(cacheKey, 'rejected');
          cache.delete(cacheKey);
          throw err;
        });
        cache.set(cacheKey, [result, controller]);
        return result;
      },

      // Synchronously peek at the promise state.
      getPeek(...args: Parameters<typeof innerStore["get"]>): 'pending' | 'fulfilled' | 'rejected' | undefined {
        this.get(...args); // Kick off the promise but do not await. TODO: do we want to do this here?
        const [key, opts] = args;
        // console.log(`LRU getPeek: ${key}`);
        const cacheKey = normalizeKey(key);
        return promiseStates.get(cacheKey);
      },

      async getRange(...args: Parameters<NonNullable<typeof innerStore["getRange"]>>): Promise<Uint8Array | undefined> {
        const [key, range, opts] = args;
        const cacheKey = normalizeKey(key, range);
        const cached = cache.get(cacheKey);
        if (cached) {
          return cached[0];
        }

        const controller = new AbortController();
        // @ts-expect-error
        let getRangeResult = innerStore.getRange(key, range, {
          signal: controller.signal,
          ...(opts ?? {})
        });

        const getRangeResultPromise = Promise.resolve(getRangeResult);
        promiseStates.set(cacheKey, 'pending');

        const result = getRangeResultPromise.then((val) => {
          promiseStates.set(cacheKey, 'fulfilled');
          return val;
        }).catch((err) => {
          promiseStates.set(cacheKey, 'rejected');
          cache.delete(cacheKey);
          throw err;
        });
        cache.set(cacheKey, [result, controller]);
        return result;
      },

      // Synchronously peek at the promise state.
      getRangePeek(...args: Parameters<NonNullable<typeof innerStore["getRange"]>>): 'pending' | 'fulfilled' | 'rejected' | undefined {
        this.getRange(...args); // Kick off the promise but do not await. TODO: do we want to do this here?
        const [key, range, opts] = args;
        const cacheKey = normalizeKey(key, range);
        return promiseStates.get(cacheKey);
      },

      clearCache() {
        // Use AbortSignal in clearCache for promises that have not yet been resolved.
        cache.forEach(([promise, controller]) => {
          // TODO: check if promise is still pending before aborting? Or just always abort?
          // TODO: verify that this aborting is actually working
          controller.abort()
        });
        cache.clear();
        promiseStates = new Map();
      },
    };
  },
);



type PromiseState = 'pending' | 'fulfilled' | 'rejected';

// The store produced by lru(): an AsyncReadable augmented with the getRange
// fallback (withGetRange) and the LRU cache methods (withLruCache).
// This is written out explicitly so the return type of lru() is nameable
// without referencing the transitive @zarrita/storage package.
export interface LruStore<S extends AsyncReadable = AsyncReadable> extends AsyncReadable {
  get(key: AbsolutePath, opts?: Parameters<AsyncReadable["get"]>[1]): Promise<Uint8Array | undefined>;
  getPeek(key: AbsolutePath, opts?: Parameters<AsyncReadable["get"]>[1]): PromiseState | undefined;
  getRange(key: AbsolutePath, range: RangeQuery, opts?: { signal?: AbortSignal }): Promise<Uint8Array | undefined>;
  getRangePeek(key: AbsolutePath, range: RangeQuery, opts?: { signal?: AbortSignal }): PromiseState | undefined;
  clearCache(): void;
}

export function lru(inner_store: AsyncReadable, maxSize = 100): LruStore {
  return zarr.extendStore(
    inner_store,
    withGetRange,
    (s) => withLruCache(s, { maxSize }),
  ) as unknown as LruStore;
}
