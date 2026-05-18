#include <Rinternals.h>
#include <string.h>
#include <stdint.h>

// Import C headers for rust API
#include "pluotr_rs/api.h"

// Helper to call R from Rust
const char* call_r_info_helper(void) {
    SEXP fun = PROTECT(Rf_lang1(Rf_install("hello_from_r")));
    SEXP res = PROTECT(Rf_eval(fun, R_GlobalEnv));
    const char *s = CHAR(STRING_ELT(res, 0));
    char *res_copy = R_alloc(strlen(s) + 1, sizeof(char));
    strcpy(res_copy, s);
    UNPROTECT(2);
    return res_copy;
}

// Helper to call Rust from R
SEXP roundtrip_wrapper(void){
  char* res_str = rust_roundtrip();
  SEXP res = PROTECT(Rf_mkCharCE(res_str, CE_UTF8));
  free_string_from_rust(res_str);
  UNPROTECT(1);
  return Rf_ScalarString(res);
}

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

// Standard R package stuff
static const R_CallMethodDef CallEntries[] = {
  {"roundtrip_wrapper", (DL_FUNC) &roundtrip_wrapper, 0},
  {"render_wrapper",    (DL_FUNC) &render_wrapper,    1},
  {NULL, NULL, 0}
};

void R_init_pluotr(DllInfo *dll) {
  R_registerRoutines(dll, NULL, CallEntries, NULL, NULL);
  R_useDynamicSymbols(dll, FALSE);
}
