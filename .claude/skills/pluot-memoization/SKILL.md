---
name: pluot-memoization
description: Use when writing layer prepare functions which load data asynchronously or otherwise need to cache or memoize results.
---

Pluot layers re-run `prepare()` on every render (every pan, zoom, brush move, or parameter change).
While the prepare function is intended to be where data is loaded/transformed, we must use memoization to avoid re-fetching or re-computing the same information on every rendered frame.
To memoize, we use `use_memo_*` functions in `crates/pluot_core/src/cache.rs`, modeled on React's `useMemo` hook.
When the memoization logic gets complex, we can extract this logic into custom `use_*` functions, analogous to the pattern of custom hook function development in the React ecosystem.

## Cache keys

Think of cache keys as being analogous to queryKeys from TanStack Query / React Query or as being analogous to React hook dependency arrays.

Typically cache keys will contain a namespace string like "zarr_numeric_data_arr", then the store name, then the array paths / indices / parameters the value actually depends on.

Omit `layer_id` when the value doesn't depend on the layer. Keying a loaded zarr array by layer means two layers reading the same column would each need to re-fetch it unnecessarily.

Prefer explicitly listing properties in cache keys as opposed to converting structs to strings wholesale. When cache key arrays become unruly, extract the key construction logic into a function.


## Eagerly split operations into independent memos when values are not dependent on one another

Give each loaded data array or derived value its own memo keyed by only what it needs.

### Cache the pieces, not the assembly

For example, `OmeZarrBitmapLayer::load_tile_data` caches each channel's slice under its own key
and deliberately does **not** cache the concatenated
multi-channel tile. Changing one channel's index re-fetches one channel; the concatenation is
cheap and re-running it avoids storing the same bytes twice. Prefer this shape: memoize the
expensive I/O and GPU reductions at their natural granularity, and recompute cheap glue.


## Leverage nested `use_memo_*` calls

A memo's initializer may call other memos, including memos of the same type. This is the main
tool for making a coarse-grained result and its expensive ingredients invalidate
independently.

For example, `ZarrHistogramLayer::prepare` nests three levels deep:

```rust
let background_future = use_memo_vec_f32(async || {
    // 1. the raw column, keyed by (store, path) alone
    let quant_arr = load_arr_as_numeric_data_memoized(
        store.clone(), &self.store_name, &self.layer_params.data_key, quant_cache_enabled,
    ).await?;

    // 2. the criteria arrays, each independently memoized by (store, path)
    let filtering_criteria = resolve_zarr_emphasis_criteria(
        store.clone(), &self.layer_params.filtering_criteria,
        &self.store_name, self.view_params.cache_enabled,
    ).await?;

    // Clone the Arc-backed handles the inner closure needs, before it takes them.
    let quant_arr_for_extent = quant_arr.as_ref().clone();
    let filtering_criteria_for_extent = filtering_criteria.clone();

    // 3. the extent — same memo type as the outer call, a different key
    let extent = use_memo_vec_f32(async || {
        let (lo, hi) = reduce_extent(gpu_context, quant_arr_for_extent, &filtering_criteria_for_extent, &[]).await.background;
        Ok::<Vec<f32>, std::convert::Infallible>(vec![lo, hi])
    }, &extent_future_deps, self.view_params.cache_enabled).await.expect("...");

    // ... bin the column using that extent, and return [min, max, counts...]
}, &background_future_deps, self.view_params.cache_enabled);
```

Rules for nesting:

- **The outer key must cover every param that reaches the outer result**, including the ones
  the inner memos are keyed by. Above, `background_future_deps` carries the data key, the
  filtering criteria, *and* `num_bins`. If an inner dependency were missing from the outer key,
  the outer entry would be served stale after that dependency changed.
- **The inner key must not pick up outer-only params.** `extent_future_deps` omits `num_bins`,
  because the extent doesn't depend on the bin count — so changing `num_bins` re-runs the
  binning but reuses the extent (and the loaded array).
- **Pass `cache_enabled` down unchanged** so disabling the cache disables the whole nest. The
  one exception is a narrowing flag like `quant_cache_enabled`
  (`cache_enabled && layer_params.cache_data`), which lets a layer keep the small derived
  result while declining to hold the full array.

Prefer nesting because a nested call is skipped entirely on an outer hit.


## Concurrency

Memo calls return futures. Build them all first, then await them together with
`futures::try_join!` (`ZarrPointLayer`) or `futures::future::join_all`
(`resolve_zarr_emphasis_criteria`, the per-channel fetches), wrapping each in `maybe_timeout!`
with `view_params.timeout`.

## Extract complex caching into its own `use_*` hook

When a `use_memo` operation (initializer function body) is more than a few lines, or is needed in more than one place, move it out of `prepare()` into a custom hook function.

## Bailing out vs. rendering partial results

When a memo can't complete in time, decide per-value whether the layer should bail or degrade:

- If the layer can render a partial result (e.g., certain sub-layers), then instantiate these sublayers and prepare them (call `sublayer.prepare()`), but still return `bailed_early: true` at the conclusion of the prepare function so that the client will know to trigger a re-render.
- If the layer cannot render even a partial result due to the lack of a value, then we can simply return early with `bailed_early: true`


## Prefer progressive loading via per-chunk sublayers

A memo is all-or-nothing per key, so **the granularity at which data can appear on screen is
the granularity of the cache keys, which in turn should be the granularity of the sublayers.**
A layer that loads its whole dataset under one key can only show everything or nothing. A layer
that partitions the data into independently-keyed chunks, each owned by its own sublayer, can
draw whatever has arrived and fill in the rest on later frames.

`OmeZarrBitmapMultiscaleLayer` is the reference implementation:

1. **Metadata first, memoized separately.** `load_metadata` fetches the OME-Zarr group once
   under `(store_name, group_path, multiscale_{i})` and caches the metadata.
2. **Sublayer construction is synchronous and loads nothing.** `build_sublayers` constructs one `OmeZarrBitmapLayer` per visible tile.
3. **Each chunk fetches and caches itself.** Each tile sublayer's own `prepare` →
   `load_tile_data` memoizes per `(store, array_path, slice_x, slice_y, z, t, channel)`. Pan
   and zoom therefore reuse every tile already loaded, at every level, and only genuinely new
   tiles hit the store.
4. **Prepare all chunks concurrently, time out once.** Nested `join_all` (tiles within a level,
   levels within the layer) wrapped in a single `maybe_timeout!`. Use `join_all`, not
   `try_join_all` — one slow or failing tile must not cancel its siblings.
5. **Parent layer aggregates sublayer bailed_early flags.** Each sublayer's `PrepareResult` is stored in
   `LevelSublayers::prepare_results`; the parent aggregates `any_bailed` into its own
   `bailed_early` (a "re-render me" signal) but keeps everything that did load. A sublayer that
   never got data has its own internal logic to simply draw nothing.

Applying the pattern elsewhere: **This generalizes past images.** Any layer whose data partitions — row groups of a dataframe, genes in a dotplot — can adopt it.
