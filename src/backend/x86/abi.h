#ifndef WAND_BACKEND_X86_ABI_H
#define WAND_BACKEND_X86_ABI_H

#include <stdint.h>
#include <stddef.h>

typedef struct {
    uint8_t* data;
    size_t size;
} WandBinary;

WandBinary wand_x86_pack_image(const uint8_t* input, size_t input_len);
void wand_binary_free(WandBinary* binary);

#endif
