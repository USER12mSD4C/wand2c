#include "abi.h"
#include <stdlib.h>
#include <string.h>

#define WAND_PACK_MAGIC 0x4B435057u

#define OUTPUT_PROGRAM 0
#define OUTPUT_OBJECT 1
#define OUTPUT_RAW 2
#define OUTPUT_KERNEL 3
#define OUTPUT_WEXP 4

#define P46_SECTION_TYPES 1u
#define P46_SECTION_EXPORTS 2u
#define P46_SECTION_REFLECT 3u
#define P46_SECTION_IMPORTS 4u
#define P46_SECTION_DEPENDENCIES 5u

#define P46_EXPORT_KIND_FUNCTION 1u
#define P46_EXPORT_KIND_VARIABLE 2u
#define P46_EXPORT_KIND_TYPE 3u

#define HEADER_SIZE 36
#define SECTION_DESC_SIZE 20
#define POINTER_SIZE 8
#define ADDRESS_SIZE 8

typedef struct {
    const uint8_t* base;
    size_t len;
    size_t pos;
} Reader;

static void reader_init(Reader* r, const uint8_t* data, size_t len) {
    r->base = data;
    r->len = len;
    r->pos = 0;
}

static uint8_t read_u8(Reader* r) {
    if (r->pos + 1 > r->len) {
        return 0;
    }
    uint8_t v = r->base[r->pos];
    r->pos += 1;
    return v;
}

static uint16_t read_u16(Reader* r) {
    if (r->pos + 2 > r->len) {
        return 0;
    }
    uint16_t v = 0;
    v |= (uint16_t)r->base[r->pos];
    v |= (uint16_t)r->base[r->pos + 1] << 8;
    r->pos += 2;
    return v;
}

static uint32_t read_u32(Reader* r) {
    if (r->pos + 4 > r->len) {
        return 0;
    }
    uint32_t v = 0;
    v |= (uint32_t)r->base[r->pos];
    v |= (uint32_t)r->base[r->pos + 1] << 8;
    v |= (uint32_t)r->base[r->pos + 2] << 16;
    v |= (uint32_t)r->base[r->pos + 3] << 24;
    r->pos += 4;
    return v;
}

static uint64_t read_u64(Reader* r) {
    if (r->pos + 8 > r->len) {
        return 0;
    }
    uint64_t v = 0;
    for (int i = 0; i < 8; i++) {
        v |= (uint64_t)r->base[r->pos + i] << (i * 8);
    }
    r->pos += 8;
    return v;
}

static const uint8_t* read_bytes(Reader* r, size_t n) {
    if (r->pos + n > r->len) {
        return NULL;
    }
    const uint8_t* p = r->base + r->pos;
    r->pos += n;
    return p;
}

typedef struct {
    uint8_t* data;
    size_t len;
    size_t cap;
} Writer;

static void writer_init(Writer* w) {
    w->cap = 65536;
    w->len = 0;
    w->data = malloc(w->cap);
    if (!w->data) {
        exit(1);
    }
}

static void writer_reserve(Writer* w, size_t extra) {
    if (w->len + extra > w->cap) {
        size_t new_cap = w->cap * 2;
        if (new_cap < w->len + extra) {
            new_cap = w->len + extra;
        }
        uint8_t* new_buf = realloc(w->data, new_cap);
        if (!new_buf) {
            exit(1);
        }
        w->data = new_buf;
        w->cap = new_cap;
    }
}

static void write_u8(Writer* w, uint8_t v) {
    writer_reserve(w, 1);
    w->data[w->len++] = v;
}

static void write_u16(Writer* w, uint16_t v) {
    writer_reserve(w, 2);
    w->data[w->len++] = (uint8_t)(v & 0xFF);
    w->data[w->len++] = (uint8_t)((v >> 8) & 0xFF);
}

static void write_u32(Writer* w, uint32_t v) {
    writer_reserve(w, 4);
    w->data[w->len++] = (uint8_t)(v & 0xFF);
    w->data[w->len++] = (uint8_t)((v >> 8) & 0xFF);
    w->data[w->len++] = (uint8_t)((v >> 16) & 0xFF);
    w->data[w->len++] = (uint8_t)((v >> 24) & 0xFF);
}

