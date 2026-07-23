sc.true

sect.sfa_globals
    u64 vram_offset = 0;
EOS

fn bit_or(u64 a, u64 b) {
    return(a | b);
}

fn bit_and(u64 a, u64 b) {
    return(a & b);
}

fn bit_shl(u64 value, u64 shift) {
    return(value << shift);
}

fn bit_shr(u64 value, u64 shift) {
    return(value >> shift);
}

fn sfa_poke_u8(u8* ptr, u64 offset, u8 value) {
    u8* target = ptr + offset;
    u8 val*o = target;
    val = value;
}

fn sfa_peek_u8(u8* ptr, u64 offset) {
    u8* target = ptr + offset;
    u8 val*i = target;
    return(val);
}

fn sfa_write_reg(u64 base, u32 offset, u32 value) {
    u64 addr = base + offset;
    u32* ptr = (u32*)addr;
    u32 val*o = ptr;
    val = value;
}

fn sfa_read_reg(u64 base, u32 offset) {
    u64 addr = base + offset;
    u32* ptr = (u32*)addr;
    u32 val*i = ptr;
    return(val);
}

fn sfa_alloc_vram(SfaDevice* dev, u64 size) {
    u64 allocated_ptr = 0;
    if (dev->is_hosted == 1) {
        allocated_ptr = mloc(null, size);
    } else {
        u64 current_offset = sfa_globals:vram_offset;
        allocated_ptr = dev->vram_base + current_offset;
        sfa_globals:vram_offset = current_offset + size;
    }
    return(allocated_ptr);
}

fn sfa_init_device(SfaDevice* dev, u32 is_hosted) {
    dev->is_hosted = is_hosted;

    if (is_hosted == 1) {
        u8 path[64];
        path[0] = 47;   // '/'
        path[1] = 100;  // 'd'
        path[2] = 101;  // 'e'
        path[3] = 118;  // 'v'
        path[4] = 47;   // '/'
        path[5] = 100;  // 'd'
        path[6] = 114;  // 'r'
        path[7] = 105;  // 'i'
        path[8] = 47;   // '/'
        path[9] = 114;  // 'r'
        path[10] = 101; // 'e'
        path[11] = 110; // 'n'
        path[12] = 100; // 'd'
        path[13] = 101; // 'e'
        path[14] = 114; // 'r'
        path[15] = 68;  // 'D'
        path[16] = 49;  // '1'
        path[17] = 50;  // '2'
        path[18] = 56;  // '8'
        path[19] = 0;   // '\0'

        dev->fd = sys_open(path*adr, 2, 0);
        dev->mmio_base = mloc(null, 4096);
        dev->vram_base = mloc(null, 65536);
        dev->gart_base = mloc(null, 65536);
    } else {
        dev->fd = 0;
        dev->mmio_base = bmloc(sfa_const:BAREMETAL_MMIO_BASE, 4096);
        dev->vram_base = bmloc(sfa_const:BAREMETAL_VRAM_BASE, 1048576);
        dev->gart_base = 0;
    }

    dev->device_id = 26591;
}

fn sfa_create_queue(SfaDevice* dev, SfaQueue* q, u32 ring_size) {
    q->ring_size = ring_size;
    q->rptr = 0;
    q->wptr = 0;

    u64 buffer_bytes = ring_size * 4;
    q->ring_buffer_ptr = sfa_alloc_vram(dev, buffer_bytes);

    if (dev->is_hosted == 1) {
        q->doorbell_ptr = mloc(null, 8);
        q->doorbell_offset = 0;
    } else {
        q->doorbell_ptr = dev->mmio_base + 4096;
        q->doorbell_offset = 0;
    }
}

fn sfa_ring_write(SfaQueue* q, u32 dword) {
    u64 ring_base = q->ring_buffer_ptr;
    u32 current_wptr = q->wptr;
    u64 slot_addr = ring_base + current_wptr * 4;

    u32* slot_ptr = (u32*)slot_addr;
    u32 slot*o = slot_ptr;
    slot = dword;

    u32 next_wptr = current_wptr + 1;
    if (next_wptr >= q->ring_size) {
        next_wptr = 0;
    }
    q->wptr = next_wptr;
}

fn make_pm4_header(u32 opcode, u32 body_count) {
    u32 header = (3 << 30) | (opcode << 16) | (1 << 8) | body_count;
    return(header);
}

fn sfa_push_nop(SfaQueue* q) {
    u32 header = make_pm4_header(sfa_const:PACKET3_NOP, 0);
    sfa_ring_write(q, header);
    sfa_ring_write(q, 0);
}

fn sfa_push_set_sh_reg(SfaQueue* q, u32 reg_offset, u32 val) {
    u32 header = make_pm4_header(sfa_const:PACKET3_SET_SH_REG, 1);
    sfa_ring_write(q, header);

    u32 relative_reg = reg_offset - sfa_const:PACKET3_SET_SH_REG_START;
    sfa_ring_write(q, relative_reg);
    sfa_ring_write(q, val);
}

fn sfa_load_kernel(SfaDevice* dev, GpuKernel* kern, u8* raw_code, u32 code_size, u32 grid_size, u32 block_size) {
    u64 dev_code_ptr = sfa_alloc_vram(dev, code_size);

    u8* code_out_ptr = (u8*)dev_code_ptr;
    u8* code_in_ptr = raw_code;

    for (u32 i = 0; i < code_size; i = i + 1) {
        u8 val*i = code_in_ptr;
        u8 out*o = code_out_ptr;
        out = val;
        code_in_ptr = code_in_ptr + 1;
        code_out_ptr = code_out_ptr + 1;
    }

    kern->code_ptr = dev_code_ptr;
    kern->code_size = code_size;
    kern->grid_dim_x = grid_size;
    kern->block_dim_x = block_size;
}

fn sfa_dispatch_kernel(SfaQueue* q, GpuKernel* kern, u64 args_vram_addr) {
    u32 code_lo = (u32)(kern->code_ptr & 4294967295);
    u32 code_hi = (u32)(kern->code_ptr >> 32);

    sfa_push_set_sh_reg(q, sfa_const:mmCOMPUTE_PGM_LO, code_lo);
    sfa_push_set_sh_reg(q, sfa_const:mmCOMPUTE_PGM_HI, code_hi);

    u32 args_lo = (u32)(args_vram_addr & 4294967295);
    u32 args_hi = (u32)(args_vram_addr >> 32);

    sfa_push_set_sh_reg(q, sfa_const:mmCOMPUTE_USER_DATA_0, args_lo);
    sfa_push_set_sh_reg(q, sfa_const:mmCOMPUTE_USER_DATA_0 + 1, args_hi);

    u32 header = make_pm4_header(sfa_const:PACKET3_DISPATCH_DIRECT, 3);
    sfa_ring_write(q, header);
    sfa_ring_write(q, kern->grid_dim_x);
    sfa_ring_write(q, 1);
    sfa_ring_write(q, 1);
    sfa_ring_write(q, 1);
}

fn sfa_submit_queue(SfaQueue* q) {
    u32* doorbell_addr = (u32*)(q->doorbell_ptr + q->doorbell_offset);
    u32 db*o = doorbell_addr;
    db = q->wptr;
}
