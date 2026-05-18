#include <Rinternals.h>
#include <string.h>
#include <stdint.h>
#include <stdlib.h>

#include "pluotr_rs/api.h"

// Call rust_render with a JSON string, return a raw vector of bytes.
SEXP render_wrapper(SEXP json_params_sexp) {
  const char *json_str = CHAR(STRING_ELT(json_params_sexp, 0));
  size_t out_len = 0;
  uint8_t *bytes = rust_render(json_str, &out_len);
  if (bytes == NULL) {
    Rf_error("pluot render failed: check JSON parameters (see stderr for details)");
    return R_NilValue;
  }
  SEXP result = PROTECT(Rf_allocVector(RAWSXP, (R_xlen_t)out_len));
  memcpy(RAW(result), bytes, out_len);
  free_bytes_from_rust(bytes, out_len);
  UNPROTECT(1);
  return result;
}

static const R_CallMethodDef CallEntries[] = {
  {"render_wrapper", (DL_FUNC) &render_wrapper, 1},
  {NULL, NULL, 0}
};

// ── Zarr store callbacks ──────────────────────────────────────────────────────
// These static functions call into R; their addresses are registered with Rust
// via pluot_init_r_zarr so the staticlib has no undefined symbols at build time.

static SEXP pluotr_ns(void) {
  static SEXP ns = NULL;
  if (!ns) {
    SEXP nm = PROTECT(Rf_mkString("pluotr"));
    ns = R_FindNamespace(nm);
    R_PreserveObject(ns);
    UNPROTECT(1);
  }
  return ns;
}

// Call an R function(store_name, key) → integer.
static int32_t r_call_int2(const char *fn_name,
                            const char *store_name, const char *key) {
  SEXP ns  = pluotr_ns();
  SEXP fn  = PROTECT(Rf_findFun(Rf_install(fn_name), ns));
  SEXP s1  = PROTECT(Rf_mkString(store_name));
  SEXP s2  = PROTECT(Rf_mkString(key));
  SEXP call = PROTECT(Rf_lang3(fn, s1, s2));
  int err = 0;
  SEXP res = PROTECT(R_tryEval(call, R_GlobalEnv, &err));
  int32_t val = err ? 2 : (int32_t)Rf_asInteger(res);
  UNPROTECT(5);
  return val;
}

// Call an R function(store_name, key, int_arg) → integer.
static int32_t r_call_int3(const char *fn_name,
                            const char *store_name, const char *key,
                            uint32_t a3) {
  SEXP ns  = pluotr_ns();
  SEXP fn  = PROTECT(Rf_findFun(Rf_install(fn_name), ns));
  SEXP s1  = PROTECT(Rf_mkString(store_name));
  SEXP s2  = PROTECT(Rf_mkString(key));
  SEXP i3  = PROTECT(Rf_ScalarInteger((int)a3));
  SEXP call = PROTECT(Rf_lang4(fn, s1, s2, i3));
  int err = 0;
  SEXP res = PROTECT(R_tryEval(call, R_GlobalEnv, &err));
  int32_t val = err ? 2 : (int32_t)Rf_asInteger(res);
  UNPROTECT(6);
  return val;
}

// Call an R function(store_name, key, int_arg, int_arg) → integer.
static int32_t r_call_int4(const char *fn_name,
                            const char *store_name, const char *key,
                            uint32_t a3, uint32_t a4) {
  SEXP ns  = pluotr_ns();
  SEXP fn  = PROTECT(Rf_findFun(Rf_install(fn_name), ns));
  SEXP s1  = PROTECT(Rf_mkString(store_name));
  SEXP s2  = PROTECT(Rf_mkString(key));
  SEXP i3  = PROTECT(Rf_ScalarInteger((int)a3));
  SEXP i4  = PROTECT(Rf_ScalarInteger((int)a4));
  SEXP call = PROTECT(Rf_lang5(fn, s1, s2, i3, i4));
  int err = 0;
  SEXP res = PROTECT(R_tryEval(call, R_GlobalEnv, &err));
  int32_t val = err ? 2 : (int32_t)Rf_asInteger(res);
  UNPROTECT(7);
  return val;
}

// Call an R function(store_name, key) → raw vector; malloc+memcpy the bytes.
static uint8_t *r_call_raw2(const char *fn_name,
                             const char *store_name, const char *key,
                             size_t *out_len) {
  SEXP ns  = pluotr_ns();
  SEXP fn  = PROTECT(Rf_findFun(Rf_install(fn_name), ns));
  SEXP s1  = PROTECT(Rf_mkString(store_name));
  SEXP s2  = PROTECT(Rf_mkString(key));
  SEXP call = PROTECT(Rf_lang3(fn, s1, s2));
  int err = 0;
  SEXP res = PROTECT(R_tryEval(call, R_GlobalEnv, &err));
  uint8_t *out = NULL;
  *out_len = 0;
  if (!err && res != R_NilValue && TYPEOF(res) == RAWSXP) {
    R_xlen_t n = XLENGTH(res);
    *out_len = (size_t)n;
    out = (uint8_t *)malloc(*out_len);
    if (out) memcpy(out, RAW(res), *out_len);
  }
  UNPROTECT(5);
  return out;
}