static void write_u64(Writer* w, uint64_t v) {
    writer_reserve(w, 8);
    for (int i = 0; i < 8; i++) {
        w->data[w->len++] = (uint8_t)((v >> (i * 8)) & 0xFF);
    }
}

static void write_bytes(Writer* w, const uint8_t* data, size_t n) {
    writer_reserve(w, n);
    memcpy(w->data + w->len, data, n);
    w->len += n;
}

typedef struct {
    uint8_t* data;
    size_t len;
    size_t cap;
} Strtab;

static void strtab_init(Strtab* s) {
    s->cap = 4096;
    s->len = 0;
    s->data = malloc(s->cap);
    if (!s->data) {
        exit(1);
    }
}

static uint32_t strtab_insert(Strtab* s, const uint8_t* str) {
    if (!str) {
        return 0;
    }
    size_t slen = strlen((const char*)str);
    if (s->len + slen + 1 > s->cap) {
        size_t new_cap = s->cap * 2;
        if (new_cap < s->len + slen + 1) {
            new_cap = s->len + slen + 1;
        }
        uint8_t* new_buf = realloc(s->data, new_cap);
        if (!new_buf) {
            exit(1);
        }
        s->data = new_buf;
        s->cap = new_cap;
    }
    uint32_t offset = (uint32_t)s->len;
    memcpy(s->data + s->len, str, slen);
    s->len += slen;
    s->data[s->len++] = 0;
    return offset;
}

typedef struct {
    const uint8_t* code;
    uint32_t code_size;
    const uint8_t* global_data;
    uint32_t global_data_size;
    const uint8_t* str_pool;
    uint32_t str_pool_size;
    uint32_t function_count;
    uint32_t param_total_count;
    uint32_t unresolved_count;
    uint32_t import_count;
    uint32_t struct_count;
    uint32_t field_total_count;
    const uint8_t* functions;
    const uint8_t* params;
    const uint8_t* unresolved;
    const uint8_t* imports;
    const uint8_t* structs;
    const uint8_t* fields;
    uint8_t output_format;
    uint8_t use_os;
    uint32_t entry_name_idx;
} PackInput;

static const uint8_t* str_from_pool(const PackInput* in, uint32_t idx) {
    if (idx >= in->str_pool_size) {
        return NULL;
    }
    return in->str_pool + idx;
}

