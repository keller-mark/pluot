#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

uint8_t * rust_render(const char *json_params, size_t *out_len);
void free_bytes_from_rust(uint8_t *ptr, size_t len);

typedef struct {
    int32_t  (*has_status)(const char *store_name, const char *key);
    int32_t  (*get_status)(const char *store_name, const char *key);
    int32_t  (*get_range_from_offset_status)(const char *store_name, const char *key,
                                             uint32_t offset, uint32_t length);
    int32_t  (*get_range_from_end_status)(const char *store_name, const char *key,
                                          uint32_t suffix_length);
    int32_t  (*has)(const char *store_name, const char *key);
    uint8_t *(*get)(const char *store_name, const char *key, size_t *out_len);
    uint8_t *(*get_range_from_offset)(const char *store_name, const char *key,
                                      uint32_t offset, uint32_t length, size_t *out_len);
    uint8_t *(*get_range_from_end)(const char *store_name, const char *key,
                                   uint32_t suffix_length, size_t *out_len);
    void     (*free_bytes)(uint8_t *ptr);
} RZarrCallbacks;

void pluot_init_r_zarr(const RZarrCallbacks *callbacks);

#ifdef __cplusplus
}
#endif
