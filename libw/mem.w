sc.true

#import string

sect.heap
    u8* arena_start = null;
    u64 arena_size = 0;
    u64 offset = 0;
EOS

fn mem_init(u64 initial_size) {
    void* raw_mem = mloc(null, initial_size);
    heap:arena_start = raw_mem;
    heap:arena_size = initial_size;
    heap:offset = 0;
}

fn malloc(u64 size) {
    u64 aligned_size = size;
    u64 rem = size % 8;
    if (rem != 0) {
        aligned_size = size + (8 - rem);
    }

    if ((heap:offset + aligned_size) <= heap:arena_size) {
        u8* res = heap:arena_start + heap:offset;
        heap:offset = heap:offset + aligned_size;
        return(res);
    }
    return(null);
}

fn calloc(u64 num, u64 size) {
    u64 total = num * size;
    void* ptr = malloc(total);
    if (ptr != null) {
        memset(ptr, 0, total);
    }
    return(ptr);
}

fn mfree_all() {
    heap:offset = 0;
}