static void build_p46_sections(const PackInput* in,
                               Writer* types,
                               Writer* exports,
                               Writer* reflect,
                               Writer* imports,
                               Writer* deps,
                               Strtab* strtab,
                               uint32_t* exports_count,
                               uint32_t* imports_count,
                               uint32_t* deps_count) {
    *exports_count = 0;
    *imports_count = 0;
    *deps_count = 0;

    for (uint32_t i = 0; i < in->import_count; i++) {
        const uint8_t* rec = in->imports + i * 20;
        uint32_t module_name_idx = *(const uint32_t*)(rec + 0);
        uint32_t major = *(const uint32_t*)(rec + 4);
        uint32_t minor = *(const uint32_t*)(rec + 8);
        uint32_t patch = *(const uint32_t*)(rec + 12);
        uint32_t build = *(const uint32_t*)(rec + 16);

        if (*deps_count == 0) {
            write_u32(deps, 0);
        }

        const uint8_t* module_name = str_from_pool(in, module_name_idx);
        uint32_t name_off = strtab_insert(strtab, module_name);

        write_u32(deps, name_off);
        write_u32(deps, major);
        write_u32(deps, minor);
        write_u32(deps, patch);
        write_u32(deps, build);
        (*deps_count)++;
    }

    if (*deps_count > 0) {
        uint32_t cnt = *deps_count;
        deps->data[0] = (uint8_t)(cnt & 0xFF);
        deps->data[1] = (uint8_t)((cnt >> 8) & 0xFF);
        deps->data[2] = (uint8_t)((cnt >> 16) & 0xFF);
        deps->data[3] = (uint8_t)((cnt >> 24) & 0xFF);
    }

    for (uint32_t i = 0; i < in->struct_count; i++) {
        const uint8_t* rec = in->structs + i * 20;
        uint32_t name_idx = *(const uint32_t*)(rec + 0);
        uint32_t version = *(const uint32_t*)(rec + 4);
        uint32_t field_count = *(const uint32_t*)(rec + 12);

        uint32_t name_off = strtab_insert(strtab, str_from_pool(in, name_idx));

        Writer fields_buf;
        writer_init(&fields_buf);

        const uint8_t* field_base = in->fields;
        uint32_t field_offset_idx = 0;
        for (uint32_t k = 0; k < i; k++) {
            const uint8_t* prev_rec = in->structs + k * 20;
            field_offset_idx += *(const uint32_t*)(prev_rec + 12);
        }

        for (uint32_t k = 0; k < field_count; k++) {
            const uint8_t* f = field_base + (field_offset_idx + k) * 20;
            uint32_t f_name_idx = *(const uint32_t*)(f + 0);
            uint32_t f_type_id = *(const uint32_t*)(f + 4);
            uint32_t f_version_added = *(const uint32_t*)(f + 8);
            uint32_t f_version_removed = *(const uint32_t*)(f + 12);

            uint32_t f_name_off = strtab_insert(strtab, str_from_pool(in, f_name_idx));
            write_u32(&fields_buf, f_name_off);
            write_u32(&fields_buf, f_type_id);
            write_u32(&fields_buf, 0);
            write_u32(&fields_buf, f_version_added);
            write_u32(&fields_buf, f_version_removed);
        }

        Writer payload;
        writer_init(&payload);
        write_u32(&payload, name_off);
        write_u32(&payload, version);
        write_u32(&payload, 16);
        write_u32(&payload, field_count);
        write_bytes(&payload, fields_buf.data, fields_buf.len);

        write_u16(types, 1);
        write_u32(types, (uint32_t)payload.len);
        write_bytes(types, payload.data, payload.len);

        free(payload.data);
        free(fields_buf.data);
    }

    if (*exports_count == 0) {
        write_u32(exports, 0);
    }

    uint32_t param_cursor = 0;
    uint32_t base_addr = 0x400078u;

    for (uint32_t i = 0; i < in->function_count; i++) {
        const uint8_t* rec = in->functions + i * 20;
        uint32_t name_idx = *(const uint32_t*)(rec + 0);
        uint32_t code_offset = *(const uint32_t*)(rec + 4);
        uint8_t flags = rec[8];
        uint8_t param_count = rec[9];
        uint32_t return_type_id = *(const uint32_t*)(rec + 12);

        uint8_t is_export = flags & 1;
        uint8_t is_extern = flags & 2;

        if (is_extern) {
            param_cursor += param_count;
            continue;
        }

        if (!is_export && in->function_count > 0) {
            uint8_t any_export = 0;
            for (uint32_t k = 0; k < in->function_count; k++) {
                if (in->functions[k * 20 + 8] & 1) {
                    any_export = 1;
                    break;
                }
            }
            if (any_export) {
                param_cursor += param_count;
                continue;
            }
        }

        uint32_t name_off = strtab_insert(strtab, str_from_pool(in, name_idx));
        uint32_t module_off = strtab_insert(strtab, (const uint8_t*)"main_module");

        write_u32(exports, name_off);
        write_u32(exports, module_off);
        write_u32(exports, 1);
        write_u8(exports, P46_EXPORT_KIND_FUNCTION);

        uint64_t abs_addr = base_addr + code_offset;
        write_u64(exports, abs_addr);
        write_u32(exports, 4);

        write_u32(exports, (uint32_t)param_count);
        write_u32(exports, return_type_id);
        for (uint8_t k = 0; k < param_count; k++) {
            const uint8_t* p = in->params + (param_cursor + k) * 4;
            uint32_t type_id = *(const uint32_t*)p;
            write_u32(exports, type_id);
        }

        param_cursor += param_count;
        (*exports_count)++;
    }

    if (*exports_count > 0) {
        uint32_t cnt = *exports_count;
        exports->data[0] = (uint8_t)(cnt & 0xFF);
        exports->data[1] = (uint8_t)((cnt >> 8) & 0xFF);
        exports->data[2] = (uint8_t)((cnt >> 16) & 0xFF);
        exports->data[3] = (uint8_t)((cnt >> 24) & 0xFF);
    }

    if (*imports_count == 0) {
        write_u32(imports, 0);
    }

    const uint8_t* module_name = NULL;
    if (in->import_count > 0) {
        const uint8_t* first_import = in->imports;
        uint32_t first_module_idx = *(const uint32_t*)first_import;
        module_name = str_from_pool(in, first_module_idx);
    } else {
        module_name = (const uint8_t*)"libc.ko";
    }

    for (uint32_t i = 0; i < in->unresolved_count; i++) {
        const uint8_t* rec = in->unresolved + i * 12;
        uint32_t name_idx = *(const uint32_t*)(rec + 0);

        if (*imports_count == 0) {
            write_u32(imports, 0);
        }

        uint32_t name_off = strtab_insert(strtab, str_from_pool(in, name_idx));
        uint32_t module_off = strtab_insert(strtab, module_name);

        write_u32(imports, name_off);
        write_u32(imports, module_off);
        write_u32(imports, 1);
        write_u32(imports, 0);
        (*imports_count)++;
    }

    if (*imports_count > 0) {
        uint32_t cnt = *imports_count;
        imports->data[0] = (uint8_t)(cnt & 0xFF);
        imports->data[1] = (uint8_t)((cnt >> 8) & 0xFF);
        imports->data[2] = (uint8_t)((cnt >> 16) & 0xFF);
        imports->data[3] = (uint8_t)((cnt >> 24) & 0xFF);
    }

    if (reflect->len == 0) {
        write_u32(reflect, 0);
    }
}

