/* Minimal Standard 4/6 continuation-object loader.
 *
 * This library is intentionally not a program. An OS, boot loader or hosted
 * harness supplies the bytes and (when needed) import/dependency callbacks.
 */
#include "wand_p46.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>

#define P46_HEADER_SIZE 36u
#define P46_DESC_SIZE 20u
#define P46_SECTION_TYPES 1u
#define P46_SECTION_EXPORTS 2u
#define P46_SECTION_REFLECT 3u
#define P46_SECTION_IMPORTS 4u
#define P46_SECTION_DEPS 5u
#define P46_SECTION_WEXP_IMAGE 100u
#define P46_SECTION_WEXP_RELOCS 101u

#define WEXP_RELOC_64 1u
#define WEXP_RELOC_PC32 4u
#define WEXP_RELOC_ENTRY_SIZE 24u

#define P46_EXPORT_KIND_FUNCTION 1u
#define P46_REFLECT_KIND_TYPE 1u
#define P46_REFLECT_KIND_EXPORT 2u

typedef struct {
    uint64_t offset;
    uint64_t size;
    uint32_t type;
} Section;

typedef struct {
    uint32_t name_off;
    uint32_t module_off;
    uint32_t version;
    uint8_t kind;
    uint64_t address;
    uint32_t type_id;
    uint32_t param_count;
    uint32_t return_type;
} Export;

typedef struct {
    uint32_t name_off;
    uint32_t module_off;
    uint32_t version;
    uint32_t type_version;
} Import;

typedef struct {
    uint64_t offset;
    uint32_t type;
    uint32_t name_off;
    int64_t addend;
} Reloc;

typedef struct {
    char* name;
    uintptr_t address;
} Trampoline;

struct WandP46Module {
    uint8_t* blob;
    size_t blob_size;
    uint8_t* image;
    size_t image_size;
    size_t mapping_size;
    size_t trampoline_offset;
    Trampoline* trampolines;
    size_t trampoline_count;
    size_t trampoline_capacity;
    uintptr_t base;
    Section* sections;
    size_t section_count;
    uint64_t strtab_offset;
    uint64_t strtab_size;
    Export* exports;
    size_t export_count;
    Import* imports;
    size_t import_count;
    Reloc* relocs;
    size_t reloc_count;
    WandP46Options options;
};

