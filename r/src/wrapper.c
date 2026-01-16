#include <Rinternals.h>
#include <string.h>

// Import C headers for rust API
#include "pluotr/api.h"

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

// Actual Wrappers
SEXP hello_wrapper(void){
  char* hello_rust = string_from_rust_async();
  SEXP hello_world_string = PROTECT(Rf_mkCharCE(hello_rust, CE_UTF8));
  free_string_from_rust(hello_rust);
  UNPROTECT(1);
  return Rf_ScalarString(hello_world_string);
}


SEXP roundtrip_wrapper(void){
  char* res_str = rust_roundtrip();
  SEXP res = PROTECT(Rf_mkCharCE(res_str, CE_UTF8));
  free_string_from_rust(res_str);
  UNPROTECT(1);
  return Rf_ScalarString(res);
}

// Standard R package stuff
static const R_CallMethodDef CallEntries[] = {
  {"hello_wrapper", (DL_FUNC) &hello_wrapper, 0},
  {"roundtrip_wrapper", (DL_FUNC) &roundtrip_wrapper, 0},
  {NULL, NULL, 0}
};

void R_init_hellorust(DllInfo *dll) {
  R_registerRoutines(dll, NULL, CallEntries, NULL, NULL);
  R_useDynamicSymbols(dll, FALSE);
}
