sc.true

#import <syscall>
#import <string>

// Simple mmap-backed arena allocator for UTMS7. 

sect.heap
    u8* arena_start = null;
    u64 arena_size = 0;
    u64 offset = 0;
EOS

fn mem_grow(u64 need) -> u64 {
    u64 add = need;
    if (add < 1048576) { add = 1048576; }
    u64 new_size = heap:arena_size + add;
    u64 nm = sys_mmap(0, new_size, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, 18446744073709551615, 0);
    if (syscall_error(nm) == 1) { return(0); }
    u8* new_mem = (u8*)nm;
    if (heap:arena_start != null && heap:offset > 0) {
        memcpy(new_mem, heap:arena_start, heap:offset);
        sys_munmap(heap:arena_start, heap:arena_size);
    }
    heap:arena_start = new_mem;
    heap:arena_size = new_size;
    return(1);
}

export fn mem_init(u64 initial_size) {
    if (heap:arena_start != null) { return; }
    u64 sz = initial_size;
    if (sz < 65536) { sz = 65536; }
    u64 raw = sys_mmap(0, sz, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, 18446744073709551615, 0);
    if (syscall_error(raw) == 1) {
        sys_exit(1);
    }
    heap:arena_start = (u8*)raw;
    heap:arena_size = sz;
    heap:offset = 0;
}

export fn malloc(u64 size) -> u8* {
    if (heap:arena_start == null) {
        mem_init(1048576);
    }
    u64 aligned_size = size;
    u64 rem = size % 8;
    if (rem != 0) {
        aligned_size = size + (8 - rem);
    }
    u64 total_block_size = aligned_size + 16;

    u8* current = heap:arena_start;
    u8* end = heap:arena_start + heap:offset;
    while (current < end) {
        u64* p_size*i = current;
        u64 b_size = p_size;
        u64* p_free*i = current + 8;
        u64 b_free = p_free;
        if (b_free == 1) {
            if (b_size >= total_block_size) {
                u64* p_free_out*o = current + 8;
                p_free_out = 0;
                return(current + 16);
            }
        }
        current = current + b_size;
    }

    if ((heap:offset + total_block_size) > heap:arena_size) {
        if (mem_grow(total_block_size) == 0) { return(null); }
    }

    u8* block = heap:arena_start + heap:offset;
    u64* p_size_out*o = block;
    p_size_out = total_block_size;
    u64* p_free_out2*o = block + 8;
    p_free_out2 = 0;
    heap:offset = heap:offset + total_block_size;
    return(block + 16);
}

export fn calloc(u64 num, u64 size) -> u8* {
    u64 total = num * size;
    u8* ptr = malloc(total);
    if (ptr != null) {
        memset(ptr, 0, total);
    }
    return(ptr);
}

export fn mfree(u8* ptr) {
    if (ptr != null) {
        u8* block = ptr - 16;
        u64* p_free*o = block + 8;
        p_free = 1;
    }
}

export fn mrealloc(u8* ptr, u64 new_size) -> u8* {
    if (ptr == null) { return(malloc(new_size)); }
    if (new_size == 0) { mfree(ptr); return(null); }

    u8* block = ptr - 16;
    u64* p_size*i = block;
    u64 old_block_size = p_size;
    u64 old_usable = old_block_size - 16;

    u64 aligned_new = new_size;
    u64 rem = new_size % 8;
    if (rem != 0) { aligned_new = new_size + (8 - rem); }
    u64 new_block_size = aligned_new + 16;
    if (new_block_size <= old_block_size) { return(ptr); }

    u8* new_ptr = malloc(new_size);
    if (new_ptr == null) { return(null); }
    u64 copy_size = old_usable;
    if (new_size < old_usable) { copy_size = new_size; }
    memcpy(new_ptr, ptr, copy_size);
    mfree(ptr);
    return(new_ptr);
}

export fn mfree_all() {
    heap:offset = 0;
}
