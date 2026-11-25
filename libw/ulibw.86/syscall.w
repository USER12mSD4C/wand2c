sc.true

#import <syscall_constants>

// Low-level UTMS7 syscall wrappers. Numbers/ABI must match UTMS7 kernel. 

export fn syscall_error(u64 ret) -> u64 {
    if (ret > 18446744073709547520) {
        return(1);
    }
    return(0);
}

// I/O 
export fn sys_read(u64 fd, u8* buf, u64 size) -> u64 {
    return(syscall3(0, fd, buf, size));
}

export fn sys_write(u64 fd, u8* buf, u64 size) -> u64 {
    return(syscall3(1, fd, buf, size));
}

export fn sys_open(u8* path, u64 flags, u64 mode) -> u64 {
    return(syscall3(2, path, flags, mode));
}

export fn sys_close(u64 fd) -> u64 {
    return(syscall1(3, fd));
}

export fn sys_lseek(u64 fd, u64 offset, u64 whence) -> u64 {
    return(syscall3(8, fd, offset, whence));
}

export fn sys_ioctl(u64 fd, u64 request, u64 arg) -> u64 {
    return(syscall3(16, fd, request, arg));
}

export fn sys_poll(u8* fds, u64 nfds, i64 timeout) -> u64 {
    return(syscall3(7, fds, nfds, timeout));
}

export fn sys_pipe2(u8* pipefd, u64 flags) -> u64 {
    return(syscall2(293, pipefd, flags));
}

export fn sys_dup(u64 oldfd) -> u64 {
    return(syscall1(32, oldfd));
}

export fn sys_dup2(u64 oldfd, u64 newfd) -> u64 {
    return(syscall2(33, oldfd, newfd));
}

export fn sys_dup3(u64 oldfd, u64 newfd, u64 flags) -> u64 {
    return(syscall3(292, oldfd, newfd, flags));
}

export fn sys_fcntl(u64 fd, u64 cmd, u64 arg) -> u64 {
    return(syscall3(72, fd, cmd, arg));
}

export fn sys_pread64(u64 fd, u8* buf, u64 count, u64 offset) -> u64 {
    return(syscall4(17, fd, buf, count, offset));
}

export fn sys_pwrite64(u64 fd, u8* buf, u64 count, u64 offset) -> u64 {
    return(syscall4(18, fd, buf, count, offset));
}

export fn sys_readv(u64 fd, u8* iov, u64 iovcnt) -> u64 {
    return(syscall3(19, fd, iov, iovcnt));
}

export fn sys_writev(u64 fd, u8* iov, u64 iovcnt) -> u64 {
    return(syscall3(20, fd, iov, iovcnt));
}

// Filesystem 
export fn sys_stat(u8* path, u8* statbuf) -> u64 {
    return(syscall2(4, path, statbuf));
}

export fn sys_fstat(u64 fd, u8* statbuf) -> u64 {
    return(syscall2(5, fd, statbuf));
}

export fn sys_openat(u64 dirfd, u8* path, u64 flags, u64 mode) -> u64 {
    return(syscall4(257, dirfd, path, flags, mode));
}

export fn sys_newfstatat(u64 dirfd, u8* path, u8* statbuf, u64 flags) -> u64 {
    return(syscall4(262, dirfd, path, statbuf, flags));
}

export fn sys_getdents64(u64 fd, u8* dirp, u64 count) -> u64 {
    return(syscall3(217, fd, dirp, count));
}

export fn sys_mkdir(u8* path, u64 mode) -> u64 {
    return(syscall2(83, path, mode));
}

export fn sys_rmdir(u8* path) -> u64 {
    return(syscall1(84, path));
}

export fn sys_unlink(u8* path) -> u64 {
    return(syscall1(87, path));
}

export fn sys_rename(u8* oldpath, u8* newpath) -> u64 {
    return(syscall2(82, oldpath, newpath));
}

export fn sys_chdir(u8* path) -> u64 {
    return(syscall1(80, path));
}

export fn sys_getcwd(u8* buf, u64 size) -> u64 {
    return(syscall2(79, buf, size));
}

export fn sys_chroot(u8* path) -> u64 {
    // SYS_chroot is registered in UTMS7 kernel/core/syscall.c at number 66.
    return(syscall1(66, path));
}

export fn sys_symlink(u8* target, u8* linkpath) -> u64 {
    return(syscall2(88, target, linkpath));
}

export fn sys_readlink(u8* path, u8* buf, u64 size) -> u64 {
    return(syscall3(89, path, buf, size));
}

export fn sys_access(u8* path, u64 mode) -> u64 {
    return(syscall2(21, path, mode));
}

export fn sys_sync() -> u64 {
    return(syscall0(162));
}