static void build_p46_header(Writer* hdr,
                             uint64_t strtab_offset,
                             uint64_t strtab_size,
                             uint64_t sections_start) {
    write_bytes(hdr, (const uint8_t[]){0x50, 0x34, 0x36, 0x00}, 4);
    write_u8(hdr, 1);
    write_u8(hdr, 6);
    write_u8(hdr, 0);
    write_u8(hdr, 1);
    write_u8(hdr, POINTER_SIZE);
    write_u8(hdr, ADDRESS_SIZE);
    write_u8(hdr, 0);
    write_u8(hdr, 0);
    write_u32(hdr, 0x01000000u);
    write_u32(hdr, 5);
    write_u64(hdr, strtab_offset);
    write_u64(hdr, strtab_size);

    uint64_t offsets[5];
    uint64_t sizes[5];
    uint32_t stypes[5] = {
        P46_SECTION_TYPES,
        P46_SECTION_EXPORTS,
        P46_SECTION_REFLECT,
        P46_SECTION_IMPORTS,
        P46_SECTION_DEPENDENCIES,
    };

    offsets[0] = sections_start;
    for (int i = 1; i < 5; i++) {
        offsets[i] = offsets[i - 1] + sizes[i - 1];
    }

    for (int i = 0; i < 5; i++) {
        write_u64(hdr, offsets[i]);
        write_u64(hdr, sizes[i]);
        write_u32(hdr, stypes[i]);
    }
}

