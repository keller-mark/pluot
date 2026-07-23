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

# Best-effort extraction of a URL / path from a pizzarr store instance.
.pluot_store_url <- function(store) {
  url <- tryCatch(store[["url"]], error = function(e) NULL)
  if (is.character(url) && length(url) == 1) return(url)
  NULL
}

.pluot_store_path <- function(store) {
  for (field in c("root", "path", "dir")) {
    val <- tryCatch(store[[field]], error = function(e) NULL)
    if (is.character(val) && length(val) == 1) return(val)
  }
  NULL
}

#' Derive portable ZarrStoreInfo metadata from a pizzarr store instance.
#'
#' Mirrors the Rust `ZarrStoreInfo` JSON (see
#' `crates/pluot_core/src/params.rs`): a `store_type` / `store_params` pair plus
#' an optional `store_extensions` list.
#'
#' Resolution order: a wrapper store may declare its own metadata via a
#' `store_metadata` field (which passes the inner store's metadata through and
#' layers on any extension); otherwise a URL yields an `HttpStore`, a
#' filesystem path yields a `LocalStore`, and anything else falls back to a
#' `MemoryStore` descriptor (the instance is still usable at render time because
#' it is registered by name).
#'
#' @param store A pizzarr store object.
#' @return A named list matching the `ZarrStoreInfo` JSON shape.
#' @export
store_instance_to_metadata <- function(store) {
  declared <- tryCatch(store[["store_metadata"]], error = function(e) NULL)
  if (is.list(declared) && !is.null(declared[["store_type"]])) {
    return(declared)
  }

  url <- .pluot_store_url(store)
  if (!is.null(url)) {
    return(list(
      store_type = "HttpStore",
      store_params = list(url = url),
      store_extensions = NULL
    ))
  }

  path <- .pluot_store_path(store)
  if (!is.null(path)) {
    return(list(
      store_type = "LocalStore",
      store_params = list(path = path),
      store_extensions = NULL
    ))
  }

  list(
    store_type = "MemoryStore",
    store_params = list(
      message = paste0("In-memory or custom store (", class(store)[1], ")")
    ),
    store_extensions = NULL
  )
}

# Registry of store-extension appliers used by store_metadata_to_instance to
# reconstruct virtual-zarr wrapper stores (inverse of the store_extensions
# recorded by store_instance_to_metadata).
.pluot_store_ext_appliers <- new.env(parent = emptyenv())

#' Register the applier used to reconstruct a `ZarrStoreExtension` wrapper.
#'
#' @param extension Character string naming the extension (e.g.
#'   "OmeTiffAsVirtualZarr").
#' @param applier A function taking a base pizzarr store and returning a wrapped
#'   store.
#' @export
pluot_register_store_extension <- function(extension, applier) {
  assign(extension, applier, envir = .pluot_store_ext_appliers)
  invisible(NULL)
}

#' Construct a pizzarr store instance from ZarrStoreInfo metadata.
#'
#' The inverse of [store_instance_to_metadata()]:
#' \itemize{
#'   \item `HttpStore` -> a pizzarr `HttpStore` for the URL;
#'   \item `LocalStore` -> a pizzarr `DirectoryStore` for the path;
#'   \item `MemoryStore` -> errors (an in-memory store has no portable
#'     representation and must be provided directly).
#' }
#' Any `store_extensions` are then applied outermost-last using appliers
#' registered via [pluot_register_store_extension()].
#'
#' @param info A named list matching the `ZarrStoreInfo` JSON shape.
#' @return A pizzarr store object.
#' @export
store_metadata_to_instance <- function(info) {
  store_type <- info[["store_type"]]
  params <- info[["store_params"]]

  store <- if (identical(store_type, "HttpStore")) {
    pizzarr::HttpStore$new(params[["url"]])
  } else if (identical(store_type, "LocalStore")) {
    pizzarr::DirectoryStore$new(params[["path"]])
  } else if (identical(store_type, "MemoryStore")) {
    stop(
      "Cannot reconstruct an in-memory store from metadata (",
      params[["message"]], "); provide the store instance directly."
    )
  } else {
    stop("Unknown store_type: ", store_type)
  }

  for (ext in info[["store_extensions"]]) {
    if (!exists(ext, envir = .pluot_store_ext_appliers, inherits = FALSE)) {
      stop(
        "No applier registered for store extension '", ext,
        "'. Register one via pluot_register_store_extension()."
      )
    }
    applier <- get(ext, envir = .pluot_store_ext_appliers, inherits = FALSE)
    store <- applier(store)
  }
  store
}

# Build the top-level `stores` metadata map that RenderParams expects.
#
# Accepts any combination of:
#   - `stores`, a named list mapping store names to either a pizzarr store
#     instance or an already-derived `ZarrStoreInfo` metadata list; and/or
#   - `store` (optionally named via `store_name`), a single pizzarr store
#     instance or `ZarrStoreInfo` list; and/or
#   - `store_name` alone, referencing a store previously registered via
#     `pluot_register_store()`.
# Store instances are registered (so the bound functions can reach them) and
# their metadata derived; `ZarrStoreInfo` lists pass through as-is.
.pluot_build_stores <- function(stores = NULL, store = NULL, store_name = NULL) {
  stores_meta <- list()

  if (!is.null(stores)) {
    for (nm in names(stores)) {
      val <- stores[[nm]]
      if (is.list(val) && !is.null(val[["store_type"]])) {
        # Already-derived ZarrStoreInfo metadata.
        stores_meta[[nm]] <- val
      } else {
        pluot_register_store(nm, val)
        stores_meta[[nm]] <- store_instance_to_metadata(val)
      }
    }
  }

  if (!is.null(store)) {
    name <- if (!is.null(store_name)) store_name else "default"
    if (is.list(store) && !is.null(store[["store_type"]])) {
      # Already-derived ZarrStoreInfo metadata; no live instance to register.
      stores_meta[[name]] <- store
    } else {
      pluot_register_store(name, store)
      stores_meta[[name]] <- store_instance_to_metadata(store)
    }
  } else if (!is.null(store_name)) {
    # No `store` instance/metadata given: treat `store_name` as referencing a
    # store already registered via `pluot_register_store()`.
    registered <- tryCatch(
      get(store_name, envir = .pluot_stores, inherits = FALSE),
      error = function(e) NULL
    )
    if (!is.null(registered)) {
      stores_meta[[store_name]] <- store_instance_to_metadata(registered)
    }
  }

  if (length(stores_meta) == 0) return(NULL)
  stores_meta
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
