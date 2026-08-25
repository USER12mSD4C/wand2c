sc.true

#import <syscall>
#import <mem>
#import <vector>

struct ProcessInfo {
    i64 pid;
    i64 status;
    i64 exit_code;
}

fn process_fork() -> i64 {
    i64 pid = sys_fork();
    if (syscall_error(pid) == 1) {
        return(-1);
    }
    return(pid);
}

fn process_exec(u8* path, u8* argv, u8* envp) -> i64 {
    i64 ret = sys_execve(path, argv, envp);
    if (syscall_error(ret) == 1) {
        return(-1);
    }
    return(0);
}

fn process_wait(i64 pid, ProcessInfo* info) -> i64 {
    u64 status = 0;
    i64 ret = sys_wait4(pid, status*adr, 0, null);

    if (syscall_error(ret) == 1) {
        return(-1);
    }

    info->pid = ret;
    info->status = status;

    if ((status & 0x7f) == 0) {
        info->exit_code = (status >> 8) & 0xff;
    } else {
        info->exit_code = -1;
    }

    return(ret);
}

fn process_wait_all(ProcessInfo* info) -> i64 {
    return(process_wait(-1, info));
}

fn process_exit(i64 code) {
    sys_exit(code);
}

fn process_kill(i64 pid, u64 signum) -> i64 {
    i64 ret = sys_kill(pid, signum);
    if (syscall_error(ret) == 1) {
        return(-1);
    }
    return(0);
}

fn process_get_pid() -> i64 {
    i64 pid = sys_getpid();
    return(pid);
}

struct ProcessGroup {
    i64* pids;
    i64 count;
    i64 capacity;
    i64 running;
}

fn process_group_init(ProcessGroup* group) {
    group->pids = null;
    group->count = 0;
    group->capacity = 0;
    group->running = 0;
}

fn process_group_add(ProcessGroup* group, i64 pid) {
    if (group->count >= group->capacity) {
        i64 new_cap = group->capacity * 2;
        if (new_cap == 0) {
            new_cap = 16;
        }

        i64* new_pids = mrealloc(group->pids, new_cap * sizeof(i64));
        if (new_pids == null) {
            return;
        }

        group->pids = new_pids;
        group->capacity = new_cap;
    }

    group->pids[group->count] = pid;
    group->count = group->count + 1;
    group->running = group->running + 1;
}

fn process_group_wait_any(ProcessGroup* group, ProcessInfo* info) -> i64 {
    if (group->running == 0) {
        return(-1);
    }

    i64 ret = process_wait_all(info);
    if (ret < 0) {
        return(ret);
    }

    for (i64 i = 0; i < group->count; i = i + 1) {
        if (group->pids[i] == ret) {
            group->running = group->running - 1;
            break;
        }
    }

    return(ret);
}

fn process_group_wait_all(ProcessGroup* group) {
    ProcessInfo info;

    while (group->running > 0) {
        i64 ret = process_group_wait_any(group, info*adr);
        if (ret < 0) {
            break;
        }
    }
}

fn process_group_kill_all(ProcessGroup* group, u64 signum) {
    for (i64 i = 0; i < group->count; i = i + 1) {
        if (group->pids[i] > 0) {
            process_kill(group->pids[i], signum);
        }
    }
}

fn process_group_free(ProcessGroup* group) {
    if (group->pids != null) {
        mfree(group->pids);
        group->pids = null;
    }
    group->count = 0;
    group->capacity = 0;
    group->running = 0;
}

fn process_exec_shell(u8* cmd) -> i64 {
    i64 pid = process_fork();

    if (pid < 0) {
        return(-1);
    }

    if (pid == 0) {
        u8* argv[4];
        argv[0] = "sh";
        argv[1] = "-c";
        argv[2] = cmd;
        argv[3] = null;

        u8* envp[1];
        envp[0] = null;

        process_exec("/bin/sh", argv*adr, envp*adr);
        process_exit(127);
    }

    return(pid);
}

fn process_exec_shell_wait(u8* cmd) -> i64 {
    i64 pid = process_exec_shell(cmd);

    if (pid < 0) {
        return(-1);
    }

    ProcessInfo info;
    i64 ret = process_wait(pid, info*adr);

    if (ret < 0) {
        return(-1);
    }

    return(info.exit_code);
}
