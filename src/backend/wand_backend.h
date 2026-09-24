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

#endif
