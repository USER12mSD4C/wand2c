sc.true

#import <mem>
#import <string>
#import <syscall>
#import <vector>

fn read_u64_ptr(u64 ptr) -> u64 {
    u8* p = ptr;
    u64 val = 0;
    val = val | ((u64)p[0]);
    val = val | (((u64)p[1]) << 8);
    val = val | (((u64)p[2]) << 16);
    val = val | (((u64)p[3]) << 24);
    val = val | (((u64)p[4]) << 32);
    val = val | (((u64)p[5]) << 40);
    val = val | (((u64)p[6]) << 48);
    val = val | (((u64)p[7]) << 56);
    return(val);
}

fn write_u64_ptr(u64 ptr, u64 val) {
    u8* p = ptr;
    p[0] = val;
    p[1] = val >> 8;
    p[2] = val >> 16;
    p[3] = val >> 24;
    p[4] = val >> 32;
    p[5] = val >> 40;
    p[6] = val >> 48;
    p[7] = val >> 56;
}

fn xmalloc(u64 size) -> u64 {
    u64 p = malloc(size);
    if (p == 0) {
        sys_exit(1);
    }
    return(p);
}

fn xstrdup(u64 s) -> u64 {
    u64 len = strlen(s);
    u64 p = xmalloc(len + 1);
    strcpy(p, s);
    return(p);
}

fn strvec_init(StrVec* v) {
    v->items = 0;
    v->count = 0;
    v->capacity = 0;
}

fn strvec_add(StrVec* v, u64 str_ptr) {
    if (v->count >= v->capacity) {
        u64 new_capacity = v->capacity * 2;
        if (new_capacity == 0) {
            new_capacity = 4;
        }
        u64 new_size = new_capacity * 8;
        u64 old_ptr = v->items;
        u64 new_ptr = 0;

        if (old_ptr == 0) {
            new_ptr = xmalloc(new_size);
        } else {
            new_ptr = mrealloc(old_ptr, new_size);
        }

        v->items = new_ptr;
        v->capacity = new_capacity;
    }

    u64 idx = v->count;
    u64 items_ptr = v->items;
    write_u64_ptr(items_ptr + idx * 8, str_ptr);
    v->count = v->count + 1;
}

fn strvec_contains(StrVec* v, u64 str_ptr) -> i64 {
    u64 i = 0;
    while (i < v->count) {
        u64 item_ptr = read_u64_ptr(v->items + i * 8);
        if (strcmp(item_ptr, str_ptr) == 0) {
            return(1);
        }
        i = i + 1;
    }
    return(0);
}

fn strvec_free(StrVec* v) {
    u64 i = 0;
    while (i < v->count) {
        u64 item_ptr = read_u64_ptr(v->items + i * 8);
        mfree(item_ptr);
        i = i + 1;
    }
    if (v->items != 0) {
        mfree(v->items);
    }
    strvec_init(v);
}

fn strvec_clear(StrVec* v) {
    u64 i = 0;
    while (i < v->count) {
        u64 item_ptr = read_u64_ptr(v->items + i * 8);
        mfree(item_ptr);
        i = i + 1;
    }
    v->count = 0;
}

fn strvec_pop(StrVec* v) {
    if (v->count > 0) {
        u64 idx = v->count - 1;
        u64 item_ptr = read_u64_ptr(v->items + idx * 8);
        mfree(item_ptr);
        v->count = v->count - 1;
    }
}
