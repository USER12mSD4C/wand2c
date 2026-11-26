#ifndef WAND_P46_H
#define WAND_P46_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Status values mirror the p46_status enum in standart-46.md, plus a few
 * continuation-object specific codes at the end. */
typedef enum {
    WAND_P46_OK = 0,
    WAND_P46_ERR_BAD_MAGIC = 1,
    WAND_P46_ERR_UNSUPPORTED_FORMAT_MAJOR = 2,
    WAND_P46_ERR_UNSUPPORTED_POINTER_SIZE = 3,
    WAND_P46_ERR_UNSUPPORTED_ADDRESS_SIZE = 4,
    WAND_P46_ERR_ENDIANNESS_MISMATCH = 5,
    WAND_P46_ERR_MISSING_DEPENDENCY = 6,
    WAND_P46_ERR_VERSION_MISMATCH = 7,
    WAND_P46_ERR_TRUNCATED = 8,
    WAND_P46_ERR_TRUNCATED_SECTION = 9,
    WAND_P46_ERR_OVERLAPPING_SECTIONS = 10,
    WAND_P46_ERR_HEADER_TOO_SMALL = 11,
    WAND_P46_ERR_CORRUPT_TYPES = 12,
    WAND_P46_ERR_CORRUPT_STRTAB = 13,
    WAND_P46_ERR_OUT_OF_MEMORY = 14,
    WAND_P46_ERR_IO = 15,
    WAND_P46_ERR_NO_CONTINUATION = 16,
    WAND_P46_ERR_UNRESOLVED_SYMBOL = 17,
    WAND_P46_ERR_BAD_RELOCATION = 18,
    WAND_P46_ERR_TOO_MANY_ARGUMENTS = 19
} WandP46Status;

typedef struct WandP46Module WandP46Module;

/* Resolver callback. Return the host address of `symbol`, or NULL if the
 * import cannot be satisfied. `version` is the required symbol version. */
typedef void* (*WandP46ResolveFn)(const char* module, const char* symbol,
                                  uint32_t version, void* ctx);

/* Dependency callback. Return 0 when the dependency is satisfied. */
typedef int (*WandP46DependencyFn)(const char* name, uint32_t major,
                                   uint32_t minor, uint32_t patch,
                                   uint32_t build, void* ctx);

typedef struct {
    WandP46ResolveFn resolve;
    WandP46DependencyFn dependency;
    void* ctx;
} WandP46Options;

/* Load a flat Standard 4/6 continuation image. The input buffer is copied;
 * the caller may free it as soon as this call returns. */
WandP46Module* wand_p46_load(const void* data, size_t size,
                             const WandP46Options* options,
                             WandP46Status* out_status);

/* Look up an exported function by name and minimum version. */
WandP46Status wand_p46_get_symbol(WandP46Module* module, const char* name,
                                  uint32_t required_version, void** out_address);

/* Call the module's `main` continuation. Wand passes up to six 64-bit values
 * in registers and returns up to four 64-bit values in rax/rdx/rcx/r8. */
WandP46Status wand_p46_call(WandP46Module* module, const uint64_t* args,
                            size_t argc, uint64_t* results, size_t result_cap,
                            size_t* out_result_count);

void wand_p46_unload(WandP46Module* module);

const char* wand_p46_status_string(WandP46Status status);

/* Hosted convenience registry used by the `jmpto` fallback. */
WandP46Status wand_p46_register(const char* name, WandP46Module* module);
WandP46Status wand_p46_register_path(const char* name, const char* path,
                                     const WandP46Options* options);
uint64_t __wand_jmpto_loader(const char* module_name, uint64_t* args,
                             uint64_t argc, uint64_t* results,
                             uint64_t result_cap);

#ifdef __cplusplus
}
#endif

#endif
