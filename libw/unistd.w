sc.true

#import <syscall>
#import <mem>
#import <string>

const SYS_PIPE = 22;
const SYS_FORK = 57;
const SYS_EXECVE = 59;

sect.popen_states
    i64 pids[64] = 0;
    i64 fds[64] = 0;
    u64 count = 0;
EOS

fn read_u64(u8* p) -> u64 {
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

fn read_u16(u8* p) -> u16 {
    u16 val = 0;
    val = val | ((u16)p[0]);
    val = val | (((u16)p[1]) << 8);
    return(val);
}

fn get_cpu_count() -> i64 {
    i64 fd = sys_open("/proc/cpuinfo", 0, 0);
    if (syscall_error(fd) == 1) {
        return(1);
    }

    u8 buf[4096];
    i64 count = 0;
    i64 bytes = 0;
    i64 running = 1;

    while (running == 1) {
        bytes = sys_read(fd, buf, 4096);
        if (bytes <= 0) {
            running = 0;
        } else {
            u64 i = 0;
            while (i < (u64)bytes - 8) {
                u64 val = read_u64(buf + i);
                if (val == 0x6f737365636f7270) {
                    if (buf[i + 8] == 114) {
                        count = count + 1;
                    }
                }
                i = i + 1;
            }
        }
    }

    sys_close(fd);
    if (count == 0) {
        return(1);
    }
    return(count);
}

fn popen(u8* command) -> i64 {
    i32 pipefd[2];
    u64 ret = syscall2(SYS_PIPE, pipefd*adr, 0);
    if (syscall_error(ret) == 1) {
        return(-1);
    }

    i64 pid = syscall0(SYS_FORK);
    if (pid == 0) {
        syscall2(33, pipefd[1], 1);
        syscall1(3, pipefd[0]);
        syscall1(3, pipefd[1]);

        u8* argv[4];
        argv[0] = "/bin/sh";
        argv[1] = "-c";
        argv[2] = command;
        argv[3] = null;

        u8* envp[1];
        envp[0] = null;

        syscall3(SYS_EXECVE, "/bin/sh", argv*adr, envp*adr);
        syscall1(60, 127);
    }

    syscall1(3, pipefd[1]);

    if (popen_states:count < 64) {
        u64 idx = popen_states:count;
        i64* pids_ptr = popen_states:pids*adr;
        i64* fds_ptr = popen_states:fds*adr;
        pids_ptr[idx] = pid;
        fds_ptr[idx] = pipefd[0];
        popen_states:count = popen_states:count + 1;
    }

    return(pipefd[0]);
}

fn pclose(i64 fd) -> i64 {
    i64 target_pid = -1;
    u64 i = 0;
    i64 found = 0;

    i64* pids_ptr = popen_states:pids*adr;
    i64* fds_ptr = popen_states:fds*adr;

    while (i < popen_states:count) {
        if (found == 0) {
            i64 cur_fd = fds_ptr[i];
            if (cur_fd == fd) {
                target_pid = pids_ptr[i];
                found = 1;
            }
        }
        i = i + 1;
    }

    syscall1(3, fd);

    u64 status = 0;
    if (target_pid > 0) {
        syscall4(61, target_pid, status*adr, 0, null);
    } else {
        syscall4(61, 4294967295, status*adr, 0, null);
    }

    return(0);
}

fn opendir(u8* path) -> DIR* {
    i64 fd = sys_open(path, 65536, 0);
    if (syscall_error(fd) == 1) {
        return(null);
    }

    DIR* dir = malloc(sizeof(DIR));
    if (dir == null) {
        sys_close(fd);
        return(null);
    }

    dir->fd = fd;
    dir->data_pos = 0;
    dir->data_len = 0;

    return(dir);
}

fn readdir(DIR* dir) -> u8* {
    while (1) {
        if (dir->data_pos >= dir->data_len) {
            i64 n = sys_getdents64(dir->fd, dir->data, 2048);
            if (n <= 0) {
                return(null);
            }
            dir->data_pos = 0;
            dir->data_len = (u64)n;
        }

        u64 offset = dir->data_pos;
        u16 reclen = read_u16(dir->data + offset + 16);

        u64 name_len = (u64)reclen - 19;
        if (name_len >= 256) {
            name_len = 255;
        }

        u64 k = 0;
        while (k < name_len) {
            dir->name[k] = dir->data[offset + 19 + k];
            k = k + 1;
        }
        dir->name[name_len] = 0;

        dir->data_pos = dir->data_pos + (u64)reclen;

        if (strcmp(dir->name, ".") != 0) {
            if (strcmp(dir->name, "..") != 0) {
                return(dir->name);
            }
        }
    }
}

fn closedir(DIR* dir) {
    if (dir != null) {
        sys_close(dir->fd);
        mfree(dir);
    }
}
