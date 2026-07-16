.pluot_stores <- new.env(parent = emptyenv())
.pluot_cache  <- new.env(parent = emptyenv())

#' Register a pizzarr store for use with pluot renderers.
#'
#' @param name Character string; the store name referenced in layer parameters.
#' @param store A pizzarr store object (e.g. MemoryStore, DirectoryStore).
#' @export
pluot_register_store <- function(name, store) {
  assign(name, store, envir = .pluot_stores)
  invisible(NULL)
}

# Cache-key helpers (same scheme as Python zarr.py)
.has_cache_key <- function(store_name, key) {
  paste0("has:", store_name, ":", key)
}
.get_cache_key <- function(store_name, key) {
  paste0("get:", store_name, ":", key)
}
.range_offset_cache_key <- function(store_name, key, offset, length) {
  paste0("roff:", store_name, ":", key, ":", offset, ":", length)
}
.range_end_cache_key <- function(store_name, key, suffix_length) {
  paste0("rend:", store_name, ":", key, ":", suffix_length)
}

.peek_status <- function(cache_key) {
  if (!exists(cache_key, envir = .pluot_cache, inherits = FALSE)) return(0L)
  val <- get(cache_key, envir = .pluot_cache, inherits = FALSE)
  if (inherits(val, "error") || inherits(val, "condition")) return(2L)
  1L
}

# Status functions. Because R is single-threaded, the first call always
# performs the synchronous fetch and caches the result.  Status is therefore
# never Pending; it is always Fulfilled or Rejected.

pluot_zarr_has_status <- function(store_name, key) {
  cache_key <- .has_cache_key(store_name, key)
  if (!exists(cache_key, envir = .pluot_cache, inherits = FALSE)) {
    store  <- get(store_name, envir = .pluot_stores, inherits = FALSE)
    result <- tryCatch(store$contains_item(key), error = function(e) e)
    assign(cache_key, result, envir = .pluot_cache)
  }
  .peek_status(cache_key)
}

pluot_zarr_get_status <- function(store_name, key) {
  cache_key <- .get_cache_key(store_name, key)
  if (!exists(cache_key, envir = .pluot_cache, inherits = FALSE)) {
    store  <- get(store_name, envir = .pluot_stores, inherits = FALSE)
    result <- tryCatch(store$get_item(key), error = function(e) e)
    assign(cache_key, result, envir = .pluot_cache)
  }
  .peek_status(cache_key)
}

pluot_zarr_get_range_from_offset_status <- function(store_name, key,
                                                    offset, length) {
  cache_key <- .range_offset_cache_key(store_name, key, offset, length)
  if (!exists(cache_key, envir = .pluot_cache, inherits = FALSE)) {
    store  <- get(store_name, envir = .pluot_stores, inherits = FALSE)
    # offset is 0-based; R sequences are 1-based
    result <- tryCatch({
      full <- store$get_item(key)
      full[(offset + 1L):(offset + length)]
    }, error = function(e) e)
    assign(cache_key, result, envir = .pluot_cache)
  }
  .peek_status(cache_key)
}

pluot_zarr_get_range_from_end_status <- function(store_name, key,
                                                 suffix_length) {
  cache_key <- .range_end_cache_key(store_name, key, suffix_length)
  if (!exists(cache_key, envir = .pluot_cache, inherits = FALSE)) {
    store  <- get(store_name, envir = .pluot_stores, inherits = FALSE)
    result <- tryCatch({
      full <- store$get_item(key)
      utils::tail(full, suffix_length)
    }, error = function(e) e)
    assign(cache_key, result, envir = .pluot_cache)
  }
  .peek_status(cache_key)
}

# Data fetch functions. Read from the cache populated by the status calls.

pluot_zarr_has <- function(store_name, key) {
  cache_key <- .has_cache_key(store_name, key)
  if (!exists(cache_key, envir = .pluot_cache, inherits = FALSE)) {
    pluot_zarr_has_status(store_name, key)
  }
  val <- get(cache_key, envir = .pluot_cache, inherits = FALSE)
  if (inherits(val, "error")) stop(val)
  isTRUE(val)
}

pluot_zarr_get <- function(store_name, key) {
  cache_key <- .get_cache_key(store_name, key)
  if (!exists(cache_key, envir = .pluot_cache, inherits = FALSE)) {
    pluot_zarr_get_status(store_name, key)
  }
  val <- get(cache_key, envir = .pluot_cache, inherits = FALSE)
  if (inherits(val, "error")) stop(val)
  val
}

pluot_zarr_get_range_from_offset <- function(store_name, key, offset, length) {
  cache_key <- .range_offset_cache_key(store_name, key, offset, length)
  if (!exists(cache_key, envir = .pluot_cache, inherits = FALSE)) {
    pluot_zarr_get_range_from_offset_status(store_name, key, offset, length)
  }
  val <- get(cache_key, envir = .pluot_cache, inherits = FALSE)
  if (inherits(val, "error")) stop(val)
  val
}

pluot_zarr_get_range_from_end <- function(store_name, key, suffix_length) {
  cache_key <- .range_end_cache_key(store_name, key, suffix_length)
  if (!exists(cache_key, envir = .pluot_cache, inherits = FALSE)) {
    pluot_zarr_get_range_from_end_status(store_name, key, suffix_length)
  }
  val <- get(cache_key, envir = .pluot_cache, inherits = FALSE)
  if (inherits(val, "error")) stop(val)
  val
}
