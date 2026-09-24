#ifndef WAND_BACKEND_X86_CODEGEN_H
#define WAND_BACKEND_X86_CODEGEN_H

#include "../wand_backend.h"

WandBinary wand_x86_compile_program(
    const uint8_t* program_buf,
    size_t program_len,
    WandOutputFormat format,
    const uint8_t* entry_name,
    size_t entry_len
);

void wand_binary_free(WandBinary* binary);

#endif
