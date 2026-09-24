#include "codegen.h"
#include <stdlib.h>
#include <string.h>

typedef struct {
    uint8_t* code;
    size_t len;
    size_t cap;
} CodeBuf;

static void cb_init(CodeBuf* cb) {
    cb->cap = 65536;
    cb->len = 0;
    cb->code = malloc(cb->cap);
    if (!cb->code) {
        exit(1);
    }
}

static void cb_reserve(CodeBuf* cb, size_t extra) {
    if (cb->len + extra > cb->cap) {
        size_t new_cap = cb->cap * 2;
        if (new_cap < cb->len + extra) {
            new_cap = cb->len + extra;
        }
        uint8_t* new_buf = realloc(cb->code, new_cap);
        if (!new_buf) {
            exit(1);
        }
        cb->code = new_buf;
        cb->cap = new_cap;
    }
}

static void emit_u8(CodeBuf* cb, uint8_t b) {
    cb_reserve(cb, 1);
    cb->code[cb->len++] = b;
}

static void emit_u16(CodeBuf* cb, uint16_t v) {
    cb_reserve(cb, 2);
    cb->code[cb->len++] = (uint8_t)(v & 0xFF);
    cb->code[cb->len++] = (uint8_t)((v >> 8) & 0xFF);
}

static void emit_u32(CodeBuf* cb, uint32_t v) {
    cb_reserve(cb, 4);
    cb->code[cb->len++] = (uint8_t)(v & 0xFF);
    cb->code[cb->len++] = (uint8_t)((v >> 8) & 0xFF);
    cb->code[cb->len++] = (uint8_t)((v >> 16) & 0xFF);
    cb->code[cb->len++] = (uint8_t)((v >> 24) & 0xFF);
}

static void emit_u64(CodeBuf* cb, uint64_t v) {
    cb_reserve(cb, 8);
    for (int i = 0; i < 8; i++) {
        cb->code[cb->len++] = (uint8_t)((v >> (i * 8)) & 0xFF);
    }
}

static void emit_bytes(CodeBuf* cb, const uint8_t* data, size_t n) {
    cb_reserve(cb, n);
    memcpy(cb->code + cb->len, data, n);
    cb->len += n;
}

static void patch_i32(CodeBuf* cb, size_t pos, int32_t offset) {
    cb->code[pos + 0] = (uint8_t)(offset & 0xFF);
    cb->code[pos + 1] = (uint8_t)((offset >> 8) & 0xFF);
    cb->code[pos + 2] = (uint8_t)((offset >> 16) & 0xFF);
    cb->code[pos + 3] = (uint8_t)((offset >> 24) & 0xFF);
}

static void emit_mov_imm64(CodeBuf* cb, uint8_t reg, uint64_t value) {
    if (value <= 0xFFFFFFFF) {
        if (reg >= 8) {
            emit_u8(cb, 0x41);
            emit_u8(cb, 0xB8 + (reg & 7));
        } else {
            emit_u8(cb, 0xB8 + reg);
        }
        emit_u32(cb, (uint32_t)value);
    } else {
        if (reg >= 8) {
            emit_u8(cb, 0x49);
            emit_u8(cb, 0xB8 + (reg & 7));
        } else {
            emit_u8(cb, 0x48);
            emit_u8(cb, 0xB8 + reg);
        }
        emit_u64(cb, value);
    }
}

static void emit_mem_op(CodeBuf* cb, uint8_t opcode, uint8_t reg, uint32_t offset) {
    uint8_t low = reg & 7;
    uint8_t rex = (reg >= 8) ? 0x4C : 0x48;
    emit_u8(cb, rex);
    emit_u8(cb, opcode);
    int32_t neg = -(int32_t)offset;
    if (neg >= -128 && neg <= 127) {
        emit_u8(cb, 0x45 | (low << 3));
        emit_u8(cb, (uint8_t)neg);
    } else {
        emit_u8(cb, 0x85 | (low << 3));
        emit_u32(cb, (uint32_t)neg);
    }
}