export fn sys_fsync(u64 fd) -> u64 {
    return(syscall1(74, fd));
}

// Memory 
export fn sys_mmap(u64 addr, u64 length, u64 prot, u64 flags, u64 fd, u64 offset) -> u64 {
    return(syscall6(9, addr, length, prot, flags, fd, offset));
}

export fn sys_munmap(u64 addr, u64 length) -> u64 {
    return(syscall2(11, addr, length));
}

export fn sys_mprotect(u64 addr, u64 len, u64 prot) -> u64 {
    return(syscall3(10, addr, len, prot));
}

export fn sys_brk(u64 addr) -> u64 {
    return(syscall1(12, addr));
}

// Process 
export fn sys_getpid() -> u64 {
    return(syscall0(39));
}

export fn sys_getppid() -> u64 {
    return(syscall0(110));
}

export fn sys_gettid() -> u64 {
    return(syscall0(186));
}

export fn sys_fork() -> u64 {
    return(syscall0(57));
}

export fn sys_execve(u8* path, u8* argv, u8* envp) -> u64 {
    return(syscall3(59, path, argv, envp));
}

export fn sys_wait4(u64 pid, u64* status, u64 options, u8* rusage) -> u64 {
    return(syscall4(61, pid, status, options, rusage));
}

export fn sys_exit(u64 code) {
    syscall1(60, code);
}

export fn sys_exit_group(u64 code) {
    syscall1(231, code);
}

export fn sys_kill(u64 pid, u64 sig) -> u64 {
    return(syscall2(62, pid, sig));
}

export fn sys_sched_yield() -> u64 {
    return(syscall0(24));
}

// Identity 
export fn sys_getuid() -> u64 {
    return(syscall0(102));
}

export fn sys_getgid() -> u64 {
    return(syscall0(104));
}

export fn sys_geteuid() -> u64 {
    return(syscall0(107));
}

export fn sys_getegid() -> u64 {
    return(syscall0(108));
}

// Time/info 
export fn sys_gettime_ticks() -> u64 {
    return(syscall0(1009));
}

export fn sys_uname(u8* buf) -> u64 {
    return(syscall1(63, buf));
}

// UTMS7-specific 
export fn u_sys_disk_list(u8* disks, u64 max) -> u64 {
    return(syscall2(1000, disks, max));
}

export fn u_sys_part_mount(u8* dev, u8* point) -> u64 {
    return(syscall3(1001, dev, point, 0));
}

export fn u_sys_part_mount_fs(u8* dev, u8* point, u8* fstype) -> u64 {
    return(syscall3(1001, dev, point, fstype));
}

export fn u_sys_part_umount() -> u64 {
    return(syscall0(1002));
}

export fn u_sys_part_format(u8* dev, u8* fstype) -> u64 {
    return(syscall2(1003, dev, fstype));
}

export fn u_sys_disk_table(u8* dev, u64 gpt) -> u64 {
    return(syscall2(1004, dev, gpt));
}

export fn u_sys_part_create(u8* dev, u64 size_mb, u64 type) -> u64 {
    return(syscall3(1005, dev, size_mb, type));
}

export fn u_sys_part_delete(u8* dev) -> u64 {
    return(syscall1(1006, dev));
}

export fn u_sys_ps(u8* processes, u64 max) -> u64 {
    return(syscall2(1008, processes, max));
}

export fn u_sys_clear() -> u64 {
    return(syscall0(1010));
}

export fn u_sys_setcolor(u64 fg, u64 bg) -> u64 {
    return(syscall2(1011, fg, bg));
}

export fn u_sys_meminfo(u64* total, u64* used, u64* free_mem) -> u64 {
    return(syscall3(1012, total, used, free_mem));
}

export fn u_sys_sleep(u64 ms) -> u64 {
    return(syscall1(1020, ms));
}

export fn u_sys_setpgid(u64 pid, u64 pgid) -> u64 {
    return(syscall2(1021, pid, pgid));
}

export fn u_sys_set_term_pgid(u64 pid) -> u64 {
    return(syscall1(1022, pid));
}

// mmap-backed allocator for simple use; header stores total size. 
export fn mloc(u64 hint, u64 size) -> u8* {
    u64 total = size + 8;
    u64 addr = sys_mmap(hint, total, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, 18446744073709551615, 0);

    if (syscall_error(addr) == 1) {
        return(null);
    }

    u64* header = (u64*)addr;
    header[0] = total;

    return((u8*)(addr + 8));
}

export fn mfree_map(u8* ptr) {
    if (ptr == null) {
        return;
    }
    u64 addr = (u64)(ptr - 8);
    u64* header = (u64*)addr;
    u64 size = header[0];
    sys_munmap(addr, size);
}
