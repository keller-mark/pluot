import * as zarr from "zarrita";
import { FetchStore, type AsyncReadable } from "zarrita";

// === Store metadata (`ZarrStoreInfo`) ===
//
// `RenderParams.stores` (see `crates/pluot_core/src/params.rs`) maps a store
// name to its portable metadata: the store type + params (an HTTP URL, a local
// path, ...) and any "virtual zarr" store extensions layered in front of it.
//
// These types mirror the serde JSON produced by the Rust structs. The Rust
// `ZarrStoreParams` is an adjacently-tagged enum flattened into `ZarrStoreInfo`,
// so on the wire each store looks like:
//
//   { "store_type": "HttpStore", "store_params": { "url": "..." },
//     "store_extensions": ["OmeTiffAsVirtualZarr"] }

/** Virtual-zarr wrapper stores that "virtualize" non-zarr data as zarr. */
export type ZarrStoreExtension =
  | "TiffAsVirtualZarr"
  | "OmeTiffAsVirtualZarr"
  | "Hdf5AsVirtualZarr"
  | "ParquetAsVirtualZarr"
  | "ZipAsVirtualZarr";

/** Params for an HTTP-backed store (mirrors `HttpStoreParams`). */
export type HttpStoreParams = {
  /** Absolute URL to the root of the zarr store directory. */
  url: string;
  /** Optional `fetch` request options (mirrors `RequestInit`). */
  options?: RequestInit | null;
};

/** Params for a local-directory store (mirrors `LocalStoreParams`). */
export type LocalStoreParams = {
  /** Path to the local Zarr store directory on disk. */
  path: string;
};

/** Params for an in-memory / non-portable store (mirrors `MemoryStoreParams`). */
export type MemoryStoreParams = {
  /** A human-readable message describing where the data originates. */
  message: string;
};

/**
 * Portable description of a single Zarr store (mirrors Rust `ZarrStoreInfo`).
 *
 * The `store_type` / `store_params` pair is the flattened, adjacently-tagged
 * `ZarrStoreParams`; `store_extensions` lists any wrapper stores needed in
 * front of the primitive store, outermost-last.
 */
export type ZarrStoreInfo =
  | {
      store_type: "HttpStore";
      store_params: HttpStoreParams;
      store_extensions?: ZarrStoreExtension[] | null;
    }
  | {
      store_type: "LocalStore";
      store_params: LocalStoreParams;
      store_extensions?: ZarrStoreExtension[] | null;
    }
  | {
      store_type: "MemoryStore";
      store_params: MemoryStoreParams;
      store_extensions?: ZarrStoreExtension[] | null;
    };

/**
 * The well-known property a store may expose to declare its own
 * [`ZarrStoreInfo`]. Wrapper stores (e.g. virtual-zarr extensions) set this so
 * that {@link storeInstanceToMetadata} can read metadata that already accounts
 * for the inner store plus the wrapper's extension. See {@link withStoreMetadata}.
 */
export type StoreWithMetadata = { storeMetadata: ZarrStoreInfo };

/**
 * Derive the portable [`ZarrStoreInfo`] metadata for a zarrita store instance.
 *
 * Resolution order:
 *  1. If the store exposes a `storeMetadata` property (set by an extension via
 *     {@link withStoreMetadata}), use it — it already passes through the inner
 *     store's metadata and layers on any store extension(s).
 *  2. Otherwise inspect the base store. zarrita's `FetchStore` exposes `.url`
 *     (which is delegated through the caching-extension proxies), mapping onto
 *     an `HttpStore`. A `.root`/`.path` string maps onto a `LocalStore`.
 *  3. Failing both, fall back to a `MemoryStore` descriptor: the instance is
 *     still usable at render time (it is registered by name), but its data is
 *     not reconstructable from metadata alone.
 */
export function storeInstanceToMetadata(store: AsyncReadable): ZarrStoreInfo {
  const declared = (store as Partial<StoreWithMetadata>).storeMetadata;
  if (declared && typeof declared === "object" && "store_type" in declared) {
    return declared;
  }

  // FetchStore exposes `.url`; the extension proxies delegate it through.
  const url = (store as { url?: string | URL }).url;
  if (url != null) {
    return {
      store_type: "HttpStore",
      store_params: { url: String(url) },
      store_extensions: null,
    };
  }

  const root =
    (store as { root?: unknown }).root ?? (store as { path?: unknown }).path;
  if (typeof root === "string") {
    return {
      store_type: "LocalStore",
      store_params: { path: root },
      store_extensions: null,
    };
  }

  return {
    store_type: "MemoryStore",
    store_params: {
      message: "In-memory or custom store (no portable URL/path)",
    },
    store_extensions: null,
  };
}