// Call an R function(store_name, key, int, int) → raw vector.
static uint8_t *r_call_raw4(const char *fn_name,
                             const char *store_name, const char *key,
                             uint32_t a3, uint32_t a4,
                             size_t *out_len) {
  SEXP ns  = pluotr_ns();
  SEXP fn  = PROTECT(Rf_findFun(Rf_install(fn_name), ns));
  SEXP s1  = PROTECT(Rf_mkString(store_name));
  SEXP s2  = PROTECT(Rf_mkString(key));
  SEXP i3  = PROTECT(Rf_ScalarInteger((int)a3));
  SEXP i4  = PROTECT(Rf_ScalarInteger((int)a4));
  SEXP call = PROTECT(Rf_lang5(fn, s1, s2, i3, i4));
  int err = 0;
  SEXP res = PROTECT(R_tryEval(call, R_GlobalEnv, &err));
  uint8_t *out = NULL;
  *out_len = 0;
  if (!err && res != R_NilValue && TYPEOF(res) == RAWSXP) {
    R_xlen_t n = XLENGTH(res);
    *out_len = (size_t)n;
    out = (uint8_t *)malloc(*out_len);
    if (out) memcpy(out, RAW(res), *out_len);
  }
  UNPROTECT(7);
  return out;
}

// Call an R function(store_name, key, int) → raw vector.
static uint8_t *r_call_raw3(const char *fn_name,
                             const char *store_name, const char *key,
                             uint32_t a3,
                             size_t *out_len) {
  SEXP ns  = pluotr_ns();
  SEXP fn  = PROTECT(Rf_findFun(Rf_install(fn_name), ns));
  SEXP s1  = PROTECT(Rf_mkString(store_name));
  SEXP s2  = PROTECT(Rf_mkString(key));
  SEXP i3  = PROTECT(Rf_ScalarInteger((int)a3));
  SEXP call = PROTECT(Rf_lang4(fn, s1, s2, i3));
  int err = 0;
  SEXP res = PROTECT(R_tryEval(call, R_GlobalEnv, &err));
  uint8_t *out = NULL;
  *out_len = 0;
  if (!err && res != R_NilValue && TYPEOF(res) == RAWSXP) {
    R_xlen_t n = XLENGTH(res);
    *out_len = (size_t)n;
    out = (uint8_t *)malloc(*out_len);
    if (out) memcpy(out, RAW(res), *out_len);
  }
  UNPROTECT(6);
  return out;
}

// Status functions (0 = Pending, 1 = Fulfilled, 2 = Rejected).

static int32_t r_zarr_has_status(const char *store_name, const char *key) {
  return r_call_int2("pluot_zarr_has_status", store_name, key);
}

static int32_t r_zarr_get_status(const char *store_name, const char *key) {
  return r_call_int2("pluot_zarr_get_status", store_name, key);
}

static int32_t r_zarr_get_range_from_offset_status(const char *store_name,
                                                    const char *key,
                                                    uint32_t offset,
                                                    uint32_t length) {
  return r_call_int4("pluot_zarr_get_range_from_offset_status",
                     store_name, key, offset, length);
}

static int32_t r_zarr_get_range_from_end_status(const char *store_name,
                                                 const char *key,
                                                 uint32_t suffix_length) {
  return r_call_int3("pluot_zarr_get_range_from_end_status",
                     store_name, key, suffix_length);
}

static int32_t r_zarr_has(const char *store_name, const char *key) {
  return r_call_int2("pluot_zarr_has", store_name, key);
}

static uint8_t *r_zarr_get(const char *store_name, const char *key,
                            size_t *out_len) {
  return r_call_raw2("pluot_zarr_get", store_name, key, out_len);
}

static uint8_t *r_zarr_get_range_from_offset(const char *store_name,
                                              const char *key,
                                              uint32_t offset, uint32_t length,
                                              size_t *out_len) {
  return r_call_raw4("pluot_zarr_get_range_from_offset",
                     store_name, key, offset, length, out_len);
}

static uint8_t *r_zarr_get_range_from_end(const char *store_name,
                                           const char *key,
                                           uint32_t suffix_length,
                                           size_t *out_len) {
  return r_call_raw3("pluot_zarr_get_range_from_end",
                     store_name, key, suffix_length, out_len);
}

static void r_zarr_free_bytes(uint8_t *ptr) {
  free(ptr);
}

void R_init_pluotr(DllInfo *dll) {
  R_registerRoutines(dll, NULL, CallEntries, NULL, NULL);
  R_useDynamicSymbols(dll, FALSE);

  static const RZarrCallbacks cbs = {
    r_zarr_has_status,
    r_zarr_get_status,
    r_zarr_get_range_from_offset_status,
    r_zarr_get_range_from_end_status,
    r_zarr_has,
    r_zarr_get,
    r_zarr_get_range_from_offset,
    r_zarr_get_range_from_end,
    r_zarr_free_bytes,
  };
  pluot_init_r_zarr(&cbs);
}