static void build_elf64_image(const PackInput* in, Writer* out) {
    Writer types, exports, reflect, imports, deps;
    writer_init(&types);
    writer_init(&exports);
    writer_init(&reflect);
    writer_init(&imports);
    writer_init(&deps);
    Strtab strtab;
    strtab_init(&strtab);

    uint32_t exports_count = 0;
    uint32_t imports_count = 0;
    uint32_t deps_count = 0;

    build_p46_sections(in, &types, &exports, &reflect, &imports, &deps,
                       &strtab, &exports_count, &imports_count, &deps_count);

    uint32_t text_offset = 120;
    uint32_t text_size = in->code_size;
    uint32_t p46_hdr_offset = text_offset + text_size;
    uint32_t p46_hdr_size = HEADER_SIZE + SECTION_DESC_SIZE * 5;
    uint32_t sections_start = p46_hdr_offset + p46_hdr_size;

    uint32_t p46_types_offset = sections_start;
    uint32_t p46_types_size = (uint32_t)types.len;
    uint32_t p46_exp_offset = p46_types_offset + p46_types_size;
    uint32_t p46_exp_size = (uint32_t)exports.len;
    uint32_t p46_refl_offset = p46_exp_offset + p46_exp_size;
    uint32_t p46_refl_size = (uint32_t)reflect.len;
    uint32_t p46_imp_offset = p46_refl_offset + p46_refl_size;
    uint32_t p46_imp_size = (uint32_t)imports.len;
    uint32_t p46_deps_offset = p46_imp_offset + p46_imp_size;
    uint32_t p46_deps_size = (uint32_t)deps.len;
    uint32_t p46_strtab_offset = p46_deps_offset + p46_deps_size;
    uint32_t p46_strtab_size = (uint32_t)strtab.len;

    Writer shstrtab;
    writer_init(&shstrtab);
    write_u8(&shstrtab, 0);

    uint32_t n_text = (uint32_t)shstrtab.len;
    write_bytes(&shstrtab, (const uint8_t*)".text\0", 6);

    uint32_t n_p46_hdr = (uint32_t)shstrtab.len;
    write_bytes(&shstrtab, (const uint8_t*)".p46_header\0", 12);

    uint32_t n_p46_typ = (uint32_t)shstrtab.len;
    write_bytes(&shstrtab, (const uint8_t*)".p46_types\0", 11);

    uint32_t n_p46_exp = (uint32_t)shstrtab.len;
    write_bytes(&shstrtab, (const uint8_t*)".p46_exports\0", 13);

    uint32_t n_p46_imp = (uint32_t)shstrtab.len;
    write_bytes(&shstrtab, (const uint8_t*)".p46_imports\0", 13);

    uint32_t n_p46_dep = (uint32_t)shstrtab.len;
    write_bytes(&shstrtab, (const uint8_t*)".p46_deps\0", 10);

    uint32_t n_p46_ref = (uint32_t)shstrtab.len;
    write_bytes(&shstrtab, (const uint8_t*)".p46_reflect\0", 13);

    uint32_t n_shstr = (uint32_t)shstrtab.len;
    write_bytes(&shstrtab, (const uint8_t*)".shstrtab\0", 10);

    uint32_t n_p46_str = (uint32_t)shstrtab.len;
    write_bytes(&shstrtab, (const uint8_t*)".p46_strtab\0", 12);

    uint32_t shstrtab_offset = p46_strtab_offset + p46_strtab_size;
    uint32_t shstrtab_size = (uint32_t)shstrtab.len;
    uint32_t sht_offset = shstrtab_offset + shstrtab_size;

    Writer p46_header;
    writer_init(&p46_header);

    uint64_t section_offsets[5] = {
        p46_types_offset,
        p46_exp_offset,
        p46_refl_offset,
        p46_imp_offset,
        p46_deps_offset,
    };
    uint64_t section_sizes[5] = {
        p46_types_size,
        p46_exp_size,
        p46_refl_size,
        p46_imp_size,
        p46_deps_size,
    };
    uint32_t section_types[5] = {
        P46_SECTION_TYPES,
        P46_SECTION_EXPORTS,
        P46_SECTION_REFLECT,
        P46_SECTION_IMPORTS,
        P46_SECTION_DEPENDENCIES,
    };

    write_bytes(&p46_header, (const uint8_t[]){0x50, 0x34, 0x36, 0x00}, 4);
    write_u8(&p46_header, 1);
    write_u8(&p46_header, 6);
    write_u8(&p46_header, 0);
    write_u8(&p46_header, 1);
    write_u8(&p46_header, POINTER_SIZE);
    write_u8(&p46_header, ADDRESS_SIZE);
    write_u8(&p46_header, 0);
    write_u8(&p46_header, 0);
    write_u32(&p46_header, 0x01000000u);
    write_u32(&p46_header, 5);
    write_u64(&p46_header, p46_strtab_offset);
    write_u64(&p46_header, p46_strtab_size);

    for (int i = 0; i < 5; i++) {
        write_u64(&p46_header, section_offsets[i]);
        write_u64(&p46_header, section_sizes[i]);
        write_u32(&p46_header, section_types[i]);
    }

    write_bytes(out, (const uint8_t[]){0x7F, 'E', 'L', 'F'}, 4);
    write_u8(out, 2);
    write_u8(out, 1);
    write_u8(out, 1);
    write_u8(out, 0);
    for (int i = 0; i < 8; i++) {
        write_u8(out, 0);
    }
    write_u16(out, 2);
    write_u16(out, 62);
    write_u32(out, 1);
    write_u64(out, 0x400078);
    write_u64(out, 64);
    write_u64(out, sht_offset);
    write_u32(out, 0);
    write_u16(out, 64);
    write_u16(out, 56);
    write_u16(out, 1);
    write_u16(out, 64);
    write_u16(out, 10);
    write_u16(out, 8);

    uint64_t total_file_size = (uint64_t)(sht_offset + 10 * 64);

    write_u32(out, 1);
    write_u32(out, 7);
    write_u64(out, 0);
    write_u64(out, 0x400000);
    write_u64(out, 0x400000);
    write_u64(out, total_file_size);
    write_u64(out, total_file_size);
    write_u64(out, 0x1000);

    write_bytes(out, in->code, in->code_size);
    write_bytes(out, p46_header.data, p46_header.len);
    write_bytes(out, types.data, types.len);
    write_bytes(out, exports.data, exports.len);
    write_bytes(out, reflect.data, reflect.len);
    write_bytes(out, imports.data, imports.len);
    write_bytes(out, deps.data, deps.len);
    write_bytes(out, strtab.data, strtab.len);
    write_bytes(out, shstrtab.data, shstrtab.len);

    free(p46_header.data);
    free(types.data);
    free(exports.data);
    free(reflect.data);
    free(imports.data);
    free(deps.data);
    free(strtab.data);
    free(shstrtab.data);
}

