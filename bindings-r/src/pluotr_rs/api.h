#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

void free_string_from_rust(char*);
char * rust_roundtrip(void);

#ifdef __cplusplus
}
#endif
