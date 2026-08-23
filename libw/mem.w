sc.true
#import <syscall>
#import <string>

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
    u64 total_block_size = aligned_size + 16;

    // 1. Поиск свободного блока в списке (First-Fit)
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

    if ((heap:offset + total_block_size) <= heap:arena_size) {
        u8* block = heap:arena_start + heap:offset;

        // Записываем заголовок: размер блока
        u64* p_size_out*o = block;
        p_size_out = total_block_size;

        // Записываем заголовок: флаг свободы (0 = занят)
        u64* p_free_out*o = block + 8;
        p_free_out = 0;

        heap:offset = heap:offset + total_block_size;
        return(block + 16);
    }
    return(null);
}

fn mfree(u8* ptr) {
    if (ptr != null) {
        u8* block = ptr - 16;
        u64* p_free*o = block + 8;
        p_free = 1;
    }
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

fn mrealloc(u8* ptr, u64 new_size) -> u8* {
    if (ptr == null) {
        return(malloc(new_size));
    }
    if (new_size == 0) {
        mfree(ptr);
        return(null);
    }

    u8* block = ptr - 16;
    u64* p_size*i = block;
    u64 old_block_size = p_size;
    u64 old_usable = old_block_size - 16;

    u64 aligned_new = new_size;
    u64 rem = new_size % 8;
    if (rem != 0) {
        aligned_new = new_size + (8 - rem);
    }
    u64 new_block_size = aligned_new + 16;

    if (new_block_size <= old_block_size) {
        return(ptr);
    }

    u8* new_ptr = malloc(new_size);
    if (new_ptr == null) {
        return(null);
    }

    u64 copy_size = old_usable;
    if (new_size < old_usable) {
        copy_size = new_size;
    }
    memcpy(new_ptr, ptr, copy_size);
    mfree(ptr);
    return(new_ptr);
}