/**
 * A composable zarrita store extension that attaches a `storeMetadata` property
 * to a wrapper store, "passing through" the inner store's metadata and appending
 * `extension` to its `store_extensions` list.
 *
 * Virtual-zarr wrapper stores (e.g. an OME-TIFF-as-zarr adapter) should compose
 * this so that {@link storeInstanceToMetadata} reports the full store type,
 * params, and extension chain:
 *
 * ```ts
 * const store = zarr.extendStore(
 *   new FetchStore(url),
 *   asVirtualOmeTiff,
 *   withStoreMetadata("OmeTiffAsVirtualZarr"),
 * );
 * storeInstanceToMetadata(store);
 * // => { store_type: "HttpStore", store_params: { url },
 * //      store_extensions: ["OmeTiffAsVirtualZarr"] }
 * ```
 */
export function withStoreMetadata(extension: ZarrStoreExtension) {
  return zarr.defineStoreExtension((innerStore: AsyncReadable) => {
    return {
      get storeMetadata(): ZarrStoreInfo {
        const inner = storeInstanceToMetadata(innerStore);
        return {
          ...inner,
          store_extensions: [...(inner.store_extensions ?? []), extension],
        };
      },
    };
  });
}

// === Constructing store instances from metadata ===

/**
 * A store-extension applier wraps a base store to "virtualize" non-zarr data as
 * zarr (e.g. `OmeTiffAsVirtualZarr`), the inverse of {@link withStoreMetadata}.
 *
 * Appliers are opt-in so that `@pluot/core` does not have to bundle every
 * virtual-zarr implementation; register the ones you need via
 * {@link registerStoreExtension}. An applier should compose
 * `withStoreMetadata(extension)` so that the resulting store round-trips back to
 * the same metadata via {@link storeInstanceToMetadata}.
 */
export type StoreExtensionApplier = (
  store: AsyncReadable,
) => AsyncReadable;

const storeExtensionAppliers: Record<string, StoreExtensionApplier> = {};

/**
 * Register the applier used by {@link storeMetadataToInstance} to reconstruct a
 * store's `ZarrStoreExtension` wrapper (e.g. "OmeTiffAsVirtualZarr").
 */
export function registerStoreExtension(
  extension: ZarrStoreExtension,
  applier: StoreExtensionApplier,
) {
  storeExtensionAppliers[extension] = applier;
}

/**
 * Construct a concrete zarrita store instance from its portable
 * [`ZarrStoreInfo`] metadata — the inverse of {@link storeInstanceToMetadata}.
 *
 *  - `HttpStore` -> a zarrita `FetchStore` (its `options` become `fetch`
 *    `overrides`).
 *  - `LocalStore` -> a `FetchStore` over the path (browsers have no filesystem;
 *    the path is treated as a relative URL served alongside the app).
 *  - `MemoryStore` -> throws: an in-memory store has no portable representation
 *    and must be provided directly (e.g. via `setStoreByName`).
 *
 * Any `store_extensions` are then applied outermost-last using the appliers
 * registered via {@link registerStoreExtension}.
 */
export function storeMetadataToInstance(
  info: ZarrStoreInfo,
): AsyncReadable {
  let store: AsyncReadable;
  switch (info.store_type) {
    case "HttpStore":
      store = new FetchStore(
        info.store_params.url,
        info.store_params.options
          ? { overrides: info.store_params.options }
          : undefined,
      );
      break;
    case "LocalStore":
      store = new FetchStore(info.store_params.path);
      break;
    case "MemoryStore":
      throw new Error(
        `Cannot reconstruct an in-memory store (${JSON.stringify(
          info.store_params.message ?? "",
        )}) from metadata; provide the store instance directly (e.g. via setStoreByName).`,
      );
    default:
      throw new Error(
        `Unknown store_type: ${(info as { store_type?: string }).store_type}`,
      );
  }

  for (const ext of info.store_extensions ?? []) {
    const applier = storeExtensionAppliers[ext];
    if (!applier) {
      throw new Error(
        `No applier registered for store extension "${ext}". ` +
          `Register one via registerStoreExtension("${ext}", applier).`,
      );
    }
    // TODO: support async applier functions? but that will require many changes downstream
    store = applier(store);
  }
  return store;
}
