#ifndef WAND_BACKEND_H
#define WAND_BACKEND_H

#include <stdint.h>
#include <stddef.h>

typedef enum {
    WAND_OUTPUT_PROGRAM = 0,
    WAND_OUTPUT_OBJECT = 1,
    WAND_OUTPUT_RAW = 2,
    WAND_OUTPUT_KERNEL = 3,
    WAND_OUTPUT_WEXP = 4
} WandOutputFormat;

typedef struct {
    uint8_t* data;
    size_t size;
} WandBinary;

/* Metadata returned by the code generator. Strings are owned by the result. */
typedef struct {
    char* name;
    uint64_t offset;
} WandSymbol;

typedef struct {
    uint64_t position;
    char* target;
} WandCallPatch;

typedef struct {
    uint8_t* data;
    size_t size;
    WandSymbol* functions;
    size_t function_count;
    WandCallPatch* call_patches;
    size_t call_patch_count;
    uint32_t error; /* 0 = ok, 1 = malformed input, 2 = out of memory, 3 = depth limit */
} WandCompileResult;

void wand_compile_result_free(WandCompileResult* result);

#endif
