sc.true

#import <syscall>
#import <syscall_structs>

struct SignalHandler {
    u64 handler;
    u64 flags;
    u64 mask;
}

fn signal_init_handler(SignalHandler* handler) {
    handler->handler = 0;
    handler->flags = 0;
    handler->mask = 0;
}

fn signal_set_handler(SignalHandler* handler, u64 func_ptr) {
    handler->handler = func_ptr;
}

fn signal_set_flags(SignalHandler* handler, u64 flags) {
    handler->flags = flags;
}

fn signal_add_to_mask(SignalHandler* handler, u64 signum) {
    u64 bit = 1;
    bit = bit << (signum - 1);
    handler->mask = handler->mask | bit;
}

fn signal_install(SignalHandler* handler, u64 signum) -> i64 {
    sigaction act;
    sigaction oldact;

    act.sa_handler = handler->handler;
    act.sa_flags = handler->flags;
    act.sa_mask = handler->mask;
    act.sa_restorer = 0;

    i64 ret = sys_rt_sigaction(signum, act*adr, oldact*adr, 8);
    if (syscall_error(ret) == 1) {
        return(-1);
    }
    return(0);
}

fn signal_ignore(u64 signum) -> i64 {
    sigaction act;
    sigaction oldact;

    act.sa_handler = 1;
    act.sa_flags = 0;
    act.sa_mask = 0;
    act.sa_restorer = 0;

    i64 ret = sys_rt_sigaction(signum, act*adr, oldact*adr, 8);
    if (syscall_error(ret) == 1) {
        return(-1);
    }
    return(0);
}

fn signal_default(u64 signum) -> i64 {
    sigaction act;
    sigaction oldact;

    act.sa_handler = 0;
    act.sa_flags = 0;
    act.sa_mask = 0;
    act.sa_restorer = 0;

    i64 ret = sys_rt_sigaction(signum, act*adr, oldact*adr, 8);
    if (syscall_error(ret) == 1) {
        return(-1);
    }
    return(0);
}

fn signal_send(u64 pid, u64 signum) -> i64 {
    i64 ret = sys_kill(pid, signum);
    if (syscall_error(ret) == 1) {
        return(-1);
    }
    return(0);
}

fn signal_block(u64 signum) -> i64 {
    u8 set[128];
    u8 oldset[128];

    memset(set*adr, 0, 128);

    u64 byte_idx = (signum - 1) / 8;
    u64 bit_idx = (signum - 1) % 8;

    if (byte_idx < 128) {
        set[byte_idx] = set[byte_idx] | (1 << bit_idx);
    }

    i64 ret = sys_rt_sigprocmask(0, set*adr, oldset*adr, 128);
    if (syscall_error(ret) == 1) {
        return(-1);
    }
    return(0);
}

fn signal_unblock(u64 signum) -> i64 {
    u8 set[128];
    u8 oldset[128];

    memset(set*adr, 0, 128);

    u64 byte_idx = (signum - 1) / 8;
    u64 bit_idx = (signum - 1) % 8;

    if (byte_idx < 128) {
        set[byte_idx] = set[byte_idx] | (1 << bit_idx);
    }

    i64 ret = sys_rt_sigprocmask(1, set*adr, oldset*adr, 128);
    if (syscall_error(ret) == 1) {
        return(-1);
    }
    return(0);
}