WandBinary wand_x86_pack_image(const uint8_t* input, size_t input_len) {
    WandBinary result;
    result.data = NULL;
    result.size = 0;

    if (input_len < 48) {
        return result;
    }

    Reader r;
    reader_init(&r, input, input_len);

    uint32_t magic = read_u32(&r);
    if (magic != WAND_PACK_MAGIC) {
        return result;
    }

    PackInput in;
    memset(&in, 0, sizeof(in));

    in.output_format = read_u8(&r);
    in.use_os = read_u8(&r);
    read_u8(&r);
    read_u8(&r);
    in.entry_name_idx = read_u32(&r);
    in.str_pool_size = read_u32(&r);
    in.code_size = read_u32(&r);
    in.global_data_size = read_u32(&r);
    in.function_count = read_u32(&r);
    in.param_total_count = read_u32(&r);
    in.unresolved_count = read_u32(&r);
    in.import_count = read_u32(&r);
    in.struct_count = read_u32(&r);
    in.field_total_count = read_u32(&r);

    in.str_pool = read_bytes(&r, in.str_pool_size);
    in.code = read_bytes(&r, in.code_size);
    in.global_data = read_bytes(&r, in.global_data_size);
    in.functions = read_bytes(&r, in.function_count * 20);
    in.params = read_bytes(&r, in.param_total_count * 4);
    in.unresolved = read_bytes(&r, in.unresolved_count * 12);
    in.imports = read_bytes(&r, in.import_count * 20);
    in.structs = read_bytes(&r, in.struct_count * 20);
    in.fields = read_bytes(&r, in.field_total_count * 20);

    if (!in.str_pool || !in.code || !in.functions || !in.params ||
        !in.unresolved || !in.imports || !in.structs || !in.fields) {
        return result;
    }

    Writer out;
    writer_init(&out);

    if (in.output_format == OUTPUT_PROGRAM || in.output_format == OUTPUT_WEXP) {
        build_elf64_image(&in, &out);
    } else {
        write_bytes(&out, in.code, in.code_size);
        write_bytes(&out, in.global_data, in.global_data_size);
    }

    result.data = out.data;
    result.size = out.len;
    return result;
}

void wand_binary_free(WandBinary* binary) {
    if (binary && binary->data) {
        free(binary->data);
        binary->data = NULL;
        binary->size = 0;
    }
}