static void emit_mem_load(CodeBuf* cb, uint8_t reg, uint32_t offset, uint32_t size) {
    uint8_t low = reg & 7;
    int32_t neg = -(int32_t)offset;
    uint8_t modrm;
    if (neg >= -128 && neg <= 127) {
        modrm = 0x45 | (low << 3);
    } else {
        modrm = 0x85 | (low << 3);
    }

    switch (size) {
        case 1: {
            uint8_t rex = (reg >= 8) ? 0x4C : 0x48;
            emit_u8(cb, rex);
            emit_u8(cb, 0x0F);
            emit_u8(cb, 0xB6);
            emit_u8(cb, modrm);
            break;
        }
        case 2: {
            uint8_t rex = (reg >= 8) ? 0x4C : 0x48;
            emit_u8(cb, rex);
            emit_u8(cb, 0x0F);
            emit_u8(cb, 0xB7);
            emit_u8(cb, modrm);
            break;
        }
        case 4: {
            if (reg >= 8) {
                emit_u8(cb, 0x44);
            }
            emit_u8(cb, 0x8B);
            emit_u8(cb, modrm);
            break;
        }
        default: {
            uint8_t rex = (reg >= 8) ? 0x4C : 0x48;
            emit_u8(cb, rex);
            emit_u8(cb, 0x8B);
            emit_u8(cb, modrm);
            break;
        }
    }

    if (neg >= -128 && neg <= 127) {
        emit_u8(cb, (uint8_t)neg);
    } else {
        emit_u32(cb, (uint32_t)neg);
    }
}

static void emit_mem_store(CodeBuf* cb, uint8_t reg, uint32_t offset, uint32_t size) {
    uint8_t low = reg & 7;
    int32_t neg = -(int32_t)offset;
    uint8_t modrm;
    if (neg >= -128 && neg <= 127) {
        modrm = 0x45 | (low << 3);
    } else {
        modrm = 0x85 | (low << 3);
    }

    switch (size) {
        case 1: {
            if (reg >= 8) {
                emit_u8(cb, 0x44);
            } else if (reg >= 4) {
                emit_u8(cb, 0x40);
            }
            emit_u8(cb, 0x88);
            emit_u8(cb, modrm);
            break;
        }
        case 2: {
            emit_u8(cb, 0x66);
            if (reg >= 8) {
                emit_u8(cb, 0x44);
            }
            emit_u8(cb, 0x89);
            emit_u8(cb, modrm);
            break;
        }
        case 4: {
            if (reg >= 8) {
                emit_u8(cb, 0x44);
            }
            emit_u8(cb, 0x89);
            emit_u8(cb, modrm);
            break;
        }
        default: {
            uint8_t rex = (reg >= 8) ? 0x4C : 0x48;
            emit_u8(cb, rex);
            emit_u8(cb, 0x89);
            emit_u8(cb, modrm);
            break;
        }
    }

    if (neg >= -128 && neg <= 127) {
        emit_u8(cb, (uint8_t)neg);
    } else {
        emit_u32(cb, (uint32_t)neg);
    }
}

static void move_rax_to_reg(CodeBuf* cb, uint8_t reg) {
    if (reg != 0) {
        uint8_t rex = (reg >= 8) ? 0x49 : 0x48;
        uint8_t modrm = 0xC0 | (reg & 7);
        emit_u8(cb, rex);
        emit_u8(cb, 0x89);
        emit_u8(cb, modrm);
    }
}

WandBinary wand_x86_compile_program(
    const uint8_t* program_buf,
    size_t program_len,
    WandOutputFormat format,
    const uint8_t* entry_name,
    size_t entry_len
) {
    (void)program_buf;
    (void)program_len;
    (void)format;
    (void)entry_name;
    (void)entry_len;

    CodeBuf cb;
    cb_init(&cb);

    WandBinary out;
    out.data = cb.code;
    out.size = cb.len;
    return out;
}

void wand_binary_free(WandBinary* binary) {
    if (binary && binary->data) {
        free(binary->data);
        binary->data = NULL;
        binary->size = 0;
    }
}
