#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

void free_string_from_rust(char*);
char * rust_roundtrip(void);

uint8_t * rust_render(const char *json_params, size_t *out_len);
void free_bytes_from_rust(uint8_t *ptr, size_t len);

#ifdef __cplusplus
}
#endif
