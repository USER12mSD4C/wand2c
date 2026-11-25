sc.true

#import <syscall>
#import <syscall_constants>
#import <process>

export fn process_fork() -> i64 {
    u64 pid = sys_fork();
    if (syscall_error(pid) == 1) {
        return(0 - (i64)(0 - pid));
    }
    return((i64)pid);
}

export fn process_exec(u8* path, u8* argv, u8* envp) -> i64 {
    u64 ret = sys_execve(path, argv, envp);
    if (syscall_error(ret) == 1) {
        return(0 - (i64)(0 - ret));
    }
    return(0);
}

export fn process_wait(i64 pid, ProcessInfo* info) -> i64 {
    u64 status = 0;
    u64 ret = sys_wait4((u64)pid, status*adr, 0, null);
    if (syscall_error(ret) == 1) {
        return(0 - (i64)(0 - ret));
    }

    info->pid = (i64)ret;
    info->status = (i64)status;
    if ((status & 127) == 0) {
        info->exit_code = (i64)((status >> 8) & 255);
    } else {
        info->exit_code = -1;
    }
    return((i64)ret);
}

export fn process_exit(i64 code) {
    sys_exit((u64)code);
}

export fn process_kill(i64 pid, u64 signum) -> i64 {
    u64 ret = sys_kill((u64)pid, signum);
    if (syscall_error(ret) == 1) {
        return(0 - (i64)(0 - ret));
    }
    return(0);
}

export fn process_get_pid() -> i64 {
    return((i64)sys_getpid());
}

export fn process_get_ppid() -> i64 {
    return((i64)sys_getppid());
}

export fn process_sleep_ms(u64 ms) -> u64 {
    return(u_sys_sleep(ms));
}

export fn process_yield() -> u64 {
    return(sys_sched_yield());
}
