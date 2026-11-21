#ifndef WAND_BACKEND_X86_CODEGEN_H
#define WAND_BACKEND_X86_CODEGEN_H

#include "../wand_backend.h"

/*
 * Compiles a serialized Wand Program (see src/codegen.rs for the wire format)
 * into an x86-64 image. The returned result owns its data and metadata;
 * release it with wand_compile_result_free().
 */
WandCompileResult wand_x86_compile_program(
    const uint8_t* program_buf,
    size_t program_len,
    WandOutputFormat format,
    const uint8_t* entry_name,
    size_t entry_len
);

#endif
