/*
 * x86-64 backend for the Wand language.
 *
 * Design notes:
 *  - The serialized AST, all lookup tables and all temporary buffers live in
 *    one arena.  One arena_free() at the end of compilation releases
 *    everything, so a failed compile cannot leave a trail of allocations.
 *  - Recursion is bounded (wire reader and constant evaluator) and the code
 *    buffer grows geometrically with overflow checks.  No expression tree is
 *    ever cloned.
 *  - Output format and metadata match the old Rust generator byte-for-byte
 *    for equivalent input.
 */
#include "codegen.h"

#include <ctype.h>
#include <math.h>
#include <stdarg.h>
#include <setjmp.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define WIRE_MAGIC 0x57414E44u
#define WIRE_VERSION 1u
#define MAX_READ_DEPTH 256u
#define MAX_CONST_DEPTH 64u
#define PAGE_SIZE 0x1000u
#define LOAD_BASE 0x400078u

/* ------------------------------------------------------------------ */
/* Error handling / arena                                              */
/* ------------------------------------------------------------------ */

typedef struct Gen Gen;
typedef struct Arena Arena;

/* Global allocation/error state is available before Gen is fully defined. */
static Arena* g_arena;
static uint32_t g_error;
static jmp_buf g_fail;

static void cue_error(uint32_t code) {
    g_error = code;
    longjmp(g_fail, 1);
}

typedef struct ArenaChunk {
    struct ArenaChunk* next;
    size_t used;
    size_t cap;
    uint8_t data[];
} ArenaChunk;

struct Arena {
    ArenaChunk* head;
};

static void* arena_alloc(Arena* a, size_t size) {
    size = (size + 7u) & ~(size_t)7u;
    if (size == 0) {
        size = 8;
    }
    ArenaChunk* chunk = a->head;
    if (!chunk || chunk->used + size > chunk->cap) {
        size_t cap = size > 65536u ? size : 65536u;
        chunk = (ArenaChunk*)malloc(sizeof(ArenaChunk) + cap);
        if (!chunk) {
            cue_error(2);
        }
        chunk->next = a->head;
        chunk->used = 0;
        chunk->cap = cap;
        a->head = chunk;
    }
    uint8_t* volatile p = chunk->data + chunk->used;
    chunk->used += size;
    memset(p, 0, size);
    return (void*)p;
}

static void arena_free(Arena* a) {
    ArenaChunk* chunk = a->head;
    while (chunk) {
        ArenaChunk* next = chunk->next;
        free(chunk);
        chunk = next;
    }
    a->head = NULL;
}

/* ------------------------------------------------------------------ */
/* Strings, maps, vectors, byte buffers                                */
/* ------------------------------------------------------------------ */

typedef struct {
    char* ptr;
    uint32_t len;
} WStr;

static WStr wstr_make(const void* data, size_t len) {
    WStr s;
    s.ptr = (char*)arena_alloc(g_arena, len + 1);
    if (len) {
        memcpy(s.ptr, data, len);
    }
    s.ptr[len] = '\0';
    s.len = (uint32_t)len;
    return s;
}

static WStr wstr_lit(const char* literal) {
    return wstr_make(literal, strlen(literal));
}

static int wstr_eq(WStr a, WStr b) {
    return a.len == b.len && (a.len == 0 || memcmp(a.ptr, b.ptr, a.len) == 0);
}

static int wstr_eq_c(WStr a, const char* b) {
    size_t n = strlen(b);
    return a.len == n && (n == 0 || memcmp(a.ptr, b, n) == 0);
}

static int wstr_starts_with(WStr a, const char* prefix) {
    size_t n = strlen(prefix);
    return a.len >= n && memcmp(a.ptr, prefix, n) == 0;
}

static uint32_t hash_bytes(const void* data, size_t len) {
    const uint8_t* p = (const uint8_t*)data;
    uint32_t h = 2166136261u;
    for (size_t i = 0; i < len; i++) {
        h ^= p[i];
        h *= 16777619u;
    }
    return h ? h : 1u;
}

typedef struct {
    char* key;
    uint32_t key_len;
    uint32_t hash;
    uint64_t value;
    uint8_t used;
} MapSlot;

typedef struct {
    MapSlot* slots;
    uint32_t cap;
    uint32_t len;
} Map;

static void map_init(Map* m, uint32_t cap) {
    uint32_t rounded = 8;
    while (rounded < cap) {
        if (rounded > UINT32_MAX / 2) {
            cue_error(2);
        }
        rounded <<= 1;
    }
    cap = rounded;
    m->slots = (MapSlot*)arena_alloc(g_arena, (size_t)cap * sizeof(MapSlot));
    m->cap = cap;
    m->len = 0;
}

static void map_grow(Map* m) {
    MapSlot* old = m->slots;
    uint32_t old_cap = m->cap;
    Map bigger;
    map_init(&bigger, old_cap * 2);
    for (uint32_t i = 0; i < old_cap; i++) {
        if (!old[i].used) {
            continue;
        }
        uint32_t idx = old[i].hash & (bigger.cap - 1);
        while (bigger.slots[idx].used) {
            idx = (idx + 1) & (bigger.cap - 1);
        }
        bigger.slots[idx] = old[i];
        bigger.len++;
    }
    *m = bigger;
}

static void map_put(Map* m, const void* key, uint32_t key_len, uint64_t value) {
    if (!m->slots) {
        map_init(m, 8);
    }
    if ((m->len + 1) * 10 >= (uint64_t)m->cap * 7) {
        map_grow(m);
    }
    uint32_t h = hash_bytes(key, key_len);
    uint32_t idx = h & (m->cap - 1);
    while (m->slots[idx].used) {
        if (m->slots[idx].hash == h && m->slots[idx].key_len == key_len &&
            (key_len == 0 || memcmp(m->slots[idx].key, key, key_len) == 0)) {
            m->slots[idx].value = value;
            return;
        }
        idx = (idx + 1) & (m->cap - 1);
    }
    m->slots[idx].used = 1;
    m->slots[idx].hash = h;
    m->slots[idx].key = (char*)arena_alloc(g_arena, key_len ? key_len : 1);
    memcpy(m->slots[idx].key, key, key_len);
    m->slots[idx].key_len = key_len;
    m->slots[idx].value = value;
    m->len++;
}

static int map_get(Map* m, const void* key, uint32_t key_len, uint64_t* out) {
    if (!m || !m->slots) {
        return 0;
    }
    uint32_t h = hash_bytes(key, key_len);
    uint32_t idx = h & (m->cap - 1);
    uint32_t probes = 0;
    while (m->slots[idx].used && probes++ < m->cap) {
        if (m->slots[idx].hash == h && m->slots[idx].key_len == key_len &&
            (key_len == 0 || memcmp(m->slots[idx].key, key, key_len) == 0)) {
            if (out) {
                *out = m->slots[idx].value;
            }
            return 1;
        }
        idx = (idx + 1) & (m->cap - 1);
    }
    return 0;
}

static int map_has(Map* m, const void* key, uint32_t key_len) {
    return map_get(m, key, key_len, NULL);
}

static void map_put_str(Map* m, WStr key, uint64_t value) {
    map_put(m, key.ptr, key.len, value);
}

static int map_get_str(Map* m, WStr key, uint64_t* out) {
    return map_get(m, key.ptr, key.len, out);
}

#define PTR(v) ((uint64_t)(uintptr_t)(v))
#define AS_PTR(t, v) ((t*)(uintptr_t)(v))

typedef struct {
    uint8_t* data;
    size_t len;
    size_t cap;
} Buf;

static void buf_reserve(Buf* b, size_t extra) {
    if (b->len + extra <= b->cap) {
        return;
    }
    size_t need = b->len + extra;
    size_t cap = b->cap ? b->cap : 256;
    while (cap < need) {
        if (cap > SIZE_MAX / 2) {
            cue_error(2);
        }
        cap *= 2;
    }
    uint8_t* next = (uint8_t*)arena_alloc(g_arena, cap);
    if (b->len) {
        memcpy(next, b->data, b->len);
    }
    b->data = next;
    b->cap = cap;
}

static void buf_bytes(Buf* b, const void* data, size_t n) {
    if (!n) {
        return;
    }
    buf_reserve(b, n);
    memcpy(b->data + b->len, data, n);
    b->len += n;
}

static void buf_u8(Buf* b, uint8_t v) {
    buf_reserve(b, 1);
    b->data[b->len++] = v;
}

static void buf_u16(Buf* b, uint16_t v) {
    uint8_t tmp[2] = {(uint8_t)v, (uint8_t)(v >> 8)};
    buf_bytes(b, tmp, 2);
}

static void buf_u32(Buf* b, uint32_t v) {
    uint8_t tmp[4] = {
        (uint8_t)v, (uint8_t)(v >> 8), (uint8_t)(v >> 16), (uint8_t)(v >> 24)};
    buf_bytes(b, tmp, 4);
}

static void buf_u64(Buf* b, uint64_t v) {
    uint8_t tmp[8];
    for (int i = 0; i < 8; i++) {
        tmp[i] = (uint8_t)(v >> (i * 8));
    }
    buf_bytes(b, tmp, 8);
}

static void buf_i32(Buf* b, int32_t v) {
    buf_u32(b, (uint32_t)v);
}

static void buf_printf(Buf* b, const char* fmt, ...) {
    char tmp[512];
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(tmp, sizeof(tmp), fmt, ap);
    va_end(ap);
    if (n < 0) {
        return;
    }
    if ((size_t)n < sizeof(tmp)) {
        buf_bytes(b, tmp, (size_t)n);
        return;
    }
    char* big = (char*)arena_alloc(g_arena, (size_t)n + 1);
    va_start(ap, fmt);
    vsnprintf(big, (size_t)n + 1, fmt, ap);
    va_end(ap);
    buf_bytes(b, big, (size_t)n);
}

#define DEFINE_VEC(name, type)                                                        \
    typedef struct {                                                                  \
        type* v;                                                                      \
        size_t n;                                                                     \
        size_t cap;                                                                   \
    } name;                                                                           \
    static void name##_push(name* a, type item) {                                     \
        if (a->n == a->cap) {                                                         \
            size_t cap = a->cap ? a->cap * 2 : 8;                                     \
            type* v = (type*)arena_alloc(g_arena, cap * sizeof(type));              \
            if (a->n) {                                                               \
                memcpy(v, a->v, a->n * sizeof(type));                                 \
            }                                                                         \
            a->v = v;                                                                 \
            a->cap = cap;                                                             \
        }                                                                             \
        a->v[a->n++] = item;                                                          \
    }

DEFINE_VEC(WStrVec, WStr)
DEFINE_VEC(SizeVec, size_t)

/* ------------------------------------------------------------------ */
/* AST                                                                 */
/* ------------------------------------------------------------------ */

typedef struct WType WType;
typedef struct WExpr WExpr;
typedef struct WStmt WStmt;

struct WType {
    uint8_t tag; /* 0..13, see codegen.rs */
    WType* inner;
    WStr name;
    uint64_t count;
};

struct WExpr {
    uint8_t tag; /* 0..13 */
    WStr str;
    WStr str2;
    uint64_t num;
    uint8_t flag;
    WExpr* a;
    WExpr* b;
    WExpr** args;
    size_t arg_count;
};

typedef struct {
    WType* type;
    WExpr* expr;
} WReturnValue;

typedef struct {
    WExpr* expr;
    WStmt** body;
    size_t body_count;
} WCase;

struct WStmt {
    uint8_t tag; /* 0..12 */
    WStr name;
    WType* type;
    uint8_t access;
    uint8_t flag;
    uint8_t flag2;
    uint32_t align;
    WExpr* e1;
    WExpr* e2;
    WStmt* s1;
    WStmt* s2;
    WStmt** stmts1;
    size_t stmt1_count;
    WStmt** stmts2;
    size_t stmt2_count;
    WExpr** exprs;
    size_t expr_count;
    WReturnValue* returns;
    size_t return_count;
    WCase* cases;
    size_t case_count;
};

typedef struct {
    WStr name;
    WType* type;
    uint8_t access;
    int has_init;
    WExpr* init;
    uint32_t align;
} WVar;

typedef struct {
    WStr name;
    WVar* vars;
    size_t count;
    uint32_t align;
    uint8_t is_ro;
    uint8_t is_noinit;
} WSection;

typedef struct {
    WStr name;
    WType* type;
    uint8_t access;
    uint32_t version_added;
    uint32_t version_removed;
} WField;

typedef struct {
    WStr name;
    uint32_t version;
    WField* fields;
    size_t field_count;
    uint8_t is_union;
    uint8_t is_packed;
    uint32_t align;
} WStruct;

typedef struct {
    WStr name;
    uint64_t value;
} WEnumValue;

typedef struct {
    WStr name;
    WEnumValue* values;
    size_t count;
} WEnum;

typedef struct {
    WStr name;
    WExpr* value;
} WConst;

typedef struct {
    WType* type;
    WStr name;
    uint8_t access;
} WParam;

typedef struct {
    WStr name;
    uint32_t version;
    WParam* params;
    size_t param_count;
    WType** returns;
    size_t return_count;
    WStmt** body;
    size_t body_count;
    int has_body;
    uint8_t is_extern;
    uint8_t is_export;
    uint8_t is_irq;
} WFunc;

typedef struct {
    uint8_t use_os;
    WStr* typedef_names;
    WType** typedef_types;
    size_t typedef_count;
    WStruct* structs;
    size_t struct_count;
    WEnum* enums;
    size_t enum_count;
    WConst* consts;
    size_t const_count;
    WSection* sections;
    size_t section_count;
    WFunc* funcs;
    size_t func_count;
} WProgram;

/* Runtime aggregate initializer helpers (defined after InitLeaf below). */
static WStruct* find_struct_by_name(Gen* g, WStr name);
static uint32_t emit_runtime_init_rbx(Gen* g, WExpr* init, WType* type,
                                      uint32_t max_bytes);
static uint32_t emit_runtime_init_local(Gen* g, WExpr* init, WType* type,
                                        uint32_t off, uint32_t max_bytes);

/* ------------------------------------------------------------------ */
/* Wire reader                                                         */
/* ------------------------------------------------------------------ */

typedef struct {
    const uint8_t* p;
    size_t len;
    size_t pos;
    uint32_t depth;
} Reader;

WStmt* rd_stmt(Reader* r);

static void rd_need(Reader* r, size_t n) {
    if (r->pos + n > r->len) {
        cue_error(1);
    }
}

static uint8_t rd_u8(Reader* r) {
    rd_need(r, 1);
    return r->p[r->pos++];
}

static uint16_t rd_u16(Reader* r) {
    rd_need(r, 2);
    uint16_t v = (uint16_t)r->p[r->pos] | ((uint16_t)r->p[r->pos + 1] << 8);
    r->pos += 2;
    return v;
}

static uint32_t rd_u32(Reader* r) {
    rd_need(r, 4);
    uint32_t v = 0;
    for (int i = 0; i < 4; i++) {
        v |= (uint32_t)r->p[r->pos + i] << (i * 8);
    }
    r->pos += 4;
    return v;
}

static uint64_t rd_u64(Reader* r) {
    rd_need(r, 8);
    uint64_t v = 0;
    for (int i = 0; i < 8; i++) {
        v |= (uint64_t)r->p[r->pos + i] << (i * 8);
    }
    r->pos += 8;
    return v;
}

static void rd_enter(Reader* r) {
    if (++r->depth > MAX_READ_DEPTH) {
        cue_error(3);
    }
}

static void rd_leave(Reader* r) {
    r->depth--;
}

static WStr rd_str(Reader* r) {
    uint32_t len = rd_u32(r);
    rd_need(r, len);
    WStr s = wstr_make(r->p + r->pos, len);
    r->pos += len;
    return s;
}

static WType* rd_type(Reader* r) {
    rd_enter(r);
    WType* t = (WType*)arena_alloc(g_arena, sizeof(WType));
    t->tag = rd_u8(r) & 0xFu;
    switch (t->tag) {
        case 10:
            t->inner = rd_type(r);
            break;
        case 11:
            t->name = rd_str(r);
            break;
        case 12:
            t->inner = rd_type(r);
            t->count = rd_u64(r);
            break;
        case 13:
            t->name = rd_str(r);
            t->inner = rd_type(r);
            break;
        default:
            break;
    }
    rd_leave(r);
    return t;
}

static WExpr* rd_expr(Reader* r) {
    rd_enter(r);
    WExpr* e = (WExpr*)arena_alloc(g_arena, sizeof(WExpr));
    e->tag = rd_u8(r);
    switch (e->tag) {
        case 0:
        case 1:
            e->num = rd_u64(r);
            break;
        case 2:
        case 3:
        case 4:
        case 11:
            e->str = rd_str(r);
            break;
        case 5:
            e->a = rd_expr(r);
            break;
        case 6:
            e->a = rd_expr(r);
            e->str2 = rd_str(r);
            e->flag = rd_u8(r);
            break;
        case 7:
            e->str = rd_str(r);
            e->str2 = rd_str(r);
            break;
        case 8:
            e->a = rd_expr(r);
            e->str = rd_str(r);
            e->b = rd_expr(r);
            break;
        case 9: {
            e->str = rd_str(r);
            uint32_t n = rd_u32(r);
            rd_need(r, (size_t)n * 4);
            e->args = (WExpr**)arena_alloc(g_arena, (size_t)(n ? n : 1) * sizeof(WExpr*));
            e->arg_count = n;
            for (uint32_t i = 0; i < n; i++) {
                e->args[i] = rd_expr(r);
            }
            break;
        }
        case 10:
            e->a = rd_expr(r);
            e->b = rd_expr(r);
            break;
        case 12: {
            uint32_t n = rd_u32(r);
            rd_need(r, (size_t)n * 4);
            e->args = (WExpr**)arena_alloc(g_arena, (size_t)(n ? n : 1) * sizeof(WExpr*));
            e->arg_count = n;
            for (uint32_t i = 0; i < n; i++) {
                e->args[i] = rd_expr(r);
            }
            break;
        }
        case 13:
            break;
        default:
            cue_error(1);
    }
    rd_leave(r);
    return e;
}

static WStmt** rd_stmt_list(Reader* r, size_t* count_out) {
    uint32_t n = rd_u32(r);
    rd_need(r, (size_t)n * 4);
    WStmt** list = (WStmt**)arena_alloc(g_arena, (size_t)(n ? n : 1) * sizeof(WStmt*));
    *count_out = n;
    for (uint32_t i = 0; i < n; i++) {
        list[i] = NULL;
    }
    for (uint32_t i = 0; i < n; i++) {
        /* forward declaration through a small local reader helper */
        extern WStmt* rd_stmt(Reader*);
        list[i] = rd_stmt(r);
    }
    return list;
}

static WVar rd_var(Reader* r) {
    WVar v;
    memset(&v, 0, sizeof(v));
    v.name = rd_str(r);
    v.type = rd_type(r);
    v.access = rd_u8(r);
    v.has_init = rd_u8(r) != 0;
    if (v.has_init) {
        v.init = rd_expr(r);
    }
    v.align = rd_u32(r);
    return v;
}

WStmt* rd_stmt(Reader* r) {
    rd_enter(r);
    WStmt* s = (WStmt*)arena_alloc(g_arena, sizeof(WStmt));
    s->tag = rd_u8(r);
    switch (s->tag) {
        case 0:
            s->name = rd_str(r);
            s->type = rd_type(r);
            s->access = rd_u8(r);
            s->flag = rd_u8(r);
            if (s->flag) {
                s->e1 = rd_expr(r);
            }
            s->align = rd_u32(r);
            break;
        case 1: {
            uint32_t n = rd_u32(r);
            rd_need(r, (size_t)n * 4);
            s->exprs = (WExpr**)arena_alloc(g_arena, (size_t)(n ? n : 1) * sizeof(WExpr*));
            s->expr_count = n;
            for (uint32_t i = 0; i < n; i++) {
                s->exprs[i] = rd_expr(r);
            }
            s->e1 = rd_expr(r);
            break;
        }
        case 2:
            s->e1 = rd_expr(r);
            s->stmts1 = rd_stmt_list(r, &s->stmt1_count);
            s->flag = rd_u8(r);
            if (s->flag) {
                s->stmts2 = rd_stmt_list(r, &s->stmt2_count);
            }
            break;
        case 3:
            s->e1 = rd_expr(r);
            s->stmts1 = rd_stmt_list(r, &s->stmt1_count);
            break;
        case 4:
            s->flag = rd_u8(r);
            if (s->flag) {
                s->s1 = rd_stmt(r);
            }
            s->e1 = rd_expr(r);
            s->flag2 = rd_u8(r);
            if (s->flag2) {
                s->s2 = rd_stmt(r);
            }
            s->stmts1 = rd_stmt_list(r, &s->stmt1_count);
            break;
        case 5: {
            s->name = rd_str(r);
            uint32_t n = rd_u32(r);
            rd_need(r, (size_t)n * 4);
            s->stmts1 = (WStmt**)arena_alloc(g_arena, (size_t)(n ? n : 1) * sizeof(WStmt*));
            s->stmt1_count = n;
            for (uint32_t i = 0; i < n; i++) {
                s->stmts1[i] = rd_stmt(r);
            }
            s->flag = rd_u8(r);
            if (s->flag) {
                s->stmts2 = rd_stmt_list(r, &s->stmt2_count);
            }
            break;
        }
        case 6: {
            uint32_t n = rd_u32(r);
            rd_need(r, (size_t)n * 5);
            s->returns = (WReturnValue*)arena_alloc(g_arena, (size_t)(n ? n : 1) * sizeof(WReturnValue));
            s->return_count = n;
            for (uint32_t i = 0; i < n; i++) {
                s->returns[i].type = rd_type(r);
                s->returns[i].expr = rd_expr(r);
            }
            break;
        }
        case 7:
            s->stmts1 = rd_stmt_list(r, &s->stmt1_count);
            break;
        case 8:
            s->name = rd_str(r);
            break;
        case 9: {
            s->e1 = rd_expr(r);
            uint32_t n = rd_u32(r);
            rd_need(r, (size_t)n * 4);
            s->cases = (WCase*)arena_alloc(g_arena, (size_t)(n ? n : 1) * sizeof(WCase));
            s->case_count = n;
            for (uint32_t i = 0; i < n; i++) {
                s->cases[i].expr = rd_expr(r);
                s->cases[i].body = rd_stmt_list(r, &s->cases[i].body_count);
            }
            s->flag = rd_u8(r);
            if (s->flag) {
                s->stmts2 = rd_stmt_list(r, &s->stmt2_count);
            }
            break;
        }
        case 10:
            s->e1 = rd_expr(r);
            break;
        case 11:
        case 12:
            break;
        default:
            cue_error(1);
    }
    rd_leave(r);
    return s;
}

static WProgram rd_program(Reader* r) {
    WProgram p;
    memset(&p, 0, sizeof(p));
    if (rd_u32(r) != WIRE_MAGIC || rd_u16(r) != WIRE_VERSION) {
        cue_error(1);
    }
    p.use_os = rd_u8(r);

    uint32_t n = rd_u32(r);
    p.typedef_count = n;
    p.typedef_names = (WStr*)arena_alloc(g_arena, (size_t)(n ? n : 1) * sizeof(WStr));
    p.typedef_types = (WType**)arena_alloc(g_arena, (size_t)(n ? n : 1) * sizeof(WType*));
    for (uint32_t i = 0; i < n; i++) {
        p.typedef_names[i] = rd_str(r);
        p.typedef_types[i] = rd_type(r);
    }

    n = rd_u32(r);
    p.struct_count = n;
    p.structs = (WStruct*)arena_alloc(g_arena, (size_t)(n ? n : 1) * sizeof(WStruct));
    for (uint32_t i = 0; i < n; i++) {
        WStruct* s = &p.structs[i];
        s->name = rd_str(r);
        s->version = rd_u32(r);
        uint32_t fields = rd_u32(r);
        s->field_count = fields;
        s->fields = (WField*)arena_alloc(g_arena, (size_t)(fields ? fields : 1) * sizeof(WField));
        for (uint32_t j = 0; j < fields; j++) {
            s->fields[j].name = rd_str(r);
            s->fields[j].type = rd_type(r);
            s->fields[j].access = rd_u8(r);
            s->fields[j].version_added = rd_u32(r);
            s->fields[j].version_removed = rd_u32(r);
        }
        s->is_union = rd_u8(r);
        s->is_packed = rd_u8(r);
        s->align = rd_u32(r);
    }

    n = rd_u32(r);
    p.enum_count = n;
    p.enums = (WEnum*)arena_alloc(g_arena, (size_t)(n ? n : 1) * sizeof(WEnum));
    for (uint32_t i = 0; i < n; i++) {
        WEnum* e = &p.enums[i];
        e->name = rd_str(r);
        uint32_t vals = rd_u32(r);
        e->count = vals;
        e->values = (WEnumValue*)arena_alloc(g_arena, (size_t)(vals ? vals : 1) * sizeof(WEnumValue));
        for (uint32_t j = 0; j < vals; j++) {
            e->values[j].name = rd_str(r);
            e->values[j].value = rd_u64(r);
        }
    }

    n = rd_u32(r);
    p.const_count = n;
    p.consts = (WConst*)arena_alloc(g_arena, (size_t)(n ? n : 1) * sizeof(WConst));
    for (uint32_t i = 0; i < n; i++) {
        p.consts[i].name = rd_str(r);
        p.consts[i].value = rd_expr(r);
    }

    n = rd_u32(r);
    p.section_count = n;
    p.sections = (WSection*)arena_alloc(g_arena, (size_t)(n ? n : 1) * sizeof(WSection));
    for (uint32_t i = 0; i < n; i++) {
        WSection* sect = &p.sections[i];
        sect->name = rd_str(r);
        uint32_t vars = rd_u32(r);
        sect->count = vars;
        sect->vars = (WVar*)arena_alloc(g_arena, (size_t)(vars ? vars : 1) * sizeof(WVar));
        for (uint32_t j = 0; j < vars; j++) {
            sect->vars[j] = rd_var(r);
        }
        sect->align = rd_u32(r);
        sect->is_ro = rd_u8(r);
        sect->is_noinit = rd_u8(r);
    }

    n = rd_u32(r);
    p.func_count = n;
    p.funcs = (WFunc*)arena_alloc(g_arena, (size_t)(n ? n : 1) * sizeof(WFunc));
    for (uint32_t i = 0; i < n; i++) {
        WFunc* f = &p.funcs[i];
        f->name = rd_str(r);
        f->version = rd_u32(r);
        uint32_t params = rd_u32(r);
        f->param_count = params;
        f->params = (WParam*)arena_alloc(g_arena, (size_t)(params ? params : 1) * sizeof(WParam));
        for (uint32_t j = 0; j < params; j++) {
            f->params[j].type = rd_type(r);
            f->params[j].name = rd_str(r);
            f->params[j].access = rd_u8(r);
        }
        uint32_t rets = rd_u32(r);
        f->return_count = rets;
        f->returns = (WType**)arena_alloc(g_arena, (size_t)(rets ? rets : 1) * sizeof(WType*));
        for (uint32_t j = 0; j < rets; j++) {
            f->returns[j] = rd_type(r);
        }
        f->has_body = rd_u8(r) != 0;
        if (f->has_body) {
            f->body = rd_stmt_list(r, &f->body_count);
        }
        f->is_extern = rd_u8(r);
        f->is_export = rd_u8(r);
        f->is_irq = rd_u8(r);
    }
    return p;
}

/* ------------------------------------------------------------------ */
/* Generator state                                                     */
/* ------------------------------------------------------------------ */

typedef struct {
    Map field_types;  /* field name -> WType* */
    Map field_offsets;/* field name -> byte offset */
    uint32_t size;
    uint32_t align;
    uint8_t resolved;
} Layout;

typedef struct {
    WStr* names;
    size_t count;
} VolatileSet;

typedef struct {
    size_t pos;
    WStr key;
    uint8_t is_rip;
    uint8_t patch_end;
    uint8_t is_global; /* pos is an offset inside global data, not code */
} AddrPatch;

typedef struct {
    size_t pos;
    WStr target;
} CallPatch;

typedef struct {
    size_t pos; /* offset inside global data */
    WStr key;
} GlobalPtrFixup;

typedef struct {
    size_t* cont;
    size_t cont_n, cont_cap;
    size_t* brk;
    size_t brk_n, brk_cap;
    /* Number of active match-scrutinee stack slots at loop entry. */
    size_t match_depth;
} LoopCtx;

typedef struct {
    WStr name;
    uint8_t reg;
} RegLocal;

typedef struct {
    WStr name;
    uint8_t reg;
} XmmLocal;

DEFINE_VEC(AddrPatchVec, AddrPatch)
DEFINE_VEC(GlobalPtrFixupVec, GlobalPtrFixup)
DEFINE_VEC(CallPatchVec, CallPatch)
DEFINE_VEC(LoopCtxVec, LoopCtx)
DEFINE_VEC(RegLocalVec, RegLocal)
DEFINE_VEC(XmmLocalVec, XmmLocal)
DEFINE_VEC(LayoutVec, Layout)
DEFINE_VEC(VolatileSetVec, VolatileSet)

struct Gen {
    Arena arena;
    jmp_buf fail;
    uint32_t error;
    WProgram prog;
    Buf code;
    Buf global;

    Map locals;
    Map local_access;
    Map local_types;
    Map local_is_param; /* set for function parameters; array-typed params are pointers */
    Map escaped_locals; /* locals whose address escapes and can alias stack slots */
    Map next_offsets; /* not used; kept for symmetry with function state */
    uint32_t next_offset;

    Map global_offsets;
    WStrVec global_keys;
    Map section_volatile;
    Map section_var_sizes;
    Map section_types;

    LayoutVec layouts;
    Map struct_layouts;
    Map struct_fields;
    Map struct_versions;
    Map struct_alignments;
    Map struct_volatile;
    VolatileSetVec volatile_sets;
    Map typedefs;

    WStrVec string_keys;
    Map string_constants;
    WStrVec float_keys;
    Map float_constants;

    Map constants;
    Map enums;
    Map function_signatures; /* function name -> WFunc* */
    Map function_return_types;
    Map function_offsets;

    AddrPatchVec address_patches;
    GlobalPtrFixupVec global_ptr_fixups;
    CallPatchVec call_patches;
    size_t patch_generation; /* incremented whenever a branch/call patch is recorded */
    size_t last_rax_store_pos; /* start of last rax-to-local store */
    uint32_t last_rax_store_len; /* byte length of that store (0 = none) */
    uint32_t last_rax_store_off; /* local stack offset stored to */
    size_t last_rax_store_gen; /* patch_generation at store time */
    LoopCtxVec loop_stack;
    RegLocalVec reg_locals; /* canonical loop induction variables kept in regs */
    XmmLocalVec xmm_locals; /* f64 locals kept in XMM registers inside loops */
    size_t stack_depth;
    size_t match_depth; /* active `match` scrutinee slots on the stack */
    uint8_t current_is_irq;
    uint8_t r10_busy; /* an outer expression is keeping a live value in r10 */
    WFunc* current_function; /* signature for return-value deref classification */
    WStr suppress_deref_var; /* *i local whose pointer slot is being updated */

    WandOutputFormat format;
    WStr entry_name;
};

/* Forward declarations for mutually recursive helpers. */
static WStr wstr_pair(WStr a, WStr b);
static WStr find_global_key(Gen* g, WStr name);
static uint64_t eval_const_depth(Gen* g, WExpr* e, uint32_t depth);
static int is_signed_expr(Gen* g, WExpr* e);
static int expr_has_call_deep(WExpr* e);
static void compile_stmt(Gen* g, WStmt* s);
static void compile_expr(Gen* g, WExpr* e, uint8_t reg, int deref_ptr);
static int index_sib_ready(Gen* g, WExpr* e, uint8_t* base, uint8_t* index,
                           int* scale_bits);
static void compile_address(Gen* g, WExpr* e, uint8_t reg);
static void compile_bool_expr(Gen* g, WExpr* e);
static int simple_left(Gen* g, WExpr* e);
static int is_float_expr(Gen* g, WExpr* e);
static int is_float_value_expr(Gen* g, WExpr* e, int deref_ptr);
static int is_f64_pointer_modifier_expr(Gen* g, WExpr* e);
static int rax_only_expr(Gen* g, WExpr* e);
static int expr_has_call(Gen* g, WExpr* e);
static int is_local_self_addsub(Gen* g, WExpr* target, WExpr* e,
                                uint8_t* op, int64_t* imm);

/* Look up a local currently kept in a register. Newest binding wins. */
static int reg_local_lookup(Gen* g, WStr name, uint8_t* reg) {
    for (size_t i = g->reg_locals.n; i > 0; i--) {
        RegLocal* r = &g->reg_locals.v[i - 1];
        if (wstr_eq(r->name, name)) {
            if (reg) *reg = r->reg;
            return 1;
        }
    }
    return 0;
}

/* Look up an f64 local currently kept in an XMM register. */
static int xmm_local_lookup(Gen* g, WStr name, uint8_t* reg) {
    for (size_t i = g->xmm_locals.n; i > 0; i--) {
        XmmLocal* x = &g->xmm_locals.v[i - 1];
        if (wstr_eq(x->name, name)) {
            if (reg) *reg = x->reg;
            return 1;
        }
    }
    return 0;
}

/* mov src -> dst for 64-bit GPRs. */
static void emit_mov_reg_to_reg(Buf* b, uint8_t src, uint8_t dst) {
    if (src == dst) return;
    buf_u8(b, (uint8_t)(0x48 | (src >= 8 ? 4 : 0) | (dst >= 8 ? 1 : 0)));
    buf_u8(b, 0x89);
    buf_u8(b, (uint8_t)(0xC0 | ((src & 7) << 3) | (dst & 7)));
}

/* mov reg -> disp(%rbp) for 64-bit GPRs. */
static void emit_mov_reg_to_local(Buf* b, uint8_t reg, uint32_t off) {
    int32_t neg = -(int32_t)off;
    buf_u8(b, (uint8_t)(0x48 | (reg >= 8 ? 4 : 0)));
    buf_u8(b, 0x89);
    if (neg >= -128 && neg <= 127) {
        buf_u8(b, (uint8_t)(0x45 | ((reg & 7) << 3)));
        buf_u8(b, (uint8_t)neg);
    } else {
        buf_u8(b, (uint8_t)(0x85 | ((reg & 7) << 3)));
        buf_u32(b, (uint32_t)neg);
    }
}

/* inc/dec/add/sub on a 64-bit register. op: 0=inc, 1=dec, 2=add imm,
 * 3=sub imm. imm is a positive magnitude. */
static void emit_addsub_reg(Buf* b, uint8_t reg, uint8_t op, int64_t imm) {
    uint8_t rex = (uint8_t)(0x48 | (reg >= 8 ? 1 : 0));
    uint8_t low = reg & 7;
    if (op == 0 || op == 1) {
        buf_u8(b, rex);
        buf_u8(b, 0xFF);
        buf_u8(b, (uint8_t)((op == 0 ? 0xC0 : 0xC8) | low));
        return;
    }
    uint8_t group = op == 2 ? 0 : 5;
    if (imm <= 127) {
        buf_u8(b, rex); buf_u8(b, 0x83);
        buf_u8(b, (uint8_t)(0xC0 | (group << 3) | low));
        buf_u8(b, (uint8_t)imm);
    } else {
        buf_u8(b, rex); buf_u8(b, 0x81);
        buf_u8(b, (uint8_t)(0xC0 | (group << 3) | low));
        buf_u32(b, (uint32_t)imm);
    }
}

/* ------------------------------------------------------------------ */
/* Emitters                                                            */
/* ------------------------------------------------------------------ */

static void patch_i32(Buf* b, size_t pos, int32_t v) {
    b->data[pos + 0] = (uint8_t)v;
    b->data[pos + 1] = (uint8_t)((uint32_t)v >> 8);
    b->data[pos + 2] = (uint8_t)((uint32_t)v >> 16);
    b->data[pos + 3] = (uint8_t)((uint32_t)v >> 24);
}

static void move_rax_to_reg(Buf* b, uint8_t reg) {
    if (reg == 0) {
        return;
    }
    buf_u8(b, reg >= 8 ? 0x49 : 0x48);
    buf_u8(b, 0x89);
    buf_u8(b, (uint8_t)(0xC0 | (reg & 7)));
}

static void emit_mov_imm64(Buf* b, uint8_t reg, uint64_t value) {
    if (value <= 0xFFFFFFFFu) {
        if (reg >= 8) {
            buf_u8(b, 0x41);
            buf_u8(b, (uint8_t)(0xB8 + (reg & 7)));
        } else {
            buf_u8(b, (uint8_t)(0xB8 + reg));
        }
        buf_u32(b, (uint32_t)value);
    } else {
        if (reg >= 8) {
            buf_u8(b, 0x49);
            buf_u8(b, (uint8_t)(0xB8 + (reg & 7)));
        } else {
            buf_u8(b, 0x48);
            buf_u8(b, (uint8_t)(0xB8 + reg));
        }
        buf_u64(b, value);
    }
}

static void emit_mem_op(Buf* b, uint8_t opcode, uint8_t reg, uint32_t offset) {
    uint8_t low = reg & 7;
    buf_u8(b, reg >= 8 ? 0x4C : 0x48);
    buf_u8(b, opcode);
    int32_t neg = -(int32_t)offset;
    if (neg >= -128 && neg <= 127) {
        buf_u8(b, (uint8_t)(0x45 | (low << 3)));
        buf_u8(b, (uint8_t)neg);
    } else {
        buf_u8(b, (uint8_t)(0x85 | (low << 3)));
        buf_u32(b, (uint32_t)neg);
    }
}

static void emit_mem_load(Buf* b, uint8_t reg, uint32_t offset, uint32_t size,
                          int is_signed) {
    uint8_t low = reg & 7;
    int32_t neg = -(int32_t)offset;
    uint8_t modrm = (uint8_t)(neg >= -128 && neg <= 127 ? 0x45 | (low << 3) : 0x85 | (low << 3));

    switch (size) {
        case 1:
            buf_u8(b, reg >= 8 ? 0x4C : 0x48);
            buf_u8(b, 0x0F);
            buf_u8(b, is_signed ? 0xBE : 0xB6);
            buf_u8(b, modrm);
            break;
        case 2:
            buf_u8(b, reg >= 8 ? 0x4C : 0x48);
            buf_u8(b, 0x0F);
            buf_u8(b, is_signed ? 0xBF : 0xB7);
            buf_u8(b, modrm);
            break;
        case 4:
            if (is_signed) {
                /* movsxd r64, r/m32 — a plain 32-bit mov would zero-extend. */
                buf_u8(b, reg >= 8 ? 0x49 : 0x48);
                buf_u8(b, 0x63);
                buf_u8(b, modrm);
            } else {
                if (reg >= 8) {
                    buf_u8(b, 0x44);
                }
                buf_u8(b, 0x8B);
                buf_u8(b, modrm);
            }
            break;
        default:
            buf_u8(b, reg >= 8 ? 0x4C : 0x48);
            buf_u8(b, 0x8B);
            buf_u8(b, modrm);
            break;
    }
    if (neg >= -128 && neg <= 127) {
        buf_u8(b, (uint8_t)neg);
    } else {
        buf_u32(b, (uint32_t)neg);
    }
}

static void emit_mem_store(Buf* b, uint8_t reg, uint32_t offset, uint32_t size) {
    uint8_t low = reg & 7;
    int32_t neg = -(int32_t)offset;
    uint8_t modrm = (uint8_t)(neg >= -128 && neg <= 127 ? 0x45 | (low << 3) : 0x85 | (low << 3));

    switch (size) {
        case 1:
            if (reg >= 8) {
                buf_u8(b, 0x44);
            } else if (reg >= 4) {
                buf_u8(b, 0x40);
            }
            buf_u8(b, 0x88);
            buf_u8(b, modrm);
            break;
        case 2:
            buf_u8(b, 0x66);
            if (reg >= 8) {
                buf_u8(b, 0x44);
            }
            buf_u8(b, 0x89);
            buf_u8(b, modrm);
            break;
        case 4:
            if (reg >= 8) {
                buf_u8(b, 0x44);
            }
            buf_u8(b, 0x89);
            buf_u8(b, modrm);
            break;
        default:
            buf_u8(b, reg >= 8 ? 0x4C : 0x48);
            buf_u8(b, 0x89);
            buf_u8(b, modrm);
            break;
    }
    if (neg >= -128 && neg <= 127) {
        buf_u8(b, (uint8_t)neg);
    } else {
        buf_u32(b, (uint32_t)neg);
    }
}

/* Store rax to [rbx + disp]. Used when copying aggregate values into a
 * computed destination address. */
static void emit_rbx_mem_store(Buf* b, uint32_t disp, uint32_t size) {
    uint8_t modrm = (uint8_t)(disp <= 127 ? 0x43 : 0x83);
    switch (size) {
        case 1:
            buf_u8(b, 0x88); buf_u8(b, modrm); break;
        case 2:
            buf_u8(b, 0x66); buf_u8(b, 0x89); buf_u8(b, modrm); break;
        case 4:
            buf_u8(b, 0x89); buf_u8(b, modrm); break;
        default:
            buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, modrm); break;
    }
    if (disp <= 127) buf_u8(b, (uint8_t)disp);
    else buf_u32(b, disp);
}

static void emit_store_xmm_param(Buf* b, uint8_t xmm, uint32_t offset) {
    int32_t neg = -(int32_t)offset;
    buf_u8(b, 0xF2);
    if (xmm >= 8) {
        buf_u8(b, 0x44);
    }
    buf_u8(b, 0x0F);
    buf_u8(b, 0x11);
    buf_u8(b, (uint8_t)(neg >= -128 && neg <= 127 ? 0x45 | ((xmm & 7) << 3)
                                                 : 0x85 | ((xmm & 7) << 3)));
    if (neg >= -128 && neg <= 127) {
        buf_u8(b, (uint8_t)neg);
    } else {
        buf_u32(b, (uint32_t)neg);
    }
}

static void emit_load_stack_param(Buf* b, uint32_t offset, uint32_t size, size_t index) {
    uint32_t disp = 16u + (uint32_t)index * 8u;
    buf_u8(b, 0x48);
    buf_u8(b, 0x8B);
    buf_u8(b, 0x85);
    buf_u32(b, disp);
    emit_mem_store(b, 0, offset, size);
}

static void emit_mov_from_stack(Buf* b, uint8_t reg, size_t offset) {
    buf_u8(b, reg >= 8 ? 0x4C : 0x48);
    buf_u8(b, 0x8B);
    if (offset < 128) {
        buf_u8(b, (uint8_t)(0x44 | ((reg & 7) << 3)));
        buf_u8(b, 0x24);
        buf_u8(b, (uint8_t)offset);
    } else {
        buf_u8(b, (uint8_t)(0x84 | ((reg & 7) << 3)));
        buf_u8(b, 0x24);
        buf_u32(b, (uint32_t)offset);
    }
}

static void emit_mov_to_stack(Buf* b, uint8_t reg, size_t offset) {
    buf_u8(b, reg >= 8 ? 0x4C : 0x48);
    buf_u8(b, 0x89);
    if (offset < 128) {
        buf_u8(b, (uint8_t)(0x44 | ((reg & 7) << 3)));
        buf_u8(b, 0x24);
        buf_u8(b, (uint8_t)offset);
    } else {
        buf_u8(b, (uint8_t)(0x84 | ((reg & 7) << 3)));
        buf_u8(b, 0x24);
        buf_u32(b, (uint32_t)offset);
    }
}

static void emit_rip_relative_lea(Gen* g, uint8_t reg, WStr key) {
    Buf* b = &g->code;
    buf_u8(b, reg >= 8 ? 0x4C : 0x48);
    buf_u8(b, 0x8D);
    buf_u8(b, (uint8_t)(0x05 | ((reg & 7) << 3)));
    size_t pos = b->len;
    buf_u32(b, 0);
    AddrPatch p;
    p.pos = pos;
    p.key = key;
    p.is_rip = 1;
    p.patch_end = 0;
    p.is_global = 0;
    AddrPatchVec_push(&g->address_patches, p);
    g->patch_generation++;
}

/* lea dst, [base + index * scale] with full 64-bit register extensions. */
static void emit_lea_scaled(Buf* b, uint8_t dst, uint8_t base, uint8_t index,
                            int scale_bits) {
    uint8_t rex = (uint8_t)(0x48 | (dst >= 8 ? 4 : 0) |
                            (index >= 8 ? 2 : 0) | (base >= 8 ? 1 : 0));
    buf_u8(b, rex);
    buf_u8(b, 0x8D);
    buf_u8(b, (uint8_t)(0x04 | ((dst & 7) << 3)));
    buf_u8(b, (uint8_t)((scale_bits << 6) | ((index & 7) << 3) | (base & 7)));
}

/* REX prefix for a SIB memory operand. rex_w selects a 64-bit data operand.
 * Every REX byte starts at 0x40; the extension bits alone are not a prefix. */
static uint8_t sib_rex(uint8_t reg_field, uint8_t base, uint8_t index, int rex_w) {
    return (uint8_t)(0x40 | (rex_w ? 8 : 0) | (reg_field >= 8 ? 4 : 0) |
                     (index >= 8 ? 2 : 0) | (base >= 8 ? 1 : 0));
}

static void emit_rex(Buf* b, uint8_t rex) {
    buf_u8(b, rex);
}

/* modrm and sib for [base + index * scale] with no displacement. Callers
 * must have checked that base's low bits are not 101 (rbp/r13), which would
 * require a disp32 encoding. */
static void emit_sib_modrm(Buf* b, uint8_t reg_field, uint8_t base,
                           uint8_t index, int scale_bits) {
    buf_u8(b, (uint8_t)(0x04 | ((reg_field & 7) << 3)));
    buf_u8(b, (uint8_t)((scale_bits << 6) | ((index & 7) << 3) | (base & 7)));
}

/* movsd xmm0, [base + index * scale] */
static void emit_movsd_sib_load(Buf* b, uint8_t base, uint8_t index, int scale_bits) {
    buf_u8(b, 0xF2);
    buf_u8(b, sib_rex(0, base, index, 1));
    buf_u8(b, 0x0F); buf_u8(b, 0x10);
    emit_sib_modrm(b, 0, base, index, scale_bits);
}

/* movsd [base + index * scale], xmm0 */
static void emit_movsd_sib_store(Buf* b, uint8_t base, uint8_t index, int scale_bits) {
    buf_u8(b, 0xF2);
    buf_u8(b, sib_rex(0, base, index, 1));
    buf_u8(b, 0x0F); buf_u8(b, 0x11);
    emit_sib_modrm(b, 0, base, index, scale_bits);
}

/* mov reg, [base + index * scale] with sign or zero extension for 1/2 bytes. */
static void emit_gpr_sib_load(Buf* b, uint8_t reg, uint8_t base, uint8_t index,
                              int scale_bits, uint32_t size, int is_signed) {
    switch (size) {
        case 1:
            emit_rex(b, sib_rex(reg, base, index, 1));
            buf_u8(b, 0x0F); buf_u8(b, is_signed ? 0xBE : 0xB6);
            break;
        case 2:
            emit_rex(b, sib_rex(reg, base, index, 1));
            buf_u8(b, 0x0F); buf_u8(b, is_signed ? 0xBF : 0xB7);
            break;
        case 4:
            emit_rex(b, sib_rex(reg, base, index, is_signed ? 1 : 0));
            buf_u8(b, is_signed ? 0x63 : 0x8B);
            break;
        default:
            emit_rex(b, sib_rex(reg, base, index, 1));
            buf_u8(b, 0x8B);
            break;
    }
    emit_sib_modrm(b, reg, base, index, scale_bits);
}

/* mov [base + index * scale], reg */
static void emit_gpr_sib_store(Buf* b, uint8_t reg, uint8_t base, uint8_t index,
                               int scale_bits, uint32_t size) {
    if (size == 1) {
        emit_rex(b, sib_rex(reg, base, index, 0));
        buf_u8(b, 0x88);
    } else if (size == 2) {
        buf_u8(b, 0x66);
        emit_rex(b, sib_rex(reg, base, index, 0));
        buf_u8(b, 0x89);
    } else {
        emit_rex(b, sib_rex(reg, base, index, size == 8 ? 1 : 0));
        buf_u8(b, 0x89);
    }
    emit_sib_modrm(b, reg, base, index, scale_bits);
}

/* movsd xmm<N>, [rip+key] — loads a float constant in one instruction
 * instead of lea/movsd/movq through rax. */
static void emit_function_epilogue(Gen* g) {
    Buf* b = &g->code;
    if (g->current_is_irq) {
        static const uint8_t pop_irq[] = {0x41, 0x5F, 0x41, 0x5E, 0x41, 0x5D, 0x41, 0x5C,
                                          0x41, 0x5B, 0x41, 0x5A, 0x41, 0x59, 0x41, 0x58,
                                          0x5F, 0x5E, 0x5A, 0x59, 0x58, 0x48, 0xCF};
        buf_bytes(b, pop_irq, sizeof(pop_irq));
    } else {
        buf_u8(b, 0xC3);
    }
}

static void patch_address(Gen* g, size_t pos, int32_t offset) {
    patch_i32(&g->code, pos, offset);
    g->patch_generation++;
}

/* ------------------------------------------------------------------ */
/* Type helpers                                                        */
/* ------------------------------------------------------------------ */

static WType* peel_typedefs(WType* t) {
    while (t && t->tag == 13) {
        t = t->inner;
    }
    return t;
}

/* Resolve named type references (tag 11) and inline typedef nodes (tag 13)
 * back to their concrete type. Struct and enum names are left unchanged. */
static WType* resolve_named_type(Gen* g, WType* t) {
    for (int guard = 0; t && guard < 32; guard++) {
        if (t->tag == 13 && t->inner) {
            t = t->inner;
            continue;
        }
        if (t->tag == 11 && t->name.ptr) {
            uint64_t v;
            if (map_get_str(&g->typedefs, t->name, &v)) {
                t = AS_PTR(WType, v);
                continue;
            }
        }
        break;
    }
    return t;
}

static int is_float_type_named(Gen* g, WType* t) {
    WType* r = resolve_named_type(g, t);
    return r && r->tag == 8;
}

static WType* type_u64(void) {
    static WType u64_type;
    u64_type.tag = 3;
    return &u64_type;
}

static WType* type_u8(void) {
    static WType u8_type;
    u8_type.tag = 0;
    return &u8_type;
}

static WType* type_pointer_to(WType* inner) {
    WType* t = (WType*)arena_alloc(g_arena, sizeof(WType));
    t->tag = 10;
    t->inner = inner;
    return t;
}

static uint32_t type_size(Gen* g, WType* t) {
    if (!t) {
        return 8;
    }
    switch (t->tag) {
        case 0:
        case 4:
            return 1;
        case 1:
        case 5:
            return 2;
        case 2:
        case 6:
            return 4;
        case 3:
        case 7:
        case 8:
            return 8;
        case 10:
            return 8;
        case 12:
            return type_size(g, t->inner) * (uint32_t)t->count;
        case 13:
            return type_size(g, t->inner);
        case 11: {
            uint64_t v;
            if (map_get_str(&g->typedefs, t->name, &v)) {
                return type_size(g, AS_PTR(WType, v));
            }
            if (map_get_str(&g->struct_layouts, t->name, &v)) {
                return g->layouts.v[v].size;
            }
            return 16;
        }
        default:
            return 0;
    }
}

static uint32_t type_alignment_of(Gen* g, WType* t) {
    if (!t) {
        return 1;
    }
    switch (t->tag) {
        case 0:
        case 4:
            return 1;
        case 1:
        case 5:
            return 2;
        case 2:
        case 6:
            return 4;
        case 3:
        case 7:
        case 8:
        case 10:
            return 8;
        case 9:
            return 1;
        case 12:
        case 13:
            return type_alignment_of(g, t->inner);
        case 11: {
            uint64_t v;
            if (map_get_str(&g->typedefs, t->name, &v)) {
                return type_alignment_of(g, AS_PTR(WType, v));
            }
            return map_get_str(&g->struct_alignments, t->name, &v) ? (uint32_t)v : 1;
        }
        default:
            return 1;
    }
}

static int can_resolve_type(Gen* g, WType* t) {
    if (!t) {
        return 1;
    }
    switch (t->tag) {
        case 11:
            return map_has(&g->struct_layouts, t->name.ptr, t->name.len) ||
                   map_has(&g->typedefs, t->name.ptr, t->name.len);
        case 12:
            return can_resolve_type(g, t->inner);
        case 13:
            return can_resolve_type(g, t->inner);
        default:
            return 1;
    }
}

static WType* resolve_expr_type(Gen* g, WExpr* e) {
    if (!e) {
        return NULL;
    }
    switch (e->tag) {
        case 2: /* StringLit: a string literal is a pointer to its bytes */
            return type_pointer_to(type_u8());
        case 3: { /* Variable */
            uint64_t v;
            if (map_get_str(&g->local_types, e->str, &v)) {
                return AS_PTR(WType, v);
            }
            /* Global/section variables are referenced by their bare name;
             * their type is recorded under "section:variable". */
            WStr key = find_global_key(g, e->str);
            if (key.ptr && map_get_str(&g->section_types, key, &v)) {
                return AS_PTR(WType, v);
            }
            return NULL;
        }
        case 7: { /* SectionAccess */
            if (map_has(&g->enums, e->str.ptr, e->str.len)) {
                return type_u64();
            }
            WStr key = wstr_pair(e->str, e->str2);
            uint64_t v;
            return map_get_str(&g->section_types, key, &v) ? AS_PTR(WType, v) : NULL;
        }
        case 6: { /* MemberAccess */
            WType* base = resolve_named_type(g, resolve_expr_type(g, e->a));
            if (!base) {
                return NULL;
            }
            WStr struct_name;
            if (base->tag == 11) {
                struct_name = base->name;
            } else if (base->tag == 10) {
                WType* inner = resolve_named_type(g, base->inner);
                if (!inner || inner->tag != 11) {
                    return NULL;
                }
                struct_name = inner->name;
            } else {
                return NULL;
            }
            uint64_t idx;
            if (!map_get_str(&g->struct_fields, struct_name, &idx)) {
                return NULL;
            }
            Layout* layout = &g->layouts.v[idx];
            uint64_t type_ptr;
            return map_get_str(&layout->field_types, e->str2, &type_ptr) ? AS_PTR(WType, type_ptr)
                                                                   : NULL;
        }
        case 10: { /* Index */
            WType* base = resolve_named_type(g, resolve_expr_type(g, e->a));
            if (base && (base->tag == 12 || base->tag == 10)) {
                return base->inner;
            }
            return NULL;
        }
        case 4: { /* AddrOf (named local or section variable) */
            uint64_t v;
            if (map_get_str(&g->local_types, e->str, &v)) {
                return type_pointer_to(AS_PTR(WType, v));
            }
            WStr key = find_global_key(g, e->str);
            if (key.ptr && map_get_str(&g->section_types, key, &v)) {
                return type_pointer_to(AS_PTR(WType, v));
            }
            return NULL;
        }
        case 5: { /* AddrOfExpr */
            WType* inner = resolve_expr_type(g, e->a);
            return inner ? type_pointer_to(inner) : NULL;
        }
        default:
            return NULL;
    }
}

static uint32_t expr_size(Gen* g, WExpr* e) {
    return type_size(g, resolve_expr_type(g, e));
}

static int is_volatile_member_expr(Gen* g, WExpr* e) {
    if (!e) {
        return 0;
    }
    if (e->tag == 10) {
        return is_volatile_member_expr(g, e->a);
    }
    if (e->tag != 6) {
        return 0;
    }
    WType* base = resolve_expr_type(g, e->a);
    if (!base) {
        return 0;
    }
    WStr name;
    base = peel_typedefs(base);
    if (base->tag == 11) {
        name = base->name;
    } else if (base->tag == 10) {
        WType* inner = peel_typedefs(base->inner);
        if (!inner || inner->tag != 11) {
            return 0;
        }
        name = inner->name;
    } else {
        return 0;
    }
    uint64_t idx;
    if (!map_get_str(&g->struct_volatile, name, &idx)) {
        return 0;
    }
    VolatileSet* set = &g->volatile_sets.v[idx];
    for (size_t i = 0; i < set->count; i++) {
        if (wstr_eq(set->names[i], e->str2)) {
            return 1;
        }
    }
    return 0;
}

static uint32_t primitive_size(WStr name) {
    if (wstr_eq_c(name, "u8") || wstr_eq_c(name, "i8")) return 1;
    if (wstr_eq_c(name, "u16") || wstr_eq_c(name, "i16")) return 2;
    if (wstr_eq_c(name, "u32") || wstr_eq_c(name, "i32")) return 4;
    if (wstr_eq_c(name, "u64") || wstr_eq_c(name, "i64") || wstr_eq_c(name, "f64") ||
        wstr_eq_c(name, "ptr"))
        return 8;
    if (wstr_eq_c(name, "void")) return 0;
    return UINT32_MAX;
}

static uint32_t resolve_type_size(Gen* g, WStr name) {
    uint32_t prim = primitive_size(name);
    if (prim != UINT32_MAX) {
        return prim;
    }
    uint64_t v;
    if (map_get_str(&g->typedefs, name, &v)) {
        return type_size(g, AS_PTR(WType, v));
    }
    if (map_get_str(&g->struct_layouts, name, &v)) {
        return g->layouts.v[v].size;
    }
    return 0;
}

static uint32_t resolve_type_alignment(Gen* g, WStr name) {
    uint32_t prim = primitive_size(name);
    if (prim != UINT32_MAX) {
        return prim == 0 ? 1 : prim;
    }
    uint64_t v;
    if (map_get_str(&g->typedefs, name, &v)) {
        return type_alignment_of(g, AS_PTR(WType, v));
    }
    if (map_get_str(&g->struct_alignments, name, &v)) {
        return (uint32_t)v;
    }
    return 1;
}

static uint64_t eval_const(Gen* g, WExpr* e) {
    return eval_const_depth(g, e, 0);
}

static uint64_t const_double_bits(double value) {
    uint64_t bits;
    memcpy(&bits, &value, sizeof(bits));
    return bits;
}

static double const_f64_value(uint64_t bits) {
    double value;
    memcpy(&value, &bits, sizeof(value));
    return value;
}

/* Match the runtime cvtsi2sd path. Unsigned 64-bit values are converted as
 * unsigned integers; cvtsi2sd has no unsigned form, so the code generator
 * splits the value and doubles the result. Host C conversion gives the same
 * IEEE-754 result here. */
static uint64_t const_int_to_f64_bits(uint64_t bits, int src_signed) {
    double value = src_signed ? (double)(int64_t)bits : (double)bits;
    return const_double_bits(value);
}

/* Match cvttsd2si: truncate toward zero and return the indefinite value for
 * NaN, infinity, or an out-of-range operand. */
static uint64_t const_f64_to_int_bits(uint64_t bits) {
    double value = const_f64_value(bits);
    if (!isfinite(value) || value >= 9223372036854775808.0 ||
        value < -9223372036854775808.0) {
        return (uint64_t)INT64_MIN;
    }
    return (uint64_t)(int64_t)value;
}

static int const_is_cast_op(WStr op) {
    return wstr_eq_c(op, "OpCastF64") || wstr_eq_c(op, "OpCastInt") ||
           wstr_eq_c(op, "OpCast") || wstr_eq_c(op, "OpCastU8") ||
           wstr_eq_c(op, "OpCastU16") || wstr_eq_c(op, "OpCastU32") ||
           wstr_eq_c(op, "OpCastU64") || wstr_eq_c(op, "OpCastI8") ||
           wstr_eq_c(op, "OpCastI16") || wstr_eq_c(op, "OpCastI32") ||
           wstr_eq_c(op, "OpCastI64");
}

/* Convert the source of a compile-time cast the same way compile_expr does.
 * Static initializers used to store the source bits unchanged, so (f64)3 and
 * (u64)3.5 both kept the integer/float representation of their operand. */
static uint64_t const_cast_operand(Gen* g, WExpr* operand, WStr op,
                                   uint32_t depth) {
    uint64_t bits = eval_const_depth(g, operand, depth + 1);
    int from_float = is_float_expr(g, operand);
    if (wstr_eq_c(op, "OpCastF64")) {
        return from_float ? bits
                          : const_int_to_f64_bits(bits, is_signed_expr(g, operand));
    }
    if (wstr_eq_c(op, "OpCast")) return bits;
    return from_float ? const_f64_to_int_bits(bits) : bits;
}

static uint64_t eval_const_depth(Gen* g, WExpr* e, uint32_t depth) {
    if (!e || depth > MAX_CONST_DEPTH) {
        return 0;
    }
    switch (e->tag) {
        case 0:
        case 1:
            return e->num;
        case 11: {
            if (!e->str.ptr) return 0;
            char* text = (char*)arena_alloc(g_arena, e->str.len + 1);
            memcpy(text, e->str.ptr, e->str.len);
            text[e->str.len] = '\0';
            double value = strtod(text, NULL);
            uint64_t bits;
            memcpy(&bits, &value, sizeof(bits));
            return bits;
        }
        case 13:
            return 0;
        case 3: {
            uint64_t v;
            return map_get_str(&g->constants, e->str, &v) ? eval_const_depth(g, AS_PTR(WExpr, v), depth + 1) : 0;
        }
        case 7: {
            uint64_t enum_idx;
            if (map_get_str(&g->enums, e->str, &enum_idx)) {
                WEnum* en = &g->prog.enums[enum_idx];
                for (size_t i = 0; i < en->count; i++) {
                    if (wstr_eq(en->values[i].name, e->str2)) {
                        return en->values[i].value;
                    }
                }
            }
            return 0;
        }
        case 8: {
            if (const_is_cast_op(e->str)) {
                uint64_t bits = const_cast_operand(g, e->a, e->str, depth);
                if (wstr_eq_c(e->str, "OpCastU8")) return bits & 0xFFu;
                if (wstr_eq_c(e->str, "OpCastU16")) return bits & 0xFFFFu;
                if (wstr_eq_c(e->str, "OpCastU32")) return bits & 0xFFFFFFFFu;
                if (wstr_eq_c(e->str, "OpCastI8"))
                    return (uint64_t)(int64_t)(int8_t)bits;
                if (wstr_eq_c(e->str, "OpCastI16"))
                    return (uint64_t)(int64_t)(int16_t)bits;
                if (wstr_eq_c(e->str, "OpCastI32"))
                    return (uint64_t)(int64_t)(int32_t)bits;
                return bits;
            }
            if (wstr_eq_c(e->str, "OpBitNot")) {
                return ~eval_const_depth(g, e->a, depth + 1);
            }
            uint64_t a = eval_const_depth(g, e->a, depth + 1);
            uint64_t b = eval_const_depth(g, e->b, depth + 1);
            WStr op = e->str;
            int sgn = is_signed_expr(g, e);
            int64_t sa = (int64_t)a, sb = (int64_t)b;
            if (wstr_eq_c(op, "OpAdd")) return a + b;
            if (wstr_eq_c(op, "OpSub")) return a - b;
            if (wstr_eq_c(op, "OpMul")) return a * b;
            if (wstr_eq_c(op, "OpDiv")) {
                if (!b) return 0;
                if (!sgn) return a / b;
                if (sa == INT64_MIN && sb == -1) return (uint64_t)INT64_MIN;
                return (uint64_t)(sa / sb);
            }
            if (wstr_eq_c(op, "OpMod")) {
                if (!b) return 0;
                if (!sgn) return a % b;
                if (sa == INT64_MIN && sb == -1) return 0;
                return (uint64_t)(sa % sb);
            }
            if (wstr_eq_c(op, "OpBitAnd")) return a & b;
            if (wstr_eq_c(op, "OpBitOr")) return a | b;
            if (wstr_eq_c(op, "OpBitXor")) return a ^ b;
            if (wstr_eq_c(op, "OpShl")) return a << (b & 63);
            if (wstr_eq_c(op, "OpShr")) return sgn ? (uint64_t)(sa >> (b & 63)) : a >> (b & 63);
            if (wstr_eq_c(op, "OpEq") || wstr_eq_c(op, "OpEqEq") || wstr_eq_c(op, "=="))
                return a == b;
            if (wstr_eq_c(op, "OpNotEq") || wstr_eq_c(op, "OpNe") || wstr_eq_c(op, "!="))
                return a != b;
            if (wstr_eq_c(op, "OpLt") || wstr_eq_c(op, "Lt") || wstr_eq_c(op, "<"))
                return sgn ? sa < sb : a < b;
            if (wstr_eq_c(op, "OpLtEq") || wstr_eq_c(op, "OpLe") || wstr_eq_c(op, "<="))
                return sgn ? sa <= sb : a <= b;
            if (wstr_eq_c(op, "OpGt") || wstr_eq_c(op, "Gt") || wstr_eq_c(op, ">"))
                return sgn ? sa > sb : a > b;
            if (wstr_eq_c(op, "OpGtEq") || wstr_eq_c(op, "OpGe") || wstr_eq_c(op, ">="))
                return sgn ? sa >= sb : a >= b;
            if (wstr_eq_c(op, "OpAnd") || wstr_eq_c(op, "&&")) return a != 0 && b != 0;
            if (wstr_eq_c(op, "OpOr") || wstr_eq_c(op, "||")) return a != 0 || b != 0;
            return 0;
        }
        case 9: { /* Call */
            WStr name = e->str;
            WExpr* arg = e->arg_count ? e->args[0] : NULL;
            if (wstr_eq_c(name, "sizeof")) {
                if (arg && arg->tag == 3) {
                    uint32_t prim = primitive_size(arg->str);
                    if (prim != UINT32_MAX) {
                        return prim;
                    }
                    return resolve_type_size(g, arg->str);
                }
                return 0;
            }
            if (wstr_eq_c(name, "alignof")) {
                return arg && arg->tag == 3 ? resolve_type_alignment(g, arg->str) : 0;
            }
            if (wstr_eq_c(name, "versionof")) {
                if (arg && arg->tag == 3) {
                    uint64_t idx;
                    if (map_get_str(&g->struct_versions, arg->str, &idx)) {
                        return idx;
                    }
                    return map_has(&g->typedefs, arg->str.ptr, arg->str.len) ? 1 : 0;
                }
                return 0;
            }
            if (wstr_eq_c(name, "fieldsof")) {
                if (arg && arg->tag == 3) {
                    uint64_t idx;
                    if (map_get_str(&g->struct_fields, arg->str, &idx)) {
                        return g->layouts.v[idx].field_types.len;
                    }
                }
                return 0;
            }
            if (wstr_eq_c(name, "offsetof")) {
                if (arg && arg->tag == 7) {
                    uint64_t idx;
                    if (map_get_str(&g->struct_layouts, arg->str, &idx)) {
                        uint64_t off;
                        if (map_get_str(&g->layouts.v[idx].field_offsets, arg->str2, &off)) {
                            return off;
                        }
                    }
                }
                return 0;
            }
            return 0;
        }
        default:
            return 0;
    }
}

static WStr wstr_pair(WStr a, WStr b) {
    size_t len = a.len + b.len + 1;
    char* p = (char*)arena_alloc(g_arena, len + 1);
    memcpy(p, a.ptr, a.len);
    p[a.len] = ':';
    memcpy(p + a.len + 1, b.ptr, b.len);
    p[len] = '\0';
    WStr s;
    s.ptr = p;
    s.len = (uint32_t)len;
    return s;
}

static WStr wstr_prefix(const char* prefix, WStr s) {
    size_t plen = strlen(prefix);
    size_t len = plen + s.len;
    char* p = (char*)arena_alloc(g_arena, len + 1);
    memcpy(p, prefix, plen);
    memcpy(p + plen, s.ptr, s.len);
    p[len] = '\0';
    WStr out;
    out.ptr = p;
    out.len = (uint32_t)len;
    return out;
}

static WStr wstr_concat(WStr a, WStr b) {
    size_t len = (size_t)a.len + (size_t)b.len;
    char* p = (char*)arena_alloc(g_arena, len + 1);
    if (a.len) memcpy(p, a.ptr, a.len);
    if (b.len) memcpy(p + a.len, b.ptr, b.len);
    p[len] = '\0';
    WStr out;
    out.ptr = p;
    out.len = (uint32_t)len;
    return out;
}

static WStr find_global_key(Gen* g, WStr name) {
    for (size_t i = 0; i < g->global_keys.n; i++) {
        WStr key = g->global_keys.v[i];
        if (wstr_eq(key, name)) {
            return key;
        }
        /* Section keys are "section:variable"; the section name can be any
         * length, so compare only the suffix after the final colon. */
        if (key.len > name.len && key.ptr[key.len - name.len - 1] == ':' &&
            memcmp(key.ptr + key.len - name.len, name.ptr, name.len) == 0) {
            return key;
        }
    }
    WStr empty = {NULL, 0};
    return empty;
}

static void add_global_offset(Gen* g, WStr key, uint32_t offset) {
    if (!map_has(&g->global_offsets, key.ptr, key.len)) {
        WStrVec_push(&g->global_keys, key);
    }
    map_put_str(&g->global_offsets, key, offset);
}

/* Emit <op> rax, imm when the operator has an immediate form.  The generic
 * binary path saves rax on the stack, evaluates the RHS, moves it to rbx and
 * pops rax back.  For the very common "variable op small constant" case that
 * is 6 wasted instructions plus a memory round-trip per evaluation.  Returns
 * 0 when the operator has no immediate encoding so the caller can fall back. */
static int emit_binary_imm(Buf* b, WStr op, int64_t imm) {
    uint8_t group;
    if (wstr_eq_c(op, "OpAdd")) group = 0;
    else if (wstr_eq_c(op, "OpSub")) group = 5;
    else if (wstr_eq_c(op, "OpBitAnd")) group = 4;
    else if (wstr_eq_c(op, "OpBitOr")) group = 1;
    else if (wstr_eq_c(op, "OpBitXor")) group = 6;
    else if (wstr_eq_c(op, "OpMul")) {
        if (imm >= -128 && imm <= 127) {
            buf_u8(b, 0x48); buf_u8(b, 0x6B); buf_u8(b, 0xC0); buf_u8(b, (uint8_t)imm);
        } else {
            buf_u8(b, 0x48); buf_u8(b, 0x69); buf_u8(b, 0xC0); buf_u32(b, (uint32_t)imm);
        }
        return 1;
    } else {
        return 0;
    }
    if (imm >= -128 && imm <= 127) {
        buf_u8(b, 0x48); buf_u8(b, 0x83);
        buf_u8(b, (uint8_t)(0xC0 | (group << 3)));
        buf_u8(b, (uint8_t)imm);
    } else {
        buf_u8(b, 0x48); buf_u8(b, 0x81);
        buf_u8(b, (uint8_t)(0xC0 | (group << 3)));
        buf_u32(b, (uint32_t)imm);
    }
    return 1;
}

/* Return the SSE opcode for a floating-point binary operator. */
static int float_op_code(WStr op) {
    if (wstr_eq_c(op, "OpAdd")) return 0x58;
    if (wstr_eq_c(op, "OpSub")) return 0x5C;
    if (wstr_eq_c(op, "OpMul")) return 0x59;
    if (wstr_eq_c(op, "OpDiv")) return 0x5E;
    return 0;
}

/* xmm0 = xmm0 <float op> xmm1 for the four ops with float sequences. */
static int emit_float_op_xmm1(Buf* b, WStr op) {
    int code = float_op_code(op);
    if (!code) {
        return 0;
    }
    if (b) {
        /* movq %rax,%xmm0 ; movq %rbx,%xmm1 ; <op>sd %xmm1,%xmm0 ;
         * movq %xmm0,%rax */
        static const uint8_t head[] = {0x66,0x48,0x0F,0x6E,0xC0,
                                       0x66,0x48,0x0F,0x6E,0xCB};
        static const uint8_t tail[] = {0x66,0x48,0x0F,0x7E,0xC0};
        buf_bytes(b, head, sizeof(head));
        buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, (uint8_t)code); buf_u8(b, 0xC1);
        buf_bytes(b, tail, sizeof(tail));
    }
    return 1;
}

/* Convert an f64 bit pattern in rax to a signed 64-bit integer in rax. */
static void emit_cvttsd2si_rax(Buf* b) {
    static const uint8_t seq[] = {0x66,0x48,0x0F,0x6E,0xC0,
                                  0xF2,0x48,0x0F,0x2C,0xC0};
    buf_bytes(b, seq, sizeof(seq));
}

/* Move f64 bit patterns between rax and xmm0. */
static void emit_movq_rax_to_xmm0(Buf* b) {
    buf_u8(b, 0x66); buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0x6E); buf_u8(b, 0xC0);
}

static void emit_movq_xmm0_to_rax(Buf* b) {
    buf_u8(b, 0x66); buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0x7E); buf_u8(b, 0xC0);
}

/* mov %reg,%rax for 64-bit GPRs. */
static void emit_mov_gpr_to_rax(Buf* b, uint8_t reg) {
    if (!reg) return;
    buf_u8(b, reg >= 8 ? 0x49 : 0x48);
    buf_u8(b, 0x89);
    buf_u8(b, (uint8_t)(0xC0 | ((reg & 7) << 3)));
}

/* Convert an integer bit pattern in reg to an f64 bit pattern in reg. The
 * unsigned path halves values with the high bit set so cvtsi2sd cannot read
 * them as negative. rcx is a scratch register and is preserved when reg is
 * rcx. */
static void emit_int_bits_to_f64_bits(Gen* g, uint8_t reg, int src_signed) {
    Buf* b = &g->code;
    int saved_rcx = reg == 1;
    if (saved_rcx) buf_u8(b, 0x51); /* push rcx */
    emit_mov_gpr_to_rax(b, reg);
    if (!src_signed) {
        buf_u8(b, 0x48); buf_u8(b, 0x85); buf_u8(b, 0xC0); /* test rax,rax */
        buf_u8(b, 0x0F); buf_u8(b, 0x89);
        size_t pos_plain = b->len;
        buf_u32(b, 0);
        buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xC1); /* mov rcx,rax */
        buf_u8(b, 0x48); buf_u8(b, 0x83); buf_u8(b, 0xE1); buf_u8(b, 0x01); /* and rcx,1 */
        buf_u8(b, 0x48); buf_u8(b, 0xD1); buf_u8(b, 0xE8); /* shr rax,1 */
        buf_u8(b, 0x48); buf_u8(b, 0x09); buf_u8(b, 0xC8); /* or rax,rcx */
        buf_u8(b, 0xF2); buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0x2A); buf_u8(b, 0xC0);
        buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, 0x58); buf_u8(b, 0xC0); /* addsd xmm0,xmm0 */
        buf_u8(b, 0xE9);
        size_t pos_end = b->len;
        buf_u32(b, 0);
        patch_address(g, pos_plain, (int32_t)(b->len - (pos_plain + 4)));
        buf_u8(b, 0xF2); buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0x2A); buf_u8(b, 0xC0);
        patch_address(g, pos_end, (int32_t)(b->len - (pos_end + 4)));
    } else {
        static const uint8_t seq[] = {0xF2, 0x48, 0x0F, 0x2A, 0xC0};
        buf_bytes(b, seq, sizeof(seq));
    }
    emit_movq_xmm0_to_rax(b);
    if (saved_rcx) buf_u8(b, 0x59); /* pop rcx */
    if (reg) move_rax_to_reg(b, reg);
}

/* Compile e into reg, converting integer operands used by f64 arithmetic. */
static void compile_float_operand_bits(Gen* g, WExpr* e, uint8_t reg,
                                       int deref_ptr) {
    compile_expr(g, e, reg, deref_ptr);
    if (e && !is_float_value_expr(g, e, deref_ptr)) {
        emit_int_bits_to_f64_bits(g, reg, is_signed_expr(g, e));
    }
}

/* movq %gpr,%xmm0 for 64-bit GPRs. */
static void emit_movq_gpr_to_xmm0(Buf* b, uint8_t gpr) {
    buf_u8(b, 0x66);
    buf_u8(b, (uint8_t)(0x48 | (gpr >= 8 ? 1 : 0)));
    buf_u8(b, 0x0F); buf_u8(b, 0x6E);
    buf_u8(b, (uint8_t)(0xC0 | (gpr & 7)));
}

/* movapd %xmm0,%xmm<reg> including high registers. */
static void emit_movapd_xmm0_to_xmm(Buf* b, uint8_t reg) {
    if (!reg) return;
    buf_u8(b, 0x66);
    if (reg >= 8) buf_u8(b, 0x44);
    buf_u8(b, 0x0F); buf_u8(b, 0x28);
    buf_u8(b, (uint8_t)(0xC0 | ((reg & 7) << 3)));
}

/* movq %gpr,%xmm<reg> for 64-bit GPRs. */
static void emit_movq_gpr_to_xmm(Buf* b, uint8_t gpr, uint8_t xmm) {
    buf_u8(b, 0x66);
    buf_u8(b, (uint8_t)(0x48 | (xmm >= 8 ? 4 : 0) | (gpr >= 8 ? 1 : 0)));
    buf_u8(b, 0x0F); buf_u8(b, 0x6E);
    buf_u8(b, (uint8_t)(0xC0 | ((xmm & 7) << 3) | (gpr & 7)));
}

/* movq %xmm<reg>,%gpr for 64-bit GPRs. */
static void emit_movq_xmm_to_gpr(Buf* b, uint8_t xmm, uint8_t gpr) {
    buf_u8(b, 0x66);
    buf_u8(b, (uint8_t)(0x48 | (xmm >= 8 ? 4 : 0) | (gpr >= 8 ? 1 : 0)));
    buf_u8(b, 0x0F); buf_u8(b, 0x7E);
    buf_u8(b, (uint8_t)(0xC0 | ((xmm & 7) << 3) | (gpr & 7)));
}

/* movapd %xmm0,%xmm<reg> and the reverse, for XMM scratch copies. */
static void emit_movapd_xmm0_to(Buf* b, uint8_t reg) {
    if (!reg) return;
    buf_u8(b, 0x66); buf_u8(b, 0x0F); buf_u8(b, 0x28);
    buf_u8(b, (uint8_t)(0xC0 | ((reg & 7) << 3)));
}

static void emit_movapd_to_xmm0(Buf* b, uint8_t reg) {
    if (!reg) return;
    buf_u8(b, 0x66); buf_u8(b, 0x0F); buf_u8(b, 0x28);
    buf_u8(b, (uint8_t)(0xC0 | (reg & 7)));
}

/* movsd xmm<reg> -> stack slot. */
static void emit_movsd_xmm_to_local(Buf* b, uint8_t reg, uint32_t off) {
    buf_u8(b, 0xF2);
    if (reg >= 8) buf_u8(b, 0x44);
    buf_u8(b, 0x0F); buf_u8(b, 0x11);
    buf_u8(b, (uint8_t)((off <= 127 ? 0x45 : 0x85) | ((reg & 7) << 3)));
    if (off <= 127) buf_u8(b, (uint8_t)(-(int32_t)off));
    else buf_u32(b, (uint32_t)(-(int32_t)off));
}

/* movapd xmm<reg> -> xmm0 and xmm0 -> xmm<reg> for high registers. */
static void emit_movapd_xmm_to_xmm0(Buf* b, uint8_t reg) {
    if (!reg) return;
    buf_u8(b, 0x66);
    if (reg >= 8) buf_u8(b, 0x41); /* REX.B extends the source rm field */
    buf_u8(b, 0x0F); buf_u8(b, 0x28);
    buf_u8(b, (uint8_t)(0xC0 | (reg & 7)));
}

/* xmm0 = xmm0 <op> float literal. Caller ensures xmm0 holds the LHS bits.
 * The memory operand removes a separate constant load and its movq round trip. */
static void emit_float_op_xmm0_mem(Gen* g, WStr op, WStr lit) {
    Buf* b = &g->code;
    int code = float_op_code(op);
    WStr key = wstr_prefix("float:", lit);
    buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, (uint8_t)code); buf_u8(b, 0x05);
    size_t pos = b->len;
    buf_u32(b, 0);
    AddrPatch p;
    p.pos = pos;
    p.key = key;
    p.is_rip = 1;
    p.patch_end = 0;
    p.is_global = 0;
    AddrPatchVec_push(&g->address_patches, p);
    g->patch_generation++;
}

/* xmm0 = float literal, loaded directly from the constant pool. */
static void emit_load_float_lit_xmm0(Gen* g, WStr lit) {
    Buf* b = &g->code;
    WStr key = wstr_prefix("float:", lit);
    buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, 0x10); buf_u8(b, 0x05);
    size_t pos = b->len;
    buf_u32(b, 0);
    AddrPatch p;
    p.pos = pos;
    p.key = key;
    p.is_rip = 1;
    p.patch_end = 0;
    p.is_global = 0;
    AddrPatchVec_push(&g->address_patches, p);
    g->patch_generation++;
}

/* Compile a float expression so its bit pattern ends in xmm0. Chained binary
 * ops with literal RHS stay in xmm0, removing a movq round trip per link. */
static void compile_float_to_xmm0_s(Gen* g, WExpr* e, uint8_t scratch);

/* Calls reload argument registers and xmm scratch registers, so any path that
 * keeps a float value in xmm1/xmm2 must reject nested calls in full. */
static int float_rhs_safe(WExpr* e) {
    if (!e) {
        return 1;
    }
    return e->tag != 8 && e->tag != 9 && !expr_has_call_deep(e);
}

/* True when e can be compiled to xmm0 without clobbering xmm1 or xmm2.
 * Used by the fused float-op path to hold the LHS in xmm1 while the RHS is
 * evaluated. Index loads use rbx/rax; variables and literals use rax only. */
static int float_direct_safe(Gen* g, WExpr* e) {
    if (!e) {
        return 1;
    }
    if (e->tag == 10 || e->tag == 3) {
        return is_float_expr(g, e) && !expr_has_call_deep(e);
    }
    if (e->tag == 11) {
        return 1;
    }
    if (e->tag == 8 && e->b && e->b->tag == 11 &&
        float_op_code(e->str) && !expr_has_call_deep(e) &&
        float_direct_safe(g, e->a)) {
        return 1;
    }
    return 0;
}

/* Compile a float binary op whose result is left in xmm0. Covers the two XMM
 * forms used by vector kernels: LHS op (RHS op2 literal) and LHS op literal.
 * Returns 0 when e needs a different codegen path. */
static int compile_float_binary_to_xmm0_s(Gen* g, WExpr* e, uint8_t scratch) {
    if (!e || e->tag != 8 || !float_op_code(e->str)) {
        return 0;
    }
    WStr op = e->str;
    if (!is_float_value_expr(g, e->a, 1) && !is_float_value_expr(g, e->b, 1)) {
        return 0;
    }
    Buf* b = &g->code;

    if (e->b && e->b->tag == 8 && e->b->b && e->b->b->tag == 11 &&
        float_op_code(e->b->str) && float_direct_safe(g, e->b)) {
        int code = float_op_code(op);
        compile_float_to_xmm0_s(g, e->a, scratch);
        buf_u8(b, 0x66); buf_u8(b, 0x0F); buf_u8(b, 0x28); buf_u8(b, 0xC8);
        compile_float_to_xmm0_s(g, e->b->a, scratch);
        emit_float_op_xmm0_mem(g, e->b->str, e->b->b->str);
        if (code == 0x5C || code == 0x5E) {
            buf_u8(b, 0x66); buf_u8(b, 0x0F); buf_u8(b, 0x28); buf_u8(b, 0xD0);
            buf_u8(b, 0x66); buf_u8(b, 0x0F); buf_u8(b, 0x28); buf_u8(b, 0xC1);
            buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, (uint8_t)code);
            buf_u8(b, 0xC2);
        } else {
            buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, (uint8_t)code);
            buf_u8(b, 0xC1);
        }
        return 1;
    }

    if (e->b && e->b->tag == 11) {
        compile_float_to_xmm0_s(g, e->a, scratch);
        emit_float_op_xmm0_mem(g, op, e->b->str);
        return 1;
    }
    return 0;
}

static int compile_float_binary_to_xmm0(Gen* g, WExpr* e) {
    return compile_float_binary_to_xmm0_s(g, e, 3);
}

/* Start a float op with the LHS bit pattern already in rax: stash it in xmm1
 * so the RHS can be evaluated without a push/pop shuffle. The caller must
 * emit the RHS into rax, then call emit_float_op_after_rhs(). */
static void emit_float_op_stash_lhs(Buf* b) {
    /* movq %rax,%xmm1 */
    buf_u8(b, 0x66); buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0x6E); buf_u8(b, 0xC8);
}

/* Finish a float op whose LHS is in xmm1 and RHS bit pattern is in rax. */
static void emit_float_op_after_rhs(Buf* b, WStr op) {
    uint8_t code;
    if (wstr_eq_c(op, "OpAdd")) code = 0x58;
    else if (wstr_eq_c(op, "OpSub")) code = 0x5C;
    else if (wstr_eq_c(op, "OpMul")) code = 0x59;
    else code = 0x5E; /* OpDiv */
    /* movq %rax,%xmm0 */
    buf_u8(b, 0x66); buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0x6E); buf_u8(b, 0xC0);
    buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, code);
    if (code == 0x5C || code == 0x5E) {
        /* Non-commutative: xmm1 (LHS) op xmm0 (RHS) */
        buf_u8(b, 0xC8);
        buf_u8(b, 0x66); buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0x7E); buf_u8(b, 0xC8);
    } else {
        buf_u8(b, 0xC1);
        buf_u8(b, 0x66); buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0x7E); buf_u8(b, 0xC0);
    }
}

static int has_binary_imm(WStr op) {
    return wstr_eq_c(op, "OpAdd") || wstr_eq_c(op, "OpSub") ||
           wstr_eq_c(op, "OpBitAnd") || wstr_eq_c(op, "OpBitOr") ||
           wstr_eq_c(op, "OpBitXor") || wstr_eq_c(op, "OpMul");
}

/* rax = rax <op> rbx for the operators with an immediate-capable form. */
static void emit_binary_rbx(Buf* b, WStr op) {
    uint8_t opcode;
    if (wstr_eq_c(op, "OpAdd")) opcode = 0x01;
    else if (wstr_eq_c(op, "OpSub")) opcode = 0x29;
    else if (wstr_eq_c(op, "OpBitAnd")) opcode = 0x21;
    else if (wstr_eq_c(op, "OpBitOr")) opcode = 0x09;
    else if (wstr_eq_c(op, "OpBitXor")) opcode = 0x31;
    else { /* OpMul */
        buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0xAF); buf_u8(b, 0xC3);
        return;
    }
    buf_u8(b, 0x48); buf_u8(b, opcode); buf_u8(b, 0xD8); /* rax op rbx */
}

/* The old generator sanitized module names for jmpto-local lookups.  New
 * locals were always inserted under their unsanitized names, but preserve the
 * lookup for source code that happens to declare a matching local. */
static WStr jmpto_prefix(WStr module) {
    static const char head[] = "__jmpto_";
    size_t len = sizeof(head) - 1 + module.len + 1;
    char* p = (char*)arena_alloc(g_arena, len + 1);
    size_t at = 0;
    memcpy(p, head, sizeof(head) - 1);
    at = sizeof(head) - 1;
    for (uint32_t i = 0; i < module.len; i++) {
        unsigned char c = (unsigned char)module.ptr[i];
        p[at++] = (isalnum(c) || c == '_') ? (char)c : '_';
    }
    p[at++] = '_';
    p[at] = '\0';
    WStr out;
    out.ptr = p;
    out.len = (uint32_t)at;
    return out;
}

static void compile_inline_jmpto(Gen* g, WStmt* s) {
    Buf* b = &g->code;
    WStr prefix = jmpto_prefix(s->name);

    for (size_t i = 0; i < s->stmt1_count; i++) {
        compile_stmt(g, s->stmts1[i]);
    }

    for (size_t i = 0; i < s->stmt2_count; i++) {
        WStmt* body = s->stmts2[i];
        if (!body) {
            continue;
        }
        if (body->tag == 6) {
            if (body->return_count == 0 || !body->returns[0].expr ||
                body->returns[0].expr->tag != 3) {
                /* The old generator dropped inlined returns whose first value
                 * was not a plain variable. */
                continue;
            }
            WExpr* value = body->returns[0].expr;
            WStr name = value->str;
            WStr prefixed = wstr_concat(prefix, name);
            WType* type = body->returns[0].type;
            uint32_t size = type_size(g, type);
            uint64_t off = 0;

            if (!map_get_str(&g->locals, name, &off)) {
                if (map_get_str(&g->locals, prefixed, &off)) {
                    /* Use the prefixed local selected by the old generator. */
                } else if (find_global_key(g, name).len == 0) {
                    uint32_t var_size = size;
                    g->next_offset += var_size;
                    off = g->next_offset;
                    map_put_str(&g->locals, name, off);
                    map_put_str(&g->local_access, name, 3);
                    map_put_str(&g->local_types, name, PTR(type));
                } else {
                    off = 0;
                }
            }
            if (map_get_str(&g->locals, name, &off) || map_get_str(&g->locals, prefixed, &off)) {
                emit_mem_store(b, 0, (uint32_t)off, size);
            }
            continue;
        }
        compile_stmt(g, body);
    }
}

static void unescape_string(WStr raw, Buf* out) {
    const uint8_t* p = (const uint8_t*)raw.ptr;
    uint32_t len = raw.len;
    for (uint32_t i = 0; i < len;) {
        if (p[i] == '\\' && i + 1 < len) {
            uint8_t c = p[i + 1];
            switch (c) {
                case 'n': buf_u8(out, 10); i += 2; continue;
                case 't': buf_u8(out, 9); i += 2; continue;
                case 'r': buf_u8(out, 13); i += 2; continue;
                case '"': buf_u8(out, 34); i += 2; continue;
                case '\\': buf_u8(out, 92); i += 2; continue;
                default:
                    if (c >= '0' && c <= '7') {
                        uint8_t value = 0;
                        uint32_t count = 0;
                        uint32_t j = i + 1;
                        while (j < len && count < 3 && p[j] >= '0' && p[j] <= '7') {
                            value = (uint8_t)(value * 8 + (p[j] - '0'));
                            j++;
                            count++;
                        }
                        buf_u8(out, value);
                        i = j;
                        continue;
                    }
                    buf_u8(out, p[i]);
                    i++;
                    continue;
            }
        }
        buf_u8(out, p[i]);
        i++;
    }
}

static void collect_expr_literals(Gen* g, WExpr* e, int strings);
static void collect_stmt_literals(Gen* g, WStmt* s, int strings);

static void collect_expr_literals(Gen* g, WExpr* e, int strings) {
    if (!e) {
        return;
    }
    if (strings && e->tag == 2) {
        if (!map_has(&g->string_constants, e->str.ptr, e->str.len)) {
            map_put_str(&g->string_constants, e->str, 0);
            WStrVec_push(&g->string_keys, e->str);
        }
        return;
    }
    if (!strings && e->tag == 11) {
        if (!map_has(&g->float_constants, e->str.ptr, e->str.len)) {
            map_put_str(&g->float_constants, e->str, 0);
            WStrVec_push(&g->float_keys, e->str);
        }
        return;
    }
    switch (e->tag) {
        case 8:
            collect_expr_literals(g, e->a, strings);
            collect_expr_literals(g, e->b, strings);
            break;
        case 9:
            if (strings && wstr_eq_c(e->str, "nameof") && e->arg_count && e->args[0]->tag == 3) {
                WStr key = e->args[0]->str;
                if (!map_has(&g->string_constants, key.ptr, key.len)) {
                    map_put_str(&g->string_constants, key, 0);
                    WStrVec_push(&g->string_keys, key);
                }
            }
            for (size_t i = 0; i < e->arg_count; i++) {
                collect_expr_literals(g, e->args[i], strings);
            }
            break;
        case 10:
            collect_expr_literals(g, e->a, strings);
            collect_expr_literals(g, e->b, strings);
            break;
        case 12:
            /* String literals nested in an aggregate initializer are still
             * referenced by RIP-relative LEAs when the aggregate holds
             * pointers. Register them so all formats emit the bytes/symbol. */
            for (size_t i = 0; i < e->arg_count; i++) {
                collect_expr_literals(g, e->args[i], strings);
            }
            break;
        case 6:
            collect_expr_literals(g, e->a, strings);
            break;
        case 5:
            collect_expr_literals(g, e->a, strings);
            break;
        default:
            break;
    }
}

static void collect_stmt_literals(Gen* g, WStmt* s, int strings) {
    if (!s) {
        return;
    }
    switch (s->tag) {
        case 0:
            if (s->e1) {
                collect_expr_literals(g, s->e1, strings);
            }
            break;
        case 1:
            for (size_t i = 0; i < s->expr_count; i++) {
                collect_expr_literals(g, s->exprs[i], strings);
            }
            collect_expr_literals(g, s->e1, strings);
            break;
        case 2:
        case 3:
            collect_expr_literals(g, s->e1, strings);
            if (s->s1) collect_stmt_literals(g, s->s1, strings);
            if (s->s2) collect_stmt_literals(g, s->s2, strings);
            for (size_t i = 0; i < s->stmt1_count; i++) collect_stmt_literals(g, s->stmts1[i], strings);
            for (size_t i = 0; i < s->stmt2_count; i++) collect_stmt_literals(g, s->stmts2[i], strings);
            break;
        case 4: /* For: init, cond, post, body */
            if (s->s1) collect_stmt_literals(g, s->s1, strings);
            collect_expr_literals(g, s->e1, strings);
            if (s->s2) collect_stmt_literals(g, s->s2, strings);
            for (size_t i = 0; i < s->stmt1_count; i++) collect_stmt_literals(g, s->stmts1[i], strings);
            break;
        case 5: /* Jmpto: strings collect module name + arg statements; floats ignore it. */
            if (strings) {
                if (!map_has(&g->string_constants, s->name.ptr, s->name.len)) {
                    map_put_str(&g->string_constants, s->name, 0);
                    WStrVec_push(&g->string_keys, s->name);
                }
                for (size_t i = 0; i < s->stmt1_count; i++) {
                    collect_stmt_literals(g, s->stmts1[i], strings);
                }
            }
            break;
        case 6:
            for (size_t i = 0; i < s->return_count; i++) {
                collect_expr_literals(g, s->returns[i].expr, strings);
            }
            break;
        case 7:
            for (size_t i = 0; i < s->stmt1_count; i++) collect_stmt_literals(g, s->stmts1[i], strings);
            break;
        case 9:
            collect_expr_literals(g, s->e1, strings);
            for (size_t i = 0; i < s->case_count; i++) {
                collect_expr_literals(g, s->cases[i].expr, strings);
                for (size_t j = 0; j < s->cases[i].body_count; j++) {
                    collect_stmt_literals(g, s->cases[i].body[j], strings);
                }
            }
            for (size_t i = 0; i < s->stmt2_count; i++) collect_stmt_literals(g, s->stmts2[i], strings);
            break;
        case 10:
            collect_expr_literals(g, s->e1, strings);
            break;
        default:
            break;
    }
}
/* ------------------------------------------------------------------ */
/* Expression classification                                           */
/* ------------------------------------------------------------------ */

static WFunc* func_by_name(Gen* g, WStr name) {
    uint64_t v;
    if (!map_get_str(&g->function_signatures, name, &v)) {
        return NULL;
    }
    return &g->prog.funcs[v - 1];
}

/* True when name is declared as an extern (body-less) function. */
static int is_extern_func(Gen* g, WStr name) {
    WFunc* f = func_by_name(g, name);
    return f && (f->is_extern || !f->has_body);
}

static int is_float_expr(Gen* g, WExpr* e) {
    if (!e) {
        return 0;
    }
    switch (e->tag) {
        case 11:
            return 1;
        case 0:
        case 1:
            return 0;
        case 3:
        case 6:
        case 7:
            return is_float_type_named(g, resolve_expr_type(g, e));
        case 10: { /* Index: classify by the element type of the base object. */
            WType* base = resolve_named_type(g, resolve_expr_type(g, e->a));
            if (base && (base->tag == 12 || base->tag == 10)) {
                return is_float_type_named(g, base->inner);
            }
            return 0;
        }
        case 8:
            /* Casts set their result type. Treating (f64)x as non-float made
             * arithmetic reconvert the already converted bit pattern. */
            if (wstr_eq_c(e->str, "OpCastF64")) {
                return 1;
            }
            if (wstr_eq_c(e->str, "OpCast")) {
                return is_float_expr(g, e->a);
            }
            if (const_is_cast_op(e->str)) {
                return 0;
            }
            return is_float_expr(g, e->a) || is_float_expr(g, e->b);
        case 9:
            return wstr_eq_c(e->str, "sin") || wstr_eq_c(e->str, "cos") ||
                   wstr_eq_c(e->str, "tan") || wstr_eq_c(e->str, "sqrt");
        default:
            return 0;
    }
}

/* Strict f64-slot classification plus value-context extension: an input or
 * input/output variable declared as f64* names an f64 value when it is read,
 * but remains a pointer in address context (copies, comparisons, walks). */
static int is_f64_pointer_modifier_expr(Gen* g, WExpr* e) {
    if (!e || e->tag != 3) {
        return 0;
    }
    uint64_t access = 3;
    map_get_str(&g->local_access, e->str, &access);
    if (access != 0 && access != 2) {
        return 0;
    }
    WType* t = resolve_named_type(g, resolve_expr_type(g, e));
    if (t && (t->tag == 10 || t->tag == 12)) {
        t = resolve_named_type(g, t->inner);
    }
    return t && t->tag == 8;
}

static int is_float_value_expr(Gen* g, WExpr* e, int deref_ptr) {
    if (is_float_expr(g, e)) {
        return 1;
    }
    return deref_ptr && is_f64_pointer_modifier_expr(g, e);
}

static int is_signed_expr(Gen* g, WExpr* e) {
    if (!e) {
        return 0;
    }
    switch (e->tag) {
        case 1:
            return 1;
        case 0:
            return 0;
        case 3:
        case 5:
        case 6:
        case 7:
        case 10: {
            WType* t = resolve_named_type(g, resolve_expr_type(g, e));
            return t && t->tag >= 4 && t->tag <= 7;
        }
        case 8:
            /* Casts take their signedness from the target type, not from the
             * source. `(u64)i` is unsigned even when i is i64; propagating the
             * source signedness made division, comparisons and f64 conversion
             * choose signed operations for unsigned values. */
            if (wstr_eq_c(e->str, "OpCastF64") ||
                wstr_eq_c(e->str, "OpCastInt") ||
                wstr_eq_c(e->str, "OpCastU8") ||
                wstr_eq_c(e->str, "OpCastU16") ||
                wstr_eq_c(e->str, "OpCastU32") ||
                wstr_eq_c(e->str, "OpCastU64")) {
                return 0;
            }
            if (wstr_eq_c(e->str, "OpCastI8") ||
                wstr_eq_c(e->str, "OpCastI16") ||
                wstr_eq_c(e->str, "OpCastI32") ||
                wstr_eq_c(e->str, "OpCastI64")) {
                return 1;
            }
            if (wstr_eq_c(e->str, "OpCast")) {
                return is_signed_expr(g, e->a);
            }
            return is_signed_expr(g, e->a) || is_signed_expr(g, e->b);
        case 9: {
            WFunc* f = func_by_name(g, e->str);
            if (f && f->return_count) {
                WType* t = resolve_named_type(g, f->returns[0]);
                return t && t->tag >= 4 && t->tag <= 7;
            }
            return 0;
        }
        default:
            return 0;
    }
}

static WStr wstr_sub(WStr s, size_t start, size_t len) {
    WStr out;
    out.ptr = s.ptr + start;
    out.len = (uint32_t)len;
    return out;
}

static void emit_call_patch(Gen* g, WStr target) {
    buf_u8(&g->code, 0xE8);
    size_t pos = g->code.len;
    buf_u32(&g->code, 0);
    CallPatch p;
    p.pos = pos;
    p.target = target;
    CallPatchVec_push(&g->call_patches, p);
    g->patch_generation++;
}

/* ------------------------------------------------------------------ */
/* Atomic operand sizing                                               */
/* ------------------------------------------------------------------ */

/* Atomic builtins take a pointer. Resolve its pointee so a u8/u16/u32
 * operation never touches adjacent memory (the old code always emitted
 * 64-bit instructions). Unknown types fall back to 8 bytes. */
static WType* atomic_pointee_type(Gen* g, WExpr* e) {
    WType* t = resolve_named_type(g, resolve_expr_type(g, e));
    if (t && t->tag == 10) {
        t = resolve_named_type(g, t->inner);
    }
    return t;
}

static uint32_t atomic_operand_size(Gen* g, WExpr* e) {
    uint32_t size = type_size(g, atomic_pointee_type(g, e));
    return (size == 1 || size == 2 || size == 4) ? size : 8;
}

static int atomic_operand_signed(Gen* g, WExpr* e) {
    WType* t = atomic_pointee_type(g, e);
    return t && (t->tag == 4 || t->tag == 5 || t->tag == 6);
}

/* Raw address loads/stores. These are compiler builtins, not new syntax:
 * existing Wand call expressions keep working while the back end emits the
 * exact-width x86 access directly. `volatile_*` variants add mfence ordering
 * for MMIO registers; all variants are unaligned-safe on x86. */
static int raw_mem_builtin(WStr name, uint32_t* width, int* is_store,
                           int* is_volatile, int* is_signed) {
    const char* prefix = NULL;
    int store = 0;
    int vol = 0;

    if (wstr_starts_with(name, "volatile_load_")) {
        prefix = "volatile_load_";
        vol = 1;
    } else if (wstr_starts_with(name, "volatile_store_")) {
        prefix = "volatile_store_";
        store = 1;
        vol = 1;
    } else if (wstr_starts_with(name, "mem_load_")) {
        prefix = "mem_load_";
    } else if (wstr_starts_with(name, "mem_store_")) {
        prefix = "mem_store_";
        store = 1;
    } else {
        return 0;
    }

    size_t prefix_len = strlen(prefix);
    WStr suffix = wstr_sub(name, prefix_len, name.len - prefix_len);
    if (wstr_eq_c(suffix, "u8")) {
        *width = 1;
    } else if (wstr_eq_c(suffix, "u16")) {
        *width = 2;
    } else if (wstr_eq_c(suffix, "u32")) {
        *width = 4;
    } else if (wstr_eq_c(suffix, "u64")) {
        *width = 8;
    } else if (wstr_eq_c(suffix, "i8")) {
        *width = 1;
    } else if (wstr_eq_c(suffix, "i16")) {
        *width = 2;
    } else if (wstr_eq_c(suffix, "i32")) {
        *width = 4;
    } else if (wstr_eq_c(suffix, "i64")) {
        *width = 8;
    } else {
        return 0;
    }
    *is_store = store;
    *is_volatile = vol;
    *is_signed = suffix.ptr && suffix.len && suffix.ptr[0] == 'i';
    return 1;
}

/* Operand-size prefix for atomic read-modify-write instructions. */
static void emit_atomic_prefix(Buf* b, uint32_t size) {
    if (size == 8) {
        buf_u8(b, 0x48);
    } else if (size == 2) {
        buf_u8(b, 0x66);
    }
}

/* Canonicalize rax after a narrow atomic operation. movzx/movsx also clear
 * stale high bits left behind by the 64-bit register scratch path. */
static void emit_atomic_rax_extend(Buf* b, uint32_t size, int is_signed) {
    if (size == 1) {
        if (is_signed) buf_u8(b, 0x48);
        buf_u8(b, 0x0F); buf_u8(b, is_signed ? 0xBE : 0xB6); buf_u8(b, 0xC0);
    } else if (size == 2) {
        if (is_signed) buf_u8(b, 0x48);
        buf_u8(b, 0x0F); buf_u8(b, is_signed ? 0xBF : 0xB7); buf_u8(b, 0xC0);
    } else if (size == 4 && is_signed) {
        buf_u8(b, 0x48); buf_u8(b, 0x63); buf_u8(b, 0xC0);
    } else if (size == 4) {
        buf_u8(b, 0x89); buf_u8(b, 0xC0);
    }
}

/* True for a simple 64-bit integer expression that can be evaluated straight
 * into rdi without a push/reload round trip. Used only for calls whose callee
 * takes one plain integer argument. */
static int simple_int_call_arg(Gen* g, WExpr* e, int depth) {
    if (!e || depth > 8 || e->tag == 4 || e->tag == 5 || e->tag == 9) {
        return 0;
    }
    if (e->tag == 0) {
        return 1;
    }
    if (e->tag == 3) {
        uint64_t off = 0, access = 3, type_ptr = 0;
        if (!map_get_str(&g->locals, e->str, &off)) return 0;
        map_get_str(&g->local_access, e->str, &access);
        if (access != 3) return 0;
        WType* t = map_get_str(&g->local_types, e->str, &type_ptr)
                       ? AS_PTR(WType, type_ptr)
                       : NULL;
        t = peel_typedefs(t);
        return t && (t->tag == 3 || t->tag == 7) && type_size(g, t) == 8;
    }
    if (e->tag == 8 && e->b && (e->b->tag == 0 || e->b->tag == 1) &&
        has_binary_imm(e->str)) {
        return simple_int_call_arg(g, e->a, depth + 1);
    }
    return 0;
}

/* Decide whether a binary/unary operand should keep the incoming pointer
 * dereference context. Plain *i locals are also used for pointer walkers
 * (`p = p + 1`), so they stay raw inside arithmetic; *i parameters and *io
 * variables name the pointed object and must be read through the slot. An
 * assignment to a *i variable temporarily suppresses dereferencing of that
 * same name so its pointer slot can be advanced. */
static int expr_deref_flag(Gen* g, WExpr* e, int deref_ptr) {
    if (!deref_ptr || !e || e->tag != 3) return deref_ptr;
    if (g->suppress_deref_var.ptr && wstr_eq(g->suppress_deref_var, e->str)) {
        return 0;
    }
    uint64_t access = 3;
    map_get_str(&g->local_access, e->str, &access);
    if (access == 0) {
        uint64_t is_param = 0;
        map_get_str(&g->local_is_param, e->str, &is_param);
        return is_param ? 1 : 0;
    }
    return deref_ptr;
}

/* Float value context always reads through input/input-output variables,
 * including *i locals used as byte walkers. The generic helper keeps those
 * locals raw for pointer arithmetic and comparisons, which is not appropriate
 * when the value feeds f64 arithmetic. */
static int float_value_deref_flag(Gen* g, WExpr* e, int deref_ptr) {
    if (!deref_ptr || !e || e->tag != 3) {
        return deref_ptr;
    }
    if (g->suppress_deref_var.ptr &&
        wstr_eq(g->suppress_deref_var, e->str)) {
        return 0;
    }
    uint64_t access = 3;
    map_get_str(&g->local_access, e->str, &access);
    return (access == 0 || access == 2) ? 1 : deref_ptr;
}

/* Pointer/array destinations preserve pointer identity: their source is
 * compiled in address context. Scalar destinations keep value context. */
static int value_deref_flag(Gen* g, WType* t) {
    t = resolve_named_type(g, t);
    return (t && (t->tag == 10 || t->tag == 12)) ? 0 : 1;
}

/* An assignment to an input or input/output variable writes the pointed
 * object, so its logical type is the pointee rather than the declared
 * pointer. A plain pointer destination still copies the address. */
static int assignment_deref_flag(Gen* g, WExpr* target) {
    WType* t = resolve_expr_type(g, target);
    if (target && target->tag == 3) {
        uint64_t access = 3;
        map_get_str(&g->local_access, target->str, &access);
        if (access == 0 || access == 2) {
            t = resolve_named_type(g, t);
            if (t && (t->tag == 10 || t->tag == 12)) {
                t = resolve_named_type(g, t->inner);
            }
        }
    }
    return value_deref_flag(g, t);
}

/* A return slot with pointer/array type must preserve the pointer value even
 * when the source variable carries an input or input/output modifier. Scalar
 * slots keep the existing value-read context. */
static int return_deref_flag(Gen* g, size_t i) {
    WFunc* f = g->current_function;
    if (f && i < f->return_count) {
        return value_deref_flag(g, f->returns[i]);
    }
    return 1;
}

/* Compile an assignment's value with the right pointer-read context. A
 * *i local/parameter assigned to itself is advancing its pointer slot, so
 * the same name must stay raw on the RHS; *io variables write through the
 * pointer and therefore read through it in the RHS. */
static void compile_assignment_value(Gen* g, WExpr* target, WExpr* value) {
    WStr saved = g->suppress_deref_var;
    if (target && target->tag == 3) {
        uint64_t access = 3;
        if (map_get_str(&g->local_access, target->str, &access) && access == 0) {
            g->suppress_deref_var = target->str;
        }
    }
    compile_expr(g, value, 0, assignment_deref_flag(g, target));
    g->suppress_deref_var = saved;
}

/* ------------------------------------------------------------------ */
/* compile_expr                                                        */
/* ------------------------------------------------------------------ */

static void compile_expr(Gen* g, WExpr* e, uint8_t reg, int deref_ptr) {
    Buf* b = &g->code;
    if (!e) {
        return;
    }
    switch (e->tag) {
        case 0:
        case 1:
            emit_mov_imm64(b, reg, e->num);
            break;
        case 2: {
            WStr key = wstr_prefix("str:", e->str);
            emit_rip_relative_lea(g, reg, key);
            return;
        }
        case 11: {
            /* f64 values travel as bit patterns in a GPR. Materialize the
             * literal directly instead of loading it into xmm and moving the
             * bits back to rax. setup_program() uses the same strtod parse,
             * so the emitted bits are identical. */
            char* text = (char*)arena_alloc(g_arena, e->str.len + 1);
            memcpy(text, e->str.ptr, e->str.len);
            text[e->str.len] = '\0';
            double value = strtod(text, NULL);
            uint64_t bits;
            memcpy(&bits, &value, sizeof(bits));
            emit_mov_imm64(b, reg, bits);
            break;
        }
        case 3: { /* Variable */
            uint64_t off;
            if (map_get_str(&g->locals, e->str, &off)) {
                uint64_t access = 3;
                map_get_str(&g->local_access, e->str, &access);
                uint64_t type_ptr;
                WType* var_type = map_get_str(&g->local_types, e->str, &type_ptr)
                                      ? AS_PTR(WType, type_ptr)
                                      : NULL;
                uint32_t var_size = type_size(g, var_type);
                uint8_t bound_reg = 0;
                uint8_t xmm_reg = 0;
                if (var_type && is_float_type_named(g,var_type) && var_size == 8 &&
                    xmm_local_lookup(g, e->str, &xmm_reg)) {
                    if (reg == 0) emit_movapd_xmm_to_xmm0(b, xmm_reg);
                    else emit_movq_xmm_to_gpr(b, xmm_reg, reg);
                    break;
                }
                if (reg_local_lookup(g, e->str, &bound_reg) && access == 3 &&
                    var_size == 8) {
                    emit_mov_reg_to_reg(b, bound_reg, reg);
                    break;
                }
                /* Peephole: if an rax store to this exact slot was the last
                 * emitted instruction and no patch was recorded since, the
                 * load back into rax is redundant. Replace it with NOPs of
                 * equal length so recorded patch positions stay valid. */
                if (reg == 0 && access == 3 && var_size == 8 &&
                    g->last_rax_store_len == 4 && g->last_rax_store_off == off &&
                    g->last_rax_store_gen == g->patch_generation &&
                    g->last_rax_store_pos + 4 == b->len &&
                    b->data[g->last_rax_store_pos] == 0x48 &&
                    b->data[g->last_rax_store_pos + 1] == 0x89 &&
                    b->data[g->last_rax_store_pos + 2] == 0x45 &&
                    b->data[g->last_rax_store_pos + 3] == (uint8_t)(-(int32_t)off)) {
                    /* No load needed: rax still holds the stored value.
                     * No patches were recorded since the store, so all
                     * recorded positions are before this point and stay
                     * valid when the 4-byte load is omitted. */
                    g->last_rax_store_len = 0;
                    break;
                }
                uint64_t is_param = 0;
                map_get_str(&g->local_is_param, e->str, &is_param);
                WType* local_type = resolve_named_type(g, var_type);
                if (local_type && local_type->tag == 12 && !is_param) {
                    emit_mem_op(b, 0x8D, reg, (uint32_t)off);
                } else if ((access == 0 || access == 2) && deref_ptr &&
                           !(g->suppress_deref_var.ptr &&
                             wstr_eq(g->suppress_deref_var, e->str))) {
                    /* Input and input/output pointer parameters read through
                     * the pointer only in value context. In address context
                     * (deref_ptr=0) they must remain the pointer itself, so
                     * they can be passed on to another function or indexed.
                     * An assignment to a *i variable also needs the raw
                     * pointer on its right-hand side. */
                    emit_mem_load(b, reg, (uint32_t)off, 8, 0);
                    WType* elem_type = NULL;
                    if (local_type && (local_type->tag == 10 || local_type->tag == 12)) {
                        elem_type = resolve_named_type(g, local_type->inner);
                    }
                    uint32_t elem_size =
                        elem_type ? type_size(g, elem_type) : var_size;
                    int elem_signed = elem_type &&
                        (elem_type->tag == 4 || elem_type->tag == 5 ||
                         elem_type->tag == 6);
                    uint8_t low = reg & 7;
                    int use_sib = low == 4 || low == 5;
                    uint8_t modrm = use_sib ? (uint8_t)((low == 5 ? 0x40 : 0) | (low << 3) | 0x04)
                                            : (uint8_t)((low << 3) | low);
                    uint8_t sib = use_sib ? (uint8_t)(0x20 | low) : 0;
                    int disp = use_sib && low == 5;
                    switch (elem_size) {
                        case 1:
                            buf_u8(b, reg >= 8 ? 0x4D : 0x48);
                            buf_u8(b, 0x0F); buf_u8(b, elem_signed ? 0xBE : 0xB6); buf_u8(b, modrm);
                            if (use_sib) buf_u8(b, sib);
                            if (disp) buf_u8(b, 0);
                            break;
                        case 2:
                            buf_u8(b, reg >= 8 ? 0x4D : 0x48);
                            buf_u8(b, 0x0F); buf_u8(b, elem_signed ? 0xBF : 0xB7); buf_u8(b, modrm);
                            if (use_sib) buf_u8(b, sib);
                            if (disp) buf_u8(b, 0);
                            break;
                        case 4:
                            if (elem_signed) {
                                buf_u8(b, reg >= 8 ? 0x49 : 0x48);
                                buf_u8(b, 0x63); buf_u8(b, modrm);
                            } else {
                                if (reg >= 8) buf_u8(b, 0x45);
                                buf_u8(b, 0x8B); buf_u8(b, modrm);
                            }
                            if (use_sib) buf_u8(b, sib);
                            if (disp) buf_u8(b, 0);
                            break;
                        default:
                            buf_u8(b, reg >= 8 ? 0x4D : 0x48);
                            buf_u8(b, 0x8B); buf_u8(b, modrm);
                            if (use_sib) buf_u8(b, sib);
                            if (disp) buf_u8(b, 0);
                            break;
                    }
                } else {
                    /* Array parameters hold a pointer in the stack slot. */
                    uint32_t load_size =
                        (local_type && local_type->tag == 12) ? 8 : var_size;
                    int local_signed = local_type && (local_type->tag == 4 ||
                                                      local_type->tag == 5 ||
                                                      local_type->tag == 6);
                    emit_mem_load(b, reg, (uint32_t)off, load_size,
                                  local_signed);
                }
            } else {
                uint64_t expr_ptr;
                if (map_get_str(&g->constants, e->str, &expr_ptr)) {
                    WExpr* c = AS_PTR(WExpr, expr_ptr);
                    if (c->tag == 2 || c->tag == 0 || c->tag == 1 || c->tag == 13) {
                        compile_expr(g, c, reg, deref_ptr);
                        break;
                    }
                    uint64_t value = eval_const(g, e);
                    buf_u8(b, 0x48);
                    buf_u8(b, (uint8_t)(0xB8 + reg));
                    buf_u64(b, value);
                } else {
                    WStr key = find_global_key(g, e->str);
                    if (key.ptr) {
                        size_t colon = 0;
                        while (colon < key.len && key.ptr[colon] != ':') colon++;
                        WExpr section = {0};
                        section.tag = 7;
                        section.str = wstr_sub(key, 0, colon);
                        section.str2 = wstr_sub(key, colon + 1, key.len - colon - 1);
                        compile_expr(g, &section, reg, deref_ptr);
                    } else if (map_get_str(&g->function_offsets, e->str, &off)) {
                        if (g->format == WAND_OUTPUT_OBJECT ||
                            g->format == WAND_OUTPUT_WEXP) {
                            WStr fnkey = wstr_prefix("fn:", e->str);
                            emit_rip_relative_lea(g, reg, fnkey);
                        } else {
                            emit_mov_imm64(b, reg, LOAD_BASE + off);
                        }
                    } else if (is_extern_func(g, e->str)) {
                        if (g->format == WAND_OUTPUT_OBJECT ||
                            g->format == WAND_OUTPUT_WEXP) {
                            WStr fnkey = wstr_prefix("fn:", e->str);
                            emit_rip_relative_lea(g, reg, fnkey);
                        } else {
                            /* A flat image has no linker to resolve an
                             * extern symbol. Keep the placeholder explicit. */
                            emit_mov_imm64(b, reg, 0);
                        }
                    }
                }
            }
            break;
        }
        case 4: /* AddrOf */
        case 5: { /* AddrOfExpr */
            compile_address(g, e->tag == 4 ? NULL : e->a, reg);
            if (e->tag == 4) {
                uint64_t off;
                if (map_get_str(&g->locals, e->str, &off)) {
                    emit_mem_op(b, 0x8D, reg, (uint32_t)off);
                } else {
                    WStr key = find_global_key(g, e->str);
                    if (key.ptr) {
                        size_t colon = 0;
                        while (colon < key.len && key.ptr[colon] != ':') colon++;
                        WExpr section = {0};
                        section.tag = 7;
                        section.str = wstr_sub(key, 0, colon);
                        section.str2 = wstr_sub(key, colon + 1, key.len - colon - 1);
                        compile_address(g, &section, reg);
                    } else if (map_get_str(&g->function_offsets, e->str, &off)) {
                        if (g->format == WAND_OUTPUT_OBJECT ||
                            g->format == WAND_OUTPUT_WEXP) {
                            WStr fnkey = wstr_prefix("fn:", e->str);
                            emit_rip_relative_lea(g, reg, fnkey);
                        } else {
                            emit_mov_imm64(b, reg, LOAD_BASE + off);
                        }
                    }
                }
            }
            break;
        }
        case 6: /* MemberAccess */
        case 10: { /* Index */
            WType* t = resolve_named_type(g, resolve_expr_type(g, e));
            uint8_t sib_base = 0, sib_index = 0;
            int sib_scale = -1;
            int sib_ready = e->tag == 10 && !(t && t->tag == 12) &&
                index_sib_ready(g, e, &sib_base, &sib_index, &sib_scale);
            if (!sib_ready) {
                /* The load below uses (%rbx) directly, so keep the computed
                 * address in rbx and skip the dead mov %rbx,%rcx. */
                compile_address(g, e, 3);
            }
            if (t && t->tag == 12) {
                if (reg == 0) {
                    buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xD8);
                }
            } else {
                uint32_t size = expr_size(g, e);
                WType* scalar = resolve_named_type(g, t);
                int is_signed = scalar && scalar->tag >= 4 && scalar->tag <= 7;
                if (sib_ready) {
                    emit_gpr_sib_load(b, reg, sib_base, sib_index, sib_scale,
                                      size, is_signed);
                    break;
                }
                uint8_t modrm = (uint8_t)(0x03 | ((reg & 7) << 3));
                /* modrm puts the destination in the reg field, so an
                 * extended destination needs REX.R as well as REX.W. */
                uint8_t load_rex = reg >= 8 ? 0x4C : 0x48;
                switch (size) {
                    case 1:
                        buf_u8(b, load_rex); buf_u8(b, 0x0F);
                        buf_u8(b, is_signed ? 0xBE : 0xB6);
                        buf_u8(b, modrm);
                        break;
                    case 2:
                        buf_u8(b, load_rex); buf_u8(b, 0x0F);
                        buf_u8(b, is_signed ? 0xBF : 0xB7);
                        buf_u8(b, modrm);
                        break;
                    case 4:
                        if (is_signed) {
                            buf_u8(b, load_rex); buf_u8(b, 0x63); buf_u8(b, modrm);
                        } else {
                            if (reg >= 8) buf_u8(b, 0x44);
                            buf_u8(b, 0x8B); buf_u8(b, modrm);
                        }
                        break;
                    default:
                        buf_u8(b, load_rex); buf_u8(b, 0x8B); buf_u8(b, modrm);
                        break;
                }
            }
            break;
        }
        case 13:
            emit_mov_imm64(b, reg, 0);
            break;
        case 7: { /* SectionAccess */
            uint64_t enum_idx;
            if (map_get_str(&g->enums, e->str, &enum_idx)) {
                WEnum* en = &g->prog.enums[enum_idx];
                for (size_t i = 0; i < en->count; i++) {
                    if (wstr_eq(en->values[i].name, e->str2)) {
                        emit_mov_imm64(b, reg, en->values[i].value);
                        return;
                    }
                }
            }
            WStr key = wstr_pair(e->str, e->str2);
            uint64_t type_ptr;
            WType* var_type = map_get_str(&g->section_types, key, &type_ptr)
                                  ? AS_PTR(WType, type_ptr)
                                  : NULL;
            /* Named typedefs are tag 11 and must resolve through g->typedefs;
             * peeling only inline tag 13 nodes loads a u32 typedef as u64. */
            WType* scalar_type = resolve_named_type(g, var_type);
            if (scalar_type && scalar_type->tag == 12) {
                emit_rip_relative_lea(g, reg, key);
            } else {
                emit_rip_relative_lea(g, 0, key);
                uint8_t tag = scalar_type ? scalar_type->tag : 3;
                switch (tag) {
                    case 0: /* u8 */
                        buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0xB6); buf_u8(b, 0x00);
                        break;
                    case 4: /* i8 */
                        buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0xBE); buf_u8(b, 0x00);
                        break;
                    case 1: /* u16 */
                        buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0xB7); buf_u8(b, 0x00);
                        break;
                    case 5: /* i16 */
                        buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0xBF); buf_u8(b, 0x00);
                        break;
                    case 2: /* u32: mov eax,[rax] zero-extends into rax */
                        buf_u8(b, 0x8B); buf_u8(b, 0x00);
                        break;
                    case 6: /* i32: movsxd rax,[rax] */
                        buf_u8(b, 0x48); buf_u8(b, 0x63); buf_u8(b, 0x00);
                        break;
                    case 3: /* u64 */
                    case 7: /* i64 */
                    case 8: /* f64 bit pattern */
                    default:
                        buf_u8(b, 0x48); buf_u8(b, 0x8B); buf_u8(b, 0x00);
                        break;
                }
                move_rax_to_reg(b, reg);
            }
            break;
        }
        case 8: { /* Binary */
            WStr op = e->str;
            if (wstr_eq_c(op, "OpAnd") || wstr_eq_c(op, "&&") || wstr_eq_c(op, "OpOr") ||
                wstr_eq_c(op, "||") || wstr_eq_c(op, "OpEq") || wstr_eq_c(op, "OpEqEq") ||
                wstr_eq_c(op, "==") || wstr_eq_c(op, "OpNotEq") || wstr_eq_c(op, "OpNe") ||
                wstr_eq_c(op, "!=") || wstr_eq_c(op, "OpLt") || wstr_eq_c(op, "Lt") ||
                wstr_eq_c(op, "<") || wstr_eq_c(op, "OpLtEq") || wstr_eq_c(op, "OpLe") ||
                wstr_eq_c(op, "<=") || wstr_eq_c(op, "OpGt") || wstr_eq_c(op, "Gt") ||
                wstr_eq_c(op, ">") || wstr_eq_c(op, "OpGtEq") || wstr_eq_c(op, "OpGe") ||
                wstr_eq_c(op, ">=")) {
                compile_bool_expr(g, e);
                move_rax_to_reg(b, reg);
                return;
            }
            if (wstr_eq_c(op, "OpCastF64")) {
                int src_is_float = is_float_expr(g, e->a);
                compile_expr(g, e->a, 0, deref_ptr);
                if (!src_is_float) {
                    emit_int_bits_to_f64_bits(g, 0, is_signed_expr(g, e->a));
                }
                if (reg != 0) move_rax_to_reg(b, reg);
                return;
            }
            if (wstr_eq_c(op, "OpCastInt")) {
                compile_expr(g, e->a, 0, deref_ptr);
                if (is_float_expr(g, e->a)) {
                    emit_cvttsd2si_rax(b);
                }
                if (reg != 0) move_rax_to_reg(b, reg);
                return;
            }
            /* Integer casts now carry the target width in the op name.
             * Narrow unsigned casts zero-extend; narrow signed casts sign-
             * extend. This fixes (u64)(u32)x for negative x, which used to
             * keep the sign extension from the i32 load. */
            if (wstr_eq_c(op, "OpCastU8") || wstr_eq_c(op, "OpCastU16") ||
                wstr_eq_c(op, "OpCastU32") || wstr_eq_c(op, "OpCastI8") ||
                wstr_eq_c(op, "OpCastI16") || wstr_eq_c(op, "OpCastI32")) {
                int from_float = is_float_expr(g, e->a);
                compile_expr(g, e->a, 0, deref_ptr);
                if (from_float) {
                    emit_cvttsd2si_rax(b);
                }
                if (wstr_eq_c(op, "OpCastU8")) {
                    buf_u8(b, 0x0F); buf_u8(b, 0xB6); buf_u8(b, 0xC0); /* movzx eax, al */
                } else if (wstr_eq_c(op, "OpCastU16")) {
                    buf_u8(b, 0x0F); buf_u8(b, 0xB7); buf_u8(b, 0xC0); /* movzx eax, ax */
                } else if (wstr_eq_c(op, "OpCastU32")) {
                    buf_u8(b, 0x89); buf_u8(b, 0xC0); /* mov eax, eax */
                } else if (wstr_eq_c(op, "OpCastI8")) {
                    buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0xBE); buf_u8(b, 0xC0);
                } else if (wstr_eq_c(op, "OpCastI16")) {
                    buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0xBF); buf_u8(b, 0xC0);
                } else {
                    buf_u8(b, 0x48); buf_u8(b, 0x63); buf_u8(b, 0xC0); /* movslq eax, rax */
                }
                if (reg != 0) move_rax_to_reg(b, reg);
                return;
            }
            if (wstr_eq_c(op, "OpCastU64") || wstr_eq_c(op, "OpCastI64")) {
                compile_expr(g, e->a, 0, deref_ptr);
                if (is_float_expr(g, e->a)) {
                    emit_cvttsd2si_rax(b);
                }
                if (reg != 0) move_rax_to_reg(b, reg);
                return;
            }
            if (wstr_eq_c(op, "OpCast")) {
                compile_expr(g, e->a, reg, deref_ptr);
                return;
            }
            if (wstr_eq_c(op, "OpBitNot")) {
                compile_expr(g, e->a, 0, expr_deref_flag(g, e->a, deref_ptr));
                buf_u8(b, 0x48); buf_u8(b, 0xF7); buf_u8(b, 0xD0);
                if (reg != 0) move_rax_to_reg(b, reg);
                return;
            }
            int float_op =
                is_float_value_expr(g, e->a, float_value_deref_flag(g, e->a, deref_ptr)) ||
                is_float_value_expr(g, e->b, float_value_deref_flag(g, e->b, deref_ptr));
            /* Fused and literal-RHS float ops stay in XMM registers. */
            if (compile_float_binary_to_xmm0(g, e)) {
                emit_movq_xmm0_to_rax(b);
                if (reg != 0) move_rax_to_reg(b, reg);
                return;
            }
            if (float_op && e->a && e->b && e->a->tag == 3 && e->b->tag == 3 &&
                wstr_eq(e->a->str, e->b->str) && emit_float_op_xmm1(NULL, op)) {
                /* x op x: load once, copy to rbx; avoids a second stack load. */
                compile_expr(g, e->a, 0, expr_deref_flag(g, e->a, deref_ptr));
                buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xC3);
                emit_float_op_xmm1(b, op);
                if (reg != 0) move_rax_to_reg(b, reg);
                return;
            }
            if (float_op && e->a && e->b && e->a->tag == 3 && e->b->tag == 3 &&
                emit_float_op_xmm1(NULL, op)) {
                /* Both operands are plain f64 stack slots. Load the LHS
                 * straight into xmm0 and apply the op to the RHS memory
                 * operand, skipping the GPR/movq round trips entirely. */
                uint64_t offa = 0, offb = 0;
                uint64_t acca = 3, accb = 3;
                map_get_str(&g->locals, e->a->str, &offa);
                map_get_str(&g->locals, e->b->str, &offb);
                map_get_str(&g->local_access, e->a->str, &acca);
                map_get_str(&g->local_access, e->b->str, &accb);
                uint64_t ta = 0, tb = 0;
                WType* va = map_get_str(&g->local_types, e->a->str, &ta)
                                ? AS_PTR(WType, ta) : NULL;
                WType* vb = map_get_str(&g->local_types, e->b->str, &tb)
                                ? AS_PTR(WType, tb) : NULL;
                if (acca == 3 && accb == 3 &&
                    is_float_type_named(g, va) && is_float_type_named(g, vb)) {
                    buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, 0x10);
                    buf_u8(b, offa <= 127 ? 0x45 : 0x85);
                    if (offa <= 127) buf_u8(b, (uint8_t)(-(int32_t)offa));
                    else buf_u32(b, (uint32_t)(-(int32_t)offa));
                    uint8_t code = (uint8_t)float_op_code(op);
                    buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, code);
                    buf_u8(b, offb <= 127 ? 0x45 : 0x85);
                    if (offb <= 127) buf_u8(b, (uint8_t)(-(int32_t)offb));
                    else buf_u32(b, (uint32_t)(-(int32_t)offb));
                    emit_movq_xmm0_to_rax(b);
                    if (reg != 0) move_rax_to_reg(b, reg);
                    return;
                }
            }
            if (float_op && simple_left(g, e->a) && emit_float_op_xmm1(NULL, op)) {
                /* Same shuffle removal as the integer path: evaluate the RHS
                 * straight into rbx, then the simple LHS into rax. Integer
                 * operands are converted to f64 before the xmm operation. */
                compile_float_operand_bits(g, e->b, 3,
                                           float_value_deref_flag(g, e->b, deref_ptr));
                compile_float_operand_bits(g, e->a, 0,
                                           float_value_deref_flag(g, e->a, deref_ptr));
                emit_float_op_xmm1(b, op);
                if (reg != 0) move_rax_to_reg(b, reg);
                return;
            }
            if (float_op && float_rhs_safe(e->b) && emit_float_op_xmm1(NULL, op)) {
                /* General case: stash LHS in xmm1, evaluate RHS into rax.
                 * Avoids push/pop and the rbx mov; safe because no call can
                 * clobber the stashed xmm1. */
                compile_float_operand_bits(g, e->a, 0,
                                           float_value_deref_flag(g, e->a, deref_ptr));
                emit_float_op_stash_lhs(b);
                compile_float_operand_bits(g, e->b, 0,
                                           float_value_deref_flag(g, e->b, deref_ptr));
                emit_float_op_after_rhs(b, op);
                if (reg != 0) move_rax_to_reg(b, reg);
                return;
            }
            if (!float_op && e->b && (e->b->tag == 0 || e->b->tag == 1)) {
                uint64_t uv = e->b->num;
                int64_t sv = (int64_t)uv;
                if ((wstr_eq_c(op, "OpShl") || wstr_eq_c(op, "OpShr")) &&
                    uv >= 1 && uv <= 63) {
                    uint8_t group = wstr_eq_c(op, "OpShl") ? 4
                                  : (is_signed_expr(g, e->a) ? 7 : 5);
                    compile_expr(g, e->a, 0, expr_deref_flag(g, e->a, deref_ptr));
                    buf_u8(b, 0x48); buf_u8(b, 0xC1);
                    buf_u8(b, (uint8_t)(0xC0 | (group << 3)));
                    buf_u8(b, (uint8_t)uv);
                    if (reg != 0) move_rax_to_reg(b, reg);
                    return;
                }
                if (has_binary_imm(op)) {
                    compile_expr(g, e->a, 0, expr_deref_flag(g, e->a, deref_ptr));
                    if (sv >= INT32_MIN && sv <= INT32_MAX) {
                        emit_binary_imm(b, op, sv);
                    } else {
                        /* Large 64-bit constant: load straight into rbx
                         * instead of push/pop round-tripping rax. */
                        buf_u8(b, 0x48); buf_u8(b, 0xBB); buf_u64(b, uv);
                        emit_binary_rbx(b, op);
                    }
                    if (reg != 0) move_rax_to_reg(b, reg);
                    return;
                }
            }
            if (!float_op && has_binary_imm(op) && e->b && e->b->tag == 3) {
                /* RHS is a plain 64-bit local: apply the op directly to its
                 * memory slot, or to its bound register when the loop keeps
                 * it in one. Both avoid rbx, which compile_address may have
                 * already loaded with the array base. Handles i*128 + p
                 * style index math without a push/pop round trip. */
                uint64_t access = 3;
                map_get_str(&g->local_access, e->b->str, &access);
                uint64_t off = 0;
                uint8_t bound_reg = 0;
                uint32_t rhs_size = expr_size(g, e->b);
                int is_bound = access == 3 && rhs_size == 8 &&
                    reg_local_lookup(g, e->b->str, &bound_reg);
                if (access == 3 && rhs_size >= 1 && rhs_size < 8 &&
                    rax_only_expr(g, e->a) &&
                    map_get_str(&g->locals, e->b->str, &off) &&
                    has_binary_imm(op)) {
                    /* Narrow stack slots need an explicit zero/sign extension.
                     * Preserve rbx because compile_address may already hold
                     * an array base there when this expression is an index. */
                    compile_expr(g, e->a, 0, expr_deref_flag(g, e->a, deref_ptr));
                    buf_u8(b, 0x53); /* push rbx */
                    emit_mem_load(b, 3, (uint32_t)off, rhs_size,
                                  is_signed_expr(g, e->b));
                    emit_binary_rbx(b, op);
                    buf_u8(b, 0x5B); /* pop rbx */
                    if (reg != 0) move_rax_to_reg(b, reg);
                    return;
                }
                if (access == 3 && rhs_size == 8 && rax_only_expr(g, e->a) &&
                    (is_bound || map_get_str(&g->locals, e->b->str, &off))) {
                    uint8_t opcode = 0;
                    if (wstr_eq_c(op, "OpAdd")) opcode = 0x03;
                    else if (wstr_eq_c(op, "OpSub")) opcode = 0x2B;
                    else if (wstr_eq_c(op, "OpBitAnd")) opcode = 0x23;
                    else if (wstr_eq_c(op, "OpBitOr")) opcode = 0x0B;
                    else if (wstr_eq_c(op, "OpBitXor")) opcode = 0x33;
                    else if (wstr_eq_c(op, "OpMul")) opcode = 0xAF;
                    if (opcode) {
                        compile_expr(g, e->a, 0, expr_deref_flag(g, e->a, deref_ptr));
                        if (is_bound) {
                            buf_u8(b, (uint8_t)(0x48 | (bound_reg >= 8 ? 1 : 0)));
                            if (opcode == 0xAF) buf_u8(b, 0x0F);
                            buf_u8(b, opcode);
                            buf_u8(b, (uint8_t)(0xC0 | (bound_reg & 7)));
                        } else {
                            buf_u8(b, 0x48);
                            if (opcode == 0xAF) buf_u8(b, 0x0F);
                            buf_u8(b, opcode);
                            buf_u8(b, off <= 127 ? 0x45 : 0x85);
                            if (off <= 127) buf_u8(b, (uint8_t)(-(int32_t)off));
                            else buf_u32(b, (uint32_t)(-(int32_t)off));
                        }
                        if (reg != 0) move_rax_to_reg(b, reg);
                        return;
                    }
                }
            }
            if (!float_op && has_binary_imm(op) &&
                (simple_left(g, e->a) || simple_left(g, e->b))) {
                /* A simple operand never clobbers rbx, so it can be compiled
                 * straight into rbx while the other side stays in rax. This
                 * removes the push/mov/pop round trip in both directions. */
                if (simple_left(g, e->a)) {
                    compile_expr(g, e->b, 3,
                                 expr_deref_flag(g, e->b, deref_ptr));
                    compile_expr(g, e->a, 0,
                                 expr_deref_flag(g, e->a, deref_ptr));
                } else {
                    compile_expr(g, e->a, 0,
                                 expr_deref_flag(g, e->a, deref_ptr));
                    compile_expr(g, e->b, 3,
                                 expr_deref_flag(g, e->b, deref_ptr));
                }
                emit_binary_rbx(b, op);
                if (reg != 0) move_rax_to_reg(b, reg);
                return;
            }
            if (!float_op && has_binary_imm(op) && !g->r10_busy &&
                !expr_has_call(g, e->a) && !expr_has_call(g, e->b)) {
                /* Neither operand is simple. Keep the LHS in r10 across the
                 * RHS evaluation, avoiding a push/pop stack round trip. r10
                 * is caller-saved, so the LHS must not contain a call and
                 * the RHS must not call or reuse r10 while the LHS is live. */
                compile_expr(g, e->a, 10,
                             expr_deref_flag(g, e->a, deref_ptr));
                g->r10_busy = 1;
                compile_expr(g, e->b, 0,
                             expr_deref_flag(g, e->b, deref_ptr));
                g->r10_busy = 0;
                if (wstr_eq_c(op, "OpSub")) {
                    buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xC3); /* mov %rax,%rbx */
                    buf_u8(b, 0x4C); buf_u8(b, 0x89); buf_u8(b, 0xD0); /* mov %r10,%rax */
                } else {
                    buf_u8(b, 0x4C); buf_u8(b, 0x89); buf_u8(b, 0xD3); /* mov %r10,%rbx */
                }
                emit_binary_rbx(b, op);
                if (reg != 0) move_rax_to_reg(b, reg);
                return;
            }
            if (float_op) {
                compile_float_operand_bits(g, e->a, 0,
                                           float_value_deref_flag(g, e->a, deref_ptr));
                buf_u8(b, 0x50);
                compile_float_operand_bits(g, e->b, 0,
                                           float_value_deref_flag(g, e->b, deref_ptr));
            } else {
                compile_expr(g, e->a, 0,
                             expr_deref_flag(g, e->a, deref_ptr));
                buf_u8(b, 0x50);
                compile_expr(g, e->b, 0,
                             expr_deref_flag(g, e->b, deref_ptr));
            }
            buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xC3);
            buf_u8(b, 0x58);
            if (float_op && emit_float_op_xmm1(b, op)) {
                /* Already emitted the float op via xmm registers. */
            } else if (wstr_eq_c(op, "OpAdd")) {
                buf_u8(b, 0x48); buf_u8(b, 0x01); buf_u8(b, 0xD8);
            } else if (wstr_eq_c(op, "OpSub")) {
                buf_u8(b, 0x48); buf_u8(b, 0x29); buf_u8(b, 0xD8);
            } else if (wstr_eq_c(op, "OpMul")) {
                buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0xAF); buf_u8(b, 0xC3);
            } else if (wstr_eq_c(op, "OpDiv")) {
                if (is_signed_expr(g, e->a) || is_signed_expr(g, e->b)) {
                    buf_u8(b, 0x48); buf_u8(b, 0x99); buf_u8(b, 0x48); buf_u8(b, 0xF7); buf_u8(b, 0xFB);
                } else {
                    buf_u8(b, 0x48); buf_u8(b, 0x31); buf_u8(b, 0xD2); buf_u8(b, 0x48); buf_u8(b, 0xF7); buf_u8(b, 0xF3);
                }
            } else if (wstr_eq_c(op, "OpMod")) {
                if (!float_op && (is_signed_expr(g, e->a) || is_signed_expr(g, e->b))) {
                    static const uint8_t seq[] = {0x48,0x99,0x48,0xF7,0xFB,0x48,0x89,0xD0};
                    buf_bytes(b, seq, sizeof(seq));
                } else {
                    static const uint8_t seq[] = {0x48,0x31,0xD2,0x48,0xF7,0xF3,0x48,0x89,0xD0};
                    buf_bytes(b, seq, sizeof(seq));
                }
            } else if (wstr_eq_c(op, "OpBitAnd")) {
                buf_u8(b, 0x48); buf_u8(b, 0x21); buf_u8(b, 0xD8);
            } else if (wstr_eq_c(op, "OpBitOr")) {
                buf_u8(b, 0x48); buf_u8(b, 0x09); buf_u8(b, 0xD8);
            } else if (wstr_eq_c(op, "OpBitXor")) {
                buf_u8(b, 0x48); buf_u8(b, 0x31); buf_u8(b, 0xD8);
            } else if (wstr_eq_c(op, "OpShl")) {
                static const uint8_t seq[] = {0x48,0x89,0xD9,0x48,0xD3,0xE0};
                buf_bytes(b, seq, sizeof(seq));
            } else if (wstr_eq_c(op, "OpShr")) {
                static const uint8_t seq_signed[] = {0x48,0x89,0xD9,0x48,0xD3,0xF8};
                static const uint8_t seq_unsigned[] = {0x48,0x89,0xD9,0x48,0xD3,0xE8};
                if (is_signed_expr(g, e->a)) {
                    buf_bytes(b, seq_signed, sizeof(seq_signed));
                } else {
                    buf_bytes(b, seq_unsigned, sizeof(seq_unsigned));
                }
            }
            break;
        }
        case 9: { /* Call */
            WStr name = e->str;
            size_t argc = e->arg_count;
            if (wstr_eq_c(name, "nameof")) {
                if (argc && e->args[0]->tag == 3) {
                    WStr key = wstr_prefix("str:", e->args[0]->str);
                    emit_rip_relative_lea(g, reg, key);
                } else {
                    emit_mov_imm64(b, reg, 0);
                }
                return;
            }
            if (wstr_eq_c(name, "sizeof") || wstr_eq_c(name, "alignof") ||
                wstr_eq_c(name, "offsetof") || wstr_eq_c(name, "versionof") ||
                wstr_eq_c(name, "fieldsof")) {
                emit_mov_imm64(b, reg, eval_const(g, e));
                return;
            }
            if (wstr_eq_c(name, "jmpto")) {
                if (argc) {
                    compile_expr(g, e->args[0], 7, 0);
                } else {
                    emit_mov_imm64(b, 7, 0);
                }
                /* rdi = module name, rsi = args (none), rdx = 0,
                 * rcx = one result slot, r8 = 1. Keep rsp 16-byte aligned. */
                static const uint8_t alloc_slots[] = {0x48, 0x83, 0xEC, 0x10};
                static const uint8_t mov_rsi_rsp[] = {0x48, 0x89, 0xE6};
                static const uint8_t xor_edx[] = {0x48, 0x31, 0xD2};
                static const uint8_t lea_rcx_slot[] = {0x48, 0x8D, 0x48, 0x08};
                static const uint8_t load_rax_slot[] = {0x48, 0x8B, 0x44, 0x24, 0x08};
                static const uint8_t free_slots[] = {0x48, 0x83, 0xC4, 0x10};
                buf_bytes(b, alloc_slots, sizeof(alloc_slots));
                buf_bytes(b, mov_rsi_rsp, sizeof(mov_rsi_rsp));
                buf_bytes(b, xor_edx, sizeof(xor_edx));
                buf_bytes(b, lea_rcx_slot, sizeof(lea_rcx_slot));
                emit_mov_imm64(b, 8, 1);
                emit_call_patch(g, wstr_lit("__wand_jmpto_loader"));
                buf_bytes(b, load_rax_slot, sizeof(load_rax_slot));
                buf_bytes(b, free_slots, sizeof(free_slots));
                move_rax_to_reg(b, reg);
                return;
            }
            if (wstr_eq_c(name, "memory_barrier")) {
                buf_u8(b, 0x0F); buf_u8(b, 0xAE); buf_u8(b, 0xF0);
                buf_u8(b, 0x48); buf_u8(b, 0x31); buf_u8(b, 0xC0);
                move_rax_to_reg(b, reg);
                return;
            }
            if (wstr_eq_c(name, "compiler_barrier")) {
                buf_u8(b, 0x48); buf_u8(b, 0x31); buf_u8(b, 0xC0);
                move_rax_to_reg(b, reg);
                return;
            }
            {
                uint32_t width = 0;
                int is_store = 0;
                int is_volatile = 0;
                int is_signed = 0;
                if (raw_mem_builtin(name, &width, &is_store, &is_volatile,
                                    &is_signed)) {
                    static const uint8_t mfence[] = {0x0F, 0xAE, 0xF0};
                    if (is_volatile) buf_bytes(b, mfence, sizeof(mfence));
                    if (!is_store) {
                        if (!argc) {
                            emit_mov_imm64(b, reg, 0);
                            return;
                        }
                        compile_expr(g, e->args[0], 0, 0);
                        if (width == 1) {
                            if (is_signed) buf_u8(b, 0x48);
                            buf_u8(b, 0x0F); buf_u8(b, is_signed ? 0xBE : 0xB6); buf_u8(b, 0x00);
                        } else if (width == 2) {
                            if (is_signed) buf_u8(b, 0x48);
                            buf_u8(b, 0x0F); buf_u8(b, is_signed ? 0xBF : 0xB7); buf_u8(b, 0x00);
                        } else if (width == 4 && is_signed) {
                            buf_u8(b, 0x48); buf_u8(b, 0x63); buf_u8(b, 0x00);
                        } else if (width == 4) {
                            buf_u8(b, 0x8B); buf_u8(b, 0x00);
                        } else {
                            buf_u8(b, 0x48); buf_u8(b, 0x8B); buf_u8(b, 0x00);
                        }
                        if (is_volatile) buf_bytes(b, mfence, sizeof(mfence));
                        move_rax_to_reg(b, reg);
                        return;
                    }
                    if (argc < 2) {
                        emit_mov_imm64(b, reg, 0);
                        return;
                    }
                    static const uint8_t alloc_slots[] = {0x48, 0x83, 0xEC, 0x10};
                    static const uint8_t free_slots[] = {0x48, 0x83, 0xC4, 0x10};
                    buf_bytes(b, alloc_slots, sizeof(alloc_slots));
                    compile_expr(g, e->args[0], 0, 0);
                    emit_mov_to_stack(b, 0, 0);
                    compile_expr(g, e->args[1], 0, 1);
                    emit_mov_from_stack(b, 1, 0);
                    if (width == 1) {
                        buf_u8(b, 0x88); buf_u8(b, 0x01);
                    } else if (width == 2) {
                        buf_u8(b, 0x66); buf_u8(b, 0x89); buf_u8(b, 0x01);
                    } else if (width == 4) {
                        buf_u8(b, 0x89); buf_u8(b, 0x01);
                    } else {
                        buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0x01);
                    }
                    if (is_volatile) buf_bytes(b, mfence, sizeof(mfence));
                    buf_bytes(b, free_slots, sizeof(free_slots));
                    emit_mov_imm64(b, reg, 0);
                    return;
                }
            }
            if (wstr_eq_c(name, "atomic_load")) {
                if (!argc) {
                    emit_mov_imm64(b, reg, 0);
                    return;
                }
                uint32_t size = atomic_operand_size(g, e->args[0]);
                int is_signed = atomic_operand_signed(g, e->args[0]);
                compile_expr(g, e->args[0], 0, 1);
                if (size == 1) {
                    if (is_signed) buf_u8(b, 0x48);
                    buf_u8(b, 0x0F); buf_u8(b, is_signed ? 0xBE : 0xB6); buf_u8(b, 0x00);
                } else if (size == 2) {
                    if (is_signed) buf_u8(b, 0x48);
                    buf_u8(b, 0x0F); buf_u8(b, is_signed ? 0xBF : 0xB7); buf_u8(b, 0x00);
                } else if (size == 4 && is_signed) {
                    buf_u8(b, 0x48); buf_u8(b, 0x63); buf_u8(b, 0x00);
                } else if (size == 4) {
                    buf_u8(b, 0x8B); buf_u8(b, 0x00);
                } else {
                    buf_u8(b, 0x48); buf_u8(b, 0x8B); buf_u8(b, 0x00);
                }
                move_rax_to_reg(b, reg);
                return;
            }
            if (wstr_eq_c(name, "atomic_store")) {
                if (argc < 2) {
                    emit_mov_imm64(b, reg, 0);
                    return;
                }
                uint32_t size = atomic_operand_size(g, e->args[0]);
                compile_expr(g, e->args[0], 0, 1);
                buf_u8(b, 0x50);
                compile_expr(g, e->args[1], 0, 1);
                buf_u8(b, 0x5B);
                if (size == 1) {
                    buf_u8(b, 0x88); buf_u8(b, 0x03);
                } else if (size == 2) {
                    buf_u8(b, 0x66); buf_u8(b, 0x89); buf_u8(b, 0x03);
                } else if (size == 4) {
                    buf_u8(b, 0x89); buf_u8(b, 0x03);
                } else {
                    buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0x03);
                }
                buf_u8(b, 0x0F); buf_u8(b, 0xAE); buf_u8(b, 0xF0);
                move_rax_to_reg(b, reg);
                return;
            }
            if (wstr_eq_c(name, "atomic_add") || wstr_eq_c(name, "atomic_sub") ||
                wstr_eq_c(name, "atomic_swap")) {
                if (argc < 2) {
                    emit_mov_imm64(b, reg, 0);
                    return;
                }
                uint32_t size = atomic_operand_size(g, e->args[0]);
                int is_signed = atomic_operand_signed(g, e->args[0]);
                compile_expr(g, e->args[0], 0, 1);
                buf_u8(b, 0x50);
                compile_expr(g, e->args[1], 0, 1);
                if (wstr_eq_c(name, "atomic_sub")) {
                    if (size == 1) {
                        buf_u8(b, 0xF6); buf_u8(b, 0xD8);
                    } else if (size == 2) {
                        buf_u8(b, 0x66); buf_u8(b, 0xF7); buf_u8(b, 0xD8);
                    } else if (size == 4) {
                        buf_u8(b, 0xF7); buf_u8(b, 0xD8);
                    } else {
                        buf_u8(b, 0x48); buf_u8(b, 0xF7); buf_u8(b, 0xD8);
                    }
                }
                buf_u8(b, 0x5B);
                if (wstr_eq_c(name, "atomic_swap")) {
                    if (size == 1) {
                        buf_u8(b, 0x86); buf_u8(b, 0x03);
                    } else {
                        emit_atomic_prefix(b, size);
                        buf_u8(b, 0x87); buf_u8(b, 0x03);
                    }
                } else {
                    buf_u8(b, 0xF0);
                    emit_atomic_prefix(b, size);
                    buf_u8(b, 0x0F); buf_u8(b, 0xC1); buf_u8(b, 0x03);
                }
                emit_atomic_rax_extend(b, size, is_signed);
                move_rax_to_reg(b, reg);
                return;
            }
            if (wstr_eq_c(name, "atomic_inc") || wstr_eq_c(name, "atomic_dec")) {
                if (!argc) {
                    emit_mov_imm64(b, reg, 0);
                    return;
                }
                uint32_t size = atomic_operand_size(g, e->args[0]);
                int is_signed = atomic_operand_signed(g, e->args[0]);
                int inc = wstr_eq_c(name, "atomic_inc");
                compile_expr(g, e->args[0], 0, 1);
                buf_u8(b, 0x50);
                emit_mov_imm64(b, 0, inc ? 1u : 0xFFFFFFFFu);
                buf_u8(b, 0x5B);
                buf_u8(b, 0xF0);
                emit_atomic_prefix(b, size);
                buf_u8(b, 0x0F); buf_u8(b, 0xC1); buf_u8(b, 0x03);
                emit_atomic_rax_extend(b, size, is_signed);
                if (size == 1) {
                    buf_u8(b, 0xFE); buf_u8(b, inc ? 0xC0 : 0xC8);
                } else if (size == 2) {
                    buf_u8(b, 0x66); buf_u8(b, 0xFF); buf_u8(b, inc ? 0xC0 : 0xC8);
                } else if (size == 4) {
                    buf_u8(b, 0xFF); buf_u8(b, inc ? 0xC0 : 0xC8);
                } else {
                    buf_u8(b, 0x48); buf_u8(b, 0xFF); buf_u8(b, inc ? 0xC0 : 0xC8);
                }
                emit_atomic_rax_extend(b, size, is_signed);
                move_rax_to_reg(b, reg);
                return;
            }
            if (wstr_eq_c(name, "atomic_cas")) {
                if (argc < 3) {
                    emit_mov_imm64(b, reg, 0);
                    return;
                }
                uint32_t size = atomic_operand_size(g, e->args[0]);
                int is_signed = atomic_operand_signed(g, e->args[0]);
                compile_expr(g, e->args[0], 0, 1);
                buf_u8(b, 0x50);
                compile_expr(g, e->args[1], 0, 1);
                buf_u8(b, 0x50);
                compile_expr(g, e->args[2], 0, 1);
                if (size == 1) {
                    buf_u8(b, 0x88); buf_u8(b, 0xC1);
                } else if (size == 2) {
                    buf_u8(b, 0x66); buf_u8(b, 0x89); buf_u8(b, 0xC1);
                } else if (size == 4) {
                    buf_u8(b, 0x89); buf_u8(b, 0xC1);
                } else {
                    buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xC1);
                }
                buf_u8(b, 0x58);
                buf_u8(b, 0x5B);
                buf_u8(b, 0xF0);
                emit_atomic_prefix(b, size);
                buf_u8(b, 0x0F); buf_u8(b, 0xB1); buf_u8(b, 0x0B);
                emit_atomic_rax_extend(b, size, is_signed);
                move_rax_to_reg(b, reg);
                return;
            }
            if (name.len == 8 && wstr_starts_with(name, "syscall")) {
                char digit = name.ptr[7];
                if (digit >= '0' && digit <= '9') {
                    int nargs = digit - '0';
                    for (int i = nargs; i >= 1; i--) {
                        if ((size_t)i < argc) {
                            compile_expr(g, e->args[i], 0, 1);
                            buf_u8(b, 0x50);
                        }
                    }
                    if (argc) {
                        compile_expr(g, e->args[0], 0, 1);
                    }
                    static const uint8_t pops[] = {0x5F, 0x5E, 0x5A, 0x41, 0x5A, 0x41, 0x58, 0x41, 0x59};
                    static const uint8_t pop_lens[] = {1, 1, 1, 2, 2, 2};
                    size_t pos = 0;
                    for (int i = 1; i <= nargs && i <= 6; i++) {
                        size_t n = pop_lens[i - 1];
                        buf_bytes(b, pops + pos, n);
                        pos += n;
                    }
                    buf_u8(b, 0x0F); buf_u8(b, 0x05);
                    return;
                }
            }
            if (wstr_eq_c(name, "inb") || wstr_eq_c(name, "inw") || wstr_eq_c(name, "inl")) {
                if (argc) compile_expr(g, e->args[0], 1, 1);
                if (wstr_eq_c(name, "inb")) {
                    static const uint8_t seq[] = {0x66,0x89,0xCA,0xEC,0x48,0x0F,0xB6,0xC0};
                    buf_bytes(b, seq, sizeof(seq));
                } else if (wstr_eq_c(name, "inw")) {
                    static const uint8_t seq[] = {0x66,0x89,0xCA,0x66,0xED,0x48,0x0F,0xB7,0xC0};
                    buf_bytes(b, seq, sizeof(seq));
                } else {
                    static const uint8_t seq[] = {0x89,0xCA,0xED};
                    buf_bytes(b, seq, sizeof(seq));
                }
                return;
            }
            if (wstr_eq_c(name, "outb") || wstr_eq_c(name, "outw") || wstr_eq_c(name, "outl")) {
                if (argc) compile_expr(g, e->args[0], 1, 1);
                if (argc > 1) compile_expr(g, e->args[1], 0, 1);
                if (wstr_eq_c(name, "outb")) {
                    static const uint8_t seq[] = {0x66,0x89,0xCA,0xEE};
                    buf_bytes(b, seq, sizeof(seq));
                } else if (wstr_eq_c(name, "outw")) {
                    static const uint8_t seq[] = {0x66,0x89,0xCA,0x66,0xEF};
                    buf_bytes(b, seq, sizeof(seq));
                } else {
                    static const uint8_t seq[] = {0x89,0xCA,0xEF};
                    buf_bytes(b, seq, sizeof(seq));
                }
                return;
            }

            WFunc* callee = func_by_name(g, name);

            /* Simple one-argument integer calls can evaluate the argument
             * straight into rdi. Recursive fibonacci used to push the value,
             * reload rdi, and adjust rsp for each of its two calls. */
            if (argc == 1 && callee && callee->param_count) {
                WParam* param = &callee->params[0];
                WType* pt = resolve_named_type(g, param->type);
                if (param->access == 3 && pt &&
                    (pt->tag == 3 || pt->tag == 7) && type_size(g, pt) == 8 &&
                    simple_int_call_arg(g, e->args[0], 0)) {
                    compile_expr(g, e->args[0], 7, 1);
                    uint32_t align_pad = 0;
                    if ((g->stack_depth % 16) != 0) {
                        buf_u8(b, 0x48); buf_u8(b, 0x83); buf_u8(b, 0xEC); buf_u8(b, 0x08);
                        g->stack_depth += 8;
                        align_pad = 8;
                    }
                    emit_call_patch(g, name);
                    if (align_pad) {
                        buf_u8(b, 0x48); buf_u8(b, 0x83); buf_u8(b, 0xC4); buf_u8(b, 0x08);
                        g->stack_depth -= 8;
                    }
                    if (callee->return_count) {
                        WType* rt = resolve_named_type(g, callee->returns[0]);
                        if (rt && rt->tag == 8) {
                            buf_u8(b, 0x66); buf_u8(b, 0x48); buf_u8(b, 0x0F);
                            buf_u8(b, 0x7E); buf_u8(b, 0xC0);
                        }
                    }
                    move_rax_to_reg(b, reg);
                    return;
                }
            }

            /* Ordinary call: classify arguments by the callee signature. */
            uint8_t* is_float = (uint8_t*)arena_alloc(g_arena, argc ? argc : 1);
            size_t* class_pos = (size_t*)arena_alloc(g_arena, (argc ? argc : 1) * sizeof(size_t));
            int int_count = 0, float_count = 0;
            uint8_t* want_pointer = (uint8_t*)arena_alloc(g_arena, argc ? argc : 1);
            for (size_t i = 0; i < argc; i++) {
                int f = 0;
                int p = 0;
                if (callee && i < callee->param_count) {
                    WType* pt = resolve_named_type(g, callee->params[i].type);
                    uint8_t access = callee->params[i].access;
                    f = pt && pt->tag == 8;
                    /* An access modifier (in, out, in-out) makes a pointer
                     * parameter even when its declared type is a scalar. */
                    p = (access == 0 || access == 1 || access == 2) ||
                        (pt && (pt->tag == 10 || pt->tag == 12));
                }
                is_float[i] = (uint8_t)f;
                want_pointer[i] = (uint8_t)p;
                class_pos[i] = (size_t)(f ? float_count : int_count);
                if (f) float_count++; else int_count++;
            }
            size_t int_reg_count = int_count < 6 ? (size_t)int_count : 6;
            size_t float_reg_count = float_count < 8 ? (size_t)float_count : 8;
            size_t stack_arg_count = (size_t)(int_count - (int)int_reg_count) +
                                     (size_t)(float_count - (int)float_reg_count);
            size_t reg_arg_count = argc - stack_arg_count;
            size_t* stack_off = (size_t*)arena_alloc(
                g_arena, (stack_arg_count ? stack_arg_count : 1) * sizeof(size_t));
            size_t stack_i = 0;
            for (size_t i = 0; i < argc; i++) {
                size_t pos = class_pos[i];
                if ((is_float[i] && pos >= float_reg_count) ||
                    (!is_float[i] && pos >= int_reg_count)) {
                    stack_off[stack_i++] = (argc - 1 - i) * 8;
                }
            }
            g->stack_depth += 8 * argc;
            for (size_t i = 0; i < argc; i++) {
                compile_expr(g, e->args[i], 0, want_pointer[i] ? 0 : 1);
                buf_u8(b, 0x50);
            }
            static const uint8_t int_regs[6] = {7, 6, 2, 1, 8, 9};
            for (size_t i = 0; i < argc; i++) {
                size_t off = (argc - 1 - i) * 8;
                if (is_float[i]) {
                    if (class_pos[i] < 8) {
                        emit_mov_from_stack(b, 0, off);
                        buf_u8(b, 0x66); buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0x6E);
                        buf_u8(b, (uint8_t)(0xC0 + ((class_pos[i] & 7) << 3)));
                    }
                } else if (class_pos[i] < 6) {
                    emit_mov_from_stack(b, int_regs[class_pos[i]], off);
                }
            }
            uint32_t align_pad = 0;
            if (stack_arg_count > 0) {
                /* Sources and destinations overlap. Move each cycle with r10
                 * as the scratch register so no unread source is clobbered. */
                int* slot_owner = (int*)arena_alloc(g_arena, (argc ? argc : 1) * sizeof(int));
                uint8_t* moved = (uint8_t*)arena_alloc(g_arena, (stack_arg_count ? stack_arg_count : 1));
                for (size_t i = 0; i < argc; i++) slot_owner[i] = -1;
                for (size_t i = 0; i < stack_arg_count; i++) {
                    slot_owner[stack_off[i] / 8] = (int)i;
                    moved[i] = 0;
                }
                for (size_t i = 0; i < stack_arg_count; i++) {
                    if (moved[i]) continue;
                    moved[i] = 1;
                    emit_mov_from_stack(b, 10, stack_off[i]);
                    size_t cur = i;
                    for (;;) {
                        size_t dst = reg_arg_count * 8 + cur * 8;
                        int next = slot_owner[dst / 8];
                        if (next < 0 || moved[next]) {
                            emit_mov_to_stack(b, 10, dst);
                            break;
                        }
                        emit_mov_from_stack(b, 11, stack_off[next]);
                        emit_mov_to_stack(b, 10, dst);
                        static const uint8_t mov_r10_r11[] = {0x4D, 0x89, 0xDA};
                        buf_bytes(b, mov_r10_r11, sizeof(mov_r10_r11));
                        moved[next] = 1;
                        cur = (size_t)next;
                    }
                }
                uint32_t remove = (uint32_t)(reg_arg_count * 8);
                if (remove) {
                    buf_u8(b, 0x48); buf_u8(b, 0x81); buf_u8(b, 0xC4); buf_u32(b, remove);
                }
                g->stack_depth -= reg_arg_count * 8;
                if ((g->stack_depth % 16) != 0) {
                    /* rsp must be 16-byte aligned at the call while the stack
                     * arguments stay at [rsp], [rsp+8], ... . Put the padding
                     * above the block and move the block down eight bytes. */
                    buf_u8(b, 0x48); buf_u8(b, 0x83); buf_u8(b, 0xEC); buf_u8(b, 0x08);
                    g->stack_depth += 8;
                    align_pad = 8;
                    for (size_t i = 0; i < stack_arg_count; i++) {
                        emit_mov_from_stack(b, 0, 8 + i * 8);
                        emit_mov_to_stack(b, 0, i * 8);
                    }
                }
            } else {
                uint32_t remove = (uint32_t)(argc * 8);
                if (remove) {
                    buf_u8(b, 0x48); buf_u8(b, 0x81); buf_u8(b, 0xC4); buf_u32(b, remove);
                }
                g->stack_depth -= remove;
                if ((g->stack_depth % 16) != 0) {
                    buf_u8(b, 0x48); buf_u8(b, 0x83); buf_u8(b, 0xEC); buf_u8(b, 0x08);
                    g->stack_depth += 8;
                    align_pad = 8;
                }
            }
            emit_call_patch(g, name);
            if (align_pad) {
                buf_u8(b, 0x48); buf_u8(b, 0x83); buf_u8(b, 0xC4); buf_u8(b, 0x08);
                g->stack_depth -= 8;
            }
            if (stack_arg_count > 0) {
                uint32_t cleanup = (uint32_t)(stack_arg_count * 8);
                buf_u8(b, 0x48); buf_u8(b, 0x81); buf_u8(b, 0xC4); buf_u32(b, cleanup);
                g->stack_depth -= cleanup;
            }
            if (callee && callee->return_count) {
                WType* rt = resolve_named_type(g, callee->returns[0]);
                if (rt && rt->tag == 8) {
                    buf_u8(b, 0x66); buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0x7E); buf_u8(b, 0xC0);
                }
            }
            break;
        }
        case 12: /* ArrayInit */
            /* In a discarded expression the leaves still have to run: an
             * initializer element may be a call or another side-effecting
             * expression. Local definitions route through
             * emit_local_array_leaves, so this only covers expression use. */
            for (size_t i = 0; i < e->arg_count; i++) {
                compile_expr(g, e->args[i], 0, 1);
            }
            emit_mov_imm64(b, reg, 0);
            break;
        default:
            break;
    }

    if (reg != 0) {
        switch (e->tag) {
            case 3: case 0: case 1: case 2: case 4: case 5: case 11:
                break;
            case 6: case 10: {
                /* Array member/index loads leave the computed address in
                 * rbx. Scalar loads already used rbx as the load base and
                 * wrote the value straight into reg, so only the array case
                 * needs the extra move. */
                WType* t = resolve_named_type(g, resolve_expr_type(g, e));
                if (t && t->tag == 12 && reg != 3) {
                    buf_u8(b, reg >= 8 ? 0x49 : 0x48);
                    buf_u8(b, 0x89);
                    buf_u8(b, (uint8_t)(0xC0 | (3 << 3) | (reg & 7)));
                }
                break;
            }
            case 7:
                /* SectionAccess already moved its loaded value into reg. */
                break;
            case 8: case 9:
                buf_u8(b, reg >= 8 ? 0x49 : 0x48);
                buf_u8(b, 0x89);
                buf_u8(b, (uint8_t)(0xC0 | (reg & 7)));
                break;
            default:
                break;
        }
    }
}

/* ------------------------------------------------------------------ */
/* Conditionals, addresses and statements                              */
/* ------------------------------------------------------------------ */

static int has_return_statement(WStmt** stmts, size_t count) {
    for (size_t i = 0; i < count; i++) {
        WStmt* s = stmts[i];
        if (!s) {
            continue;
        }
        if (s->tag == 6) {
            return 1;
        }
        if (s->tag == 2) {
            /* An if only covers the function when both arms return. */
            if (has_return_statement(s->stmts1, s->stmt1_count) && s->flag &&
                has_return_statement(s->stmts2, s->stmt2_count)) {
                return 1;
            }
        } else if (s->tag == 9) {
            /* A match only covers the function when every case and the
             * default arm return. */
            int all_cases = 1;
            for (size_t j = 0; j < s->case_count; j++) {
                if (!has_return_statement(s->cases[j].body, s->cases[j].body_count)) {
                    all_cases = 0;
                    break;
                }
            }
            if (all_cases && s->flag && has_return_statement(s->stmts2, s->stmt2_count)) {
                return 1;
            }
        }
    }
    return 0;
}

/* cmp rax, imm with the shortest encoding.  Used for the common loop case
 * "i < constant" instead of mov/push/mov/mov/pop/cmp. */
static void emit_cmp_rax_imm(Buf* b, int64_t imm) {
    if (imm >= -128 && imm <= 127) {
        buf_u8(b, 0x48); buf_u8(b, 0x83); buf_u8(b, 0xF8); buf_u8(b, (uint8_t)imm);
    } else {
        buf_u8(b, 0x48); buf_u8(b, 0x81); buf_u8(b, 0xF8); buf_u32(b, (uint32_t)imm);
    }
}

/* cmp rhs,lhs with full 64-bit register extensions. Flags describe lhs-rhs,
 * matching the old cmp %rbx,%rax sequence. */
static void emit_cmp_two_regs(Buf* b, uint8_t lhs, uint8_t rhs) {
    emit_rex(b, (uint8_t)(0x48 | (rhs >= 8 ? 4 : 0) | (lhs >= 8 ? 1 : 0)));
    buf_u8(b, 0x39);
    buf_u8(b, (uint8_t)(0xC0 | ((rhs & 7) << 3) | (lhs & 7)));
}

/* cmp imm,lhs for the int32 range that emit_cmp_rax_imm already accepts.
 * Callers fall back to the rax path for wider constants. */
static void emit_cmp_reg_imm(Buf* b, uint8_t lhs, int64_t imm) {
    emit_rex(b, (uint8_t)(0x48 | (lhs >= 8 ? 1 : 0)));
    if (imm >= -128 && imm <= 127) {
        buf_u8(b, 0x83); buf_u8(b, (uint8_t)(0xF8 | (lhs & 7)));
        buf_u8(b, (uint8_t)imm);
    } else {
        buf_u8(b, 0x81); buf_u8(b, (uint8_t)(0xF8 | (lhs & 7)));
        buf_u32(b, (uint32_t)imm);
    }
}

static int is_float_comparison(Gen* g, WExpr* e) {
    return e && e->tag == 8 &&
           (is_float_value_expr(g, e->a, expr_deref_flag(g, e->a, 1)) ||
            is_float_value_expr(g, e->b, expr_deref_flag(g, e->b, 1)));
}

/* f64 values travel as bit patterns in GPRs. Compare them numerically with
 * ucomisd and materialize a 0/1 boolean in rax. Unordered (NaN) results are
 * false for every relation except "!=", matching IEEE-754 semantics. */
static void emit_float_compare_bool(Gen* g, WExpr* e) {
    Buf* b = &g->code;
    WStr op = e->str;
    uint8_t cc;
    int unordered_is_true = 0;
    int need_not_pf = 0;

    compile_float_operand_bits(g, e->a, 0, 1);
    size_t lhs_slot = g->stack_depth;
    emit_mov_to_stack(b, 0, lhs_slot);
    g->stack_depth += 8;
    compile_float_operand_bits(g, e->b, 0, 1);
    emit_mov_from_stack(b, 1, lhs_slot); /* rcx = lhs bits */
    g->stack_depth -= 8;
    buf_u8(b, 0x66); buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0x6E); buf_u8(b, 0xC1); /* movq xmm0, rcx */
    buf_u8(b, 0x66); buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0x6E); buf_u8(b, 0xC8); /* movq xmm1, rax */
    buf_u8(b, 0x66); buf_u8(b, 0x0F); buf_u8(b, 0x2E); buf_u8(b, 0xC1); /* ucomisd xmm0, xmm1 */

    if (wstr_eq_c(op, "OpEq") || wstr_eq_c(op, "OpEqEq") || wstr_eq_c(op, "==")) {
        cc = 0x94; need_not_pf = 1; /* sete al */
    } else if (wstr_eq_c(op, "OpNotEq") || wstr_eq_c(op, "OpNe") || wstr_eq_c(op, "!=")) {
        cc = 0x95; unordered_is_true = 1; /* setne al */
    } else if (wstr_eq_c(op, "OpLt") || wstr_eq_c(op, "Lt") || wstr_eq_c(op, "<")) {
        cc = 0x92; need_not_pf = 1; /* setb al */
    } else if (wstr_eq_c(op, "OpLtEq") || wstr_eq_c(op, "OpLe") || wstr_eq_c(op, "<=")) {
        cc = 0x96; need_not_pf = 1; /* setbe al */
    } else if (wstr_eq_c(op, "OpGt") || wstr_eq_c(op, "Gt") || wstr_eq_c(op, ">")) {
        cc = 0x97; /* seta al: unordered clears CF */
    } else {
        cc = 0x93; /* setae al: unordered clears CF */
    }

    buf_u8(b, 0x0F); buf_u8(b, cc); buf_u8(b, 0xC0);
    if (need_not_pf) {
        buf_u8(b, 0x0F); buf_u8(b, 0x9B); buf_u8(b, 0xC1); /* setnp cl */
        buf_u8(b, 0x20); buf_u8(b, 0xC8);                   /* and al, cl */
    } else if (unordered_is_true) {
        buf_u8(b, 0x0F); buf_u8(b, 0x9A); buf_u8(b, 0xC1); /* setp cl */
        buf_u8(b, 0x08); buf_u8(b, 0xC8);                   /* or al, cl */
    }
    buf_u8(b, 0x0F); buf_u8(b, 0xB6); buf_u8(b, 0xC0); /* movzx eax, al */
}

/* Return 1 if e is a comparison; *setcc receives the setcc opcode. Shared by
 * the boolean-materialization path and the direct-branch path so the two can
 * never disagree on signedness or polarity. */
static int comparison_setcc(Gen* g, WExpr* e, uint8_t* setcc) {
    if (!e || e->tag != 8) {
        return 0;
    }
    WStr op = e->str;
    static const char* cmps[] = {
        "OpEq", "OpEqEq", "==", "OpNotEq", "OpNe", "!=",
        "OpLt", "Lt", "<", "OpLtEq", "OpLe", "<=",
        "OpGt", "Gt", ">", "OpGtEq", "OpGe", ">="
    };
    int is_cmp = 0;
    for (size_t i = 0; i < sizeof(cmps) / sizeof(cmps[0]); i++) {
        if (wstr_eq_c(op, cmps[i])) { is_cmp = 1; break; }
    }
    if (!is_cmp) {
        return 0;
    }
    int signed_cmp = is_signed_expr(g, e->a) || is_signed_expr(g, e->b);
    if (wstr_eq_c(op, "OpEq") || wstr_eq_c(op, "OpEqEq") || wstr_eq_c(op, "==")) {
        *setcc = 0x94;
    } else if (wstr_eq_c(op, "OpNotEq") || wstr_eq_c(op, "OpNe") || wstr_eq_c(op, "!=")) {
        *setcc = 0x95;
    } else if (wstr_eq_c(op, "OpLt") || wstr_eq_c(op, "Lt") || wstr_eq_c(op, "<")) {
        *setcc = signed_cmp ? 0x9C : 0x92;
    } else if (wstr_eq_c(op, "OpLtEq") || wstr_eq_c(op, "OpLe") || wstr_eq_c(op, "<=")) {
        *setcc = signed_cmp ? 0x9E : 0x96;
    } else if (wstr_eq_c(op, "OpGt") || wstr_eq_c(op, "Gt") || wstr_eq_c(op, ">")) {
        *setcc = signed_cmp ? 0x9F : 0x97;
    } else {
        *setcc = signed_cmp ? 0x9D : 0x93;
    }
    return 1;
}

/* Emit operands and cmp for a comparison expression, leaving flags set. */
static void emit_cmp_operands(Gen* g, WExpr* e) {
    Buf* b = &g->code;
    uint8_t lhs_reg = 0;
    int lhs_bound = e && e->a && e->a->tag == 3 &&
                    reg_local_lookup(g, e->a->str, &lhs_reg) &&
                    !is_float_expr(g, e->a);
    if (lhs_bound && e->b && (e->b->tag == 0 || e->b->tag == 1)) {
        int64_t sv = (int64_t)e->b->num;
        if (sv >= INT32_MIN && sv <= INT32_MAX) {
            emit_cmp_reg_imm(b, lhs_reg, sv);
            return;
        }
    }
    if (lhs_bound && e->b && e->b->tag == 3) {
        uint8_t rhs_reg = 0;
        if (reg_local_lookup(g, e->b->str, &rhs_reg)) {
            emit_cmp_two_regs(b, lhs_reg, rhs_reg);
        } else {
            compile_expr(g, e->b, 3, expr_deref_flag(g, e->b, 1));
            emit_cmp_two_regs(b, lhs_reg, 3);
        }
        return;
    }
    if (e->b && (e->b->tag == 0 || e->b->tag == 1) &&
        !is_float_expr(g, e->a)) {
        int64_t sv = (int64_t)e->b->num;
        compile_expr(g, e->a, 0, expr_deref_flag(g, e->a, 1));
        if (sv >= INT32_MIN && sv <= INT32_MAX) {
            emit_cmp_rax_imm(b, sv);
        } else {
            buf_u8(b, 0x48); buf_u8(b, 0xBB); buf_u64(b, e->b->num);
            buf_u8(b, 0x48); buf_u8(b, 0x39); buf_u8(b, 0xD8);
        }
    } else if (simple_left(g, e->a)) {
        compile_expr(g, e->b, 3, expr_deref_flag(g, e->b, 1));
        compile_expr(g, e->a, 0, expr_deref_flag(g, e->a, 1));
        buf_u8(b, 0x48); buf_u8(b, 0x39); buf_u8(b, 0xD8);
    } else if (!g->r10_busy && !expr_has_call(g, e->a) && !expr_has_call(g, e->b)) {
        /* Keep LHS in r10 instead of push/pop. No calls in either operand,
         * and the RHS must not reuse r10 while the LHS is live. */
        compile_expr(g, e->a, 10, expr_deref_flag(g, e->a, 1));
        g->r10_busy = 1;
        compile_expr(g, e->b, 0, expr_deref_flag(g, e->b, 1));
        g->r10_busy = 0;
        buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xC3); /* mov %rax,%rbx */
        buf_u8(b, 0x4C); buf_u8(b, 0x89); buf_u8(b, 0xD0); /* mov %r10,%rax */
        buf_u8(b, 0x48); buf_u8(b, 0x39); buf_u8(b, 0xD8);
    } else {
        compile_expr(g, e->a, 0, expr_deref_flag(g, e->a, 1));
        buf_u8(b, 0x50);
        compile_expr(g, e->b, 0, expr_deref_flag(g, e->b, 1));
        buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xC3);
        buf_u8(b, 0x58);
        buf_u8(b, 0x48); buf_u8(b, 0x39); buf_u8(b, 0xD8);
    }
}

/* Compile an f64 index expression straight into xmm0. The address stays in
 * rbx, so movsd (%rbx),%xmm0 replaces the old load-to-rax plus movq path.
 * Returns 1 when e is an f64 index; 0 otherwise. */
static int compile_f64_index_to_xmm0(Gen* g, WExpr* e) {
    if (!e || e->tag != 10 || !is_float_expr(g, e)) {
        return 0;
    }
    Buf* b = &g->code;
    uint8_t sib_base = 0, sib_index = 0;
    int sib_scale = -1;
    if (index_sib_ready(g, e, &sib_base, &sib_index, &sib_scale)) {
        emit_movsd_sib_load(b, sib_base, sib_index, sib_scale);
        return 1;
    }
    compile_address(g, e, 3);
    buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, 0x10); buf_u8(b, 0x03);
    return 1;
}

/* Compile e leaving its f64 bit pattern in xmm0. Binary ops with a literal
 * RHS recurse so a chain such as a * 0.5 + 1.0 keeps the value in xmm0
 * between links instead of round-tripping through rax. */
static void compile_float_to_xmm0_s(Gen* g, WExpr* e, uint8_t scratch) {
    Buf* b = &g->code;
    if (compile_float_binary_to_xmm0_s(g, e, scratch)) {
        return;
    }
    if (e && e->tag == 8 && e->b && e->b->tag == 11 &&
        float_op_code(e->str) &&
        (is_float_value_expr(g, e->a, 1) || is_float_value_expr(g, e->b, 1))) {
        compile_float_to_xmm0_s(g, e->a, scratch);
        emit_float_op_xmm0_mem(g, e->str, e->b->str);
        return;
    }
    /* Plain f64 stack slot: movsd straight into xmm0, skipping the rax and
     * movq round trip. Register-bound slots come from a GPR instead. */
    if (e && e->tag == 3 && is_float_expr(g, e)) {
        uint64_t off = 0, access = 3;
        uint8_t bound_reg = 0;
        uint8_t xmm_reg = 0;
        map_get_str(&g->local_access, e->str, &access);
        if (xmm_local_lookup(g, e->str, &xmm_reg)) {
            emit_movapd_xmm_to_xmm0(b, xmm_reg);
            return;
        }
        if (access == 3 && reg_local_lookup(g, e->str, &bound_reg)) {
            emit_movq_gpr_to_xmm0(b, bound_reg);
            return;
        }
        if (access == 3 && map_get_str(&g->locals, e->str, &off)) {
            buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, 0x10);
            buf_u8(b, off <= 127 ? 0x45 : 0x85);
            if (off <= 127) buf_u8(b, (uint8_t)(-(int32_t)off));
            else buf_u32(b, (uint32_t)(-(int32_t)off));
            return;
        }
    }
    /* x op x: load once and apply the op to xmm0 itself. Avoids a second
     * stack load and the scratch copy. Limited to commutative ops. */
    if (e && e->tag == 8 && e->a && e->b && e->a->tag == 3 && e->b->tag == 3 &&
        wstr_eq(e->a->str, e->b->str) && is_float_expr(g, e->a) &&
        (wstr_eq_c(e->str, "OpAdd") || wstr_eq_c(e->str, "OpMul"))) {
        compile_float_to_xmm0_s(g, e->a, scratch);
        buf_u8(b, 0xF2); buf_u8(b, 0x0F);
        buf_u8(b, wstr_eq_c(e->str, "OpAdd") ? 0x58 : 0x59);
        buf_u8(b, 0xC0);
        return;
    }
    /* Literal LHS with a plain f64 stack slot RHS: movsd the literal from
     * the constant pool straight into xmm0, then apply the op with the RHS
     * as a memory operand. Skips movabs, movq, and scratch copies. */
    if (e && e->tag == 8 && e->a && e->a->tag == 11 &&
        e->b && e->b->tag == 3 && float_op_code(e->str) &&
        is_float_expr(g, e->b)) {
        uint64_t off = 0, access = 3;
        uint8_t bound = 0, xmm_reg = 0;
        map_get_str(&g->locals, e->b->str, &off);
        map_get_str(&g->local_access, e->b->str, &access);
        if (access == 3 && !reg_local_lookup(g, e->b->str, &bound) &&
            !xmm_local_lookup(g, e->b->str, &xmm_reg)) {
            emit_load_float_lit_xmm0(g, e->a->str);
            uint8_t code = (uint8_t)float_op_code(e->str);
            buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, code);
            buf_u8(b, off <= 127 ? 0x45 : 0x85);
            if (off <= 127) buf_u8(b, (uint8_t)(-(int32_t)off));
            else buf_u32(b, (uint32_t)(-(int32_t)off));
            return;
        }
    }
    /* Both operands are plain f64 stack slots: load the LHS into xmm0 and
     * apply the op with the RHS as a memory operand. Avoids the scratch copy
     * and the GPR round trip used by the general path. */
    if (e && e->tag == 8 && e->a && e->a->tag == 3 &&
        e->b && e->b->tag == 3 && float_op_code(e->str) &&
        is_float_expr(g, e->a) && is_float_expr(g, e->b)) {
        uint64_t offa = 0, offb = 0, acca = 3, accb = 3;
        uint8_t bounda = 0, boundb = 0, xa = 0, xb = 0;
        map_get_str(&g->locals, e->a->str, &offa);
        map_get_str(&g->locals, e->b->str, &offb);
        map_get_str(&g->local_access, e->a->str, &acca);
        map_get_str(&g->local_access, e->b->str, &accb);
        if (acca == 3 && accb == 3 &&
            !reg_local_lookup(g, e->a->str, &bounda) &&
            !reg_local_lookup(g, e->b->str, &boundb) &&
            !xmm_local_lookup(g, e->a->str, &xa) &&
            !xmm_local_lookup(g, e->b->str, &xb)) {
            if (offa <= 127) {
                buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, 0x10);
                buf_u8(b, 0x45); buf_u8(b, (uint8_t)(-(int32_t)offa));
            } else {
                buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, 0x10);
                buf_u8(b, 0x85); buf_u32(b, (uint32_t)(-(int32_t)offa));
            }
            uint8_t code = (uint8_t)float_op_code(e->str);
            buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, code);
            buf_u8(b, offb <= 127 ? 0x45 : 0x85);
            if (offb <= 127) buf_u8(b, (uint8_t)(-(int32_t)offb));
            else buf_u32(b, (uint32_t)(-(int32_t)offb));
            return;
        }
    }
    /* General f64 binary op: keep both operands in XMM registers. The
     * scratch copy holds the LHS while the RHS is compiled recursively;
     * each nesting level uses a higher register so nested ops are safe. */
    if (e && e->tag == 8 && e->a && e->b && float_op_code(e->str) &&
        (is_float_expr(g, e->a) || is_float_expr(g, e->b)) &&
        !expr_has_call(g, e->b) && scratch < 7) {
        int code = float_op_code(e->str);
        int lhs_in_scratch = 0;
        if (e->a->tag == 3 && is_float_expr(g, e->a)) {
            uint64_t off = 0, access = 3;
            uint8_t bound = 0, xmm_reg = 0;
            map_get_str(&g->locals, e->a->str, &off);
            map_get_str(&g->local_access, e->a->str, &access);
            if (access == 3 && !reg_local_lookup(g, e->a->str, &bound) &&
                !xmm_local_lookup(g, e->a->str, &xmm_reg)) {
                /* movsd stack slot straight into the scratch register,
                 * skipping the xmm0 load and the movapd copy. */
                buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, 0x10);
                buf_u8(b, (uint8_t)((off <= 127 ? 0x45 : 0x85) |
                                    ((scratch & 7) << 3)));
                if (off <= 127) buf_u8(b, (uint8_t)(-(int32_t)off));
                else buf_u32(b, (uint32_t)(-(int32_t)off));
                lhs_in_scratch = 1;
            }
        }
        if (!lhs_in_scratch) {
            compile_float_to_xmm0_s(g, e->a, (uint8_t)(scratch + 1));
            emit_movapd_xmm0_to(b, scratch);
        }
        compile_float_to_xmm0_s(g, e->b, (uint8_t)(scratch + 1));
        if (code == 0x5C || code == 0x5E) {
            buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, (uint8_t)code);
            buf_u8(b, (uint8_t)(0xC0 | ((scratch & 7) << 3)));
            emit_movapd_to_xmm0(b, scratch);
        } else {
            buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, (uint8_t)code);
            buf_u8(b, (uint8_t)(0xC0 | (scratch & 7)));
        }
        return;
    }
    if (e && e->tag == 8 && wstr_eq_c(e->str, "OpCastF64") && e->a &&
        !is_float_expr(g, e->a)) {
        /* Integer to f64: convert straight into xmm0, skipping the movq
         * round trip that the generic rax-based cast emits. A full-width
         * unsigned source can have its high bit set, which cvtsi2sd would
         * read as negative; use the halving path for that case. */
        compile_expr(g, e->a, 0, float_value_deref_flag(g, e->a, 1));
        if (is_signed_expr(g, e->a) || expr_size(g, e->a) < 8) {
            buf_u8(b, 0xF2); buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0x2A); buf_u8(b, 0xC0);
        } else {
            emit_int_bits_to_f64_bits(g, 0, 0);
            emit_movq_rax_to_xmm0(b);
        }
        return;
    }
    if (compile_f64_index_to_xmm0(g, e)) {
        return;
    }
    /* Integer operands of f64 arithmetic must become IEEE-754 values, not
     * raw bit patterns. This covers mixed expressions such as `3.0 * 2`. */
    if (e && !is_float_value_expr(g, e, 1)) {
        compile_expr(g, e, 0, float_value_deref_flag(g, e, 1));
        emit_int_bits_to_f64_bits(g, 0, is_signed_expr(g, e));
        emit_movq_rax_to_xmm0(b);
        return;
    }
    compile_expr(g, e, 0, float_value_deref_flag(g, e, 1));
    emit_movq_rax_to_xmm0(b);
}

static void compile_float_to_xmm0(Gen* g, WExpr* e) {
    compile_float_to_xmm0_s(g, e, 3);
}

/* True when compiling e into rax cannot clobber rbx. This holds for plain
 * stack slots, literals, binary ops with an immediate RHS, and shifts by a
 * literal count. It does not hold for generic binary ops (which use rbx as
 * scratch) or for calls. Used to skip a push/pop of the index base. */
static int rax_only_expr(Gen* g, WExpr* e) {
    if (!e) {
        return 1;
    }
    if (e->tag == 0 || e->tag == 1 || e->tag == 11) {
        return 1;
    }
    if (e->tag == 3) {
        uint64_t access = 3;
        map_get_str(&g->local_access, e->str, &access);
        return access == 3;
    }
    if (e->tag == 8 && e->b && e->b->tag == 11 && has_binary_imm(e->str)) {
        return rax_only_expr(g, e->a);
    }
    if (e->tag == 8 && e->b && (e->b->tag == 0 || e->b->tag == 1) &&
        (wstr_eq_c(e->str, "OpShl") || wstr_eq_c(e->str, "OpShr")) &&
        e->b->num >= 1 && e->b->num <= 63) {
        return rax_only_expr(g, e->a);
    }
    if (e->tag == 8 && e->b && e->b->tag == 3 && has_binary_imm(e->str)) {
        uint64_t access = 3;
        map_get_str(&g->local_access, e->b->str, &access);
        if (access == 3 && rax_only_expr(g, e->a)) {
            return 1; /* memory-operand path: only rax is touched */
        }
    }
    return 0;
}

/* True when e contains a function call, which would clobber caller-saved r10. */
static int expr_has_call(Gen* g, WExpr* e) {
    if (!e) {
        return 0;
    }
    if (e->tag == 9) {
        return 1;
    }
    return expr_has_call(g, e->a) || expr_has_call(g, e->b);
}

/* Deeper call scan that also walks call arguments. r11 is caller-saved, so
 * any call in a loop body would destroy a register-bound induction variable. */
static int expr_has_call_deep(WExpr* e) {
    if (!e) {
        return 0;
    }
    if (e->tag == 9) {
        return 1;
    }
    if (expr_has_call_deep(e->a) || expr_has_call_deep(e->b)) {
        return 1;
    }
    for (size_t i = 0; i < e->arg_count; i++) {
        if (expr_has_call_deep(e->args[i])) return 1;
    }
    return 0;
}

static int expr_mentions_name(WExpr* e, WStr name, int addr_only);

static int expr_mentions_name_args(WExpr* e, WStr name, int addr_only) {
    if (!e) {
        return 0;
    }
    for (size_t i = 0; i < e->arg_count; i++) {
        if (expr_mentions_name(e->args[i], name, addr_only)) return 1;
    }
    return 0;
}

/* True when e reads or writes the named local, or takes its address. */
static int expr_mentions_name(WExpr* e, WStr name, int addr_only) {
    if (!e) {
        return 0;
    }
    if ((e->tag == 3 || e->tag == 4) && wstr_eq(e->str, name)) {
        if (!addr_only || e->tag == 4) {
            return 1;
        }
    }
    if (e->tag == 5 && e->a && e->a->tag == 3 && wstr_eq(e->a->str, name)) {
        return 1;
    }
    return expr_mentions_name(e->a, name, addr_only) ||
           expr_mentions_name(e->b, name, addr_only) ||
           expr_mentions_name_args(e, name, addr_only);
}

/* Record locals whose address is taken anywhere in the current function. A
 * pointer to such a local can alias its stack slot, so the slot and a bound
 * register would diverge. */
static void collect_escaped_expr(Gen* g, WExpr* e) {
    if (!e) {
        return;
    }
    if (e->tag == 4 || e->tag == 5) { /* AddrOf, parenthesized AddrOf */
        if (e->str.ptr) {
            map_put_str(&g->escaped_locals, e->str, 1);
        }
        if (e->a && e->a->tag == 3) {
            map_put_str(&g->escaped_locals, e->a->str, 1);
        }
    }
    collect_escaped_expr(g, e->a);
    collect_escaped_expr(g, e->b);
    for (size_t i = 0; i < e->arg_count; i++) {
        collect_escaped_expr(g, e->args[i]);
    }
}

static void collect_escaped_stmt_tree(Gen* g, WStmt** v, size_t n);

static void collect_escaped_stmt(Gen* g, WStmt* s) {
    if (!s) {
        return;
    }
    collect_escaped_expr(g, s->e1);
    collect_escaped_expr(g, s->e2);
    for (size_t i = 0; i < s->expr_count; i++) {
        collect_escaped_expr(g, s->exprs[i]);
    }
    for (size_t i = 0; i < s->return_count; i++) {
        collect_escaped_expr(g, s->returns[i].expr);
    }
    for (size_t i = 0; i < s->case_count; i++) {
        collect_escaped_expr(g, s->cases[i].expr);
        for (size_t j = 0; j < s->cases[i].body_count; j++) {
            collect_escaped_stmt(g, s->cases[i].body[j]);
        }
    }
    collect_escaped_stmt(g, s->s1);
    collect_escaped_stmt(g, s->s2);
    collect_escaped_stmt_tree(g, s->stmts1, s->stmt1_count);
    collect_escaped_stmt_tree(g, s->stmts2, s->stmt2_count);
}

static void collect_escaped_stmt_tree(Gen* g, WStmt** v, size_t n) {
    for (size_t i = 0; i < n; i++) {
        collect_escaped_stmt(g, v[i]);
    }
}

/* A loop body cannot keep its induction variable in a caller-saved register
 * if it might call, return, redefine the variable, take its address, or start
 * a jmpto. Returns bypass the exit spill block, and callees may clobber r11. */
static int stmt_forbidden_reg_loop(Gen* g, WStmt* s, WStr name) {
    if (!s) {
        return 0;
    }
    if (s->tag == 5 || s->tag == 6 || s->tag == 8) {
        return 1;
    }
    if (s->tag == 0 && wstr_eq(s->name, name)) {
        return 1;
    }
    if (s->tag == 1 && s->expr_count > 0) {
        WExpr* target = s->exprs[0];
        /* A direct assignment redefines the induction variable. An indexed
         * or member target that merely reads it as an index is fine. */
        if (target && target->tag == 3 && wstr_eq(target->str, name)) {
            return 1;
        }
        if (expr_mentions_name(target, name, 1)) {
            return 1;
        }
    }
    WExpr* exprs[2] = {s->e1, s->e2};
    for (size_t i = 0; i < 2; i++) {
        if (expr_has_call_deep(exprs[i]) ||
            expr_mentions_name(exprs[i], name, 1)) {
            return 1;
        }
    }
    for (size_t i = 0; i < s->expr_count; i++) {
        if (expr_has_call_deep(s->exprs[i]) ||
            expr_mentions_name(s->exprs[i], name, 1)) {
            return 1;
        }
    }
    if (stmt_forbidden_reg_loop(g, s->s1, name) ||
        stmt_forbidden_reg_loop(g, s->s2, name)) {
        return 1;
    }
    for (size_t i = 0; i < s->stmt1_count; i++) {
        if (stmt_forbidden_reg_loop(g, s->stmts1[i], name)) return 1;
    }
    for (size_t i = 0; i < s->stmt2_count; i++) {
        if (stmt_forbidden_reg_loop(g, s->stmts2[i], name)) return 1;
    }
    for (size_t i = 0; i < s->case_count; i++) {
        if (expr_has_call_deep(s->cases[i].expr) ||
            expr_mentions_name(s->cases[i].expr, name, 1)) {
            return 1;
        }
        for (size_t j = 0; j < s->cases[i].body_count; j++) {
            if (stmt_forbidden_reg_loop(g, s->cases[i].body[j], name)) return 1;
        }
    }
    return 0;
}

/* True when e contains a call or takes the address of any local. Used to
 * guard XMM register binding for f64 loop locals. */
static int expr_unsafe_for_xmm(WExpr* e);

static int expr_unsafe_for_xmm_args(WExpr* e) {
    if (!e) return 0;
    for (size_t i = 0; i < e->arg_count; i++) {
        if (expr_unsafe_for_xmm(e->args[i])) return 1;
    }
    return 0;
}

static int expr_unsafe_for_xmm(WExpr* e) {
    if (!e) return 0;
    if (e->tag == 9 || e->tag == 4 || e->tag == 5) return 1;
    return expr_unsafe_for_xmm(e->a) || expr_unsafe_for_xmm(e->b) ||
           expr_unsafe_for_xmm_args(e);
}

static int stmt_unsafe_for_xmm(WStmt* s);

static int stmt_unsafe_for_xmm_tree(WStmt** v, size_t n) {
    for (size_t i = 0; i < n; i++) {
        if (stmt_unsafe_for_xmm(v[i])) return 1;
    }
    return 0;
}

/* Scan a statement tree for anything that would make XMM binding unsafe. */
static int stmt_unsafe_for_xmm(WStmt* s) {
    if (!s) return 0;
    if (s->tag == 3 || s->tag == 4 || s->tag == 5 || s->tag == 6 ||
        s->tag == 8 || s->tag == 11 || s->tag == 12) return 1;
    if (expr_unsafe_for_xmm(s->e1) || expr_unsafe_for_xmm(s->e2)) return 1;
    for (size_t i = 0; i < s->expr_count; i++) {
        if (expr_unsafe_for_xmm(s->exprs[i])) return 1;
    }
    for (size_t i = 0; i < s->case_count; i++) {
        if (expr_unsafe_for_xmm(s->cases[i].expr)) return 1;
        for (size_t j = 0; j < s->cases[i].body_count; j++) {
            if (stmt_unsafe_for_xmm(s->cases[i].body[j])) return 1;
        }
    }
    return stmt_unsafe_for_xmm(s->s1) || stmt_unsafe_for_xmm(s->s2) ||
           stmt_unsafe_for_xmm_tree(s->stmts1, s->stmt1_count) ||
           stmt_unsafe_for_xmm_tree(s->stmts2, s->stmt2_count);
}

/* Collect f64 VarDefinitions in a loop body for XMM register binding. */
static void collect_xmm_loop_locals(Gen* g, WStmt* s, XmmLocalVec* out,
                                    uint8_t* next_reg) {
    if (!s || *next_reg > 15) return;
    if (s->tag == 0 && s->type && is_float_type_named(g,s->type) &&
        type_size(g, s->type) == 8 && s->access == 3 && s->name.ptr &&
        !map_has(&g->escaped_locals, s->name.ptr, s->name.len) && s->e1) {
        uint64_t existing = 0;
        if (map_get_str(&g->locals, s->name, &existing)) return;
        XmmLocal x = {0};
        x.name = s->name;
        x.reg = (*next_reg)++;
        XmmLocalVec_push(out, x);
    }
    collect_xmm_loop_locals(g, s->s1, out, next_reg);
    collect_xmm_loop_locals(g, s->s2, out, next_reg);
    for (size_t i = 0; i < s->stmt1_count; i++)
        collect_xmm_loop_locals(g, s->stmts1[i], out, next_reg);
    for (size_t i = 0; i < s->stmt2_count; i++)
        collect_xmm_loop_locals(g, s->stmts2[i], out, next_reg);
}

/* Write XMM-bound locals back to their stack slots. */
static void spill_xmm_loop_locals(Gen* g, XmmLocalVec* xs) {
    Buf* b = &g->code;
    for (size_t i = 0; i < xs->n; i++) {
        uint64_t off = 0;
        if (map_get_str(&g->locals, xs->v[i].name, &off)) {
            emit_movsd_xmm_to_local(b, xs->v[i].reg, (uint32_t)off);
        }
    }
}

/* r8 and r9 are unused by generated scalar code. They are caller-saved, so
 * GPR loop binding is only enabled for loops without calls, jmpto, returns,
 * or addresses of locals. */
static int reg_in_use(Gen* g, uint8_t reg) {
    for (size_t i = 0; i < g->reg_locals.n; i++) {
        if (g->reg_locals.v[i].reg == reg) return 1;
    }
    return 0;
}

static int expr_unsafe_for_gpr(WExpr* e) {
    if (!e) return 0;
    if (e->tag == 4 || e->tag == 5 || e->tag == 9) return 1;
    if (expr_unsafe_for_gpr(e->a) || expr_unsafe_for_gpr(e->b)) return 1;
    for (size_t i = 0; i < e->arg_count; i++) {
        if (expr_unsafe_for_gpr(e->args[i])) return 1;
    }
    return 0;
}

static int stmt_has_call_deep(WStmt* s);

static int stmt_has_call_deep_tree(WStmt** v, size_t n) {
    for (size_t i = 0; i < n; i++) {
        if (stmt_has_call_deep(v[i])) return 1;
    }
    return 0;
}

static int stmt_has_call_deep(WStmt* s) {
    if (!s) return 0;
    if (expr_has_call_deep(s->e1) || expr_has_call_deep(s->e2)) return 1;
    for (size_t i = 0; i < s->expr_count; i++) {
        if (expr_has_call_deep(s->exprs[i])) return 1;
    }
    for (size_t i = 0; i < s->case_count; i++) {
        if (expr_has_call_deep(s->cases[i].expr)) return 1;
        for (size_t j = 0; j < s->cases[i].body_count; j++) {
            if (stmt_has_call_deep(s->cases[i].body[j])) return 1;
        }
    }
    return stmt_has_call_deep(s->s1) || stmt_has_call_deep(s->s2) ||
           stmt_has_call_deep_tree(s->stmts1, s->stmt1_count) ||
           stmt_has_call_deep_tree(s->stmts2, s->stmt2_count);
}

static int stmt_unsafe_for_gpr(WStmt* s);

static int stmt_unsafe_for_gpr_tree(WStmt** v, size_t n) {
    for (size_t i = 0; i < n; i++) {
        if (stmt_unsafe_for_gpr(v[i])) return 1;
    }
    return 0;
}

static int stmt_unsafe_for_gpr(WStmt* s) {
    if (!s) return 0;
    if (s->tag == 5 || s->tag == 6 || s->tag == 7 || s->tag == 8) return 1;
    if (expr_unsafe_for_gpr(s->e1) || expr_unsafe_for_gpr(s->e2)) return 1;
    for (size_t i = 0; i < s->expr_count; i++) {
        if (expr_unsafe_for_gpr(s->exprs[i])) return 1;
    }
    for (size_t i = 0; i < s->case_count; i++) {
        if (expr_unsafe_for_gpr(s->cases[i].expr)) return 1;
        for (size_t j = 0; j < s->cases[i].body_count; j++) {
            if (stmt_unsafe_for_gpr(s->cases[i].body[j])) return 1;
        }
    }
    return stmt_unsafe_for_gpr(s->s1) || stmt_unsafe_for_gpr(s->s2) ||
           stmt_unsafe_for_gpr_tree(s->stmts1, s->stmt1_count) ||
           stmt_unsafe_for_gpr_tree(s->stmts2, s->stmt2_count);
}

static int gpr_bindable_local(Gen* g, WStr name) {
    uint64_t off = 0;
    uint64_t access = 3;
    uint64_t is_param = 0;
    uint64_t type_ptr = 0;
    uint8_t dummy = 0;
    if (!map_get_str(&g->locals, name, &off)) return 0;
    map_get_str(&g->local_access, name, &access);
    map_get_str(&g->local_is_param, name, &is_param);
    if (access != 3 || is_param) return 0;
    if (map_has(&g->escaped_locals, name.ptr, name.len)) return 0;
    if (reg_local_lookup(g, name, &dummy) || xmm_local_lookup(g, name, &dummy)) {
        return 0;
    }
    WType* t = map_get_str(&g->local_types, name, &type_ptr)
                   ? AS_PTR(WType, type_ptr) : NULL;
    WType* scalar = peel_typedefs(t);
    if (!scalar || scalar->tag == 8 || scalar->tag == 10 || scalar->tag == 12 ||
        type_size(g, t) != 8) {
        return 0;
    }
    return 1;
}

/* Collect scalar locals assigned in a loop body for r8/r9 binding. */
static void collect_gpr_loop_locals(Gen* g, WStmt* s, RegLocalVec* out,
                                    uint8_t* next_reg) {
    if (!s || *next_reg > 9) return;
    if (s->tag == 1 && s->expr_count > 0 && s->exprs[0] &&
        s->exprs[0]->tag == 3 && gpr_bindable_local(g, s->exprs[0]->str)) {
        WStr name = s->exprs[0]->str;
        int already = 0;
        for (size_t i = 0; i < out->n; i++) {
            if (wstr_eq(out->v[i].name, name)) already = 1;
        }
        if (!already) {
            while (*next_reg <= 9 && reg_in_use(g, *next_reg)) (*next_reg)++;
            if (*next_reg <= 9) {
                RegLocal r = {0};
                r.name = name;
                r.reg = (*next_reg)++;
                RegLocalVec_push(out, r);
            }
        }
    }
    /* A nested loop owns its init and post expressions. Scanning them here
     * would bind its induction variable at the outer loop level. */
    if (s->tag == 3 || s->tag == 4) {
        for (size_t i = 0; i < s->stmt1_count; i++)
            collect_gpr_loop_locals(g, s->stmts1[i], out, next_reg);
        return;
    }
    collect_gpr_loop_locals(g, s->s1, out, next_reg);
    collect_gpr_loop_locals(g, s->s2, out, next_reg);
    for (size_t i = 0; i < s->stmt1_count; i++)
        collect_gpr_loop_locals(g, s->stmts1[i], out, next_reg);
    for (size_t i = 0; i < s->stmt2_count; i++)
        collect_gpr_loop_locals(g, s->stmts2[i], out, next_reg);
    for (size_t i = 0; i < s->case_count; i++) {
        for (size_t j = 0; j < s->cases[i].body_count; j++) {
            collect_gpr_loop_locals(g, s->cases[i].body[j], out, next_reg);
        }
    }
}

/* True when name is a non-param pointer local that can safely live in r8 for
 * one loop. Escaped locals are excluded because a pointer to the slot can
 * desynchronize the register from the stack value. */
static int invariant_pointer_bindable(Gen* g, WStr name) {
    uint64_t off = 0, access = 3, is_param = 0, type_ptr = 0;
    uint8_t bound = 0;
    if (!name.ptr || !map_get_str(&g->locals, name, &off)) return 0;
    map_get_str(&g->local_access, name, &access);
    map_get_str(&g->local_is_param, name, &is_param);
    if (access != 3 || is_param ||
        map_has(&g->escaped_locals, name.ptr, name.len) ||
        reg_local_lookup(g, name, &bound) || xmm_local_lookup(g, name, &bound)) {
        return 0;
    }
    WType* t = map_get_str(&g->local_types, name, &type_ptr)
                   ? AS_PTR(WType, type_ptr)
                   : NULL;
    t = peel_typedefs(t);
    return t && t->tag == 10 && type_size(g, t) == 8;
}

static int expr_find_invariant_pointer(Gen* g, WExpr* e, WStr* found,
                                       WStr exclude);

static int expr_find_invariant_pointer_args(Gen* g, WExpr* e, WStr* found,
                                            WStr exclude) {
    if (!e) return 0;
    for (size_t i = 0; i < e->arg_count; i++) {
        if (expr_find_invariant_pointer(g, e->args[i], found, exclude)) return 1;
    }
    return 0;
}

/* Find a pointer local used as the base of an index or arrow access. */
static int expr_find_invariant_pointer(Gen* g, WExpr* e, WStr* found,
                                       WStr exclude) {
    if (!e) return 0;
    if ((e->tag == 10 || (e->tag == 6 && e->flag)) && e->a &&
        e->a->tag == 3 && invariant_pointer_bindable(g, e->a->str) &&
        (!exclude.ptr || !wstr_eq(e->a->str, exclude))) {
        *found = e->a->str;
        return 1;
    }
    return expr_find_invariant_pointer(g, e->a, found, exclude) ||
           expr_find_invariant_pointer(g, e->b, found, exclude) ||
           expr_find_invariant_pointer_args(g, e, found, exclude);
}

static int stmt_find_invariant_pointer(Gen* g, WStmt* s, WStr* found,
                                       WStr exclude);

static int stmt_find_invariant_pointer_tree(Gen* g, WStmt** v, size_t n,
                                            WStr* found, WStr exclude) {
    for (size_t i = 0; i < n; i++) {
        if (stmt_find_invariant_pointer(g, v[i], found, exclude)) return 1;
    }
    return 0;
}

static int stmt_find_invariant_pointer(Gen* g, WStmt* s, WStr* found,
                                       WStr exclude) {
    if (!s) return 0;
    if (expr_find_invariant_pointer(g, s->e1, found, exclude) ||
        expr_find_invariant_pointer(g, s->e2, found, exclude)) {
        return 1;
    }
    for (size_t i = 0; i < s->expr_count; i++) {
        if (expr_find_invariant_pointer(g, s->exprs[i], found, exclude)) return 1;
    }
    for (size_t i = 0; i < s->return_count; i++) {
        if (expr_find_invariant_pointer(g, s->returns[i].expr, found, exclude)) return 1;
    }
    for (size_t i = 0; i < s->case_count; i++) {
        if (expr_find_invariant_pointer(g, s->cases[i].expr, found, exclude)) return 1;
        for (size_t j = 0; j < s->cases[i].body_count; j++) {
            if (stmt_find_invariant_pointer(g, s->cases[i].body[j], found, exclude)) return 1;
        }
    }
    return stmt_find_invariant_pointer(g, s->s1, found, exclude) ||
           stmt_find_invariant_pointer(g, s->s2, found, exclude) ||
           stmt_find_invariant_pointer_tree(g, s->stmts1, s->stmt1_count, found, exclude) ||
           stmt_find_invariant_pointer_tree(g, s->stmts2, s->stmt2_count, found, exclude);
}

static int stmt_has_nested_loop_tree(WStmt** v, size_t n);

/* Bind a loop-invariant pointer base to reg (8 or 12). exclude skips a
 * pointer already bound to another register. Nested loops reserve r8 for
 * their own bindings, and calls or returns are handled by the existing loop
 * safety scans at the call sites. */
static int try_bind_invariant_pointer(Gen* g, WStmt** body, size_t body_count,
                                      uint8_t reg, WStr exclude, WStr* name) {
    if (!body_count || reg_in_use(g, reg)) return 0;
    if (stmt_has_nested_loop_tree(body, body_count)) return 0;
    WStr found = {0};
    if (!stmt_find_invariant_pointer_tree(g, body, body_count, &found, exclude)) return 0;
    if (!invariant_pointer_bindable(g, found)) return 0;
    for (size_t i = 0; i < body_count; i++) {
        if (stmt_forbidden_reg_loop(g, body[i], found)) return 0;
    }
    *name = found;
    return 1;
}

/* Write GPR-bound locals back to their stack slots. */
static void spill_gpr_loop_locals(Gen* g, RegLocalVec* rs) {
    Buf* b = &g->code;
    for (size_t i = 0; i < rs->n; i++) {
        uint64_t off = 0;
        if (map_get_str(&g->locals, rs->v[i].name, &off)) {
            emit_mov_reg_to_local(b, rs->v[i].reg, (uint32_t)off);
        }
    }
}

/* True when e can be evaluated while an induction variable lives in a GPR.
 * Calls and address-taking would clobber or alias the bound register, and
 * float expressions use a different register class. */
static int expr_safe_in_reg_loop(Gen* g, WExpr* e) {
    if (!e || e->tag == 4 || e->tag == 5 || e->tag == 9) {
        return 0;
    }
    if (expr_has_call_deep(e) || is_float_expr(g, e)) {
        return 0;
    }
    return 1;
}

static int stmt_has_nested_loop(WStmt* s);

static int stmt_has_nested_loop_tree(WStmt** v, size_t n) {
    for (size_t i = 0; i < n; i++) {
        if (stmt_has_nested_loop(v[i])) return 1;
    }
    return 0;
}

/* True when s contains another loop. Outer loops use only r11 so nested
 * loops can keep r8 and r9 available for their own induction variables and
 * accumulators. */
static int stmt_has_nested_loop(WStmt* s) {
    if (!s) return 0;
    if (s->tag == 3 || s->tag == 4) return 1;
    return stmt_has_nested_loop(s->s1) || stmt_has_nested_loop(s->s2) ||
           stmt_has_nested_loop_tree(s->stmts1, s->stmt1_count) ||
           stmt_has_nested_loop_tree(s->stmts2, s->stmt2_count);
}

/* Recognise a for loop whose induction variable can stay in a GPR:
 *   var = init; var < bound; var = var + step
 * The init must not read var. The bound and step may read it because the
 * generated code evaluates them after the binding is active. */
static int try_bind_for_reg(Gen* g, WStmt* s, uint8_t* reg, WStr* name,
                            WExpr** init_expr_out) {
    if (!s || !s->s1 || !s->e1 || !s->s2) {
        return 0;
    }
    WExpr* init_expr = NULL;
    WStr var = {0};
    if (s->s1->tag == 0) {
        var = s->s1->name;
        init_expr = s->s1->e1;
    } else if (s->s1->tag == 1 && s->s1->expr_count == 1 &&
               s->s1->exprs[0] && s->s1->exprs[0]->tag == 3) {
        var = s->s1->exprs[0]->str;
        init_expr = s->s1->e1;
    } else {
        return 0;
    }
    if (!var.ptr || !init_expr || !expr_safe_in_reg_loop(g, init_expr) ||
        expr_mentions_name(init_expr, var, 0)) {
        return 0;
    }
    uint64_t off = 0, access = 3, type_ptr = 0, is_param = 0;
    uint8_t bound_reg = 0;
    if (!map_get_str(&g->locals, var, &off) ||
        reg_local_lookup(g, var, &bound_reg)) {
        return 0;
    }
    map_get_str(&g->local_access, var, &access);
    map_get_str(&g->local_is_param, var, &is_param);
    WType* t = map_get_str(&g->local_types, var, &type_ptr)
                   ? AS_PTR(WType, type_ptr) : NULL;
    if (access != 3 || is_param ||
        map_has(&g->escaped_locals, var.ptr, var.len) ||
        !t || t->tag == 8 || t->tag == 10 || t->tag == 12 ||
        type_size(g, t) != 8) {
        return 0;
    }
    if (s->e1->tag != 8 ||
        (!wstr_eq_c(s->e1->str, "OpLt") &&
         !wstr_eq_c(s->e1->str, "OpLtEq")) ||
        !expr_safe_in_reg_loop(g, s->e1->a) ||
        !expr_safe_in_reg_loop(g, s->e1->b)) {
        return 0;
    }
    if (!s->s2 || s->s2->tag != 1 || s->s2->expr_count == 0 ||
        !s->s2->exprs[0] || s->s2->exprs[0]->tag != 3 ||
        !wstr_eq(s->s2->exprs[0]->str, var) ||
        !s->s2->e1 || s->s2->e1->tag != 8) {
        return 0;
    }
    WExpr* assign = s->s2->e1;
    int is_add = wstr_eq_c(assign->str, "OpAdd");
    int is_sub = wstr_eq_c(assign->str, "OpSub");
    if (!is_add && !is_sub) {
        return 0;
    }
    int lhs_is_var = assign->a && assign->a->tag == 3 &&
                     wstr_eq(assign->a->str, var);
    int rhs_is_var = assign->b && assign->b->tag == 3 &&
                     wstr_eq(assign->b->str, var);
    WExpr* step = NULL;
    if (is_add) {
        if (!lhs_is_var && !rhs_is_var) {
            return 0;
        }
        step = lhs_is_var ? assign->b : assign->a;
    } else {
        if (!lhs_is_var) {
            return 0;
        }
        step = assign->b;
    }
    if (!expr_safe_in_reg_loop(g, step)) {
        return 0;
    }
    for (size_t i = 0; i < s->stmt1_count; i++) {
        if (stmt_forbidden_reg_loop(g, s->stmts1[i], var)) return 0;
    }
    static const uint8_t inner_candidates[3] = {11, 9, 8};
    static const uint8_t outer_candidates[1] = {11};
    int nested = stmt_has_nested_loop_tree(s->stmts1, s->stmt1_count);
    const uint8_t* candidates = nested ? outer_candidates : inner_candidates;
    size_t candidate_count = nested ? 1 : 3;
    uint8_t chosen = 0;
    for (size_t i = 0; i < candidate_count; i++) {
        if (!reg_in_use(g, candidates[i])) {
            chosen = candidates[i];
            break;
        }
    }
    if (!chosen) {
        return 0;
    }
    *reg = chosen;
    *name = var;
    *init_expr_out = init_expr;
    return 1;
}


/* Recognise x = x + y or x = x - y where x and y are plain 64-bit stack
 * slots and y is not x. The caller loads y once and applies an addq/subq
 * with the destination still in memory, avoiding a load of x and a store. */
static int is_local_self_addsub_var(Gen* g, WExpr* target, WExpr* e,
                                    WStr* rhs_name, int* is_sub) {
    if (!target || target->tag != 3 || !e || e->tag != 8) {
        return 0;
    }
    int is_add = wstr_eq_c(e->str, "OpAdd");
    *is_sub = wstr_eq_c(e->str, "OpSub");
    if (!is_add && !*is_sub) {
        return 0;
    }
    if (!e->a || e->a->tag != 3 || !wstr_eq(e->a->str, target->str) ||
        !e->b || e->b->tag != 3 || wstr_eq(e->b->str, target->str)) {
        return 0;
    }
    *rhs_name = e->b->str;
    uint64_t access = 3, off = 0;
    uint8_t dummy = 0;
    map_get_str(&g->local_access, target->str, &access);
    if (access != 3 || !map_get_str(&g->locals, target->str, &off) ||
        expr_size(g, target) != 8 || is_float_expr(g, target) ||
        reg_local_lookup(g, target->str, &dummy)) {
        return 0;
    }
    access = 3;
    map_get_str(&g->local_access, e->b->str, &access);
    if (access != 3 || !map_get_str(&g->locals, e->b->str, &off) ||
        expr_size(g, e->b) != 8 || is_float_expr(g, e->b)) {
        return 0;
    }
    return 1;
}

static int simple_left(Gen* g, WExpr* e) {
    if (!e) {
        return 0;
    }
    if (e->tag == 0 || e->tag == 1 || e->tag == 11) {
        return 1; /* integer/float literal: never clobbers rbx */
    }
    if (e->tag == 3) {
        uint64_t access = 3;
        map_get_str(&g->local_access, e->str, &access);
        return access == 3; /* plain stack slot; pointer locals load via rbx */
    }
    return 0;
}

/* Emit a conditional jump over the body when cond is false.  Returns the
 * offset of the 4-byte displacement, which the caller patches.  A plain
 * comparison branches straight off cmp flags: no setcc/movzbq/test. */
static size_t compile_condition_jump(Gen* g, WExpr* cond) {
    Buf* b = &g->code;
    uint8_t setcc;
    uint8_t jump_op = 0x84; /* jz */
    if (is_float_comparison(g, cond)) {
        /* Numeric f64 comparison needs ucomisd and NaN checks; materialize
         * the boolean first, then branch on it like any other condition. */
        compile_bool_expr(g, cond);
        buf_u8(b, 0x48); buf_u8(b, 0x85); buf_u8(b, 0xC0); /* test rax, rax */
    } else if (comparison_setcc(g, cond, &setcc)) {
        emit_cmp_operands(g, cond);
        /* setcc = 0x90 + cc; the inverse jcc is 0x80 + (cc ^ 1). */
        jump_op = (uint8_t)(0x80 + ((setcc - 0x90) ^ 1));
    } else {
        compile_bool_expr(g, cond);
        buf_u8(b, 0x48); buf_u8(b, 0x85); buf_u8(b, 0xC0);
    }
    buf_u8(b, 0x0F); buf_u8(b, jump_op);
    size_t patch = b->len;
    buf_u32(b, 0);
    return patch;
}

static void compile_bool_expr(Gen* g, WExpr* e) {
    Buf* b = &g->code;
    if (!e) {
        return;
    }

    if (e->tag == 8) {
        WStr op = e->str;
        if (wstr_eq_c(op, "OpAnd") || wstr_eq_c(op, "&&")) {
            compile_bool_expr(g, e->a);
            buf_u8(b, 0x48); buf_u8(b, 0x85); buf_u8(b, 0xC0);
            buf_u8(b, 0x0F); buf_u8(b, 0x84);
            size_t false_patch = b->len;
            buf_u32(b, 0);

            compile_bool_expr(g, e->b);

            buf_u8(b, 0xE9);
            size_t end_patch = b->len;
            buf_u32(b, 0);

            size_t false_pos = b->len;
            patch_address(g, false_patch, (int32_t)(false_pos - (false_patch + 4)));
            buf_u8(b, 0x48); buf_u8(b, 0x31); buf_u8(b, 0xC0);
            size_t end_pos = b->len;
            patch_address(g, end_patch, (int32_t)(end_pos - (end_patch + 4)));
            return;
        }
        if (wstr_eq_c(op, "OpOr") || wstr_eq_c(op, "||")) {
            compile_bool_expr(g, e->a);
            buf_u8(b, 0x48); buf_u8(b, 0x85); buf_u8(b, 0xC0);
            buf_u8(b, 0x0F); buf_u8(b, 0x85);
            size_t true_patch = b->len;
            buf_u32(b, 0);

            compile_bool_expr(g, e->b);

            buf_u8(b, 0xE9);
            size_t end_patch = b->len;
            buf_u32(b, 0);

            size_t true_pos = b->len;
            patch_address(g, true_patch, (int32_t)(true_pos - (true_patch + 4)));
            buf_u8(b, 0x48); buf_u8(b, 0xC7); buf_u8(b, 0xC0);
            buf_u32(b, 1);
            size_t end_pos = b->len;
            patch_address(g, end_patch, (int32_t)(end_pos - (end_patch + 4)));
            return;
        }

        if (is_float_comparison(g, e)) {
            emit_float_compare_bool(g, e);
            return;
        }

        uint8_t setcc;
        if (comparison_setcc(g, e, &setcc)) {
            emit_cmp_operands(g, e);
            buf_u8(b, 0x0F); buf_u8(b, setcc); buf_u8(b, 0xC0);
            buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0xB6); buf_u8(b, 0xC0);
            return;
        }
    }

    compile_expr(g, e, 0, expr_deref_flag(g, e, 1));
    buf_u8(b, 0x48); buf_u8(b, 0x85); buf_u8(b, 0xC0);
    buf_u8(b, 0x0F); buf_u8(b, 0x95); buf_u8(b, 0xC0);
    buf_u8(b, 0x48); buf_u8(b, 0x0F); buf_u8(b, 0xB6); buf_u8(b, 0xC0);
}

/* True when compile_address() leaves the address of the base expression in
 * the register instead of the pointer value it holds. An arrow access must
 * load that pointer before adding the member offset. Plain pointer locals and
 * parameters already load their value in compile_address(Variable), so they
 * must not be loaded twice. */
static int address_expr_needs_deref(WExpr* e) {
    if (!e) {
        return 0;
    }
    switch (e->tag) {
        case 6: /* MemberAccess */
        case 7: /* SectionAccess */
        case 10: /* Index */
            return 1;
        default:
            return 0;
    }
}

/* True when e is an Index whose base and index are both held in registers
 * right now. A caller can then use (base, index, scale) as a memory operand
 * and skip the separate lea into rbx. No code is emitted here. */
static int index_sib_ready(Gen* g, WExpr* e, uint8_t* base, uint8_t* index,
                           int* scale_bits) {
    if (!e || e->tag != 10 || !e->a || !e->b) return 0;
    if (e->a->tag != 3 || e->b->tag != 3) return 0;
    WType* bt = resolve_named_type(g, resolve_expr_type(g, e->a));
    uint32_t elem = 8;
    if (bt && (bt->tag == 12 || bt->tag == 10)) elem = type_size(g, bt->inner);
    int scale = -1;
    switch (elem) {
        case 1: scale = 0; break;
        case 2: scale = 1; break;
        case 4: scale = 2; break;
        case 8: scale = 3; break;
        default: return 0;
    }
    uint8_t b = 0, i = 0;
    uint64_t ba = 3, ia = 3;
    if (!reg_local_lookup(g, e->a->str, &b) ||
        !reg_local_lookup(g, e->b->str, &i)) return 0;
    if (!map_get_str(&g->local_access, e->a->str, &ba) ||
        !map_get_str(&g->local_access, e->b->str, &ia)) return 0;
    if (ba != 3 || ia != 3) return 0;
    if ((b & 7) == 5) return 0; /* mod=00 with base=101 needs disp32 */
    *base = b;
    *index = i;
    *scale_bits = scale;
    return 1;
}

static void compile_address(Gen* g, WExpr* e, uint8_t reg) {
    Buf* b = &g->code;
    uint8_t internal_reg = reg == 0 ? 0 : 3;

    if (!e) {
        return;
    }

    switch (e->tag) {
        case 3: { /* Variable */
            uint64_t off;
            if (map_get_str(&g->locals, e->str, &off)) {
                uint64_t access = 3;
                map_get_str(&g->local_access, e->str, &access);
                WType* type = NULL;
                uint64_t type_ptr;
                if (map_get_str(&g->local_types, e->str, &type_ptr)) {
                    type = AS_PTR(WType, type_ptr);
                }
                uint64_t is_param = 0;
                map_get_str(&g->local_is_param, e->str, &is_param);
                WType* pointer_check = resolve_named_type(g, type);
                int is_pointer = pointer_check && pointer_check->tag == 10;
                uint8_t bound_reg = 0;
                if (is_pointer && reg_local_lookup(g, e->str, &bound_reg)) {
                    emit_mov_reg_to_reg(b, bound_reg, internal_reg);
                    break;
                }
                /* An array-typed parameter is really a pointer passed by the
                 * caller, so its stack slot holds the address, not the array. */
                int is_array = pointer_check && pointer_check->tag == 12 && !is_param;
                if (is_array) {
                    emit_mem_op(b, 0x8D, internal_reg, (uint32_t)off);
                } else if (access == 0 || access == 1 || access == 2 || is_pointer ||
                           is_param || (pointer_check && pointer_check->tag == 12)) {
                    /* Parameters store their incoming value/pointer in the
                     * stack slot; use it as the base of an index instead of
                     * the address of the slot itself. */
                    emit_mem_op(b, 0x8B, internal_reg, (uint32_t)off);
                } else {
                    emit_mem_op(b, 0x8D, internal_reg, (uint32_t)off);
                }
            } else {
                WStr key = find_global_key(g, e->str);
                if (key.ptr) {
                    size_t colon = 0;
                    while (colon < key.len && key.ptr[colon] != ':') colon++;
                    WExpr sect = {0};
                    sect.tag = 7;
                    sect.str = wstr_sub(key, 0, colon);
                    sect.str2 = wstr_sub(key, colon + 1, key.len - colon - 1);
                    compile_address(g, &sect, internal_reg);
                    /* A global pointer variable stores the address in its
                     * section slot. When the variable is used as the base of
                     * an index or member access, load that address instead of
                     * using the slot itself. Arrays keep the slot address. */
                    WType* base_type = resolve_named_type(g, resolve_expr_type(g, e));
                    if (base_type && base_type->tag == 10) {
                        buf_u8(b, 0x48); buf_u8(b, 0x8B);
                        buf_u8(b, internal_reg == 0 ? 0x00 : 0x1B);
                    }
                }
            }
            break;
        }
        case 10: { /* Index */
            WType* base_type = resolve_named_type(g, resolve_expr_type(g, e->a));
            uint32_t elem_size = 8;
            if (base_type && (base_type->tag == 12 || base_type->tag == 10)) {
                elem_size = type_size(g, base_type->inner);
            }
            /* The base of an index is an address, not a value: compile it as
             * an lvalue so pointer parameters and local pointer variables stay
             * pointers. compile_expr(..., deref_ptr=1) used to load the first
             * element here, turning a[j] into a[0][j]. */
            int scale_bits = -1;
            switch (elem_size) {
                case 1: scale_bits = 0; break;
                case 2: scale_bits = 1; break;
                case 4: scale_bits = 2; break;
                case 8: scale_bits = 3; break;
                default: break;
            }

            /* If both the base pointer and the index are already in loop
             * registers, fold the address into one lea. This removes three
             * moves per array access in tight kernels. */
            uint8_t base_reg = 0, index_reg = 0;
            uint64_t base_access = 3, index_access = 3;
            if (scale_bits >= 0 && e->a && e->a->tag == 3 && e->b &&
                e->b->tag == 3 &&
                reg_local_lookup(g, e->a->str, &base_reg) &&
                reg_local_lookup(g, e->b->str, &index_reg) &&
                map_get_str(&g->local_access, e->a->str, &base_access) &&
                map_get_str(&g->local_access, e->b->str, &index_access) &&
                base_access == 3 && index_access == 3) {
                emit_lea_scaled(b, internal_reg == 0 ? 0 : internal_reg,
                                base_reg, index_reg, scale_bits);
                break;
            }

            int index_simple = rax_only_expr(g, e->b);
            compile_address(g, e->a, 3);
            /* Some address expressions leave the address of a slot that holds
             * a pointer: state:p, s.ptr, and a[i] when the element is a
             * pointer. Load that slot before indexing. Plain variables already
             * load their pointer value in the Variable branch, and AddrOf
             * expressions must keep the address they name. */
            if (address_expr_needs_deref(e->a) && base_type &&
                base_type->tag == 10) {
                buf_u8(b, 0x48); buf_u8(b, 0x8B); buf_u8(b, 0x1B);
            }
            if (!index_simple) {
                buf_u8(b, 0x53); /* push rbx if index may clobber it */
                g->stack_depth += 8;
            }
            compile_expr(g, e->b, 0, 1);

            /* lea rbx, [rbx + rax * scale] replaces shift plus add for the
             * common element widths. SIB base=rbx (3), index=rax (0), and
             * scale bits encode 1, 2, 4, or 8. */
            if (scale_bits >= 0) {
                if (!index_simple) {
                    g->stack_depth -= 8;
                    buf_u8(b, 0x5B); /* pop rbx */
                }
                buf_u8(b, 0x48); buf_u8(b, 0x8D); buf_u8(b, 0x1C);
                buf_u8(b, (uint8_t)((scale_bits << 6) | 0x03));
            } else {
                /* imul rax, rax, imm32 is REX.W 69 C0 imm32; the old
                 * 0F 69 prefix decoded as punpckhwd and corrupted the
                 * instruction stream for struct arrays. */
                buf_u8(b, 0x48); buf_u8(b, 0x69); buf_u8(b, 0xC0);
                buf_u32(b, elem_size);
                if (!index_simple) {
                    g->stack_depth -= 8;
                    buf_u8(b, 0x5B); /* pop rbx */
                }
                buf_u8(b, 0x48); buf_u8(b, 0x01); buf_u8(b, 0xC3);
            }
            if (internal_reg == 0) {
                buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xD8);
            }
            break;
        }
        case 6: { /* MemberAccess */
            compile_address(g, e->a, internal_reg);
            if (e->flag && address_expr_needs_deref(e->a)) {
                if (internal_reg == 0) {
                    buf_u8(b, 0x48); buf_u8(b, 0x8B); buf_u8(b, 0x00);
                } else {
                    buf_u8(b, 0x48); buf_u8(b, 0x8B); buf_u8(b, 0x1B);
                }
            }
            WType* base = resolve_named_type(g, resolve_expr_type(g, e->a));
            WStr struct_name = wstr_lit("");
            if (base && base->tag == 11) {
                struct_name = base->name;
            } else if (base && base->tag == 10) {
                WType* inner = resolve_named_type(g, base->inner);
                if (inner && inner->tag == 11) {
                    struct_name = inner->name;
                }
            }
            uint64_t layout_idx;
            if (map_get_str(&g->struct_layouts, struct_name, &layout_idx)) {
                uint64_t field_offset;
                if (map_get_str(&g->layouts.v[layout_idx].field_offsets, e->str2, &field_offset)) {
                    uint8_t add_opcode = internal_reg == 0 ? 0xC0 : 0xC3;
                    buf_u8(b, 0x48); buf_u8(b, 0x81); buf_u8(b, add_opcode);
                    buf_u32(b, (uint32_t)field_offset);
                }
            }
            break;
        }
        case 7: { /* SectionAccess */
            WStr key = wstr_pair(e->str, e->str2);
            emit_rip_relative_lea(g, internal_reg, key);
            break;
        }
        case 4: /* AddrOf */
            compile_expr(g, e, internal_reg, 0);
            break;
        case 5: /* AddrOfExpr */
            compile_address(g, e->a, internal_reg);
            break;
        case 8: { /* Binary */
            if (wstr_eq_c(e->str, "OpAdd") || wstr_eq_c(e->str, "+")) {
                compile_expr(g, e->a, 3, 1);
                buf_u8(b, 0x53);
                g->stack_depth += 8;
                compile_expr(g, e->b, 0, 1);
                g->stack_depth -= 8;
                buf_u8(b, 0x5B);
                buf_u8(b, 0x48); buf_u8(b, 0x01); buf_u8(b, 0xC3);
            } else if (wstr_eq_c(e->str, "OpSub") || wstr_eq_c(e->str, "-")) {
                compile_expr(g, e->a, 3, 1);
                buf_u8(b, 0x53);
                g->stack_depth += 8;
                compile_expr(g, e->b, 0, 1);
                g->stack_depth -= 8;
                buf_u8(b, 0x5B);
                buf_u8(b, 0x48); buf_u8(b, 0x29); buf_u8(b, 0xC3);
            } else {
                compile_expr(g, e, internal_reg, 0);
                return;
            }
            if (internal_reg == 0) {
                buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xD8);
            }
            return;
        }
        default:
            break;
    }

    if (reg != 0 && reg != 3) {
        buf_u8(b, reg >= 8 ? 0x49 : 0x48);
        buf_u8(b, 0x89);
        buf_u8(b, (uint8_t)(0xC0 | (3 << 3) | (reg & 7)));
    }
}

/* True for an Index/MemberAccess whose declared type is f64. Such stores can
 * write xmm0 directly with movsd instead of round-tripping the bits through
 * rax and r10. */
/* True when e is a plain f64 local currently bound to an XMM register. */
static int is_xmm_bound_f64_local(Gen* g, WExpr* e, uint8_t* reg) {
    if (!e || e->tag != 3 || !e->str.ptr) return 0;
    uint64_t type_ptr = 0;
    if (!map_get_str(&g->local_types, e->str, &type_ptr)) return 0;
    WType* t = AS_PTR(WType, type_ptr);
    if (!t || !is_float_type_named(g,t) || type_size(g, t) != 8) return 0;
    uint8_t xmm_reg = 0;
    if (!xmm_local_lookup(g, e->str, &xmm_reg)) return 0;
    if (reg) *reg = xmm_reg;
    return 1;
}

static int is_f64_store_target(Gen* g, WExpr* e) {
    if (!e) {
        return 0;
    }
    if (e->tag == 3) {
        uint64_t type_ptr = 0;
        if (!map_get_str(&g->local_types, e->str, &type_ptr)) {
            return 0;
        }
        WType* t = AS_PTR(WType, type_ptr);
        if (!is_float_type_named(g,t) || type_size(g, t) != 8) {
            return 0;
        }
        uint64_t access = 3, off = 0;
        uint8_t bound_reg = 0;
        map_get_str(&g->local_access, e->str, &access);
        if (access != 3 || reg_local_lookup(g, e->str, &bound_reg) ||
            !map_get_str(&g->locals, e->str, &off)) {
            return 0;
        }
        return 1;
    }
    if (e->tag != 6 && e->tag != 10) {
        return 0;
    }
    WType* t = resolve_named_type(g, resolve_expr_type(g, e));
    return t && t->tag == 8;
}

/* Recognise x = x + imm or x = x - imm on a plain 64-bit stack slot. The
 * caller replaces the load/op/store sequence with one incq, decq, addq, or
 * subq memory instruction. op: 0=inc, 1=dec, 2=add imm, 3=sub imm. */
static int is_local_self_addsub(Gen* g, WExpr* target, WExpr* e,
                                uint8_t* op, int64_t* imm) {
    if (!target || target->tag != 3 || !e || e->tag != 8) {
        return 0;
    }
    int is_add = wstr_eq_c(e->str, "OpAdd");
    int is_sub = wstr_eq_c(e->str, "OpSub");
    if (!is_add && !is_sub) {
        return 0;
    }
    if (!e->b || e->b->tag != 0) {
        return 0;
    }
    if (!e->a || e->a->tag != 3 || !wstr_eq(e->a->str, target->str)) {
        return 0;
    }
    uint64_t access = 3;
    map_get_str(&g->local_access, target->str, &access);
    uint64_t off = 0;
    if (access != 3 || !map_get_str(&g->locals, target->str, &off)) {
        return 0;
    }
    if (expr_size(g, target) != 8 || is_float_expr(g, target)) {
        return 0;
    }
    int64_t sv = (int64_t)e->b->num;
    if (sv < INT32_MIN || sv > INT32_MAX) {
        return 0;
    }
    int64_t delta = is_add ? sv : -sv;
    if (delta == 0) {
        return 0;
    }
    int negate = delta < 0;
    if (negate) delta = -delta;
    if (delta > INT32_MAX) {
        return 0;
    }
    if (delta == 1) {
        *op = negate ? 1 : 0;
    } else {
        *op = negate ? 3 : 2;
        *imm = delta;
    }
    return 1;
}

/* Recognise state:x = state:x + imm on a plain 64-bit section variable.
 * Volatile, enum, float, and narrow variables keep the generic load/store path. */
static int is_section_self_addsub(Gen* g, WExpr* target, WExpr* e,
                                  uint8_t* op, int64_t* imm) {
    if (!target || target->tag != 7 || !e || e->tag != 8) {
        return 0;
    }
    if (map_has(&g->enums, target->str.ptr, target->str.len)) {
        return 0;
    }
    WStr key = wstr_pair(target->str, target->str2);
    if (map_has(&g->section_volatile, key.ptr, key.len)) {
        return 0;
    }
    uint64_t type_ptr = 0;
    WType* t = map_get_str(&g->section_types, key, &type_ptr)
                   ? AS_PTR(WType, type_ptr)
                   : NULL;
    while (t && t->tag == 13) {
        t = t->inner;
    }
    if (!t || (t->tag != 3 && t->tag != 7)) {
        return 0;
    }
    uint64_t size = 0;
    if (!map_get_str(&g->section_var_sizes, key, &size) || size != 8) {
        return 0;
    }
    int is_add = wstr_eq_c(e->str, "OpAdd");
    int is_sub = wstr_eq_c(e->str, "OpSub");
    if (!is_add && !is_sub) {
        return 0;
    }
    if (!e->b || e->b->tag != 0) {
        return 0;
    }
    if (!e->a || e->a->tag != 7 || !wstr_eq(e->a->str, target->str) ||
        !wstr_eq(e->a->str2, target->str2)) {
        return 0;
    }
    int64_t sv = (int64_t)e->b->num;
    int64_t delta = is_add ? sv : -sv;
    if (delta == 0) {
        return 0;
    }
    int negate = delta < 0;
    if (negate) delta = -delta;
    if (delta > INT32_MAX) {
        return 0;
    }
    if (delta == 1) {
        *op = negate ? 1 : 0;
    } else {
        *op = negate ? 3 : 2;
        *imm = delta;
    }
    return 1;
}

/* RIP-relative read-modify-write for a section scalar. The four forms are
 * incq [rip+d], decq [rip+d], addq [rip+d], imm and subq [rip+d], imm. */
static void emit_rip_relative_mem_rmw(Gen* g, WStr key, uint8_t op, int64_t imm) {
    Buf* b = &g->code;
    size_t pos;
    if (op < 2) {
        buf_u8(b, 0x48);
        buf_u8(b, 0xFF);
        buf_u8(b, op == 0 ? 0x05 : 0x0D);
        pos = b->len;
        buf_u32(b, 0);
    } else {
        uint8_t group = op == 2 ? 0 : 5;
        if (imm <= 127) {
            buf_u8(b, 0x48);
            buf_u8(b, 0x83);
            buf_u8(b, (uint8_t)(0x05 | (group << 3)));
            pos = b->len;
            buf_u32(b, 0);
            buf_u8(b, (uint8_t)imm);
        } else {
            buf_u8(b, 0x48);
            buf_u8(b, 0x81);
            buf_u8(b, (uint8_t)(0x05 | (group << 3)));
            pos = b->len;
            buf_u32(b, 0);
            buf_u32(b, (uint32_t)imm);
        }
    }
    AddrPatch p;
    p.pos = pos;
    p.key = key;
    p.is_rip = 1;
    p.patch_end = (uint8_t)(op >= 2);
    p.is_global = 0;
    AddrPatchVec_push(&g->address_patches, p);
    g->patch_generation++;
}

static void store_assignment_target_from_rax(Gen* g, WExpr* target) {
    Buf* b = &g->code;
    if (!target) {
        return;
    }
    switch (target->tag) {
        case 3: { /* Variable */
            uint64_t access = 3;
            map_get_str(&g->local_access, target->str, &access);
            uint64_t off;
            if (map_get_str(&g->locals, target->str, &off)) {
                uint8_t bound_reg = 0;
                uint8_t xmm_reg = 0;
                if (is_xmm_bound_f64_local(g, target, &xmm_reg) && access == 3) {
                    emit_movq_gpr_to_xmm(b, 0, xmm_reg);
                    break;
                }
                if (reg_local_lookup(g, target->str, &bound_reg) && access == 3 &&
                    expr_size(g, target) == 8) {
                    emit_mov_reg_to_reg(b, 0, bound_reg);
                    break;
                }
                if (access == 1 || access == 2) {
                    emit_mem_load(b, 3, (uint32_t)off, 8, 0);
                    uint32_t elem_size = 8;
                    uint64_t type_ptr;
                    if (map_get_str(&g->local_types, target->str, &type_ptr)) {
                        WType* t = resolve_named_type(g, AS_PTR(WType, type_ptr));
                        if (t && t->tag == 10) {
                            elem_size = type_size(g, resolve_named_type(g, t->inner));
                        }
                    }
                    switch (elem_size) {
                        case 1: buf_u8(b, 0x88); buf_u8(b, 0x03); break;
                        case 2: buf_u8(b, 0x66); buf_u8(b, 0x89); buf_u8(b, 0x03); break;
                        case 4: buf_u8(b, 0x89); buf_u8(b, 0x03); break;
                        default: buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0x03); break;
                    }
                } else {
                    uint32_t size = expr_size(g, target);
                    size_t store_start = b->len;
                    emit_mem_store(b, 0, (uint32_t)off, size);
                    if (size == 8 && access == 3) {
                        g->last_rax_store_pos = store_start;
                        g->last_rax_store_len = (uint32_t)(b->len - store_start);
                        g->last_rax_store_off = (uint32_t)off;
                        g->last_rax_store_gen = g->patch_generation;
                    }
                }
            } else {
                WStr key = find_global_key(g, target->str);
                if (key.ptr) {
                    size_t colon = 0;
                    while (colon < key.len && key.ptr[colon] != ':') colon++;
                    WExpr sect = {0};
                    sect.tag = 7;
                    sect.str = wstr_sub(key, 0, colon);
                    sect.str2 = wstr_sub(key, colon + 1, key.len - colon - 1);
                    store_assignment_target_from_rax(g, &sect);
                }
            }
            break;
        }
        case 4: { /* AddrOf */
            uint64_t off;
            if (map_get_str(&g->locals, target->str, &off)) {
                emit_mem_store(b, 0, (uint32_t)off, 8);
            } else {
                WStr key = find_global_key(g, target->str);
                if (key.ptr) {
                    size_t colon = 0;
                    while (colon < key.len && key.ptr[colon] != ':') colon++;
                    WExpr sect = {0};
                    sect.tag = 7;
                    sect.str = wstr_sub(key, 0, colon);
                    sect.str2 = wstr_sub(key, colon + 1, key.len - colon - 1);
                    store_assignment_target_from_rax(g, &sect);
                }
            }
            break;
        }
        case 7: { /* SectionAccess */
            WStr key = wstr_pair(target->str, target->str2);
            uint64_t type_ptr;
            WType* var_type = map_get_str(&g->section_types, key, &type_ptr)
                                  ? AS_PTR(WType, type_ptr)
                                  : NULL;
            uint32_t size = var_type ? type_size(g, var_type) : 8;
            if (size == 0) size = 8;
            emit_rip_relative_lea(g, 3, key);
            switch (size) {
                case 1: buf_u8(b, 0x88); buf_u8(b, 0x03); break;
                case 2: buf_u8(b, 0x66); buf_u8(b, 0x89); buf_u8(b, 0x03); break;
                case 4: buf_u8(b, 0x89); buf_u8(b, 0x03); break;
                default: buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0x03); break;
            }
            if (map_has(&g->section_volatile, key.ptr, key.len)) {
                buf_u8(b, 0x0F); buf_u8(b, 0xAE); buf_u8(b, 0xF0);
            }
            break;
        }
        case 6: /* MemberAccess */
        case 10: { /* Index */
            uint8_t sib_base = 0, sib_index = 0;
            int sib_scale = -1;
            if (target->tag == 10 &&
                index_sib_ready(g, target, &sib_base, &sib_index, &sib_scale)) {
                /* The value is already in rax and both address registers are
                 * live, so store straight through the SIB operand. */
                uint32_t sib_size = expr_size(g, target);
                if (sib_size == 0) sib_size = 8;
                emit_gpr_sib_store(b, 0, sib_base, sib_index, sib_scale, sib_size);
                if (is_volatile_member_expr(g, target)) {
                    buf_u8(b, 0x0F); buf_u8(b, 0xAE); buf_u8(b, 0xF0);
                }
                break;
            }
            /* For simple indices, save rax in r10 instead of pushing it on
             * the stack. compile_address uses only rbx/rax for simple
             * operands, so r10 survives and no memory round-trip is needed. */
            int use_r10 = target->tag == 10 && simple_left(g, target->b);
            if (use_r10) {
                buf_u8(b, 0x49); buf_u8(b, 0x89); buf_u8(b, 0xC2); /* mov %rax,%r10 */
            } else {
                buf_u8(b, 0x50); /* push %rax */
            }
            compile_address(g, target, 3);
            if (use_r10) {
                buf_u8(b, 0x4C); buf_u8(b, 0x89); buf_u8(b, 0xD0); /* mov %r10,%rax */
            } else {
                buf_u8(b, 0x58); /* pop %rax */
            }
            uint32_t size = expr_size(g, target);
            switch (size) {
                case 1: buf_u8(b, 0x88); buf_u8(b, 0x03); break;
                case 2: buf_u8(b, 0x66); buf_u8(b, 0x89); buf_u8(b, 0x03); break;
                case 4: buf_u8(b, 0x89); buf_u8(b, 0x03); break;
                default: buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0x03); break;
            }
            if (is_volatile_member_expr(g, target)) {
                buf_u8(b, 0x0F); buf_u8(b, 0xAE); buf_u8(b, 0xF0);
            }
            break;
        }
        default:
            break;
    }
}

static int try_compile_array_assignment(Gen* g, WExpr* target, WExpr* value) {
    if (!target || !value) {
        return 0;
    }
    WType* resolved = resolve_named_type(g, resolve_expr_type(g, target));
    if (!resolved) {
        return 0;
    }
    Buf* b = &g->code;
    /* Whole struct assignment copies every field, including structs reached
     * through an array index such as `arr[0] = p0`. */
    if (resolved->tag == 11) {
        uint32_t total = type_size(g, resolved);
        if ((value->tag == 12 || value->tag == 2) && total) {
            /* Brace or string initializer for a whole struct: use field
             * offsets and zero-fill unspecified fields instead of storing
             * the string address as a scalar. */
            compile_address(g, target, 3);
            emit_runtime_init_rbx(g, value, resolved, total);
            return 1;
        }
        WType* value_type = resolve_named_type(g, resolve_expr_type(g, value));
        if (total > 8 && value_type && value_type->tag == 11 &&
            type_size(g, value_type) == total &&
            (value->tag == 3 || value->tag == 6 || value->tag == 7 ||
             value->tag == 10)) {
            compile_address(g, target, 3);
            buf_u8(b, 0x53); /* push destination address */
            g->stack_depth += 8;
            compile_address(g, value, 3);
            g->stack_depth -= 8;
            buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xDE); /* mov %rbx,%rsi */
            buf_u8(b, 0x5F); /* pop %rdi */
            emit_mov_imm64(b, 1, total);
            buf_u8(b, 0xFC);
            buf_u8(b, 0xF3); buf_u8(b, 0xA4);
            return 1;
        }
        return 0;
    }
    if (resolved->tag != 12 || resolved->count == 0) {
        return 0;
    }
    WType* elem = resolve_named_type(g, resolved->inner);
    uint32_t elem_size = elem ? type_size(g, elem) : 0;
    if (!elem_size) {
        return 0;
    }
    uint64_t total = resolved->count * elem_size;

    if (value->tag == 2) {
        /* Whole-array string copy, including the terminating NUL. */
        if (elem->tag != 0 && elem->tag != 4) {
            return 0;
        }
        compile_address(g, target, 3);
        Buf bytes = {0};
        unescape_string(value->str, &bytes);
        uint64_t count = resolved->count;
        uint64_t copy_len = bytes.len + 1 <= count ? bytes.len + 1 : count - 1;
        buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xDF);
        if (copy_len > 0) {
            WStr key = wstr_prefix("str:", value->str);
            emit_rip_relative_lea(g, 6, key);
            emit_mov_imm64(b, 1, copy_len);
            buf_u8(b, 0xFC);
            buf_u8(b, 0xF3); buf_u8(b, 0xA4);
        }
        if (bytes.len + 1 > count) {
            buf_u8(b, 0xC6); buf_u8(b, 0x07); buf_u8(b, 0x00);
        }
        return 1;
    }

    if (value->tag == 12) {
        /* Flatten nested braces and byte-string leaves using the declared
         * type's real field offsets. Pointer-array string leaves stay addresses. */
        compile_address(g, target, 3);
        emit_runtime_init_rbx(g, value, resolved, (uint32_t)total);
        return 1;
    }

    /* An array-typed value is an lvalue in Wand: copy the elements rather
     * than storing the address bytes into the destination. */
    WType* value_type = resolve_named_type(g, resolve_expr_type(g, value));
    if (value_type && value_type->tag == 12 && value_type->count &&
        type_size(g, value_type->inner) == elem_size) {
        compile_address(g, target, 3);
        buf_u8(b, 0x53); /* push destination address */
        g->stack_depth += 8;
        compile_address(g, value, 3);
        g->stack_depth -= 8;
        buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xDE); /* mov %rbx,%rsi */
        buf_u8(b, 0x5F); /* pop %rdi */
        emit_mov_imm64(b, 1, value_type->count < resolved->count
                                  ? value_type->count * elem_size
                                  : total);
        buf_u8(b, 0xFC);
        buf_u8(b, 0xF3); buf_u8(b, 0xA4);
        return 1;
    }
    return 0;
}

static char* trim_line(char* s) {
    while (*s && isspace((unsigned char)*s)) s++;
    char* end = s + strlen(s);
    while (end > s && isspace((unsigned char)end[-1])) *--end = '\0';
    char* slash = strstr(s, "//");
    if (slash) *slash = '\0';
    char* semi = strchr(s, ';');
    if (semi) *semi = '\0';
    while (*s && isspace((unsigned char)*s)) s++;
    end = s + strlen(s);
    while (end > s && isspace((unsigned char)end[-1])) *--end = '\0';
    return s;
}

static int nasm_reg_size(WStr s, uint8_t* reg, uint32_t* size) {
    if (wstr_eq_c(s, "rax")) { *reg = 0; *size = 8; return 1; }
    if (wstr_eq_c(s, "rcx")) { *reg = 1; *size = 8; return 1; }
    if (wstr_eq_c(s, "rdx")) { *reg = 2; *size = 8; return 1; }
    if (wstr_eq_c(s, "rbx")) { *reg = 3; *size = 8; return 1; }
    if (wstr_eq_c(s, "rsi")) { *reg = 6; *size = 8; return 1; }
    if (wstr_eq_c(s, "rdi")) { *reg = 7; *size = 8; return 1; }
    /* Documented rdtsc example uses eax/edx; accept all narrow aliases. */
    if (wstr_eq_c(s, "eax")) { *reg = 0; *size = 4; return 1; }
    if (wstr_eq_c(s, "ecx")) { *reg = 1; *size = 4; return 1; }
    if (wstr_eq_c(s, "edx")) { *reg = 2; *size = 4; return 1; }
    if (wstr_eq_c(s, "ebx")) { *reg = 3; *size = 4; return 1; }
    if (wstr_eq_c(s, "esi")) { *reg = 6; *size = 4; return 1; }
    if (wstr_eq_c(s, "edi")) { *reg = 7; *size = 4; return 1; }
    if (wstr_eq_c(s, "ax")) { *reg = 0; *size = 2; return 1; }
    if (wstr_eq_c(s, "cx")) { *reg = 1; *size = 2; return 1; }
    if (wstr_eq_c(s, "dx")) { *reg = 2; *size = 2; return 1; }
    if (wstr_eq_c(s, "bx")) { *reg = 3; *size = 2; return 1; }
    if (wstr_eq_c(s, "si")) { *reg = 6; *size = 2; return 1; }
    if (wstr_eq_c(s, "di")) { *reg = 7; *size = 2; return 1; }
    if (wstr_eq_c(s, "al")) { *reg = 0; *size = 1; return 1; }
    if (wstr_eq_c(s, "cl")) { *reg = 1; *size = 1; return 1; }
    if (wstr_eq_c(s, "dl")) { *reg = 2; *size = 1; return 1; }
    if (wstr_eq_c(s, "bl")) { *reg = 3; *size = 1; return 1; }
    return 0;
}

static void compile_nasm(Gen* g, WStr code) {
    Buf* b = &g->code;
    char* copy = (char*)arena_alloc(g_arena, code.len + 1);
    if (!copy) {
        return;
    }
    memcpy(copy, code.ptr, code.len);
    copy[code.len] = '\0';

    char* save = NULL;
    for (char* line = strtok_r(copy, "\n", &save); line; line = strtok_r(NULL, "\n", &save)) {
        char* t = trim_line(line);
        if (!*t) {
            continue;
        }
        size_t before = b->len;
        if (strcmp(t, "syscall") == 0) {
            buf_u8(b, 0x0F); buf_u8(b, 0x05);
        } else if (strcmp(t, "pause") == 0) {
            /* Spin-loop hint; keeps the zram lock free of GCC builtins. */
            buf_u8(b, 0xF3); buf_u8(b, 0x90);
        } else if (strcmp(t, "rdtsc") == 0) {
            buf_u8(b, 0x0F); buf_u8(b, 0x31);
        } else if (strcmp(t, "hlt") == 0) {
            buf_u8(b, 0xF4);
        } else if (strcmp(t, "nop") == 0) {
            buf_u8(b, 0x90);
        } else if (strcmp(t, "mfence") == 0) {
            buf_u8(b, 0x0F); buf_u8(b, 0xAE); buf_u8(b, 0xF0);
        } else if (strcmp(t, "lfence") == 0) {
            buf_u8(b, 0x0F); buf_u8(b, 0xAE); buf_u8(b, 0xE8);
        } else if (strcmp(t, "sfence") == 0) {
            buf_u8(b, 0x0F); buf_u8(b, 0xAE); buf_u8(b, 0xF8);
        } else if (strcmp(t, "cli") == 0) {
            buf_u8(b, 0xFA);
        } else if (strcmp(t, "sti") == 0) {
            buf_u8(b, 0xFB);
        } else if (strcmp(t, "rep movsb") == 0) {
            buf_u8(b, 0xF3); buf_u8(b, 0xA4);
        } else if (strcmp(t, "rep stosb") == 0) {
            buf_u8(b, 0xF3); buf_u8(b, 0xAA);
        } else if (strcmp(t, "cld") == 0) {
            buf_u8(b, 0xFC);
        } else if (strcmp(t, "ret") == 0) {
            buf_u8(b, 0xC3);
        } else if (strcmp(t, "push rbp") == 0) {
            buf_u8(b, 0x55);
        } else if (strcmp(t, "pop rbp") == 0) {
            buf_u8(b, 0x5D);
        } else if (strncmp(t, "mov ", 4) == 0) {
            char* comma = strchr(t + 4, ',');
            if (!comma) {
                continue;
            }
            *comma = '\0';
            char* dst = trim_line(t + 4);
            char* src = trim_line(comma + 1);
            uint8_t dreg, sreg;
            uint32_t dsize = 8, ssize = 8;
            if (dst[0] == '[' && dst[strlen(dst) - 1] == ']') {
                dst[strlen(dst) - 1] = '\0';
                WStr name = wstr_lit(dst + 1);
                uint64_t off;
                if (nasm_reg_size(wstr_lit(src), &sreg, &ssize) &&
                    map_get_str(&g->locals, name, &off)) {
                    emit_mem_store(b, sreg, (uint32_t)off, ssize);
                }
            } else if (nasm_reg_size(wstr_lit(dst), &dreg, &dsize)) {
                if (src[0] == '[' && src[strlen(src) - 1] == ']') {
                    src[strlen(src) - 1] = '\0';
                    WStr name = wstr_lit(src + 1);
                    uint64_t off;
                    if (map_get_str(&g->locals, name, &off)) {
                        emit_mem_load(b, dreg, (uint32_t)off, dsize, 0);
                    }
                } else if (strspn(src, "0123456789-") == strlen(src) && *src) {
                    long value = strtol(src, NULL, 10);
                    if (dsize == 8) {
                        buf_u8(b, 0x48); buf_u8(b, 0xC7); buf_u8(b, (uint8_t)(0xC0 | dreg));
                        buf_u32(b, (uint32_t)(int32_t)value);
                    } else if (dsize == 4) {
                        buf_u8(b, (uint8_t)(0xB8 | dreg));
                        buf_u32(b, (uint32_t)(int32_t)value);
                    } else if (dsize == 2) {
                        buf_u8(b, 0x66); buf_u8(b, (uint8_t)(0xB8 | dreg));
                        buf_u16(b, (uint16_t)value);
                    } else {
                        buf_u8(b, (uint8_t)(0xB0 | dreg));
                        buf_u8(b, (uint8_t)value);
                    }
                } else if (nasm_reg_size(wstr_lit(src), &sreg, &ssize)) {
                    if (dsize == 1) {
                        buf_u8(b, 0x88);
                    } else if (dsize == 2) {
                        buf_u8(b, 0x66); buf_u8(b, 0x89);
                    } else if (dsize == 4) {
                        buf_u8(b, 0x89);
                    } else {
                        buf_u8(b, 0x48); buf_u8(b, 0x89);
                    }
                    buf_u8(b, (uint8_t)(0xC0 | (sreg << 3) | dreg));
                }
            }
        }
        if (b->len == before) {
            fprintf(stderr, "[wand2c] warning: unsupported ::nasm:: line ignored: '%s'\n", t);
        }
    }
}

static void wstr_vec_push_unique(WStrVec* v, WStr name) {
    if (!name.ptr || !name.len) return;
    for (size_t i = 0; i < v->n; i++) {
        if (wstr_eq(v->v[i], name)) return;
    }
    WStrVec_push(v, name);
}

/* Dynamic `jmpto`. There is no new syntax: variables prepared by the block
 * become the continuation's arguments, and `return(var)` entries in the block
 * name variables that receive the continuation's results. With no return
 * entries the first result is still available in rax, as before. */
static void compile_dynamic_jmpto(Gen* g, WStmt* s, int variable_module) {
    Buf* b = &g->code;
    WStrVec arg_names = {0};
    WStrVec result_targets = {0};

    for (size_t i = 0; i < s->stmt1_count; i++) {
        WStmt* arg = s->stmts1[i];
        if (!arg) continue;
        if (arg->tag == 6) {
            for (size_t j = 0; j < arg->return_count; j++) {
                WExpr* expr = arg->returns[j].expr;
                if (expr && expr->tag == 3) {
                    wstr_vec_push_unique(&result_targets, expr->str);
                }
            }
            continue;
        }
        if (arg->tag == 0 && arg->type) {
            uint64_t type_ptr;
            uint32_t size = 8;
            if (map_get_str(&g->local_types, arg->name, &type_ptr)) {
                size = type_size(g, AS_PTR(WType, type_ptr));
            }
            if (size && size <= 8) wstr_vec_push_unique(&arg_names, arg->name);
        } else if (arg->tag == 1) {
            for (size_t j = 0; j < arg->expr_count; j++) {
                if (arg->exprs[j] && arg->exprs[j]->tag == 3) {
                    wstr_vec_push_unique(&arg_names, arg->exprs[j]->str);
                }
            }
        }
        compile_stmt(g, arg);
    }

    size_t arg_count = arg_names.n < 6 ? arg_names.n : 6;
    size_t result_count = result_targets.n < 4 ? result_targets.n : 4;
    size_t slot_count = arg_count + result_count;
    if (slot_count == 0) slot_count = 2;
    slot_count = (slot_count + 1) & ~(size_t)1; /* keep rsp 16-byte aligned */
    size_t slot_bytes = slot_count * 8u;

    buf_u8(b, 0x48); buf_u8(b, 0x81); buf_u8(b, 0xEC);
    buf_u32(b, (uint32_t)slot_bytes);

    for (size_t i = 0; i < arg_count; i++) {
        WExpr var = {0};
        var.tag = 3;
        var.str = arg_names.v[i];
        compile_expr(g, &var, 0, 1);
        emit_mov_to_stack(b, 0, i * 8u);
    }

    if (variable_module) {
        WExpr var = {0};
        var.tag = 3;
        var.str = s->name;
        compile_expr(g, &var, 7, 0);
    } else {
        WStr key = wstr_prefix("str:", s->name);
        emit_rip_relative_lea(g, 7, key);
    }
    static const uint8_t mov_rsi_rsp[] = {0x48, 0x89, 0xE6};
    static const uint8_t mov_rcx_rsp[] = {0x48, 0x89, 0xE1};
    buf_bytes(b, mov_rsi_rsp, sizeof(mov_rsi_rsp));
    emit_mov_imm64(b, 2, (uint64_t)arg_count);
    if (arg_count) {
        static const uint8_t lea_rcx_stack[] = {0x48, 0x8D, 0x8C, 0x24};
        buf_bytes(b, lea_rcx_stack, sizeof(lea_rcx_stack));
        buf_u32(b, (uint32_t)(arg_count * 8u));
    } else {
        buf_bytes(b, mov_rcx_rsp, sizeof(mov_rcx_rsp));
    }
    emit_mov_imm64(b, 8, (uint64_t)result_count);
    emit_call_patch(g, wstr_lit("__wand_jmpto_loader"));

    for (size_t i = 0; i < result_count; i++) {
        emit_mov_from_stack(b, 0, (arg_count + i) * 8u);
        WType* type = NULL;
        uint64_t type_ptr;
        uint32_t size = 8;
        if (map_get_str(&g->local_types, result_targets.v[i], &type_ptr)) {
            type = AS_PTR(WType, type_ptr);
            size = type_size(g, type);
            if (size == 0) size = 8;
        }
        uint64_t off;
        if (map_get_str(&g->locals, result_targets.v[i], &off)) {
            emit_mem_store(b, 0, (uint32_t)off, size);
        } else {
            WExpr target = {0};
            target.tag = 3;
            target.str = result_targets.v[i];
            store_assignment_target_from_rax(g, &target);
        }
    }
    buf_u8(b, 0x48); buf_u8(b, 0x81); buf_u8(b, 0xC4);
    buf_u32(b, (uint32_t)slot_bytes);
}

static void compile_stmt(Gen* g, WStmt* s) {
    Buf* b = &g->code;
    if (!s) {
        return;
    }

    switch (s->tag) {
        case 0: { /* VarDefinition */
            uint32_t var_size = type_size(g, s->type);
            if (var_size == 0) var_size = 1;
            uint32_t align = s->align ? s->align : type_alignment_of(g, s->type);
            if (align == 0) align = 1;
            g->next_offset = ((g->next_offset + align - 1) / align) * align;
            g->next_offset += var_size;
            uint32_t off = g->next_offset;
            map_put_str(&g->locals, s->name, off);
            map_put_str(&g->local_access, s->name, s->access);
            map_put_str(&g->local_types, s->name, PTR(s->type));
            if (s->e1) {
                WType* vtype = s->type;
                int aggregate_init = 0;
                WType* declared = resolve_named_type(g, s->type);
                int aggregate_value = 0;
                if (s->e1->tag == 3 || s->e1->tag == 6 ||
                    s->e1->tag == 7 || s->e1->tag == 10) {
                    WType* vt = resolve_named_type(g, resolve_expr_type(g, s->e1));
                    aggregate_value = vt && (vt->tag == 11 || vt->tag == 12);
                }
                if (declared && (declared->tag == 11 || declared->tag == 12) &&
                    (s->e1->tag == 12 || s->e1->tag == 2 || aggregate_value)) {
                    emit_runtime_init_local(g, s->e1, s->type, off, var_size);
                    aggregate_init = 1;
                }
                uint8_t xmm_reg = 0;
                if (!aggregate_init && var_size == 8 && s->access == 3 &&
                    is_float_type_named(g,vtype) &&
                    xmm_local_lookup(g, s->name, &xmm_reg)) {
                    /* Keep f64 loop locals in an XMM register. */
                    if (is_float_expr(g, s->e1)) {
                        compile_float_to_xmm0(g, s->e1);
                        emit_movapd_xmm0_to_xmm(b, xmm_reg);
                    } else {
                        compile_expr(g, s->e1, 0, 1);
                        emit_movq_gpr_to_xmm(b, 0, xmm_reg);
                    }
                } else if (!aggregate_init && var_size == 8 && s->access == 3 &&
                    is_float_type_named(g,vtype) && is_float_expr(g, s->e1)) {
                    /* Store the f64 bit pattern straight from xmm0 instead of
                     * routing it through rax and emit_mem_store. */
                    compile_float_to_xmm0(g, s->e1);
                    buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, 0x11);
                    buf_u8(b, off <= 127 ? 0x45 : 0x85);
                    if (off <= 127) buf_u8(b, (uint8_t)(-(int32_t)off));
                    else buf_u32(b, (uint32_t)(-(int32_t)off));
                } else if (!aggregate_init) {
                    compile_expr(g, s->e1, 0, value_deref_flag(g, s->type));
                    emit_mem_store(b, 0, off, var_size);
                }
            }
            break;
        }
        case 1: { /* Assignment */
            if (s->expr_count == 1) {
                int copied = try_compile_array_assignment(g, s->exprs[0], s->e1);
                uint8_t addsub_op = 0;
                int64_t addsub_imm = 0;
                int local_addsub = !copied &&
                    is_local_self_addsub(g, s->exprs[0], s->e1,
                                         &addsub_op, &addsub_imm);
                uint8_t sec_op = 0;
                int64_t sec_imm = 0;
                int section_addsub = !copied &&
                    is_section_self_addsub(g, s->exprs[0], s->e1,
                                           &sec_op, &sec_imm);
                WStr addsub_var = {0};
                int addsub_var_sub = 0;
                int local_addsub_var = !copied && !local_addsub &&
                    is_local_self_addsub_var(g, s->exprs[0], s->e1,
                                             &addsub_var, &addsub_var_sub);
                uint8_t target_xmm = 0;
                int direct_float_store =
                    !copied && is_f64_store_target(g, s->exprs[0]) &&
                    !is_volatile_member_expr(g, s->exprs[0]) &&
                    is_float_expr(g, s->e1);
                if (section_addsub) {
                    emit_rip_relative_mem_rmw(
                        g, wstr_pair(s->exprs[0]->str, s->exprs[0]->str2),
                        sec_op, sec_imm);
                } else if (local_addsub) {
                    uint64_t off = 0;
                    map_get_str(&g->locals, s->exprs[0]->str, &off);
                    uint8_t bound_reg = 0;
                    uint64_t access = 3;
                    map_get_str(&g->local_access, s->exprs[0]->str, &access);
                    if (access == 3 && expr_size(g, s->exprs[0]) == 8 &&
                        reg_local_lookup(g, s->exprs[0]->str, &bound_reg)) {
                        emit_addsub_reg(b, bound_reg, addsub_op, addsub_imm);
                    } else if (addsub_op < 2) {
                        emit_mem_op(b, 0xFF, addsub_op, (uint32_t)off);
                    } else {
                        uint8_t group = addsub_op == 2 ? 0 : 5;
                        if (addsub_imm <= 127) {
                            emit_mem_op(b, 0x83, group, (uint32_t)off);
                            buf_u8(b, (uint8_t)addsub_imm);
                        } else {
                            emit_mem_op(b, 0x81, group, (uint32_t)off);
                            buf_u32(b, (uint32_t)addsub_imm);
                        }
                    }
                } else if (local_addsub_var) {
                    uint64_t off = 0;
                    map_get_str(&g->locals, s->exprs[0]->str, &off);
                    uint8_t rhs_reg = 0;
                    if (reg_local_lookup(g, addsub_var, &rhs_reg)) {
                        emit_mov_reg_to_reg(b, rhs_reg, 0);
                    } else {
                        compile_expr(g, s->e1->b, 0, 0);
                    }
                    emit_mem_op(b, addsub_var_sub ? 0x29 : 0x01, 0, (uint32_t)off);
                } else if (!copied && is_xmm_bound_f64_local(g, s->exprs[0], &target_xmm)) {
                    /* Reassign an f64 local kept in an XMM register. The RHS
                     * may read the same register; compile it before writing. */
                    compile_float_to_xmm0(g, s->e1);
                    emit_movapd_xmm0_to_xmm(b, target_xmm);
                } else if (!copied && direct_float_store) {
                    /* Keep the f64 value in xmm0, compute the destination in
                     * rbx, and store with movsd. This removes the rax/r10
                     * save-restore used by the generic integer store path. */
                    compile_float_to_xmm0(g, s->e1);
                    if (s->exprs[0]->tag == 3) {
                        uint64_t off = 0;
                        map_get_str(&g->locals, s->exprs[0]->str, &off);
                        buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, 0x11);
                        buf_u8(b, off <= 127 ? 0x45 : 0x85);
                        if (off <= 127) buf_u8(b, (uint8_t)(-(int32_t)off));
                        else buf_u32(b, (uint32_t)(-(int32_t)off));
                    } else {
                        uint8_t sib_base = 0, sib_index = 0;
                        int sib_scale = -1;
                        if (index_sib_ready(g, s->exprs[0], &sib_base,
                                            &sib_index, &sib_scale)) {
                            emit_movsd_sib_store(b, sib_base, sib_index, sib_scale);
                        } else {
                            compile_address(g, s->exprs[0], 3);
                            buf_u8(b, 0xF2); buf_u8(b, 0x0F); buf_u8(b, 0x11); buf_u8(b, 0x03);
                        }
                    }
                } else if (!copied) {
                    WExpr* tgt = s->exprs[0];
                    int addr_first = tgt && tgt->tag == 10 &&
                        simple_left(g, tgt->b) && rax_only_expr(g, s->e1) &&
                        !expr_has_call(g, s->e1);
                    if (addr_first) {
                        /* Compute the address into rbx first, then compile
                         * the value into rax. The value never touches rbx,
                         * so no r10 save/restore is needed. */
                        compile_address(g, tgt, 3);
                        uint32_t store_size = expr_size(g, tgt);
                        if (store_size == 1 && s->e1->tag == 0 &&
                            s->e1->num <= 255) {
                            /* movb $imm,(%rbx) avoids a byte register load. */
                            buf_u8(b, 0xC6); buf_u8(b, 0x03);
                            buf_u8(b, (uint8_t)s->e1->num);
                        } else {
                            compile_expr(g, s->e1, 0, 0);
                            switch (store_size) {
                                case 1: buf_u8(b, 0x88); buf_u8(b, 0x03); break;
                                case 2: buf_u8(b, 0x66); buf_u8(b, 0x89); buf_u8(b, 0x03); break;
                                case 4: buf_u8(b, 0x89); buf_u8(b, 0x03); break;
                                default: buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0x03); break;
                            }
                        }
                    } else {
                        compile_assignment_value(g, tgt, s->e1);
                        store_assignment_target_from_rax(g, tgt);
                    }
                }
                WExpr* target = s->exprs[0];
                if (target->tag == 3) {
                    uint64_t access = 3;
                    if (map_get_str(&g->local_access, target->str, &access) &&
                        (access == 4 || access == 5)) {
                        buf_u8(b, 0x0F); buf_u8(b, 0xAE); buf_u8(b, 0xF0);
                    }
                } else if (copied && is_volatile_member_expr(g, target)) {
                    buf_u8(b, 0x0F); buf_u8(b, 0xAE); buf_u8(b, 0xF0);
                } else if (copied && target->tag == 7) {
                    WStr key = wstr_pair(target->str, target->str2);
                    if (map_has(&g->section_volatile, key.ptr, key.len)) {
                        buf_u8(b, 0x0F); buf_u8(b, 0xAE); buf_u8(b, 0xF0);
                    }
                }
            } else if (s->expr_count > 0) {
                compile_assignment_value(g, s->exprs[0], s->e1);
                store_assignment_target_from_rax(g, s->exprs[0]);
                if (s->exprs[0]->tag == 3) {
                    uint64_t access = 3;
                    if (map_get_str(&g->local_access, s->exprs[0]->str, &access) &&
                        (access == 4 || access == 5)) {
                        buf_u8(b, 0x0F); buf_u8(b, 0xAE); buf_u8(b, 0xF0);
                    }
                }
                static const uint8_t moves[3][3] = {
                    {0x48, 0x89, 0xD0}, {0x48, 0x89, 0xC8}, {0x4C, 0x89, 0xC0}
                };
                for (size_t i = 1; i < s->expr_count && i <= 3; i++) {
                    buf_bytes(b, moves[i - 1], 3);
                    store_assignment_target_from_rax(g, s->exprs[i]);
                }
            }
            break;
        }
        case 2: { /* If */
            size_t false_patch = compile_condition_jump(g, s->e1);
            for (size_t i = 0; i < s->stmt1_count; i++) compile_stmt(g, s->stmts1[i]);
            if (s->flag) {
                buf_u8(b, 0xE9);
                size_t end_patch = b->len;
                buf_u32(b, 0);
                patch_address(g, false_patch, (int32_t)(b->len - (false_patch + 4)));
                for (size_t i = 0; i < s->stmt2_count; i++) compile_stmt(g, s->stmts2[i]);
                patch_address(g, end_patch, (int32_t)(b->len - (end_patch + 4)));
            } else {
                patch_address(g, false_patch, (int32_t)(b->len - (false_patch + 4)));
            }
            break;
        }
        case 3: { /* While */
            size_t gpr_base = g->reg_locals.n;
            RegLocalVec gpr_bindings = {0};
            if (!expr_has_call_deep(s->e1) &&
                !stmt_unsafe_for_gpr_tree(s->stmts1, s->stmt1_count)) {
                uint8_t next_gpr = 8;
                WStr skip = {0};
                const uint8_t ptr_regs[2] = {8, 12};
                for (size_t pi = 0; pi < 2; pi++) {
                    WStr ptr_name = {0};
                    if (try_bind_invariant_pointer(g, s->stmts1, s->stmt1_count,
                                                   ptr_regs[pi], skip, &ptr_name)) {
                        RegLocal r = {0};
                        r.name = ptr_name;
                        r.reg = ptr_regs[pi];
                        RegLocalVec_push(&gpr_bindings, r);
                        skip = ptr_name;
                        if (ptr_regs[pi] == 8) next_gpr = 9;
                    }
                }
                for (size_t i = 0; i < s->stmt1_count; i++) {
                    collect_gpr_loop_locals(g, s->stmts1[i], &gpr_bindings,
                                            &next_gpr);
                }
                for (size_t i = 0; i < gpr_bindings.n; i++) {
                    uint64_t off = 0;
                    if (map_get_str(&g->locals, gpr_bindings.v[i].name, &off)) {
                        emit_mem_load(b, gpr_bindings.v[i].reg, (uint32_t)off, 8, 0);
                    }
                    RegLocalVec_push(&g->reg_locals, gpr_bindings.v[i]);
                }
            }
            size_t loop_start = b->len;
            /* A backedge re-enters at loop_start without re-executing the
             * store that fed the rax-store peephole. Never elide the
             * condition's local load at a loop header. */
            g->last_rax_store_len = 0;
            LoopCtx ctx;
            memset(&ctx, 0, sizeof(ctx));
            ctx.match_depth = g->match_depth;
            LoopCtxVec_push(&g->loop_stack, ctx);
            size_t exit_patch = compile_condition_jump(g, s->e1);
            for (size_t i = 0; i < s->stmt1_count; i++) compile_stmt(g, s->stmts1[i]);
            ctx = g->loop_stack.v[g->loop_stack.n - 1];
            g->loop_stack.n--;
            for (size_t i = 0; i < ctx.cont_n; i++) {
                patch_address(g, ctx.cont[i], (int32_t)(loop_start - (ctx.cont[i] + 4)));
            }
            int32_t jmp = (int32_t)loop_start - (int32_t)(b->len + 5);
            buf_u8(b, 0xE9);
            buf_i32(b, jmp);
            /* Exits must enter the spill block, not jump past it. */
            size_t after_loop = b->len;
            if (gpr_bindings.n) {
                spill_gpr_loop_locals(g, &gpr_bindings);
                g->reg_locals.n = gpr_base;
            }
            patch_address(g, exit_patch, (int32_t)(after_loop - (exit_patch + 4)));
            for (size_t i = 0; i < ctx.brk_n; i++) {
                patch_address(g, ctx.brk[i], (int32_t)(after_loop - (ctx.brk[i] + 4)));
            }
            break;
        }
        case 4: { /* For */
            /* Declare the induction variable first so try_bind_for_reg can
             * find it in g->locals. */
            if (s->s1) compile_stmt(g, s->s1);
            uint8_t loop_reg = 0;
            WStr loop_name = {0};
            WExpr* loop_init_expr = NULL;
            int reg_loop = try_bind_for_reg(g, s, &loop_reg, &loop_name,
                                            &loop_init_expr);
            uint64_t loop_off = 0;
            if (reg_loop) {
                map_get_str(&g->locals, loop_name, &loop_off);
                if (loop_init_expr->tag == 0) {
                    emit_mov_imm64(b, loop_reg, loop_init_expr->num);
                } else {
                    compile_expr(g, loop_init_expr, 0, 0);
                    emit_mov_reg_to_reg(b, 0, loop_reg);
                }
                emit_mov_reg_to_local(b, loop_reg, (uint32_t)loop_off);
                RegLocal rl = {0};
                rl.name = loop_name;
                rl.reg = loop_reg;
                RegLocalVec_push(&g->reg_locals, rl);
            }
            size_t gpr_base = g->reg_locals.n;
            RegLocalVec gpr_bindings = {0};
            if (!expr_has_call_deep(s->e1) &&
                (!s->s2 || !stmt_has_call_deep(s->s2)) &&
                !stmt_unsafe_for_gpr_tree(s->stmts1, s->stmt1_count)) {
                uint8_t next_gpr = 8;
                WStr skip = {0};
                const uint8_t ptr_regs[2] = {8, 12};
                for (size_t pi = 0; pi < 2; pi++) {
                    WStr ptr_name = {0};
                    if (try_bind_invariant_pointer(g, s->stmts1, s->stmt1_count,
                                                   ptr_regs[pi], skip, &ptr_name)) {
                        RegLocal r = {0};
                        r.name = ptr_name;
                        r.reg = ptr_regs[pi];
                        RegLocalVec_push(&gpr_bindings, r);
                        skip = ptr_name;
                        if (ptr_regs[pi] == 8) next_gpr = 9;
                    }
                }
                for (size_t i = 0; i < s->stmt1_count; i++) {
                    collect_gpr_loop_locals(g, s->stmts1[i], &gpr_bindings,
                                            &next_gpr);
                }
                for (size_t i = 0; i < gpr_bindings.n; i++) {
                    uint64_t off = 0;
                    if (map_get_str(&g->locals, gpr_bindings.v[i].name, &off)) {
                        emit_mem_load(b, gpr_bindings.v[i].reg, (uint32_t)off, 8, 0);
                    }
                    RegLocalVec_push(&g->reg_locals, gpr_bindings.v[i]);
                }
            }
            size_t xmm_base = g->xmm_locals.n;
            int xmm_loop = 0;
            XmmLocalVec xmm_bindings = {0};
            if (!expr_unsafe_for_xmm(s->e1) &&
                !stmt_unsafe_for_xmm(s->s2) &&
                !stmt_unsafe_for_xmm_tree(s->stmts1, s->stmt1_count)) {
                uint8_t next_xmm = 8;
                for (size_t i = 0; i < s->stmt1_count; i++) {
                    collect_xmm_loop_locals(g, s->stmts1[i], &xmm_bindings,
                                            &next_xmm);
                }
                if (xmm_bindings.n) {
                    for (size_t i = 0; i < xmm_bindings.n; i++) {
                        XmmLocalVec_push(&g->xmm_locals, xmm_bindings.v[i]);
                    }
                    xmm_loop = 1;
                }
            }
            size_t loop_start = b->len;
            /* Same rax-store hazard as a while loop: the backedge jumps here
             * directly, so the condition must always reload its locals. */
            g->last_rax_store_len = 0;
            LoopCtx ctx;
            memset(&ctx, 0, sizeof(ctx));
            ctx.match_depth = g->match_depth;
            LoopCtxVec_push(&g->loop_stack, ctx);
            size_t exit_patch = compile_condition_jump(g, s->e1);
            for (size_t i = 0; i < s->stmt1_count; i++) compile_stmt(g, s->stmts1[i]);
            size_t post_pos = b->len;
            if (s->s2) compile_stmt(g, s->s2);
            ctx = g->loop_stack.v[g->loop_stack.n - 1];
            g->loop_stack.n--;
            for (size_t i = 0; i < ctx.cont_n; i++) {
                patch_address(g, ctx.cont[i], (int32_t)(post_pos - (ctx.cont[i] + 4)));
            }
            int32_t jmp = (int32_t)loop_start - (int32_t)(b->len + 5);
            buf_u8(b, 0xE9);
            buf_i32(b, jmp);
            /* Exits must enter the spill block, not jump past it. */
            size_t after_loop = b->len;
            if (gpr_bindings.n) {
                spill_gpr_loop_locals(g, &gpr_bindings);
                g->reg_locals.n = gpr_base;
            }
            if (reg_loop) {
                emit_mov_reg_to_local(b, loop_reg, (uint32_t)loop_off);
                g->reg_locals.n--;
            }
            if (xmm_loop) {
                spill_xmm_loop_locals(g, &xmm_bindings);
                g->xmm_locals.n = xmm_base;
            }
            patch_address(g, exit_patch, (int32_t)(after_loop - (exit_patch + 4)));
            for (size_t i = 0; i < ctx.brk_n; i++) {
                patch_address(g, ctx.brk[i], (int32_t)(after_loop - (ctx.brk[i] + 4)));
            }
            break;
        }
        case 5: { /* Jmpto */
            uint64_t module_off;
            if (!s->flag && map_get_str(&g->locals, s->name, &module_off)) {
                compile_dynamic_jmpto(g, s, 1);
                break;
            }
            if (s->flag) {
                compile_inline_jmpto(g, s);
                break;
            }
            compile_dynamic_jmpto(g, s, 0);
            break;
        }
        case 6: { /* Return */
            size_t n = s->return_count < 4 ? s->return_count : 4;
            if (n <= 1) {
                /* Preserve the single-value path exactly. */
                if (n == 1) {
                    compile_expr(g, s->returns[0].expr, 0,
                                 return_deref_flag(g, 0));
                }
            } else {
                /* Multi-value returns use rax, rdx, rcx, r8. compile_expr
                 * leaves each result in rax and may clobber rdx/rcx/r8 (for
                 * example when evaluating a call with many arguments), so
                 * compiling values in source order loses earlier ones:
                 * return(7, 9) used to emit mov $7,rax; mov $9,rax;
                 * mov rax,rdx, returning 9 in both slots.  Save each value
                 * on the stack as it is computed, then pop them in reverse
                 * order into the ABI registers.  The epilogue below resets
                 * rsp, so the temporary pushes need no explicit cleanup. */
                for (size_t i = 0; i < n; i++) {
                    compile_expr(g, s->returns[i].expr, 0,
                                 return_deref_flag(g, i));
                    buf_u8(b, 0x50); /* push rax */
                }
                for (size_t k = n; k > 0; k--) {
                    size_t i = k - 1;
                    if (i == 3) buf_u8(b, 0x41); /* REX.B: pop r8 */
                    switch (i) {
                        case 0: buf_u8(b, 0x58); break; /* pop rax */
                        case 1: buf_u8(b, 0x5A); break; /* pop rdx */
                        case 2: buf_u8(b, 0x59); break; /* pop rcx */
                        default: buf_u8(b, 0x58); break; /* pop r8 */
                    }
                }
            }
            buf_u8(b, 0x48); buf_u8(b, 0x8D); buf_u8(b, 0x65);
            buf_u8(b, g->current_is_irq ? 0xF8 : 0xF0);
            if (!g->current_is_irq) {
                buf_u8(b, 0x41); buf_u8(b, 0x5C); /* pop r12 */
            }
            buf_u8(b, 0x5B);
            buf_u8(b, 0x5D);
            emit_function_epilogue(g);
            break;
        }
        case 7: { /* Critical */
            if (!g->prog.use_os) {
                buf_u8(b, 0x9C);
                buf_u8(b, 0x58);
                buf_u8(b, 0x50);
                buf_u8(b, 0xFA);
            }
            for (size_t i = 0; i < s->stmt1_count; i++) compile_stmt(g, s->stmts1[i]);
            if (!g->prog.use_os) {
                buf_u8(b, 0x58);
                buf_u8(b, 0x9D);
            }
            break;
        }
        case 8: /* Nasm */
            compile_nasm(g, s->name);
            break;
        case 9: { /* Match */
            compile_expr(g, s->e1, 0, expr_deref_flag(g, s->e1, 1));
            buf_u8(b, 0x50); /* push rax - save scrutinee */
            g->match_depth++;
            SizeVec je_patches = {0};
            for (size_t i = 0; i < s->case_count; i++) {
                compile_expr(g, s->cases[i].expr, 0,
                             expr_deref_flag(g, s->cases[i].expr, 1));
                /* cmp qword [rsp], rax */
                buf_u8(b, 0x48); buf_u8(b, 0x39); buf_u8(b, 0x04); buf_u8(b, 0x24);
                buf_u8(b, 0x0F); buf_u8(b, 0x84);
                size_t p = b->len;
                buf_u32(b, 0);
                SizeVec_push(&je_patches, p);
            }
            buf_u8(b, 0xE9);
            size_t default_jmp = b->len;
            buf_u32(b, 0);
            SizeVec end_jmps = {0};
            for (size_t i = 0; i < s->case_count; i++) {
                patch_address(g, je_patches.v[i],
                              (int32_t)(b->len - (je_patches.v[i] + 4)));
                for (size_t j = 0; j < s->cases[i].body_count; j++) {
                    compile_stmt(g, s->cases[i].body[j]);
                }
                buf_u8(b, 0xE9);
                size_t p = b->len;
                buf_u32(b, 0);
                SizeVec_push(&end_jmps, p);
            }
            patch_address(g, default_jmp, (int32_t)(b->len - (default_jmp + 4)));
            for (size_t i = 0; i < s->stmt2_count; i++) compile_stmt(g, s->stmts2[i]);
            for (size_t i = 0; i < end_jmps.n; i++) {
                patch_address(g, end_jmps.v[i], (int32_t)(b->len - (end_jmps.v[i] + 4)));
            }
            buf_u8(b, 0x48); buf_u8(b, 0x83); buf_u8(b, 0xC4); buf_u8(b, 0x08); /* add rsp, 8 */
            g->match_depth--;
            break;
        }
        case 10: /* Expr */
            compile_expr(g, s->e1, 0, 1);
            break;
        case 11: { /* Continue */
            if (g->loop_stack.n) {
                LoopCtx* ctx = &g->loop_stack.v[g->loop_stack.n - 1];
                /* Jumping out of one or more `match` bodies skips their
                 * `add rsp,8`; restore those slots here. */
                if (g->match_depth > ctx->match_depth) {
                    uint8_t slots = (uint8_t)((g->match_depth - ctx->match_depth) * 8u);
                    buf_u8(b, 0x48); buf_u8(b, 0x83); buf_u8(b, 0xC4); buf_u8(b, slots);
                }
                buf_u8(b, 0xE9);
                size_t p = b->len;
                buf_u32(b, 0);
                SizeVec_push((SizeVec*)&ctx->cont, p);
            }
            break;
        }
        case 12: { /* Break */
            if (g->loop_stack.n) {
                LoopCtx* ctx = &g->loop_stack.v[g->loop_stack.n - 1];
                if (g->match_depth > ctx->match_depth) {
                    uint8_t slots = (uint8_t)((g->match_depth - ctx->match_depth) * 8u);
                    buf_u8(b, 0x48); buf_u8(b, 0x83); buf_u8(b, 0xC4); buf_u8(b, slots);
                }
                buf_u8(b, 0xE9);
                size_t p = b->len;
                buf_u32(b, 0);
                SizeVec_push((SizeVec*)&ctx->brk, p);
            }
            break;
        }
        default:
            break;
    }
}

/* ------------------------------------------------------------------ */
/* Program setup, layouts, functions and top-level driver              */
/* ------------------------------------------------------------------ */

static void build_relocatable_elf(Gen* g, Buf* global_data);

static void calculate_and_insert_layout(Gen* g, WStruct* s) {
    Layout layout;
    memset(&layout, 0, sizeof(layout));
    map_init(&layout.field_types, (uint32_t)(s->field_count * 2 + 8));
    map_init(&layout.field_offsets, (uint32_t)(s->field_count * 2 + 8));

    uint32_t declared_alignment = s->align ? s->align : 1;

    if (s->is_union) {
        uint32_t max_size = 0;
        uint32_t max_alignment = declared_alignment;
        for (size_t i = 0; i < s->field_count; i++) {
            WField* f = &s->fields[i];
            map_put_str(&layout.field_offsets, f->name, 0);
            map_put_str(&layout.field_types, f->name, PTR(f->type));
            uint32_t size = type_size(g, f->type);
            if (size > max_size) {
                max_size = size;
            }
            uint32_t alignment = s->is_packed ? 1 : type_alignment_of(g, f->type);
            if (!alignment) alignment = 1;
            if (alignment > 8) {
                alignment = 8;
            }
            if (alignment > max_alignment) {
                max_alignment = alignment;
            }
        }
        if (max_alignment && max_size % max_alignment != 0) {
            max_size += max_alignment - (max_alignment - (max_size % max_alignment));
        }
        layout.size = max_size;
        layout.align = max_alignment;
    } else {
        uint32_t current_offset = 0;
        uint32_t max_alignment = declared_alignment;
        for (size_t i = 0; i < s->field_count; i++) {
            WField* f = &s->fields[i];
            uint32_t size = type_size(g, f->type);
            uint32_t alignment = s->is_packed ? 1 : type_alignment_of(g, f->type);
            if (!alignment) alignment = 1;
            if (alignment > 8) {
                alignment = 8;
            }
            if (alignment > max_alignment) {
                max_alignment = alignment;
            }
            if (alignment && current_offset % alignment != 0) {
                current_offset += alignment - (current_offset % alignment);
            }
            map_put_str(&layout.field_offsets, f->name, current_offset);
            map_put_str(&layout.field_types, f->name, PTR(f->type));
            current_offset += size;
        }
        if (max_alignment && current_offset % max_alignment != 0) {
            current_offset += max_alignment - (current_offset % max_alignment);
        }
        layout.size = current_offset;
        layout.align = max_alignment;
    }

    uint64_t idx;
    if (map_get_str(&g->struct_layouts, s->name, &idx)) {
        g->layouts.v[idx] = layout;
    } else {
        idx = g->layouts.n;
        LayoutVec_push(&g->layouts, layout);
        map_put_str(&g->struct_layouts, s->name, idx);
    }
    map_put_str(&g->struct_alignments, s->name, layout.align);
}

/* Return the global-data key for an initializer that asks for an address.
 * `*adr` of a section variable becomes AddrOf("section:name"); a parenthesized
 * section access can become AddrOfExpr(SectionAccess). */
static int global_init_address_key(Gen* g, WExpr* e, WStr* key) {
    if (!e) {
        return 0;
    }
    if (e->tag == 4 && e->str.ptr) {
        for (uint32_t i = 0; i < e->str.len; i++) {
            if (e->str.ptr[i] == ':') {
                *key = e->str;
                return 1;
            }
        }
        WStr found = find_global_key(g, e->str);
        if (found.ptr) {
            *key = found;
            return 1;
        }
        return 0;
    }
    if (e->tag == 5 && e->a && e->a->tag == 7) {
        *key = wstr_pair(e->a->str, e->a->str2);
        return 1;
    }
    return 0;
}

static WStruct* find_struct_by_name(Gen* g, WStr name) {
    for (size_t i = 0; i < g->prog.struct_count; i++) {
        if (wstr_eq(g->prog.structs[i].name, name)) {
            return &g->prog.structs[i];
        }
    }
    return NULL;
}

static WVar* global_var_by_key(Gen* g, WStr key) {
    for (size_t i = 0; i < g->prog.section_count; i++) {
        WSection* sect = &g->prog.sections[i];
        for (size_t j = 0; j < sect->count; j++) {
            if (wstr_eq(wstr_pair(sect->name, sect->vars[j].name), key)) {
                return &sect->vars[j];
            }
        }
    }
    return NULL;
}

/* Resolve a bare variable or section access to the key of the object itself.
 * A byte array initialized by a string literal must name the array, not the
 * separate string-table copy. */
static int global_object_key(Gen* g, WExpr* e, WStr* key) {
    WStr candidate = {0};
    if (e->tag == 7 && e->str.ptr && e->str2.ptr) {
        candidate = wstr_pair(e->str, e->str2);
    } else if (e->tag == 3 && e->str.ptr) {
        candidate = find_global_key(g, e->str);
    }
    if (!candidate.ptr || !global_var_by_key(g, candidate)) {
        return 0;
    }
    *key = candidate;
    return 1;
}

/* Resolve an address-valued initializer. A bare pointer global is followed to
 * its own initializer, so `Holder h = { q }` and `u64* r = q` reuse the same
 * constant address as `q = arr*adr` instead of storing null. Any other object
 * names its own address, so `u64* p = arr` and `u8* p = arr` point at the
 * array rather than at a string-table copy. */
static int global_pointer_init_key_depth(Gen* g, WExpr* e, WStr* key,
                                         uint32_t depth) {
    if (!e || depth > 16) {
        return 0;
    }
    if (e->tag == 2) {
        collect_expr_literals(g, e, 1);
        *key = wstr_prefix("str:", e->str);
        return 1;
    }
    if (global_init_address_key(g, e, key)) {
        return 1;
    }
    WStr obj_key = {0};
    if (global_object_key(g, e, &obj_key)) {
        WVar* var = global_var_by_key(g, obj_key);
        WType* vt = resolve_named_type(g, var->type);
        if (vt && vt->tag == 10 && type_size(g, vt) == 8 &&
            var->has_init && var->init) {
            return global_pointer_init_key_depth(g, var->init, key, depth + 1);
        }
        *key = obj_key;
        return 1;
    }
    return 0;
}

static int global_pointer_init_key(Gen* g, WExpr* e, WStr* key) {
    return global_pointer_init_key_depth(g, e, key, 0);
}

typedef struct {
    size_t off; /* byte offset inside the variable */
    WType* type;
} InitLeaf;
DEFINE_VEC(InitLeafVec, InitLeaf)

/* Flatten an aggregate into scalar leaves in declaration order. Arrays use
 * element order; structs use field order and the calculated field offsets. */
static void collect_init_leaves(Gen* g, WType* t, size_t base,
                                InitLeafVec* leaves, uint32_t depth) {
    if (!t || depth > 16) {
        return;
    }
    WType* peeled = resolve_named_type(g, t);
    if (!peeled) {
        return;
    }
    if (peeled->tag == 12) {
        uint32_t elem_size = type_size(g, peeled->inner);
        if (!elem_size) elem_size = 1;
        for (uint32_t i = 0; i < peeled->count; i++) {
            collect_init_leaves(g, peeled->inner, base + (size_t)i * elem_size,
                                leaves, depth + 1);
        }
        return;
    }
    if (peeled->tag == 11) {
        WStruct* st = find_struct_by_name(g, peeled->name);
        uint64_t layout_idx = 0;
        Layout* layout = map_get_str(&g->struct_layouts, peeled->name,
                                     &layout_idx)
                             ? &g->layouts.v[layout_idx]
                             : NULL;
        size_t field_count = st ? (st->is_union ? 1 : st->field_count) : 0;
        for (size_t i = 0; i < field_count; i++) {
            uint64_t off = 0;
            if (layout) {
                map_get_str(&layout->field_offsets, st->fields[i].name, &off);
            }
            collect_init_leaves(g, st->fields[i].type, base + off, leaves,
                                depth + 1);
        }
        return;
    }
    InitLeaf leaf;
    leaf.off = base;
    leaf.type = peeled;
    InitLeafVec_push(leaves, leaf);
}

static size_t emit_global_leaf(Gen* g, Buf* global, size_t var_start,
                               uint32_t var_size, InitLeaf* leaves,
                               size_t leaf_count, size_t pos, WExpr* arg,
                               size_t max_leaves) {
    if (!arg) {
        return pos;
    }
    if (!leaves || pos >= leaf_count) {
        return pos + 1;
    }
    InitLeaf* leaf = &leaves[pos];
    if (leaf->off >= var_size) {
        return pos + 1;
    }
    WType* t = resolve_named_type(g, leaf->type);
    uint32_t width = t ? type_size(g, t) : 8;
    if (!width) width = 1;
    if (width > var_size - leaf->off) {
        width = (uint32_t)(var_size - leaf->off);
    }
    size_t at = var_start + leaf->off;
    /* A string literal assigned to a byte leaf fills consecutive byte leaves,
     * including the NUL and any remaining array slots. Stop at the declared
     * array so the next struct field keeps its own initializer. This covers
     * `Rec rec = { 3, "hi" }` and whole byte arrays. */
    if (arg->tag == 2 && t && (t->tag == 0 || t->tag == 4)) {
        Buf bytes = {0};
        unescape_string(arg->str, &bytes);
        size_t limit = max_leaves ? max_leaves : leaf_count - pos;
        if (limit > leaf_count - pos) {
            limit = leaf_count - pos;
        }
        size_t consumed = 0;
        while (consumed < limit) {
            InitLeaf* l = &leaves[pos + consumed];
            WType* lt = resolve_named_type(g, l->type);
            if (!lt || (lt->tag != 0 && lt->tag != 4) ||
                l->off != leaf->off + consumed) {
                break;
            }
            size_t dest = var_start + l->off;
            uint8_t value = consumed < bytes.len
                                ? (uint8_t)bytes.data[consumed]
                                : 0;
            if (dest < var_start + var_size) {
                global->data[dest] = value;
            }
            consumed++;
        }
        return pos + (consumed ? consumed : 1);
    }
    if (t && t->tag == 10 && width == 8) {
        WStr key = {0};
        if (global_pointer_init_key(g, arg, &key)) {
            GlobalPtrFixup fixup;
            fixup.pos = at;
            fixup.key = key;
            GlobalPtrFixupVec_push(&g->global_ptr_fixups, fixup);
            return pos + 1;
        }
    }
    uint64_t value = eval_const(g, arg);
    for (uint32_t byte = 0; byte < width; byte++) {
        global->data[at + byte] = byte < 8 ? (uint8_t)(value >> (byte * 8)) : 0;
    }
    return pos + 1;
}

/* Return the element count when t is a byte array, including named typedefs.
 * Zero means the type is not a byte array. */
static uint32_t byte_array_count(Gen* g, WType* t) {
    WType* peeled = resolve_named_type(g, t);
    if (!peeled || peeled->tag != 12 || !peeled->count) {
        return 0;
    }
    WType* elem = resolve_named_type(g, peeled->inner);
    if (!elem || (elem->tag != 0 && elem->tag != 4)) {
        return 0;
    }
    return (uint32_t)peeled->count;
}

/* Number of characters a string literal contributes as an initializer leaf.
 * The terminal NUL is supplied by whole-array/zero-fill handling, not here. */
static uint32_t string_char_count(WExpr* e) {
    if (!e || e->tag != 2) {
        return 0;
    }
    Buf bytes = {0};
    unescape_string(e->str, &bytes);
    return bytes.len ? (uint32_t)bytes.len : 1;
}

static size_t leaf_count_for_type(Gen* g, WType* t) {
    InitLeafVec tmp = {0};
    collect_init_leaves(g, t, 0, &tmp, 0);
    return tmp.n;
}

/* Find the first byte array reachable through brace elision inside an
 * aggregate type. A string initializer for an aggregate selects that byte
 * array; *prefix is the number of leaves before it and *count is its declared
 * byte count. Zero means the aggregate has no byte-array leaf. */
static int first_byte_array_span(Gen* g, WType* t, size_t* prefix,
                                 uint32_t* count, uint32_t depth) {
    if (!g || !t || !prefix || !count || depth > 16) {
        return 0;
    }
    WType* peeled = resolve_named_type(g, t);
    if (!peeled) {
        return 0;
    }
    uint32_t byte_count = byte_array_count(g, peeled);
    if (byte_count) {
        *prefix = 0;
        *count = byte_count;
        return 1;
    }
    if (peeled->tag == 11) {
        WStruct* st = find_struct_by_name(g, peeled->name);
        size_t fields = st ? (st->is_union ? 1 : st->field_count) : 0;
        size_t before = 0;
        for (size_t i = 0; i < fields; i++) {
            size_t sub_prefix = 0;
            uint32_t sub_count = 0;
            if (first_byte_array_span(g, st->fields[i].type, &sub_prefix,
                                      &sub_count, depth + 1)) {
                *prefix = before + sub_prefix;
                *count = sub_count;
                return 1;
            }
            before += leaf_count_for_type(g, st->fields[i].type);
        }
        return 0;
    }
    if (peeled->tag == 12 && peeled->count) {
        size_t elem_leaves = leaf_count_for_type(g, peeled->inner);
        size_t before = 0;
        for (uint32_t i = 0; i < peeled->count; i++) {
            size_t sub_prefix = 0;
            uint32_t sub_count = 0;
            if (first_byte_array_span(g, peeled->inner, &sub_prefix,
                                      &sub_count, depth + 1)) {
                *prefix = before + sub_prefix;
                *count = sub_count;
                return 1;
            }
            before += elem_leaves;
        }
    }
    return 0;
}

/* `Struct arr[1] = { {field0}, field1, ... }` puts one struct element's field
 * initializers at the top level when the first field is itself an aggregate
 * and the first argument is a brace or string. A scalar first field must stay
 * flat: `Pair p[1] = {{1,2},{3,4}}` initializes p[0] from the first brace and
 * ignores the excess second brace, rather than treating both braces as fields.
 * Likewise `{ 1, 2 }` fills leaves in declaration order. */
static int is_single_struct_field_list(Gen* g, WType* t, WExpr* init) {
    if (!t || !init || init->tag != 12 || !init->arg_count) {
        return 0;
    }
    WType* peeled = resolve_named_type(g, t);
    if (!peeled || peeled->tag != 12 || peeled->count != 1) {
        return 0;
    }
    WType* inner = resolve_named_type(g, peeled->inner);
    if (!inner || inner->tag != 11) {
        return 0;
    }
    WStruct* st = find_struct_by_name(g, inner->name);
    if (!st || !st->field_count) {
        return 0;
    }
    WType* first_field = resolve_named_type(g, st->fields[0].type);
    if (!first_field || (first_field->tag != 11 && first_field->tag != 12)) {
        return 0;
    }
    WExpr* first = init->args[0];
    return first && (first->tag == 12 || first->tag == 2);
}

/* ------------------------------------------------------------------ */
/* Runtime aggregate initialization                                   */
/* ------------------------------------------------------------------ */

typedef struct {
    int use_rbx;     /* destination is [rbx] instead of [rbp-base] */
    uint32_t base;   /* rbp displacement of the object start (local mode) */
    uint32_t size;   /* total declared object size */
    uint32_t used;   /* highest byte offset written so far */
} RuntimeInitCtx;

static void zero_runtime_range(Gen* g, RuntimeInitCtx* ctx, uint32_t end);

static uint32_t runtime_leaf_width(Gen* g, WType* t) {
    WType* p = resolve_named_type(g, t);
    uint32_t w = p ? type_size(g, p) : 0;
    if (!w) w = 1;
    if (w > 8) w = 8;
    return w;
}

static void emit_runtime_leaf_store(Gen* g, RuntimeInitCtx* ctx, InitLeaf* leaf,
                                    WExpr* arg) {
    if (!ctx || !leaf || !arg) {
        return;
    }
    Buf* b = &g->code;
    uint32_t width = runtime_leaf_width(g, leaf->type);
    if (leaf->off >= ctx->size) {
        return;
    }
    if (width > ctx->size - leaf->off) {
        width = ctx->size - leaf->off;
    }
    if (!width) {
        return;
    }
    if (ctx->use_rbx) {
        /* Leaf expression codegen may use rbx as scratch. Track the extra
         * push so calls inside the initializer still see 16-byte alignment. */
        buf_u8(b, 0x53); /* push rbx */
        g->stack_depth += 8;
        compile_expr(g, arg, 0, 1);
        g->stack_depth -= 8;
        buf_u8(b, 0x5B); /* pop rbx */
        emit_rbx_mem_store(b, leaf->off, width);
    } else {
        if (leaf->off > ctx->base) {
            return;
        }
        compile_expr(g, arg, 0, 1);
        emit_mem_store(b, 0, ctx->base - leaf->off, width);
    }
    uint32_t end = leaf->off + width;
    if (end > ctx->used) {
        ctx->used = end;
    }
}

/* Fill consecutive byte leaves with a string, including its NUL. */
static size_t emit_runtime_string_leaves(Gen* g, RuntimeInitCtx* ctx,
                                         InitLeaf* leaves, size_t leaf_count,
                                         size_t pos, WExpr* arg,
                                         uint32_t max_count) {
    if (!arg || pos >= leaf_count || !max_count) {
        return pos;
    }
    Buf* b = &g->code;
    Buf bytes = {0};
    unescape_string(arg->str, &bytes);
    size_t limit = max_count;
    if (limit > leaf_count - pos) {
        limit = leaf_count - pos;
    }
    size_t first_off = leaves[pos].off;
    size_t consumed = 0;
    while (consumed < limit) {
        InitLeaf* leaf = &leaves[pos + consumed];
        WType* lt = resolve_named_type(g, leaf->type);
        if (!lt || (lt->tag != 0 && lt->tag != 4) ||
            leaf->off != first_off + consumed || leaf->off >= ctx->size) {
            break;
        }
        uint8_t value = consumed < bytes.len ? (uint8_t)bytes.data[consumed] : 0;
        emit_mov_imm64(b, 0, value);
        if (ctx->use_rbx) {
            emit_rbx_mem_store(b, leaf->off, 1);
        } else {
            if (leaf->off > ctx->base) {
                break;
            }
            emit_mem_store(b, 0, ctx->base - leaf->off, 1);
        }
        if (leaf->off + 1 > ctx->used) {
            ctx->used = leaf->off + 1;
        }
        consumed++;
    }
    return pos + (consumed ? consumed : 1);
}

static int is_aggregate_lvalue_expr(WExpr* e) {
    return e && (e->tag == 3 || e->tag == 6 || e->tag == 7 || e->tag == 10);
}

/* Copy a whole struct/array lvalue into the destination at the current leaf. */
static size_t emit_runtime_aggregate_copy(Gen* g, RuntimeInitCtx* ctx, WType* t,
                                          InitLeaf* leaves, size_t leaf_count,
                                          size_t pos, WExpr* arg) {
    if (!is_aggregate_lvalue_expr(arg) || pos >= leaf_count) {
        return pos;
    }
    WType* peeled = resolve_named_type(g, t);
    if (!peeled || (peeled->tag != 11 && peeled->tag != 12)) {
        return pos;
    }
    uint32_t total = type_size(g, t);
    uint32_t at = leaves[pos].off;
    if (!total || at >= ctx->size) {
        return pos;
    }
    /* Copy only the source object's size; a smaller source leaves the tail to
     * the caller's zero-fill instead of reading past the source. */
    WType* src_type = resolve_named_type(g, resolve_expr_type(g, arg));
    uint32_t src_total = src_type ? type_size(g, src_type) : 0;
    if (src_total && src_total < total) {
        total = src_total;
    }
    uint32_t n = total <= ctx->size - at ? total : ctx->size - at;
    Buf* b = &g->code;
    if (ctx->use_rbx) {
        buf_u8(b, 0x53); /* push destination base */
        g->stack_depth += 8;
        compile_address(g, arg, 3);
        g->stack_depth -= 8;
        buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xDE); /* mov %rbx,%rsi */
        buf_u8(b, 0x5F); /* pop %rdi */
        if (at) {
            buf_u8(b, 0x48); buf_u8(b, 0x81); buf_u8(b, 0xC7);
            buf_u32(b, at); /* add $at,%rdi */
        }
    } else {
        if (at > ctx->base) {
            return pos;
        }
        compile_address(g, arg, 3);
        buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xDE); /* mov %rbx,%rsi */
        emit_mem_op(b, 0x8D, 7, ctx->base - at); /* lea [rbp-disp],%rdi */
    }
    emit_mov_imm64(b, 1, n);
    buf_u8(b, 0xFC);
    buf_u8(b, 0xF3); buf_u8(b, 0xA4);
    if (at + n > ctx->used) {
        ctx->used = at + n;
    }
    InitLeafVec tmp = {0};
    collect_init_leaves(g, src_type && type_size(g, src_type) ? src_type : t,
                        0, &tmp, 0);
    size_t advance = tmp.n ? tmp.n : 1;
    if (advance > leaf_count - pos) {
        advance = leaf_count - pos;
    }
    return pos + advance;
}

/* Store a flat list of scalar leaves, expanding nested braces. This covers
 * `u8 n[4] = {{1,2},{3,4}}` and pointer-array string leaves. */
static size_t emit_runtime_flat_args(Gen* g, RuntimeInitCtx* ctx,
                                     InitLeaf* leaves, size_t leaf_count,
                                     size_t pos, WExpr* init, uint32_t depth) {
    if (!init || pos >= leaf_count || depth > 16) {
        return pos;
    }
    if (init->tag == 12) {
        for (size_t i = 0; i < init->arg_count && pos < leaf_count; i++) {
            pos = emit_runtime_flat_args(g, ctx, leaves, leaf_count, pos,
                                         init->args[i], depth + 1);
        }
        return pos;
    }
    if (init->tag == 2) {
        WType* lt = resolve_named_type(g, leaves[pos].type);
        if (lt && (lt->tag == 0 || lt->tag == 4)) {
            /* A nested string contributes exactly its characters; the final
             * tail zero-fill supplies the NUL. Only a string that initializes
             * the whole byte array gets its own NUL, and that case is handled
             * by byte_array_count before this flat path. */
            return emit_runtime_string_leaves(g, ctx, leaves, leaf_count, pos,
                                              init, string_char_count(init));
        }
    }
    emit_runtime_leaf_store(g, ctx, &leaves[pos], init);
    return pos + 1;
}

static size_t emit_runtime_init_expr(Gen* g, RuntimeInitCtx* ctx, WType* t,
                                     WExpr* init, InitLeaf* leaves,
                                     size_t leaf_count, size_t pos,
                                     uint32_t depth) {
    if (!init || !leaves || pos >= leaf_count || depth > 16) {
        return pos;
    }
    size_t after = emit_runtime_aggregate_copy(g, ctx, t, leaves, leaf_count,
                                               pos, init);
    if (after != pos) {
        return after;
    }
    WType* peeled = resolve_named_type(g, t);
    if (init->tag == 2) {
        uint32_t byte_count = byte_array_count(g, t);
        if (byte_count) {
            return emit_runtime_string_leaves(g, ctx, leaves, leaf_count, pos,
                                              init, byte_count);
        }
        /* Brace elision: a string assigned to an aggregate initializes the
         * first byte array inside it, not the aggregate's address. */
        size_t prefix = 0;
        uint32_t first_count = 0;
        if (first_byte_array_span(g, t, &prefix, &first_count, 0)) {
            size_t start = pos + prefix;
            if (start >= leaf_count) {
                return pos;
            }
            /* The final tail fill only covers bytes at or above ctx->used,
             * so clear any fields/padding before the selected byte array. */
            zero_runtime_range(g, ctx, (uint32_t)leaves[start].off);
            return emit_runtime_string_leaves(g, ctx, leaves, leaf_count, start,
                                              init, first_count);
        }
    }
    if (init->tag == 12) {
        size_t sub_count = 0;
        WStruct* st = NULL;
        if (peeled && peeled->tag == 11) {
            st = find_struct_by_name(g, peeled->name);
            if (st) sub_count = st->is_union ? 1 : st->field_count;
        } else if (peeled && peeled->tag == 12) {
            sub_count = peeled->count;
        }
        if (peeled && peeled->tag == 12 && init->arg_count &&
            init->args[0] && init->args[0]->tag == 2) {
            uint32_t byte_count = byte_array_count(g, t);
            if (byte_count) {
                return emit_runtime_string_leaves(g, ctx, leaves, leaf_count,
                                                  pos, init->args[0], byte_count);
            }
        }
        if (peeled && peeled->tag == 12) {
            WType* inner = resolve_named_type(g, peeled->inner);
            int scalar_elem = inner && inner->tag != 11 && inner->tag != 12;
            /* `Bytes s[1] = {{"ABCD"},{"XYZ"}}` is one struct element whose
             * two field initializers were written as sibling braces. Treat
             * the whole brace as the element initializer instead of flattening
             * it into the parent array's leaves. */
            if (is_single_struct_field_list(g, t, init)) {
                return emit_runtime_init_expr(g, ctx, inner, init, leaves,
                                              leaf_count, pos, depth + 1);
            }
            if (scalar_elem || (sub_count && init->arg_count > sub_count)) {
                /* A flat list longer than the declared sub-aggregate fills
                 * leaves in order. This also covers `Outer arr[1] = {...}`
                 * where the element's nested field braces precede a tag. */
                return emit_runtime_flat_args(g, ctx, leaves, leaf_count, pos,
                                              init, depth + 1);
            }
        }
        if (peeled && peeled->tag == 11 && st) {
            /* A one-element brace list containing only a string uses the
             * same field-list rule as a bare string: it fills the first byte
             * array reachable through brace elision, not an earlier scalar. */
            if (init->arg_count == 1 && init->args[0] &&
                init->args[0]->tag == 2) {
                size_t prefix = 0;
                uint32_t first_count = 0;
                if (first_byte_array_span(g, t, &prefix, &first_count, 0)) {
                    size_t start = pos + prefix;
                    if (start < leaf_count) {
                        zero_runtime_range(g, ctx, (uint32_t)leaves[start].off);
                        return emit_runtime_string_leaves(g, ctx, leaves,
                                                          leaf_count, start,
                                                          init->args[0],
                                                          first_count);
                    }
                }
            }
            size_t count = st->is_union ? 1 : st->field_count;
            for (size_t i = 0; i < count && pos < leaf_count; i++) {
                WType* field_type = st->fields[i].type;
                size_t before = pos;
                size_t span = leaf_count_for_type(g, field_type);
                if (i < init->arg_count && init->args[i]) {
                    pos = emit_runtime_init_expr(g, ctx, field_type,
                                                 init->args[i], leaves,
                                                 leaf_count, pos, depth + 1);
                }
                /* Skipped or partially initialized leaves still occupy their
                 * declared offsets. Zero them now so the next field starts at
                 * its own leaf instead of the skipped one. */
                uint32_t start = (uint32_t)leaves[before].off;
                uint64_t field_bytes = type_size(g, field_type);
                uint64_t end = (uint64_t)start + field_bytes;
                zero_runtime_range(g, ctx, (uint32_t)end);
                size_t expected = before + span;
                if (expected > leaf_count) expected = leaf_count;
                if (pos < expected) pos = expected;
            }
            return pos;
        }
        if (peeled && peeled->tag == 12) {
            size_t count = peeled->count;
            for (size_t i = 0; i < count && pos < leaf_count; i++) {
                WType* elem_type = peeled->inner;
                size_t before = pos;
                size_t span = leaf_count_for_type(g, elem_type);
                if (i < init->arg_count && init->args[i]) {
                    pos = emit_runtime_init_expr(g, ctx, elem_type, init->args[i],
                                                 leaves, leaf_count, pos,
                                                 depth + 1);
                }
                uint32_t start = (uint32_t)leaves[before].off;
                uint64_t elem_bytes = type_size(g, elem_type);
                uint64_t end = (uint64_t)start + elem_bytes;
                zero_runtime_range(g, ctx, (uint32_t)end);
                size_t expected = before + span;
                if (expected > leaf_count) expected = leaf_count;
                if (pos < expected) pos = expected;
            }
            return pos;
        }
        if (init->arg_count) {
            emit_runtime_leaf_store(g, ctx, &leaves[pos], init->args[0]);
            return pos + 1;
        }
        return pos;
    }
    emit_runtime_leaf_store(g, ctx, &leaves[pos], init);
    return pos + 1;
}

/* Zero bytes that no supplied initializer leaf reached. This is needed before
 * advancing over omitted sub-aggregate leaves, otherwise a later field lands
 * on a skipped leaf instead of its own offset. */
static void zero_runtime_range(Gen* g, RuntimeInitCtx* ctx, uint32_t end) {
    Buf* b = &g->code;
    if (end > ctx->size) {
        end = ctx->size;
    }
    while (ctx->used < end) {
        emit_mov_imm64(b, 0, 0);
        if (ctx->use_rbx) {
            emit_rbx_mem_store(b, ctx->used, 1);
        } else {
            if (ctx->used > ctx->base) {
                break;
            }
            emit_mem_store(b, 0, ctx->base - ctx->used, 1);
        }
        ctx->used++;
    }
}

static uint32_t finish_runtime_init(Gen* g, RuntimeInitCtx* ctx) {
    zero_runtime_range(g, ctx, ctx->size);
    return ctx->used;
}

static uint32_t emit_runtime_init_rbx(Gen* g, WExpr* init, WType* type,
                                      uint32_t max_bytes) {
    if (!init || !type || !max_bytes) {
        return 0;
    }
    uint32_t size = type_size(g, type);
    if (!size || size > max_bytes) {
        size = max_bytes;
    }
    InitLeafVec leaves = {0};
    collect_init_leaves(g, type, 0, &leaves, 0);
    if (!leaves.n) {
        return 0;
    }
    RuntimeInitCtx ctx = {1, 0, size, 0};
    emit_runtime_init_expr(g, &ctx, type, init, leaves.v, leaves.n, 0, 0);
    return finish_runtime_init(g, &ctx);
}

static uint32_t emit_runtime_init_local(Gen* g, WExpr* init, WType* type,
                                        uint32_t off, uint32_t max_bytes) {
    if (!init || !type || !max_bytes) {
        return 0;
    }
    uint32_t size = type_size(g, type);
    if (!size || size > max_bytes) {
        size = max_bytes;
    }
    InitLeafVec leaves = {0};
    collect_init_leaves(g, type, 0, &leaves, 0);
    if (!leaves.n) {
        return 0;
    }
    RuntimeInitCtx ctx = {0, off, size, 0};
    emit_runtime_init_expr(g, &ctx, type, init, leaves.v, leaves.n, 0, 0);
    return finish_runtime_init(g, &ctx);
}

/* Flat global brace lists fill leaves in declaration order. Nested strings
 * contribute only their characters; the global data buffer is pre-zeroed, so
 * the terminal NUL and any skipped leaves stay zero. */
static size_t emit_global_flat_args(Gen* g, Buf* global, size_t var_start,
                                    uint32_t var_size, InitLeaf* leaves,
                                    size_t leaf_count, size_t pos, WExpr* init,
                                    uint32_t depth) {
    if (!init || pos >= leaf_count || depth > 16) {
        return pos;
    }
    if (init->tag == 12) {
        for (size_t i = 0; i < init->arg_count && pos < leaf_count; i++) {
            pos = emit_global_flat_args(g, global, var_start, var_size, leaves,
                                        leaf_count, pos, init->args[i],
                                        depth + 1);
        }
        return pos;
    }
    uint32_t max_leaves = 1;
    if (init->tag == 2) {
        WType* lt = resolve_named_type(g, leaves[pos].type);
        if (lt && (lt->tag == 0 || lt->tag == 4)) {
            max_leaves = string_char_count(init);
        }
    }
    return emit_global_leaf(g, global, var_start, var_size, leaves,
                            leaf_count, pos, init, max_leaves);
}

/* Consume initializer arguments against the flattened leaf list. Braces map
 * to their sub-aggregate; a flat list fills leaves in declaration order, which
 * is what `Pair ps[2] = { 11, 22, 33, 44 }` requires. */
static size_t emit_global_init_expr(Gen* g, Buf* global, size_t var_start,
                                    uint32_t var_size, WType* t, WExpr* init,
                                    InitLeaf* leaves, size_t leaf_count,
                                    size_t pos, uint32_t depth) {
    if (!init || !leaves || pos >= leaf_count || depth > 16) {
        return pos;
    }
    if (init->tag == 2) {
        uint32_t byte_count = byte_array_count(g, t);
        if (byte_count) {
            return emit_global_leaf(g, global, var_start, var_size, leaves,
                                    leaf_count, pos, init, byte_count);
        }
        size_t prefix = 0;
        uint32_t first_count = 0;
        if (first_byte_array_span(g, t, &prefix, &first_count, 0)) {
            size_t start = pos + prefix;
            if (start >= leaf_count) {
                return pos;
            }
            return emit_global_leaf(g, global, var_start, var_size, leaves,
                                    leaf_count, start, init, first_count);
        }
    }
    if (init->tag == 12) {
        WType* peeled = resolve_named_type(g, t);
        size_t sub_count = 0;
        WStruct* st = NULL;
        WType* inner = NULL;
        if (peeled && peeled->tag == 11) {
            st = find_struct_by_name(g, peeled->name);
            if (st) sub_count = st->is_union ? 1 : st->field_count;
        } else if (peeled && peeled->tag == 12) {
            sub_count = peeled->count;
            inner = resolve_named_type(g, peeled->inner);
        }
        /* `u8 a[4] = { "abc" }` fills the whole array with bytes, not just the
         * first leaf. The same applies to a typedef'd array field. */
        if (peeled && peeled->tag == 12 && init->arg_count &&
            init->args[0] && init->args[0]->tag == 2) {
            uint32_t byte_count = byte_array_count(g, t);
            if (byte_count) {
                pos = emit_global_leaf(g, global, var_start, var_size, leaves,
                                       leaf_count, pos, init->args[0], byte_count);
                return pos;
            }
        }
        /* `Bytes g[1] = { "ABCD", "XYZ" }` is one element's field list. */
        if (is_single_struct_field_list(g, t, init)) {
            return emit_global_init_expr(g, global, var_start, var_size,
                                         peeled->inner, init, leaves,
                                         leaf_count, pos, depth + 1);
        }
        /* A scalar element type always flattens; an aggregate array flattens
         * when the list is longer than the declared element count. This also
         * covers `Outer a[1] = {{{1,2},{3,4}}, 55}` with nested braces. */
        int scalar_elem = inner && inner->tag != 11 && inner->tag != 12;
        if (scalar_elem || (sub_count && init->arg_count > sub_count)) {
            return emit_global_flat_args(g, global, var_start, var_size,
                                         leaves, leaf_count, pos, init,
                                         depth + 1);
        }
        if (peeled && peeled->tag == 11 && st) {
            size_t count = st->is_union ? 1 : st->field_count;
            for (size_t i = 0; i < count; i++) {
                WType* field_type = st->fields[i].type;
                size_t before = pos;
                size_t span = leaf_count_for_type(g, field_type);
                if (i < init->arg_count && init->args[i]) {
                    pos = emit_global_init_expr(g, global, var_start, var_size,
                                                field_type, init->args[i],
                                                leaves, leaf_count, pos,
                                                depth + 1);
                }
                /* The global buffer is pre-zeroed; only the leaf position
                 * needs to skip fields that had no initializer. */
                size_t expected = before + span;
                if (expected > leaf_count) expected = leaf_count;
                if (pos < expected) pos = expected;
            }
            return pos;
        }
        if (peeled && peeled->tag == 12) {
            size_t count = peeled->count;
            for (size_t i = 0; i < count; i++) {
                WType* elem_type = peeled->inner;
                size_t before = pos;
                size_t span = leaf_count_for_type(g, elem_type);
                if (i < init->arg_count && init->args[i]) {
                    pos = emit_global_init_expr(g, global, var_start, var_size,
                                                elem_type, init->args[i],
                                                leaves, leaf_count, pos,
                                                depth + 1);
                }
                size_t expected = before + span;
                if (expected > leaf_count) expected = leaf_count;
                if (pos < expected) pos = expected;
            }
            return pos;
        }
        if (init->arg_count) {
            pos = emit_global_leaf(g, global, var_start, var_size, leaves,
                                   leaf_count, pos, init->args[0], 1);
        }
        return pos;
    }
    uint32_t byte_count = byte_array_count(g, t);
    size_t limit = byte_count ? byte_count : 1;
    return emit_global_leaf(g, global, var_start, var_size, leaves, leaf_count,
                            pos, init, limit);
}

static void setup_program(Gen* g, Buf* global) {
    g->next_offset = 8;

    map_init(&g->global_offsets, (uint32_t)(g->prog.section_count * 8 + 64));
    map_init(&g->section_volatile, 16);
    map_init(&g->section_var_sizes, 16);
    map_init(&g->section_types, 16);
    map_init(&g->struct_layouts, (uint32_t)(g->prog.struct_count * 2 + 8));
    map_init(&g->struct_fields, (uint32_t)(g->prog.struct_count * 2 + 8));
    map_init(&g->struct_versions, (uint32_t)(g->prog.struct_count * 2 + 8));
    map_init(&g->struct_alignments, (uint32_t)(g->prog.struct_count * 2 + 8));
    map_init(&g->struct_volatile, (uint32_t)(g->prog.struct_count * 2 + 8));
    map_init(&g->typedefs, (uint32_t)(g->prog.typedef_count * 2 + 8));
    map_init(&g->string_constants, 32);
    map_init(&g->float_constants, 16);
    map_init(&g->constants, (uint32_t)(g->prog.const_count * 2 + 8));
    map_init(&g->enums, (uint32_t)(g->prog.enum_count * 2 + 8));
    map_init(&g->function_signatures, (uint32_t)(g->prog.func_count * 2 + 8));
    map_init(&g->function_return_types, (uint32_t)(g->prog.func_count * 2 + 8));
    map_init(&g->function_offsets, (uint32_t)(g->prog.func_count * 2 + 8));

    for (size_t i = 0; i < g->prog.typedef_count; i++) {
        map_put_str(&g->typedefs, g->prog.typedef_names[i], PTR(g->prog.typedef_types[i]));
    }

    for (size_t i = 0; i < g->prog.func_count; i++) {
        WFunc* f = &g->prog.funcs[i];
        map_put_str(&g->function_signatures, f->name, i + 1);
        map_put_str(&g->function_return_types, f->name, PTR(f));
    }

    for (size_t i = 0; i < g->prog.struct_count; i++) {
        WStruct* s = &g->prog.structs[i];
        VolatileSet set;
        memset(&set, 0, sizeof(set));
        size_t nv = 0;
        for (size_t j = 0; j < s->field_count; j++) {
            if (s->fields[j].access == 4 || s->fields[j].access == 5) {
                nv++;
            }
        }
        set.names = (WStr*)arena_alloc(g_arena, (nv ? nv : 1) * sizeof(WStr));
        for (size_t j = 0; j < s->field_count; j++) {
            if (s->fields[j].access == 4 || s->fields[j].access == 5) {
                set.names[set.count++] = s->fields[j].name;
            }
        }
        if (set.count) {
            map_put_str(&g->struct_volatile, s->name, g->volatile_sets.n);
            VolatileSetVec_push(&g->volatile_sets, set);
        }
        map_put_str(&g->struct_versions, s->name, s->version);
        map_put_str(&g->struct_fields, s->name, g->layouts.n);
    }

    for (size_t i = 0; i < g->prog.const_count; i++) {
        map_put_str(&g->constants, g->prog.consts[i].name, PTR(g->prog.consts[i].value));
    }
    for (size_t i = 0; i < g->prog.enum_count; i++) {
        map_put_str(&g->enums, g->prog.enums[i].name, i);
    }

    int resolved_any = 1;
    while (resolved_any) {
        resolved_any = 0;
        for (size_t i = 0; i < g->prog.struct_count; i++) {
            WStruct* s = &g->prog.structs[i];
            if (map_has(&g->struct_layouts, s->name.ptr, s->name.len)) {
                continue;
            }
            int can_resolve = 1;
            for (size_t j = 0; j < s->field_count; j++) {
                if (!can_resolve_type(g, s->fields[j].type)) {
                    can_resolve = 0;
                    break;
                }
            }
            if (can_resolve) {
                calculate_and_insert_layout(g, s);
                resolved_any = 1;
            }
        }
    }
    for (size_t i = 0; i < g->prog.struct_count; i++) {
        WStruct* s = &g->prog.structs[i];
        if (!map_has(&g->struct_layouts, s->name.ptr, s->name.len)) {
            calculate_and_insert_layout(g, s);
        }
    }
    for (size_t i = 0; i < g->prog.struct_count; i++) {
        WStruct* s = &g->prog.structs[i];
        uint64_t idx;
        if (map_get_str(&g->struct_layouts, s->name, &idx)) {
            map_put_str(&g->struct_fields, s->name, idx);
        }
    }

    for (size_t i = 0; i < g->prog.const_count; i++) {
        collect_expr_literals(g, g->prog.consts[i].value, 1);
    }
    for (size_t i = 0; i < g->prog.func_count; i++) {
        WFunc* f = &g->prog.funcs[i];
        for (size_t j = 0; j < f->body_count; j++) {
            collect_stmt_literals(g, f->body[j], 1);
        }
    }

    for (size_t i = 0; i < g->prog.section_count; i++) {
        WSection* sect = &g->prog.sections[i];
        for (size_t j = 0; j < sect->count; j++) {
            WVar* var = &sect->vars[j];
            WStr key = wstr_pair(sect->name, var->name);
            add_global_offset(g, key, (uint32_t)global->len);
            uint32_t var_size = type_size(g, var->type);
            if (var->access == 4 || var->access == 5) {
                map_put_str(&g->section_volatile, key, 1);
            }
            map_put_str(&g->section_var_sizes, key, var_size);
            map_put_str(&g->section_types, key, PTR(var->type));
            size_t start_len = global->len;
            for (uint32_t k = 0; k < var_size; k++) {
                buf_u8(global, 0);
            }
            if (var->has_init && var->init) {
                InitLeafVec leaves = {0};
                collect_init_leaves(g, var->type, 0, &leaves, 0);
                emit_global_init_expr(g, global, start_len, var_size,
                                      var->type, var->init, leaves.v,
                                      leaves.n, 0, 0);
            }
        }
    }

    for (size_t i = 0; i < g->string_keys.n; i++) {
        WStr key = g->string_keys.v[i];
        uint32_t off = (uint32_t)global->len;
        Buf bytes = {0};
        unescape_string(key, &bytes);
        buf_bytes(global, bytes.data, bytes.len);
        buf_u8(global, 0);
        WStr full = wstr_prefix("str:", key);
        add_global_offset(g, full, off);
    }

    for (size_t i = 0; i < g->global_ptr_fixups.n; i++) {
        GlobalPtrFixup* fixup = &g->global_ptr_fixups.v[i];
        uint64_t off = 0;
        if (!map_get_str(&g->global_offsets, fixup->key, &off)) {
            continue;
        }
        AddrPatch p;
        p.pos = fixup->pos;
        p.key = fixup->key;
        p.is_rip = 0;
        p.patch_end = 0;
        p.is_global = 1;
        AddrPatchVec_push(&g->address_patches, p);
    }

    for (size_t i = 0; i < g->prog.func_count; i++) {
        WFunc* f = &g->prog.funcs[i];
        for (size_t j = 0; j < f->body_count; j++) {
            collect_stmt_literals(g, f->body[j], 0);
        }
    }
    for (size_t i = 0; i < g->float_keys.n; i++) {
        WStr key = g->float_keys.v[i];
        uint32_t off = (uint32_t)global->len;
        char* nul = (char*)arena_alloc(g_arena, key.len + 1);
        memcpy(nul, key.ptr, key.len);
        nul[key.len] = '\0';
        double value = strtod(nul, NULL);
        uint64_t bits;
        memcpy(&bits, &value, sizeof(bits));
        buf_u64(global, bits);
        WStr full = wstr_prefix("float:", key);
        add_global_offset(g, full, off);
    }
}

static WType* pointer_type_for(Gen* g, WType* inner) {
    (void)g;
    WType* t = (WType*)arena_alloc(g_arena, sizeof(WType));
    t->tag = 10;
    t->inner = inner;
    return t;
}

static void compile_function(Gen* g, WFunc* f) {
    g->locals.slots = NULL;
    g->local_access.slots = NULL;
    g->local_types.slots = NULL;
    g->local_is_param.slots = NULL;
    g->escaped_locals.slots = NULL;
    g->locals.len = g->local_access.len = g->local_types.len = g->local_is_param.len = 0;
    g->escaped_locals.len = 0;
    g->next_offset = f->is_irq ? 16 : 24; /* leave rbp-16 for saved r12 */
    g->current_is_irq = f->is_irq;
    g->current_function = f;
    g->suppress_deref_var = (WStr){0};
    g->stack_depth = 0;
    g->loop_stack.n = 0;

    if (f->has_body) {
        for (size_t i = 0; i < f->body_count; i++) {
            collect_escaped_stmt(g, f->body[i]);
        }
    }

    Buf* b = &g->code;
    if (f->is_irq) {
        static const uint8_t push_irq[] = {0x50, 0x51, 0x52, 0x56, 0x57, 0x41, 0x50, 0x41,
                                           0x51, 0x41, 0x52, 0x41, 0x53, 0x41, 0x54, 0x41,
                                           0x55, 0x41, 0x56, 0x41, 0x57};
        buf_bytes(b, push_irq, sizeof(push_irq));
    }
    buf_u8(b, 0x55);
    buf_u8(b, 0x48); buf_u8(b, 0x89); buf_u8(b, 0xE5);
    buf_u8(b, 0x53);
    if (!f->is_irq) {
        buf_u8(b, 0x41); buf_u8(b, 0x54); /* push r12 */
    }

    size_t sub_rsp_offset = b->len;
    buf_u8(b, 0x48); buf_u8(b, 0x81); buf_u8(b, 0xEC);
    buf_u32(b, 0);

    size_t int_param_pos = 0;
    size_t float_param_pos = 0;
    size_t stack_param_pos = 0;
    static const uint8_t int_regs[6] = {7, 6, 2, 1, 8, 9};

    for (size_t i = 0; i < f->param_count; i++) {
        WParam* p = &f->params[i];
        int is_ptr_modifier = p->access == 0 || p->access == 1 || p->access == 2;
        WType* actual = p->type;
        WType* declared = resolve_named_type(g, actual);
        /* Array parameters decay to a pointer in the caller, even when the
         * declared type is a named typedef. Reserve a full 64-bit slot; the
         * load side already treats an array parameter slot as an address. */
        uint32_t var_size =
            (is_ptr_modifier || (declared && declared->tag == 12))
                ? 8
                : type_size(g, p->type);
        uint32_t align_mask = var_size >= 8 ? 7 : (var_size >= 4 ? 3 : (var_size >= 2 ? 1 : 0));
        g->next_offset = (g->next_offset + align_mask) & ~align_mask;
        g->next_offset += var_size;
        uint32_t off = g->next_offset;

        map_put_str(&g->locals, p->name, off);
        map_put_str(&g->local_access, p->name, p->access);
        map_put_str(&g->local_is_param, p->name, 1);

        /* A named typedef can already be a pointer. Wrapping it again would
         * make `*i`/`*o` parameters dereference a pointer-to-pointer and use
         * 8-byte accessors even for u8/i16 pointees. */
        if (is_ptr_modifier && (!declared || declared->tag != 10)) {
            actual = pointer_type_for(g, actual);
        }
        map_put_str(&g->local_types, p->name, PTR(actual));

        /* The caller classifies named typedef parameters through
         * resolve_named_type(); the prologue must use the same resolution or
         * a typedef'd f64 arrives in xmm0 but is stored from a GPR slot. */
        WType* peeled = resolve_named_type(g, actual);
        int is_float = peeled && peeled->tag == 8;
        if (is_float && float_param_pos < 8) {
            emit_store_xmm_param(b, (uint8_t)float_param_pos, off);
            float_param_pos++;
        } else if (!is_float && int_param_pos < 6) {
            emit_mem_store(b, int_regs[int_param_pos], off, var_size);
            int_param_pos++;
        } else {
            emit_load_stack_param(b, off, var_size ? var_size : 8, stack_param_pos);
            stack_param_pos++;
        }
    }

    int has_explicit_return = 0;
    if (f->has_body) {
        for (size_t i = 0; i < f->body_count; i++) {
            compile_stmt(g, f->body[i]);
        }
        has_explicit_return = has_return_statement(f->body, f->body_count);
    }

    uint32_t final_stack_size = ((g->next_offset + 7) & ~7u) | 8u;
    b->data[sub_rsp_offset + 3] = (uint8_t)(final_stack_size & 0xFF);
    b->data[sub_rsp_offset + 4] = (uint8_t)((final_stack_size >> 8) & 0xFF);
    b->data[sub_rsp_offset + 5] = (uint8_t)((final_stack_size >> 16) & 0xFF);
    b->data[sub_rsp_offset + 6] = (uint8_t)((final_stack_size >> 24) & 0xFF);

    if (!has_explicit_return) {
        buf_u8(b, 0x48); buf_u8(b, 0x81); buf_u8(b, 0xC4);
        buf_u32(b, final_stack_size);
        if (!g->current_is_irq) {
            buf_u8(b, 0x41); buf_u8(b, 0x5C); /* pop r12 */
        }
        buf_u8(b, 0x5B);
        buf_u8(b, 0x5D);
        emit_function_epilogue(g);
    }
}

static void patch_rel32(Buf* b, size_t pos, uint64_t target, int64_t extra) {
    int32_t rel = (int32_t)target - (int32_t)(pos + 4 + extra);
    patch_i32(b, pos, rel);
}

static void emit_prologue(Gen* g, WandOutputFormat format, size_t* entry_patch) {
    Buf* b = &g->code;
    *entry_patch = SIZE_MAX;
    switch (format) {
        case WAND_OUTPUT_PROGRAM: {
            static const uint8_t seq1[] = {0x48, 0x8B, 0x3C, 0x24};
            static const uint8_t seq2[] = {0x48, 0x8D, 0x74, 0x24, 0x08};
            static const uint8_t seq3[] = {0x48, 0x89, 0xFA};
            static const uint8_t seq4[] = {0x48, 0xFF, 0xC2};
            static const uint8_t seq5[] = {0x48, 0xC1, 0xE2, 0x03};
            static const uint8_t seq6[] = {0x48, 0x01, 0xF2};
            static const uint8_t seq8[] = {0x48, 0x89, 0xC7};
            static const uint8_t seq9[] = {0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00};
            static const uint8_t seq10[] = {0x0F, 0x05};
            buf_bytes(b, seq1, sizeof(seq1));
            buf_bytes(b, seq2, sizeof(seq2));
            buf_bytes(b, seq3, sizeof(seq3));
            buf_bytes(b, seq4, sizeof(seq4));
            buf_bytes(b, seq5, sizeof(seq5));
            buf_bytes(b, seq6, sizeof(seq6));
            buf_u8(b, 0xE8);
            *entry_patch = b->len;
            buf_u32(b, 0);
            buf_bytes(b, seq8, sizeof(seq8));
            buf_bytes(b, seq9, sizeof(seq9));
            buf_bytes(b, seq10, sizeof(seq10));
            break;
        }
        case WAND_OUTPUT_OBJECT:
            break;
        case WAND_OUTPUT_RAW:
            buf_u8(b, 0xE8);
            *entry_patch = b->len;
            buf_u32(b, 0);
            buf_u8(b, 0xEB); buf_u8(b, 0xFE);
            break;
        case WAND_OUTPUT_KERNEL:
            buf_u8(b, 0xE8);
            *entry_patch = b->len;
            buf_u32(b, 0);
            buf_u8(b, 0xFA); buf_u8(b, 0xF4); buf_u8(b, 0xEB); buf_u8(b, 0xFD);
            break;
        case WAND_OUTPUT_WEXP:
            /* A .wexp module is a continuation object, not a process image.
             * There is no _start stub: the loader calls `main` directly. */
            break;
    }
}

static void finalize_output(Gen* g, Buf* global, WandOutputFormat format,
                            WStr entry_name, size_t entry_patch) {
    Buf* b = &g->code;
    size_t functions_end = b->len;
    size_t global_start = (functions_end + 0xFFFu) & ~(size_t)0xFFFu;
    while (b->len < global_start) {
        buf_u8(b, 0);
    }
    buf_bytes(b, global->data, global->len);

    WStr entry_target = wstr_lit("main");
    if (format != WAND_OUTPUT_PROGRAM && format != WAND_OUTPUT_WEXP && entry_name.len) {
        entry_target = entry_name;
    }
    if (entry_patch != SIZE_MAX) {
        uint64_t off;
        if (map_get_str(&g->function_offsets, entry_target, &off)) {
            patch_rel32(b, entry_patch, off, 0);
        }
    }

    size_t out = 0;
    for (size_t i = 0; i < g->call_patches.n; i++) {
        CallPatch p = g->call_patches.v[i];
        uint64_t off;
        if (map_get_str(&g->function_offsets, p.target, &off)) {
            patch_rel32(b, p.pos, off, 0);
        } else {
            g->call_patches.v[out++] = p;
        }
    }
    g->call_patches.n = out;

    for (size_t i = 0; i < g->address_patches.n; i++) {
        AddrPatch p = g->address_patches.v[i];
        if (p.is_global) {
            /* Global fixups were recorded as offsets inside the global data
             * buffer; the image places that buffer at global_start. */
            p.pos += global_start;
            g->address_patches.v[i].pos = p.pos;
        }
        if (format == WAND_OUTPUT_WEXP && p.is_rip &&
            wstr_starts_with(p.key, "fn:")) {
            /* WEXP keeps function addresses as relocations. The loader fills
             * the disp32 after it chooses the module base. */
            continue;
        }
        uint64_t local_off = 0;
        map_get_str(&g->global_offsets, p.key, &local_off);
        uint64_t target = (uint64_t)global_start + local_off;
        if (p.is_rip) {
            /* RIP-relative displacements are invariant under relocation. For
             * add/sub the immediate follows disp32, so RIP at the memory
             * operand is pos + 4 + imm_size. */
            int64_t extra = 0;
            if (p.patch_end) {
                uint8_t opcode = b->data[p.pos - 2];
                extra = opcode == 0x81 ? 4 : 1;
            }
            patch_rel32(b, p.pos, target, extra);
        } else if (format == WAND_OUTPUT_WEXP) {
            /* Leave the module-relative offset in place. The P46 loader adds
             * the module base when it applies the relocation entry. */
            for (int k = 0; k < 8; k++) {
                b->data[p.pos + k] = (uint8_t)(target >> (k * 8));
            }
        } else {
            uint64_t abs = LOAD_BASE + target;
            for (int k = 0; k < 8; k++) {
                b->data[p.pos + k] = (uint8_t)(abs >> (k * 8));
            }
        }
    }
}

static WandCompileResult make_error_result(uint32_t error) {
    WandCompileResult r;
    memset(&r, 0, sizeof(r));
    r.error = error;
    return r;
}

static char* dup_wstr(WStr s) {
    char* p = (char*)malloc(s.len + 1);
    if (!p) {
        return NULL;
    }
    memcpy(p, s.ptr, s.len);
    p[s.len] = '\0';
    return p;
}

WandCompileResult wand_x86_compile_program(
    const uint8_t* program_buf,
    size_t program_len,
    WandOutputFormat format,
    const uint8_t* entry_name,
    size_t entry_len
) {
    Arena arena;
    memset(&arena, 0, sizeof(arena));
    g_arena = &arena;
    g_error = 0;

    if (setjmp(g_fail)) {
        WandCompileResult r = make_error_result(g_error ? g_error : 1);
        arena_free(&arena);
        g_arena = NULL;
        return r;
    }

    Gen* g = (Gen*)arena_alloc(g_arena, sizeof(Gen));
    memset(g, 0, sizeof(Gen));
    map_init(&g->locals, 32);
    map_init(&g->local_access, 32);
    map_init(&g->local_types, 32);
    map_init(&g->local_is_param, 32);
    map_init(&g->escaped_locals, 32);
    g->format = format;
    g->entry_name = wstr_make(entry_name ? (const void*)entry_name : (const void*)"", entry_name ? entry_len : 0);

    Reader reader;
    reader.p = program_buf;
    reader.len = program_len;
    reader.pos = 0;
    reader.depth = 0;
    g->prog = rd_program(&reader);

    Buf global;
    memset(&global, 0, sizeof(global));
    setup_program(g, &global);

    size_t entry_patch = SIZE_MAX;
    emit_prologue(g, format, &entry_patch);

    for (size_t i = 0; i < g->prog.func_count; i++) {
        WFunc* f = &g->prog.funcs[i];
        if (f->is_extern || !f->has_body) {
            continue;
        }
        map_put_str(&g->function_offsets, f->name, g->code.len);
        compile_function(g, f);
    }

    if (format == WAND_OUTPUT_OBJECT) {
        build_relocatable_elf(g, &global);
    } else {
        finalize_output(g, &global, format, g->entry_name, entry_patch);
    }

    WandCompileResult result;
    memset(&result, 0, sizeof(result));
    if (g->code.len) {
        result.data = (uint8_t*)malloc(g->code.len);
        if (!result.data) {
            arena_free(&arena);
            g_arena = NULL;
            return make_error_result(2);
        }
        memcpy(result.data, g->code.data, g->code.len);
        result.size = g->code.len;
    }

    size_t symbol_count = g->function_offsets.slots ? g->function_offsets.cap : 0;
    result.functions = (WandSymbol*)calloc(symbol_count ? symbol_count : 1, sizeof(WandSymbol));
    if (!result.functions) {
        free(result.data);
        arena_free(&arena);
        g_arena = NULL;
        return make_error_result(2);
    }
    for (uint32_t i = 0; i < g->function_offsets.cap; i++) {
        MapSlot* slot = &g->function_offsets.slots[i];
        if (!slot->used) {
            continue;
        }
        WStr name;
        name.ptr = slot->key;
        name.len = slot->key_len;
        result.functions[result.function_count].name = dup_wstr(name);
        result.functions[result.function_count].offset = slot->value;
        result.function_count++;
    }

    if (g->call_patches.n) {
        result.call_patches = (WandCallPatch*)calloc(g->call_patches.n, sizeof(WandCallPatch));
        if (!result.call_patches) {
            wand_compile_result_free(&result);
            arena_free(&arena);
            g_arena = NULL;
            return make_error_result(2);
        }
        for (size_t i = 0; i < g->call_patches.n; i++) {
            result.call_patches[i].position = g->call_patches.v[i].pos;
            result.call_patches[i].target = dup_wstr(g->call_patches.v[i].target);
            result.call_patch_count++;
        }
    }

    if (g->address_patches.n) {
        result.addr_patches = (WandAddrPatch*)calloc(g->address_patches.n, sizeof(WandAddrPatch));
        if (!result.addr_patches) {
            wand_compile_result_free(&result);
            arena_free(&arena);
            g_arena = NULL;
            return make_error_result(2);
        }
        for (size_t i = 0; i < g->address_patches.n; i++) {
            result.addr_patches[i].position = g->address_patches.v[i].pos;
            result.addr_patches[i].key = dup_wstr(g->address_patches.v[i].key);
            result.addr_patches[i].is_rip = g->address_patches.v[i].is_rip;
            result.addr_patches[i].patch_end = g->address_patches.v[i].patch_end;
            result.addr_patch_count++;
        }
    }

    arena_free(&arena);
    g_arena = NULL;
    return result;
}

void wand_compile_result_free(WandCompileResult* result) {
    if (!result) {
        return;
    }
    free(result->data);
    for (size_t i = 0; i < result->function_count; i++) {
        free(result->functions[i].name);
    }
    free(result->functions);
    for (size_t i = 0; i < result->call_patch_count; i++) {
        free(result->call_patches[i].target);
    }
    free(result->call_patches);
    for (size_t i = 0; i < result->addr_patch_count; i++) {
        free(result->addr_patches[i].key);
    }
    free(result->addr_patches);
    memset(result, 0, sizeof(*result));
}
/*
 * Port of Rust CodeGen::build_relocatable_elf() and its ELF helpers
 * (codegen.rs.orig, lines 3328-3765), appended to src/backend/x86/codegen.c.
 *
 * ORDERING CAVEAT:
 *   Rust collected the "str:" / "float:" symbol keys by iterating
 *   self.global_offsets, which is a HashMap (arbitrary iteration order), and
 *   then performed a *stable* sort_by_key on the offset before emitting the
 *   ".LstrN" / ".LfloatN" symbols.  The C generator keeps only the insertion
 *   ordered g->global_keys list, so this port iterates that list and stable
 *   sorts by offset.  Every string/float constant is appended to global_data
 *   sequentially, so their offsets are unique and the emitted order matches
 *   Rust exactly; only a hypothetical duplicate offset could differ from
 *   Rust's HashMap-dependent tie order.  Likewise, the strtab insertion order
 *   is reproduced from g->global_keys (the C equivalent of the Rust
 *   string_constants insertion order used when global_data was built).
 *
 * Temporary Bufs in this file are allocated through g_arena, so they cannot be
 * released with free() on the individual buffer pointers.  A private Arena is
 * installed for the duration of this function and released with arena_free()
 * before returning; g_arena is restored before appending the finished ELF to
 * g->code so that the caller's code buffer keeps using its original arena.
 */

typedef struct {
    uint32_t name;
    uint8_t info;
    uint8_t other;
    uint16_t shndx;
    uint64_t value;
    uint64_t size;
} ElfSym; /* 24 bytes, matches Elf64_Sym layout used by the Rust code. */

typedef struct {
    WStr name;
    uint32_t start;
    uint32_t size;
    uint32_t align;
    uint8_t is_ro;
    uint8_t is_noinit;
    uint32_t name_off; /* offset of ".w.<name>" in .shstrtab */
    uint64_t off;      /* file offset of the section contents */
} ElfSectRange;

typedef struct {
    WStr key;
    uint32_t off;
} ElfKeyOff;

#define ELF_STR(s) ((s).ptr ? (s).ptr : "")

static uint32_t elf_strtab_insert(Buf* strtab, Map* cache, WStr s) {
    uint64_t cached = 0;
    if (map_get_str(cache, s, &cached)) {
        return (uint32_t)cached;
    }
    uint32_t off = (uint32_t)strtab->len;
    buf_bytes(strtab, s.ptr, s.len);
    buf_u8(strtab, 0);
    map_put_str(cache, s, (uint64_t)off);
    return off;
}

static void elf_push_u8(Buf* b, uint8_t value) { buf_u8(b, value); }

static void elf_push_u16(Buf* b, uint16_t value) { buf_u16(b, value); }

static void elf_push_u32(Buf* b, uint32_t value) { buf_u32(b, value); }

static void elf_push_u64(Buf* b, uint64_t value) { buf_u64(b, value); }

static void elf_push_i64(Buf* b, int64_t value) { buf_u64(b, (uint64_t)value); }

static void elf_push_shdr(Buf* b, uint32_t name, uint32_t sh_type, uint64_t flags,
                          uint64_t addr, uint64_t offset, uint64_t size,
                          uint32_t link, uint32_t info, uint64_t addralign,
                          uint64_t entsize) {
    elf_push_u32(b, name);
    elf_push_u32(b, sh_type);
    elf_push_u64(b, flags);
    elf_push_u64(b, addr);
    elf_push_u64(b, offset);
    elf_push_u64(b, size);
    elf_push_u32(b, link);
    elf_push_u32(b, info);
    elf_push_u64(b, addralign);
    elf_push_u64(b, entsize);
}

static void elf_sym_push(Buf* symbols, uint32_t name, uint8_t info, uint8_t other,
                         uint16_t shndx, uint64_t value, uint64_t size) {
    ElfSym sym;
    sym.name = name;
    sym.info = info;
    sym.other = other;
    sym.shndx = shndx;
    sym.value = value;
    sym.size = size;
    buf_bytes(symbols, &sym, sizeof(sym));
}

#define ELF_SYM_COUNT(b) ((uint32_t)((b).len / sizeof(ElfSym)))

static WStr elf_fmt_wstr(const char* fmt, ...) {
    Buf b = {0};
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(NULL, 0, fmt, ap);
    va_end(ap);
    if (n < 0) {
        return wstr_lit("");
    }
    buf_reserve(&b, (size_t)n + 1u);
    va_start(ap, fmt);
    vsnprintf((char*)b.data, (size_t)n + 1u, fmt, ap);
    va_end(ap);
    b.len = (size_t)n;
    return wstr_make(b.data, (size_t)n);
}

/* Append global_data[start, start+size), zero filling any missing tail. */
static void elf_append_global_slice(Buf* body, Buf* global_data, uint32_t start,
                                    uint32_t size) {
    uint64_t s = (uint64_t)start;
    uint64_t e = (uint64_t)start + (uint64_t)size;
    size_t glen = global_data ? global_data->len : 0u;
    if (s > (uint64_t)glen) s = (uint64_t)glen;
    if (e > (uint64_t)glen) e = (uint64_t)glen;
    size_t copied = (e > s) ? (size_t)(e - s) : 0u;
    if (copied) {
        buf_bytes(body, global_data->data + (size_t)s, copied);
    }
    for (size_t i = copied; i < (size_t)size; i++) {
        buf_u8(body, 0);
    }
}

#define ELF_GROW(ptr, count, cap, type)                                              \
    do {                                                                             \
        if ((count) == (cap)) {                                                      \
            size_t new_cap_ = (cap) ? (cap) * 2u : 8u;                              \
            type* new_ptr_ =                                                         \
                (type*)arena_alloc(g_arena, new_cap_ * sizeof(type));               \
            if ((count)) {                                                           \
                memcpy(new_ptr_, (ptr), (count) * sizeof(type));                    \
            }                                                                        \
            (ptr) = new_ptr_;                                                        \
            (cap) = new_cap_;                                                        \
        }                                                                            \
    } while (0)

static void elf_note_undefined(Gen* g, WStr target, WStr** undefined,
                               size_t* n, size_t* cap) {
    if (map_has(&g->function_offsets, target.ptr, target.len)) {
        return;
    }
    for (size_t i = 0; i < *n; i++) {
        if (wstr_eq((*undefined)[i], target)) return;
    }
    ELF_GROW(*undefined, *n, *cap, WStr);
    (*undefined)[(*n)++] = target;
}

static void build_relocatable_elf(Gen* g, Buf* global_data) {
    Arena tmp_arena = {0};
    Arena* saved_arena = g_arena;
    g_arena = &tmp_arena;

    WProgram* p = &g->prog;

    Buf shstrtab = {0};
    Buf strtab = {0};
    Buf rela = {0};
    Buf symtab = {0};
    Buf body = {0};
    Buf elf = {0};
    Buf symbols = {0};
    Buf* sect_rela = NULL;        /* one relocation buffer per Wand section */
    uint32_t* sect_rela_name = NULL;
    uint64_t* sect_rela_off = NULL;
    uint64_t* sect_rela_size = NULL;
    size_t sect_rela_count = 0;

    Map str_cache = {0};
    Map data_sym = {0};
    Map func_sym = {0};

    /* ---------------------------------------------------------------- */
    /* .shstrtab names                                                   */
    /* ---------------------------------------------------------------- */
    buf_u8(&shstrtab, 0);
    uint32_t n_text = (uint32_t)shstrtab.len;
    buf_bytes(&shstrtab, ".text", sizeof(".text")); /* writes ".text\0" */

    ElfSectRange* sects = NULL;
    size_t sects_n = 0, sects_cap = 0;
    for (size_t i = 0; i < p->section_count; i++) {
        WSection* sect = &p->sections[i];
        uint32_t name_off = (uint32_t)shstrtab.len;
        buf_printf(&shstrtab, ".w.%.*s", (int)sect->name.len, ELF_STR(sect->name));
        buf_u8(&shstrtab, 0);
        ELF_GROW(sects, sects_n, sects_cap, ElfSectRange);
        ElfSectRange* range = &sects[sects_n++];
        range->name = sect->name;
        range->start = 0;
        range->size = 0;
        range->align = 0;
        range->is_ro = sect->is_ro;
        range->is_noinit = sect->is_noinit;
        range->name_off = name_off;
        range->off = 0;
    }
    if (sects_n) {
        sect_rela = (Buf*)arena_alloc(g_arena, sects_n * sizeof(Buf));
        sect_rela_name = (uint32_t*)arena_alloc(g_arena, sects_n * sizeof(uint32_t));
        sect_rela_off = (uint64_t*)arena_alloc(g_arena, sects_n * sizeof(uint64_t));
        sect_rela_size = (uint64_t*)arena_alloc(g_arena, sects_n * sizeof(uint64_t));
        memset(sect_rela, 0, sects_n * sizeof(Buf));
        memset(sect_rela_name, 0, sects_n * sizeof(uint32_t));
        memset(sect_rela_off, 0, sects_n * sizeof(uint64_t));
        memset(sect_rela_size, 0, sects_n * sizeof(uint64_t));
    }
    uint32_t n_rodata = (uint32_t)shstrtab.len;
    buf_bytes(&shstrtab, ".rodata", sizeof(".rodata"));
    uint32_t n_rela = (uint32_t)shstrtab.len;
    buf_bytes(&shstrtab, ".rela.text", sizeof(".rela.text"));
    uint32_t n_symtab = (uint32_t)shstrtab.len;
    buf_bytes(&shstrtab, ".symtab", sizeof(".symtab"));
    uint32_t n_strtab = (uint32_t)shstrtab.len;
    buf_bytes(&shstrtab, ".strtab", sizeof(".strtab"));
    uint32_t n_shstrtab = (uint32_t)shstrtab.len;
    buf_bytes(&shstrtab, ".shstrtab", sizeof(".shstrtab"));
    uint32_t n_note = (uint32_t)shstrtab.len;
    buf_bytes(&shstrtab, ".note.GNU-stack", sizeof(".note.GNU-stack"));

    /* ---------------------------------------------------------------- */
    /* .strtab and section data ranges                                   */
    /* ---------------------------------------------------------------- */
    buf_u8(&strtab, 0);
    uint32_t text_name = elf_strtab_insert(&strtab, &str_cache, wstr_lit(".text"));

    uint32_t cur_off = 0;
    for (size_t i = 0; i < p->section_count; i++) {
        WSection* sect = &p->sections[i];
        uint32_t start = cur_off;
        for (size_t j = 0; j < sect->count; j++) {
            cur_off += type_size(g, sect->vars[j].type);
        }
        ElfSectRange* range = &sects[i];
        range->start = start;
        range->size = cur_off - start;
        range->align = sect->align ? sect->align : 8u;
    }
    uint32_t user_data_end = cur_off;

    /* ---------------------------------------------------------------- */
    /* Symbols: mandatory, section variables, strings, floats            */
    /* ---------------------------------------------------------------- */
    elf_sym_push(&symbols, 0, 0, 0, 0, 0, 0);
    elf_sym_push(&symbols, text_name, 3, 0, 1, 0, 0);

    uint16_t rodata_shndx = (uint16_t)(2u + p->section_count);

    /* Keys come from g->global_keys (insertion order); Rust iterated a
     * HashMap, then stable-sorted by offset.  See caveat at top of file. */
    ElfKeyOff* string_keys = NULL;
    size_t string_keys_n = 0, string_keys_cap = 0;
    ElfKeyOff* float_keys = NULL;
    size_t float_keys_n = 0, float_keys_cap = 0;
    for (size_t i = 0; i < g->global_keys.n; i++) {
        WStr key = g->global_keys.v[i];
        uint64_t off64 = 0;
        if (!map_get_str(&g->global_offsets, key, &off64)) {
            continue;
        }
        uint32_t off = (uint32_t)off64;
        if (off < user_data_end) {
            continue;
        }
        if (wstr_starts_with(key, "str:")) {
            ELF_GROW(string_keys, string_keys_n, string_keys_cap, ElfKeyOff);
            string_keys[string_keys_n].key = key;
            string_keys[string_keys_n].off = off;
            string_keys_n++;
        } else if (wstr_starts_with(key, "float:")) {
            ELF_GROW(float_keys, float_keys_n, float_keys_cap, ElfKeyOff);
            float_keys[float_keys_n].key = key;
            float_keys[float_keys_n].off = off;
            float_keys_n++;
        }
    }

/* Rust's sort_by_key is stable; insertion sort preserves that property. */
#define ELF_SORT_BY_OFF(arr, n)                                                    \
    do {                                                                           \
        for (size_t i_ = 1; i_ < (n); i_++) {                                      \
            ElfKeyOff value_ = (arr)[i_];                                          \
            size_t j_ = i_;                                                        \
            while (j_ > 0 && (arr)[j_ - 1].off > value_.off) {                     \
                (arr)[j_] = (arr)[j_ - 1];                                         \
                j_--;                                                              \
            }                                                                      \
            (arr)[j_] = value_;                                                    \
        }                                                                          \
    } while (0)

    ELF_SORT_BY_OFF(string_keys, string_keys_n);
    ELF_SORT_BY_OFF(float_keys, float_keys_n);

    for (size_t i = 0; i < string_keys_n; i++) {
        WStr sym_name = elf_fmt_wstr(".Lstr%u", (unsigned)i);
        uint32_t name_off = elf_strtab_insert(&strtab, &str_cache, sym_name);
        uint32_t idx = ELF_SYM_COUNT(symbols);
        uint32_t local_off = string_keys[i].off - user_data_end;
        elf_sym_push(&symbols, name_off, 1, 0, rodata_shndx, (uint64_t)local_off, 0);
        map_put_str(&data_sym, string_keys[i].key, (uint64_t)idx);
    }
    for (size_t i = 0; i < float_keys_n; i++) {
        WStr sym_name = elf_fmt_wstr(".Lfloat%u", (unsigned)i);
        uint32_t name_off = elf_strtab_insert(&strtab, &str_cache, sym_name);
        uint32_t idx = ELF_SYM_COUNT(symbols);
        uint32_t local_off = float_keys[i].off - user_data_end;
        elf_sym_push(&symbols, name_off, 1, 0, rodata_shndx, (uint64_t)local_off, 8);
        map_put_str(&data_sym, float_keys[i].key, (uint64_t)idx);
    }

    /* ---------------------------------------------------------------- */
    /* Symbols: defined functions (locals first, then globals), then     */
    /* undefined call targets.                                           */
    /* ---------------------------------------------------------------- */
    int has_explicit_exports = 0;
    for (size_t i = 0; i < p->func_count; i++) {
        WFunc* f = &p->funcs[i];
        if (f->is_export && !f->is_extern && f->has_body) {
            has_explicit_exports = 1;
            break;
        }
    }

    uint32_t first_global = 0; /* sh_info: index of first non-local symbol */
    for (int global_pass = 0; global_pass < 2; global_pass++) {
        if (global_pass == 1) {
            /* sh_info is the index of the first global symbol. Section
             * variables are global objects, so they must come after every
             * local symbol and before first_global is recorded. Emitting them
             * earlier made linkers treat them as locals despite their GLOBAL
             * binding. */
            first_global = ELF_SYM_COUNT(symbols);
            for (size_t si = 0; si < sects_n; si++) {
                ElfSectRange* range = &sects[si];
                WSection* sect = &p->sections[si];
                uint16_t shndx = (uint16_t)(2u + si);
                for (size_t vi = 0; vi < sect->count; vi++) {
                    WVar* var = &sect->vars[vi];
                    WStr key = elf_fmt_wstr("%.*s:%.*s", (int)range->name.len,
                                            ELF_STR(range->name), (int)var->name.len,
                                            ELF_STR(var->name));
                    uint64_t global_off = 0;
                    uint32_t local_off =
                        map_get_str(&g->global_offsets, key, &global_off)
                            ? (uint32_t)global_off
                            : 0u;
                    local_off -= range->start; /* u32 wrapping matches Rust */

                    WStr sym_name = elf_fmt_wstr("%.*s.%.*s", (int)range->name.len,
                                                 ELF_STR(range->name),
                                                 (int)var->name.len,
                                                 ELF_STR(var->name));
                    uint32_t name_off = elf_strtab_insert(&strtab, &str_cache, sym_name);
                    uint32_t idx = ELF_SYM_COUNT(symbols);
                    uint32_t var_size = type_size(g, var->type);
                    /* GLOBAL | OBJECT: section variables are data, not code. */
                    elf_sym_push(&symbols, name_off, 17, 0, shndx,
                                 (uint64_t)local_off, (uint64_t)var_size);
                    map_put_str(&data_sym, key, (uint64_t)idx);
                }
            }
        }
        for (size_t i = 0; i < p->func_count; i++) {
            WFunc* f = &p->funcs[i];
            if (f->is_extern || !f->has_body) {
                continue;
            }
            int is_global = !has_explicit_exports || f->is_export ||
                            wstr_eq_c(f->name, "main");
            if (is_global != global_pass) {
                continue;
            }
            uint64_t off = 0;
            if (!map_get_str(&g->function_offsets, f->name, &off)) {
                continue;
            }
            uint32_t name_off = elf_strtab_insert(&strtab, &str_cache, f->name);
            uint32_t idx = ELF_SYM_COUNT(symbols);
            elf_sym_push(&symbols, name_off, global_pass ? 18 : 2, 0, 1, off, 0);
            map_put_str(&func_sym, f->name, (uint64_t)idx);
        }
    }

    WStr* undefined = NULL;
    size_t undefined_n = 0, undefined_cap = 0;
    for (size_t i = 0; i < g->call_patches.n; i++) {
        elf_note_undefined(g, g->call_patches.v[i].target,
                           &undefined, &undefined_n, &undefined_cap);
    }
    /* Taking the address of an extern function records an fn: address
     * patch but no call patch, so it must also create an undefined symbol. */
    for (size_t i = 0; i < g->address_patches.n; i++) {
        AddrPatch* ap = &g->address_patches.v[i];
        if (wstr_starts_with(ap->key, "fn:")) {
            elf_note_undefined(g, wstr_sub(ap->key, 3, ap->key.len - 3),
                               &undefined, &undefined_n, &undefined_cap);
        }
    }
    for (size_t i = 0; i < undefined_n; i++) {
        uint32_t name_off = elf_strtab_insert(&strtab, &str_cache, undefined[i]);
        uint32_t idx = ELF_SYM_COUNT(symbols);
        elf_sym_push(&symbols, name_off, 16, 0, 0, 0, 0);
        map_put_str(&func_sym, undefined[i], (uint64_t)idx);
    }

    /* ---------------------------------------------------------------- */
    /* .rela.text                                                        */
    /* ---------------------------------------------------------------- */
    for (size_t i = 0; i < g->call_patches.n; i++) {
        CallPatch* cp = &g->call_patches.v[i];
        uint64_t sym = 0;
        map_get_str(&func_sym, cp->target, &sym);
        elf_push_u64(&rela, (uint64_t)cp->pos);
        elf_push_u64(&rela, ((uint64_t)(uint32_t)sym << 32) | 4u);
        elf_push_i64(&rela, -4);
    }
    for (size_t i = 0; i < g->address_patches.n; i++) {
        AddrPatch* ap = &g->address_patches.v[i];
        if (ap->is_global) {
            continue; /* handled in the per-section relocation loop below */
        }
        if (wstr_starts_with(ap->key, "fn:")) {
            WStr func_name = wstr_sub(ap->key, 3, ap->key.len - 3);
            uint64_t sym = 0;
            map_get_str(&func_sym, func_name, &sym);
            elf_push_u64(&rela, (uint64_t)ap->pos);
            elf_push_u64(&rela, ((uint64_t)(uint32_t)sym << 32) | 4u);
            elf_push_i64(&rela, -4);
            continue;
        }
        uint64_t sym = 0;
        map_get_str(&data_sym, ap->key, &sym);
        uint64_t type = ap->is_rip ? 2u : 1u;
        int64_t addend = ap->is_rip ? -4 : 0;
        if (ap->is_rip && ap->patch_end) {
            uint8_t opcode = g->code.data[ap->pos - 2];
            addend -= opcode == 0x81 ? 4 : 1;
        }
        elf_push_u64(&rela, (uint64_t)ap->pos);
        elf_push_u64(&rela, ((uint64_t)(uint32_t)sym << 32) | type);
        elf_push_i64(&rela, addend);
    }

    /* Pointer globals live in section data, so their R_X86_64_64 entries
     * must be in a .rela section that targets that section, not .text. */
    for (size_t i = 0; i < g->address_patches.n; i++) {
        AddrPatch* ap = &g->address_patches.v[i];
        if (!ap->is_global) {
            continue;
        }
        size_t si = 0;
        while (si < sects_n) {
            ElfSectRange* range = &sects[si];
            if (ap->pos >= range->start &&
                ap->pos + 8 <= (size_t)range->start + range->size) {
                break;
            }
            si++;
        }
        if (si == sects_n) {
            continue;
        }
        if (!sect_rela_name[si]) {
            sect_rela_name[si] = (uint32_t)shstrtab.len;
            buf_printf(&shstrtab, ".rela.w.%.*s", (int)sects[si].name.len,
                       ELF_STR(sects[si].name));
            buf_u8(&shstrtab, 0);
            sect_rela_count++;
        }
        uint64_t sym = 0;
        map_get_str(&data_sym, ap->key, &sym);
        elf_push_u64(&sect_rela[si], (uint64_t)(ap->pos - sects[si].start));
        elf_push_u64(&sect_rela[si], ((uint64_t)(uint32_t)sym << 32) | 1u);
        elf_push_i64(&sect_rela[si], 0);
    }

    /* ---------------------------------------------------------------- */
    /* .symtab (serialize ElfSym entries exactly like Rust)              */
    /* ---------------------------------------------------------------- */
    ElfSym* sym_arr = (ElfSym*)symbols.data;
    size_t sym_count = symbols.len / sizeof(ElfSym);
    for (size_t i = 0; i < sym_count; i++) {
        elf_push_u32(&symtab, sym_arr[i].name);
        elf_push_u8(&symtab, sym_arr[i].info);
        elf_push_u8(&symtab, sym_arr[i].other);
        elf_push_u16(&symtab, sym_arr[i].shndx);
        elf_push_u64(&symtab, sym_arr[i].value);
        elf_push_u64(&symtab, sym_arr[i].size);
    }

    size_t text_size = g->code.len;
    size_t rela_size = rela.len;
    size_t symtab_size = symtab.len;
    size_t strtab_size = strtab.len;
    size_t shstrtab_size = shstrtab.len;
    size_t rodata_size = 0; /* raw .rodata length; padding is not part of it */

    /* ---------------------------------------------------------------- */
    /* File body: .text, section data, .rodata, then tables               */
    /* ---------------------------------------------------------------- */
    size_t text_off = 64u + body.len;
    buf_bytes(&body, g->code.data, g->code.len);
    while ((64u + body.len) % 16u != 0u) {
        buf_u8(&body, 0);
    }

    for (size_t i = 0; i < sects_n; i++) {
        ElfSectRange* range = &sects[i];
        range->off = (uint64_t)(64u + body.len);
        if (!range->is_noinit) {
            elf_append_global_slice(&body, global_data, range->start, range->size);
        }
        uint64_t align = range->align ? (uint64_t)range->align : 1u;
        while ((64u + body.len) % (size_t)align != 0u) {
            buf_u8(&body, 0);
        }
    }

    size_t rodata_off = 64u + body.len;
    {
        size_t glen = global_data ? global_data->len : 0u;
        uint64_t rs = (uint64_t)user_data_end;
        if (rs > (uint64_t)glen) {
            rs = (uint64_t)glen;
        }
        rodata_size = glen - (size_t)rs;
        if (rodata_size) {
            buf_bytes(&body, global_data->data + (size_t)rs, rodata_size);
        }
        while ((64u + body.len) % 8u != 0u) {
            buf_u8(&body, 0);
        }
    }

    size_t rela_off = 64u + body.len;
    buf_bytes(&body, rela.data, rela.len);
    while ((64u + body.len) % 8u != 0u) {
        buf_u8(&body, 0);
    }
    for (size_t i = 0; i < sects_n; i++) {
        if (!sect_rela[i].len) {
            continue;
        }
        sect_rela_off[i] = (uint64_t)(64u + body.len);
        sect_rela_size[i] = sect_rela[i].len;
        buf_bytes(&body, sect_rela[i].data, sect_rela[i].len);
        while ((64u + body.len) % 8u != 0u) {
            buf_u8(&body, 0);
        }
    }
    size_t symtab_off = 64u + body.len;
    buf_bytes(&body, symtab.data, symtab.len);
    while ((64u + body.len) % 8u != 0u) {
        buf_u8(&body, 0);
    }
    size_t strtab_off = 64u + body.len;
    buf_bytes(&body, strtab.data, strtab.len);
    while ((64u + body.len) % 8u != 0u) {
        buf_u8(&body, 0);
    }
    size_t shstrtab_off = 64u + body.len;
    buf_bytes(&body, shstrtab.data, shstrtab.len);
    while ((64u + body.len) % 8u != 0u) {
        buf_u8(&body, 0);
    }
    size_t shoff = 64u + body.len;

    /* ---------------------------------------------------------------- */
    /* ELF header                                                        */
    /* ---------------------------------------------------------------- */
    static const uint8_t elf_ident[16] = {
        0x7F, 'E', 'L', 'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0};
    buf_bytes(&elf, elf_ident, sizeof(elf_ident));
    elf_push_u16(&elf, 1);                       /* e_type: ET_REL */
    elf_push_u16(&elf, 62);                      /* e_machine: EM_X86_64 */
    elf_push_u32(&elf, 1);                       /* e_version */
    elf_push_u64(&elf, 0);                       /* e_entry */
    elf_push_u64(&elf, 0);                       /* e_phoff */
    elf_push_u64(&elf, (uint64_t)shoff);         /* e_shoff */
    elf_push_u32(&elf, 0);                       /* e_flags */
    elf_push_u16(&elf, 64);                      /* e_ehsize */
    elf_push_u16(&elf, 0);                       /* e_phentsize */
    elf_push_u16(&elf, 0);                       /* e_phnum */
    elf_push_u16(&elf, 64);                      /* e_shentsize */
    elf_push_u16(&elf, (uint16_t)(8u + p->section_count + sect_rela_count)); /* e_shnum */
    elf_push_u16(&elf, (uint16_t)(6u + p->section_count)); /* e_shstrndx */

    buf_bytes(&elf, body.data, body.len);

    /* ---------------------------------------------------------------- */
    /* Section headers                                                   */
    /* ---------------------------------------------------------------- */
    elf_push_shdr(&elf, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    elf_push_shdr(&elf, n_text, 1, 6, 0, (uint64_t)text_off, (uint64_t)text_size,
                  0, 0, 16, 0);

    uint32_t symtab_shndx = (uint32_t)(2u + p->section_count + 1u + 1u);
    uint32_t text_shndx = 1u;

    for (size_t i = 0; i < sects_n; i++) {
        ElfSectRange* range = &sects[i];
        uint32_t sh_type = range->is_noinit ? 8u : 1u;
        uint64_t flags = range->is_noinit ? 3u : (range->is_ro ? 2u : 3u);
        elf_push_shdr(&elf, range->name_off, sh_type, flags, 0, range->off,
                      (uint64_t)range->size, 0, 0, (uint64_t)range->align, 0);
    }

    elf_push_shdr(&elf, n_rodata, 1, 2, 0, (uint64_t)rodata_off,
                  (uint64_t)rodata_size, 0, 0, 8, 0);

    elf_push_shdr(&elf, n_rela, 4, 64, 0, (uint64_t)rela_off, (uint64_t)rela_size,
                  symtab_shndx, text_shndx, 8, 24);
    elf_push_shdr(&elf, n_symtab, 2, 0, 0, (uint64_t)symtab_off,
                  (uint64_t)symtab_size, symtab_shndx + 1u, first_global, 8, 24);
    elf_push_shdr(&elf, n_strtab, 3, 0, 0, (uint64_t)strtab_off,
                  (uint64_t)strtab_size, 0, 0, 1, 0);
    elf_push_shdr(&elf, n_shstrtab, 3, 0, 0, (uint64_t)shstrtab_off,
                  (uint64_t)shstrtab_size, 0, 0, 1, 0);
    elf_push_shdr(&elf, n_note, 1, 0, 0, 0, 0, 0, 0, 1, 0);

    for (size_t i = 0; i < sects_n; i++) {
        if (!sect_rela[i].len) {
            continue;
        }
        elf_push_shdr(&elf, sect_rela_name[i], 4, 64, 0, sect_rela_off[i],
                      sect_rela_size[i], symtab_shndx, (uint32_t)(2u + i), 8, 24);
    }

    /* ---------------------------------------------------------------- */
    /* Hand back exactly the ELF bytes; caller resets g->code beforehand  */
    /* ---------------------------------------------------------------- */
    g_arena = saved_arena;
    /* The machine code has already been copied into the ELF .text section;
     * replace g->code so the Object result is exactly the ELF image. */
    g->code.len = 0;
    buf_bytes(&g->code, elf.data, elf.len);
    arena_free(&tmp_arena);
}

#undef ELF_SORT_BY_OFF
#undef ELF_GROW
#undef ELF_SYM_COUNT
#undef ELF_STR
