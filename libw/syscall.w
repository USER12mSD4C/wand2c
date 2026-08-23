sc.true

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

export fn sys_stat(u8* path, u8* statbuf) -> u64 {
    return(syscall3(4, path, statbuf, 0));
}

export fn sys_fstat(u64 fd, u8* statbuf) -> u64 {
    return(syscall3(5, fd, statbuf, 0));
}

export fn sys_lstat(u8* path, u8* statbuf) -> u64 {
    return(syscall3(6, path, statbuf, 0));
}

export fn sys_lseek(u64 fd, u64 offset, u64 whence) -> u64 {
    return(syscall3(8, fd, offset, whence));
}

export fn sys_ioctl(u64 fd, u64 request, u64 arg) -> u64 {
    return(syscall3(16, fd, request, arg));
}

export fn sys_dup2(u64 oldfd, u64 newfd) -> u64 {
    return(syscall2(33, oldfd, newfd));
}

export fn sys_nanosleep(u8* req, u8* rem) -> u64 {
    return(syscall2(35, req, rem));
}

export fn sys_getpid() -> u64 {
    return(syscall0(39));
}

export fn sys_fork() -> u64 {
    return(syscall0(57));
}

export fn sys_execve(u8* path, u8* argv, u8* envp) -> u64 {
    return(syscall3(59, path, argv, envp));
}

export fn sys_exit(u64 code) {
    syscall1(60, code);
}

export fn sys_wait4(u64 pid, u64* status, u64 options, u8* rusage) -> u64 {
    return(syscall4(61, pid, status, options, rusage));
}

export fn sys_kill(u64 pid, u64 sig) -> u64 {
    return(syscall2(62, pid, sig));
}

export fn sys_unlink(u8* path) -> u64 {
    return(syscall1(87, path));
}

export fn sys_readlink(u8* path, u8* buf, u64 size) -> u64 {
    return(syscall3(89, path, buf, size));
}

export fn sys_chmod(u8* path, u64 mode) -> u64 {
    return(syscall2(90, path, mode));
}

export fn sys_chown(u8* path, u64 uid, u64 gid) -> u64 {
    return(syscall3(92, path, uid, gid));
}

export fn sys_getuid() -> u64 {
    return(syscall0(102));
}

export fn sys_syslog(u64 kind, u8* buf, u64 len) -> u64 {
    return(syscall3(103, kind, buf, len));
}

export fn sys_getgid() -> u64 {
    return(syscall0(104));
}

export fn sys_setuid(u64 uid) -> u64 {
    return(syscall1(105, uid));
}

export fn sys_setgid(u64 gid) -> u64 {
    return(syscall1(106, gid));
}

export fn sys_geteuid() -> u64 {
    return(syscall0(107));
}

export fn sys_getegid() -> u64 {
    return(syscall0(108));
}

export fn sys_setpgid(u64 pid, u64 pgid) -> u64 {
    return(syscall2(109, pid, pgid));
}

export fn sys_setsid() -> u64 {
    return(syscall0(112));
}

export fn sys_getrlimit(u64 resource, u8* rlim) -> u64 {
    return(syscall2(97, resource, rlim));
}

export fn sys_setrlimit(u64 resource, u8* rlim) -> u64 {
    return(syscall2(160, resource, rlim));
}

export fn sys_mount(u8* source, u8* target, u8* fstype, u64 flags, u8* data) -> u64 {
    return(syscall5(165, source, target, fstype, flags, data));
}

export fn sys_umount2(u8* target, u64 flags) -> u64 {
    return(syscall2(166, target, flags));
}

export fn sys_reboot(u64 magic1, u64 magic2, u64 cmd, u8* arg) -> u64 {
    return(syscall4(169, magic1, magic2, cmd, arg));
}

export fn sys_prctl(u64 option, u64 a2, u64 a3, u64 a4, u64 a5) -> u64 {
    return(syscall5(157, option, a2, a3, a4, a5));
}

export fn sys_getdents64(u64 fd, u8* dirp, u64 count) -> u64 {
    return(syscall3(217, fd, dirp, count));
}

export fn sys_clock_gettime(u64 clockid, u8* tp) -> u64 {
    return(syscall2(228, clockid, tp));
}

export fn sys_rt_sigaction(u64 signum, u8* act, u8* oldact, u64 sigsetsize) -> u64 {
    return(syscall4(13, signum, act, oldact, sigsetsize));
}

export fn sys_rt_sigprocmask(u64 how, u8* set, u8* oldset, u64 sigsetsize) -> u64 {
    return(syscall4(14, how, set, oldset, sigsetsize));
}

export fn sys_unshare(u64 flags) -> u64 {
    return(syscall1(272, flags));
}

export fn sys_setns(u64 fd, u64 nstype) -> u64 {
    return(syscall2(308, fd, nstype));
}

export fn sys_exit_group(u64 code) {
    syscall1(231, code);
}

export fn sys_socket(u64 domain, u64 type, u64 protocol) -> u64 {
    return(syscall3(41, domain, type, protocol));
}

export fn sys_connect(u64 sockfd, u8* addr, u64 addrlen) -> u64 {
    return(syscall3(42, sockfd, addr, addrlen));
}

export fn sys_accept(u64 sockfd, u8* addr, u64* addrlen) -> u64 {
    return(syscall3(43, sockfd, addr, addrlen));
}

export fn sys_sendto(u64 sockfd, u8* buf, u64 len, u64 flags, u8* dest_addr, u64 addrlen) -> u64 {
    return(syscall6(44, sockfd, buf, len, flags, dest_addr, addrlen));
}

export fn sys_recvfrom(u64 sockfd, u8* buf, u64 len, u64 flags, u8* src_addr, u64* addrlen) -> u64 {
    return(syscall6(45, sockfd, buf, len, flags, src_addr, addrlen));
}

export fn sys_bind(u64 sockfd, u8* addr, u64 addrlen) -> u64 {
    return(syscall3(49, sockfd, addr, addrlen));
}

export fn sys_listen(u64 sockfd, u64 backlog) -> u64 {
    return(syscall2(50, sockfd, backlog));
}

export fn sys_epoll_create(u64 flags) -> u64 {
    return(syscall1(291, flags));
}

export fn sys_epoll_ctl(u64 epfd, u64 op, u64 fd, u8* event) -> u64 {
    return(syscall4(233, epfd, op, fd, event));
}

export fn sys_epoll_wait(u64 epfd, u8* events, u64 maxevents, u64 timeout) -> u64 {
    return(syscall4(232, epfd, events, maxevents, timeout));
}

export fn mloc(u64 hint, u64 size) -> u8* {
    u64 total = size + 8;
    u64 addr = syscall6(9, hint, total, 3, 34, 4294967295, 0);
    if (addr == 18446744073709551615) {
        return(null);
    }
    u64* header = addr;
    header[0] = total;
    return(addr + 8);
}

export fn mfree(u8* ptr) {
    u64 addr = ptr - 8;
    u64* header = addr;
    u64 size = header[0];
    syscall2(11, addr, size);
}

export fn sys_mkdir(u8* path, u64 mode) -> u64 {
    return(syscall3(83, path, mode, 0));
}

export fn sys_rmdir(u8* path) -> u64 {
    return(syscall1(84, path));
}

export fn syscall_error(u64 ret) -> u64 {
    if (ret > 18446744073709547520) {
        return(1);
    }
    return(0);
}