static uint32_t rd32(const uint8_t* p) {
    return (uint32_t)p[0] | ((uint32_t)p[1] << 8) |
           ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

static uint64_t rd64(const uint8_t* p) {
    return (uint64_t)rd32(p) | ((uint64_t)rd32(p + 4) << 32);
}

static char* dup_string(const char* s) {
    size_t n = strlen(s) + 1;
    char* copy = (char*)malloc(n);
    if (copy) memcpy(copy, s, n);
    return copy;
}

static int fits(uint64_t offset, uint64_t size, uint64_t total) {
    return offset <= total && size <= total - offset;
}

static const uint8_t* section_data(const WandP46Module* m, uint32_t type, size_t* size) {
    for (size_t i = 0; i < m->section_count; i++) {
        if (m->sections[i].type == type) {
            if (size) *size = (size_t)m->sections[i].size;
            return m->blob + (size_t)m->sections[i].offset;
        }
    }
    if (size) *size = 0;
    return NULL;
}

static const char* cstr(const WandP46Module* m, uint32_t offset) {
    if (offset >= m->strtab_size) return NULL;
    const char* s = (const char*)(m->blob + (size_t)(m->strtab_offset + offset));
    size_t available = (size_t)(m->strtab_size - offset);
    if (memchr(s, 0, available) == NULL) return NULL;
    return s;
}

static WandP46Status find_export(const WandP46Module* m, const char* name,
                                 uint32_t required_version, uint64_t* address) {
    for (size_t i = 0; i < m->export_count; i++) {
        const Export* e = &m->exports[i];
        if (e->kind != P46_EXPORT_KIND_FUNCTION) continue;
        const char* n = cstr(m, e->name_off);
        if (!n || strcmp(n, name) != 0) continue;
        if (e->version < required_version) continue;
        *address = e->address;
        return WAND_P46_OK;
    }
    return WAND_P46_ERR_UNRESOLVED_SYMBOL;
}

static const char* import_module_for(const WandP46Module* m, const char* symbol) {
    for (size_t i = 0; i < m->import_count; i++) {
        const char* n = cstr(m, m->imports[i].name_off);
        if (n && strcmp(n, symbol) == 0) {
            return cstr(m, m->imports[i].module_off);
        }
    }
    return m->import_count ? cstr(m, m->imports[0].module_off) : "libc.ko";
}

static uint32_t import_version_for(const WandP46Module* m, const char* symbol) {
    for (size_t i = 0; i < m->import_count; i++) {
        const char* n = cstr(m, m->imports[i].name_off);
        if (n && strcmp(n, symbol) == 0) {
            return m->imports[i].version;
        }
    }
    return 1;
}

/* Host functions can live arbitrarily far from the mmap'd module, so a
 * PC32 import call is patched to a short trampoline inside the same mapping. */
static WandP46Status make_trampoline(WandP46Module* m, const char* name,
                                     uintptr_t target, uintptr_t* out) {
    for (size_t i = 0; i < m->trampoline_count; i++) {
        if (strcmp(m->trampolines[i].name, name) == 0) {
            *out = m->trampolines[i].address;
            return WAND_P46_OK;
        }
    }
    if (m->trampoline_count >= m->trampoline_capacity) {
        return WAND_P46_ERR_OUT_OF_MEMORY;
    }
    size_t slot = m->trampoline_count++;
    size_t offset = m->trampoline_offset + slot * 32u;
    uint8_t* p = m->image + offset;
    p[0] = 0x48; p[1] = 0xB8; /* movabs rax, imm64 */
    for (int k = 0; k < 8; k++) p[2 + (size_t)k] = (uint8_t)(target >> (k * 8));
    p[10] = 0xFF; p[11] = 0xE0; /* jmp rax */
    for (size_t k = 12; k < 32; k++) p[k] = 0xCC;

    Trampoline* t = &m->trampolines[slot];
    t->name = dup_string(name);
    if (!t->name) return WAND_P46_ERR_OUT_OF_MEMORY;
    t->address = m->base + (uintptr_t)offset;
    *out = t->address;
    return WAND_P46_OK;
}

static WandP46Status resolve_symbol(WandP46Module* m, const char* name,
                                    uint32_t version, uintptr_t* out) {
    if (!name || !*name) return WAND_P46_ERR_UNRESOLVED_SYMBOL;

    if (strncmp(name, "@offset:", 8) == 0) {
        char* end = NULL;
        unsigned long long offset = strtoull(name + 8, &end, 10);
        if (end == name + 8 || (end && *end)) return WAND_P46_ERR_BAD_RELOCATION;
        *out = m->base + (uintptr_t)offset;
        return WAND_P46_OK;
    }

    uint64_t address = 0;
    if (find_export(m, name, version, &address) == WAND_P46_OK) {
        *out = m->base + (uintptr_t)address;
        return WAND_P46_OK;
    }

    if (m->options.resolve) {
        void* fn = m->options.resolve(import_module_for(m, name), name, version,
                                      m->options.ctx);
        if (fn) {
            return make_trampoline(m, name, (uintptr_t)fn, out);
        }
    }
    return WAND_P46_ERR_UNRESOLVED_SYMBOL;
}

static WandP46Status check_dependencies(WandP46Module* m) {
    size_t size = 0;
    const uint8_t* deps = section_data(m, P46_SECTION_DEPS, &size);
    if (!deps || size < 4) return WAND_P46_ERR_TRUNCATED_SECTION;
    uint32_t count = rd32(deps);
    if ((uint64_t)count * 20u + 4u > (uint64_t)size) return WAND_P46_ERR_TRUNCATED_SECTION;
    for (uint32_t i = 0; i < count; i++) {
        const uint8_t* e = deps + 4 + (size_t)i * 20u;
        const char* name = cstr(m, rd32(e));
        if (!name) return WAND_P46_ERR_CORRUPT_STRTAB;
        if (m->options.dependency &&
            m->options.dependency(name, rd32(e + 4), rd32(e + 8), rd32(e + 12),
                                  rd32(e + 16), m->options.ctx) != 0) {
            return WAND_P46_ERR_MISSING_DEPENDENCY;
        }
    }
    return WAND_P46_OK;
}

static WandP46Status parse_exports(WandP46Module* m) {
    size_t size = 0;
    const uint8_t* data = section_data(m, P46_SECTION_EXPORTS, &size);
    if (!data || size < 4) return WAND_P46_ERR_TRUNCATED_SECTION;
    uint32_t count = rd32(data);
    size_t pos = 4;
    m->exports = (Export*)calloc(count ? count : 1, sizeof(Export));
    if (!m->exports) return WAND_P46_ERR_OUT_OF_MEMORY;
    for (uint32_t i = 0; i < count; i++) {
        if (size - pos < 33u) return WAND_P46_ERR_TRUNCATED_SECTION;
        Export* e = &m->exports[i];
        e->name_off = rd32(data + pos); pos += 4;
        e->module_off = rd32(data + pos); pos += 4;
        e->version = rd32(data + pos); pos += 4;
        e->kind = data[pos++];
        e->address = rd64(data + pos); pos += 8;
        e->type_id = rd32(data + pos); pos += 4;
        e->param_count = rd32(data + pos); pos += 4;
        e->return_type = rd32(data + pos); pos += 4;
        if (!cstr(m, e->name_off) || !cstr(m, e->module_off)) {
            return WAND_P46_ERR_CORRUPT_STRTAB;
        }
        uint64_t param_bytes = (uint64_t)e->param_count * 4u;
        if (param_bytes > (uint64_t)size - pos) return WAND_P46_ERR_TRUNCATED_SECTION;
        pos += (size_t)param_bytes;
    }
    m->export_count = count;
    return WAND_P46_OK;
}

static WandP46Status parse_imports(WandP46Module* m) {
    size_t size = 0;
    const uint8_t* data = section_data(m, P46_SECTION_IMPORTS, &size);
    if (!data || size < 4) return WAND_P46_ERR_TRUNCATED_SECTION;
    uint32_t count = rd32(data);
    if ((uint64_t)count * 16u + 4u > (uint64_t)size) return WAND_P46_ERR_TRUNCATED_SECTION;
    m->imports = (Import*)calloc(count ? count : 1, sizeof(Import));
    if (!m->imports) return WAND_P46_ERR_OUT_OF_MEMORY;
    for (uint32_t i = 0; i < count; i++) {
        const uint8_t* e = data + 4 + (size_t)i * 16u;
        m->imports[i].name_off = rd32(e);
        m->imports[i].module_off = rd32(e + 4);
        m->imports[i].version = rd32(e + 8);
        m->imports[i].type_version = rd32(e + 12);
        if (!cstr(m, m->imports[i].name_off) || !cstr(m, m->imports[i].module_off)) {
            return WAND_P46_ERR_CORRUPT_STRTAB;
        }
    }
    m->import_count = count;
    return WAND_P46_OK;
}

static WandP46Status parse_relocs(WandP46Module* m) {
    size_t size = 0;
    const uint8_t* data = section_data(m, P46_SECTION_WEXP_RELOCS, &size);
    if (!data) return WAND_P46_OK;
    if (size % WEXP_RELOC_ENTRY_SIZE != 0) return WAND_P46_ERR_BAD_RELOCATION;
    m->reloc_count = size / WEXP_RELOC_ENTRY_SIZE;
    m->relocs = (Reloc*)calloc(m->reloc_count ? m->reloc_count : 1, sizeof(Reloc));
    if (!m->relocs) return WAND_P46_ERR_OUT_OF_MEMORY;
    for (size_t i = 0; i < m->reloc_count; i++) {
        const uint8_t* p = data + i * WEXP_RELOC_ENTRY_SIZE;
        m->relocs[i].offset = rd64(p);
        m->relocs[i].type = rd32(p + 8);
        m->relocs[i].name_off = rd32(p + 12);
        m->relocs[i].addend = (int64_t)rd64(p + 16);
    }
    return WAND_P46_OK;
}

static WandP46Status apply_relocs(WandP46Module* m) {
    for (size_t i = 0; i < m->reloc_count; i++) {
        const Reloc* r = &m->relocs[i];
        const char* name = cstr(m, r->name_off);
        if (!name) return WAND_P46_ERR_CORRUPT_STRTAB;
        if (r->offset > m->image_size) return WAND_P46_ERR_BAD_RELOCATION;
        uintptr_t target = 0;
        uint32_t version = import_version_for(m, name);
        WandP46Status status = resolve_symbol(m, name, version, &target);
        if (status != WAND_P46_OK) return status;

        if (r->type == WEXP_RELOC_64) {
            if (r->offset + 8u > m->image_size) return WAND_P46_ERR_BAD_RELOCATION;
            uint64_t value = (uint64_t)(target + (uintptr_t)r->addend);
            for (int k = 0; k < 8; k++) {
                m->image[r->offset + (size_t)k] = (uint8_t)(value >> (k * 8));
            }
        } else if (r->type == WEXP_RELOC_PC32) {
            if (r->offset + 4u > m->image_size) return WAND_P46_ERR_BAD_RELOCATION;
            /* S + A - P: the -4 call-relocation addend is already in A. */
            int64_t delta = (int64_t)target -
                            (int64_t)((uintptr_t)(m->image + r->offset)) +
                            (int64_t)r->addend;
            if (delta < INT32_MIN || delta > INT32_MAX) return WAND_P46_ERR_BAD_RELOCATION;
            int32_t rel = (int32_t)delta;
            for (int k = 0; k < 4; k++) {
                m->image[r->offset + (size_t)k] = (uint8_t)((uint32_t)rel >> (k * 8));
            }
        } else {
            return WAND_P46_ERR_BAD_RELOCATION;
        }
    }
    return WAND_P46_OK;
}

static WandP46Status validate_sections(const WandP46Module* m) {
    int seen[6] = {0};
    for (size_t i = 0; i < m->section_count; i++) {
        const Section* s = &m->sections[i];
        if (s->type >= 1 && s->type <= 5) {
            if (seen[s->type]++) return WAND_P46_ERR_TRUNCATED;
        }
        if (!fits(s->offset, s->size, m->blob_size)) return WAND_P46_ERR_TRUNCATED_SECTION;
    }
    for (int t = 1; t <= 5; t++) {
        if (!seen[t]) return WAND_P46_ERR_TRUNCATED;
    }

    for (size_t i = 0; i < m->section_count; i++) {
        uint64_t a0 = m->sections[i].offset;
        uint64_t a1 = a0 + m->sections[i].size;
        uint64_t b0 = 0;
        uint64_t b1 = P46_HEADER_SIZE + P46_DESC_SIZE * (uint64_t)m->section_count;
        if (a0 < b1 && b0 < a1) return WAND_P46_ERR_OVERLAPPING_SECTIONS;
        b0 = m->strtab_offset;
        b1 = b0 + m->strtab_size;
        if (a0 < b1 && b0 < a1) return WAND_P46_ERR_OVERLAPPING_SECTIONS;
        for (size_t j = i + 1; j < m->section_count; j++) {
            b0 = m->sections[j].offset;
            b1 = b0 + m->sections[j].size;
            if (a0 < b1 && b0 < a1) return WAND_P46_ERR_OVERLAPPING_SECTIONS;
        }
    }
    return WAND_P46_OK;
}

WandP46Module* wand_p46_load(const void* data, size_t size,
                             const WandP46Options* options,
                             WandP46Status* out_status) {
    WandP46Status status = WAND_P46_OK;
    WandP46Module* m = NULL;
#define FAIL(code) do { status = (code); goto fail; } while (0)

    if (!data || size < P46_HEADER_SIZE) FAIL(WAND_P46_ERR_HEADER_TOO_SMALL);
    if (memcmp(data, "P46\0", 4) != 0) FAIL(WAND_P46_ERR_BAD_MAGIC);
    const uint8_t* bytes = (const uint8_t*)data;
    if (bytes[4] != 1) FAIL(WAND_P46_ERR_UNSUPPORTED_FORMAT_MAJOR);
    if (bytes[7] != 1) FAIL(WAND_P46_ERR_ENDIANNESS_MISMATCH);
    if (bytes[8] != 1 && bytes[8] != 2 && bytes[8] != 4 && bytes[8] != 8) {
        FAIL(WAND_P46_ERR_UNSUPPORTED_POINTER_SIZE);
    }
    if (bytes[9] != 1 && bytes[9] != 2 && bytes[9] != 4 && bytes[9] != 8) {
        FAIL(WAND_P46_ERR_UNSUPPORTED_ADDRESS_SIZE);
    }

    uint32_t section_count = rd32(bytes + 16);
    uint64_t strtab_offset = rd64(bytes + 20);
    uint64_t strtab_size = rd64(bytes + 28);
    if (section_count < 5) FAIL(WAND_P46_ERR_TRUNCATED);
    uint64_t desc_end = P46_HEADER_SIZE + P46_DESC_SIZE * (uint64_t)section_count;
    if (desc_end > size) FAIL(WAND_P46_ERR_TRUNCATED);
    if (!fits(strtab_offset, strtab_size, size)) FAIL(WAND_P46_ERR_TRUNCATED);

    m = (WandP46Module*)calloc(1, sizeof(WandP46Module));
    if (!m) FAIL(WAND_P46_ERR_OUT_OF_MEMORY);
    m->blob = (uint8_t*)malloc(size ? size : 1);
    if (!m->blob) FAIL(WAND_P46_ERR_OUT_OF_MEMORY);
    memcpy(m->blob, data, size);
    m->blob_size = size;
    m->sections = (Section*)calloc(section_count, sizeof(Section));
    if (!m->sections) FAIL(WAND_P46_ERR_OUT_OF_MEMORY);
    m->section_count = section_count;
    m->strtab_offset = strtab_offset;
    m->strtab_size = strtab_size;
    if (options) m->options = *options;

    for (uint32_t i = 0; i < section_count; i++) {
        const uint8_t* d = m->blob + P46_HEADER_SIZE + (size_t)i * P46_DESC_SIZE;
        m->sections[i].offset = rd64(d);
        m->sections[i].size = rd64(d + 8);
        m->sections[i].type = rd32(d + 16);
    }
    if ((status = validate_sections(m)) != WAND_P46_OK) goto fail;

    if ((status = parse_exports(m)) != WAND_P46_OK) goto fail;
    if ((status = parse_imports(m)) != WAND_P46_OK) goto fail;
    if ((status = parse_relocs(m)) != WAND_P46_OK) goto fail;

    size_t image_size = 0;
    const uint8_t* image = section_data(m, P46_SECTION_WEXP_IMAGE, &image_size);
    if (!image || image_size == 0) FAIL(WAND_P46_ERR_TRUNCATED_SECTION);
    m->trampoline_offset = (image_size + 15u) & ~(size_t)15u;
    m->trampoline_capacity = m->import_count + 1u;
    m->mapping_size = m->trampoline_offset + 32u * m->trampoline_capacity;
    m->trampolines = (Trampoline*)calloc(m->trampoline_capacity, sizeof(Trampoline));
    if (!m->trampolines) FAIL(WAND_P46_ERR_OUT_OF_MEMORY);
    m->image = (uint8_t*)mmap(NULL, m->mapping_size, PROT_READ | PROT_WRITE | PROT_EXEC,
                              MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (m->image == MAP_FAILED) {
        m->image = NULL;
        FAIL(WAND_P46_ERR_OUT_OF_MEMORY);
    }
    memcpy(m->image, image, image_size);
    m->image_size = image_size;
    m->base = (uintptr_t)m->image;

    if ((status = check_dependencies(m)) != WAND_P46_OK) goto fail;
    if ((status = apply_relocs(m)) != WAND_P46_OK) goto fail;

    if (out_status) *out_status = WAND_P46_OK;
    return m;

fail:
    wand_p46_unload(m);
    if (out_status) *out_status = status;
    return NULL;
#undef FAIL
}

WandP46Status wand_p46_get_symbol(WandP46Module* m, const char* name,
                                  uint32_t required_version, void** out_address) {
    if (!m || !name || !out_address) return WAND_P46_ERR_UNRESOLVED_SYMBOL;
    uint64_t address = 0;
    WandP46Status status = find_export(m, name, required_version, &address);
    if (status == WAND_P46_OK) *out_address = (void*)(m->base + (uintptr_t)address);
    return status;
}

static void call_continuation(uintptr_t fn, const uint64_t* args, size_t argc,
                              uint64_t* out) {
    /* The generated module uses the stack below the call site. Force a real
     * frame so the compiler does not keep live values in the red zone. */
    uint64_t stack_guard[16];
    __asm__ volatile("" : "+m"(stack_guard));
    uint64_t a0 = argc > 0 ? args[0] : 0;
    uint64_t a1 = argc > 1 ? args[1] : 0;
    register uint64_t a2 __asm__("rdx") = argc > 2 ? args[2] : 0;
    register uint64_t a3 __asm__("rcx") = argc > 3 ? args[3] : 0;
    register uint64_t a4 __asm__("r8") = argc > 4 ? args[4] : 0;
    register uint64_t a5 __asm__("r9") = argc > 5 ? args[5] : 0;
    uint64_t r0 = 0;
    __asm__ volatile(
        "call *%[target]\n"
        : "=a"(r0), "+d"(a2), "+c"(a3), "+r"(a4), "+r"(a5)
        : [target] "r"(fn), "D"(a0), "S"(a1)
        : "memory", "cc", "r10", "r11", "rbx", "r12", "r13", "r14", "r15",
          "xmm0", "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7",
          "xmm8", "xmm9", "xmm10", "xmm11", "xmm12", "xmm13", "xmm14", "xmm15");
    out[0] = r0;
    out[1] = a2;
    out[2] = a3;
    out[3] = a4;
}

WandP46Status wand_p46_call(WandP46Module* m, const uint64_t* args, size_t argc,
                            uint64_t* results, size_t result_cap,
                            size_t* out_result_count) {
    if (!m) return WAND_P46_ERR_NO_CONTINUATION;
    if (argc > 6) return WAND_P46_ERR_TOO_MANY_ARGUMENTS;
    uint64_t address = 0;
    WandP46Status status = find_export(m, "main", 1, &address);
    if (status != WAND_P46_OK) return WAND_P46_ERR_NO_CONTINUATION;
    if (address >= m->image_size) return WAND_P46_ERR_NO_CONTINUATION;

    uint64_t values[4] = {0, 0, 0, 0};
    call_continuation(m->base + (uintptr_t)address, args, argc, values);
    size_t count = result_cap < 4 ? result_cap : 4;
    for (size_t i = 0; i < count; i++) results[i] = values[i];
    if (out_result_count) *out_result_count = count;
    return WAND_P46_OK;
}

void wand_p46_unload(WandP46Module* m) {
    if (!m) return;
    if (m->image) munmap(m->image, m->mapping_size);
    for (size_t i = 0; i < m->trampoline_count; i++) free(m->trampolines[i].name);
    free(m->trampolines);
    free(m->blob);
    free(m->sections);
    free(m->exports);
    free(m->imports);
    free(m->relocs);
    free(m);
}

const char* wand_p46_status_string(WandP46Status status) {
    switch (status) {
        case WAND_P46_OK: return "ok";
        case WAND_P46_ERR_BAD_MAGIC: return "bad magic";
        case WAND_P46_ERR_UNSUPPORTED_FORMAT_MAJOR: return "unsupported format major";
        case WAND_P46_ERR_UNSUPPORTED_POINTER_SIZE: return "unsupported pointer size";
        case WAND_P46_ERR_UNSUPPORTED_ADDRESS_SIZE: return "unsupported address size";
        case WAND_P46_ERR_ENDIANNESS_MISMATCH: return "endianness mismatch";
        case WAND_P46_ERR_MISSING_DEPENDENCY: return "missing dependency";
        case WAND_P46_ERR_VERSION_MISMATCH: return "version mismatch";
        case WAND_P46_ERR_TRUNCATED: return "truncated module";
        case WAND_P46_ERR_TRUNCATED_SECTION: return "truncated section";
        case WAND_P46_ERR_OVERLAPPING_SECTIONS: return "overlapping sections";
        case WAND_P46_ERR_HEADER_TOO_SMALL: return "header too small";
        case WAND_P46_ERR_CORRUPT_TYPES: return "corrupt type records";
        case WAND_P46_ERR_CORRUPT_STRTAB: return "corrupt string table";
        case WAND_P46_ERR_OUT_OF_MEMORY: return "out of memory";
        case WAND_P46_ERR_IO: return "I/O error";
        case WAND_P46_ERR_NO_CONTINUATION: return "no main continuation";
        case WAND_P46_ERR_UNRESOLVED_SYMBOL: return "unresolved symbol";
        case WAND_P46_ERR_BAD_RELOCATION: return "bad relocation";
        case WAND_P46_ERR_TOO_MANY_ARGUMENTS: return "too many arguments";
        default: return "unknown status";
    }
}

/* ------------------------------------------------------------------ */
/* Hosted jmpto registry                                               */
/* ------------------------------------------------------------------ */

#define WAND_JMPTO_MAX 32

static struct {
    char* name;
    WandP46Module* module;
} g_jmpto_registry[WAND_JMPTO_MAX];

WandP46Status wand_p46_register(const char* name, WandP46Module* module) {
    if (!name || !module) return WAND_P46_ERR_UNRESOLVED_SYMBOL;
    for (size_t i = 0; i < WAND_JMPTO_MAX; i++) {
        if (g_jmpto_registry[i].name && strcmp(g_jmpto_registry[i].name, name) == 0) {
            g_jmpto_registry[i].module = module;
            return WAND_P46_OK;
        }
    }
    for (size_t i = 0; i < WAND_JMPTO_MAX; i++) {
        if (!g_jmpto_registry[i].name) {
            g_jmpto_registry[i].name = dup_string(name);
            g_jmpto_registry[i].module = module;
            return g_jmpto_registry[i].name ? WAND_P46_OK : WAND_P46_ERR_OUT_OF_MEMORY;
        }
    }
    return WAND_P46_ERR_OUT_OF_MEMORY;
}

WandP46Status wand_p46_register_path(const char* name, const char* path,
                                     const WandP46Options* options) {
    FILE* f = fopen(path, "rb");
    if (!f) return WAND_P46_ERR_IO;
    if (fseek(f, 0, SEEK_END) != 0) { fclose(f); return WAND_P46_ERR_IO; }
    long n = ftell(f);
    if (n <= 0) { fclose(f); return WAND_P46_ERR_IO; }
    rewind(f);
    uint8_t* buf = (uint8_t*)malloc((size_t)n);
    if (!buf) { fclose(f); return WAND_P46_ERR_OUT_OF_MEMORY; }
    size_t got = fread(buf, 1, (size_t)n, f);
    fclose(f);
    if (got != (size_t)n) { free(buf); return WAND_P46_ERR_IO; }
    WandP46Status status = WAND_P46_OK;
    WandP46Module* m = wand_p46_load(buf, got, options, &status);
    free(buf);
    if (!m) return status;
    return wand_p46_register(name, m);
}

uint64_t __wand_jmpto_loader(const char* module_name, uint64_t* args,
                             uint64_t argc, uint64_t* results,
                             uint64_t result_cap) {
    if (!module_name) return 0;
    for (size_t i = 0; i < WAND_JMPTO_MAX; i++) {
        if (g_jmpto_registry[i].name && strcmp(g_jmpto_registry[i].name, module_name) == 0) {
            size_t count = 0;
            if (wand_p46_call(g_jmpto_registry[i].module, args, (size_t)argc, results,
                              (size_t)result_cap, &count) != WAND_P46_OK) {
                return 0;
            }
            return (uint64_t)count;
        }
    }
    return 0;
}
