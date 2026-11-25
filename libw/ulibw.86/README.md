# ulibw.86 - UTMS7 Wand Userspace Library

`ulibw.86` is a Wand-native userspace library set for **UTMS7** (x86_64). It is modeled after `../llibw.86`, but it only targets syscalls/features present in the UTMS7 kernel ABI.

## Target / Status

- **Target OS**: UTMS7 (`/home/user12ms/Projects/utms7`)
- **ABI**: Linux-like x86_64 syscall numbers/args as implemented by UTMS7 (`include/syscall.h`, `kernel/core/syscall.c`)
- **Status**: Practical initial subset for UTMS7 userspace/apps
- **Language**: Wand (`.w`/`.wh`) only, no C files

## Modules

- `syscall_constants.wh` - UTMS7-supported constants (flags, errno subset, poll/mmap, ioctl, AT_*, UTMS7 `uSYS_*`).
- `syscall_structs.wh` - UTMS7-compatible layouts: `stat` (matches kernel `linux_stat`), `linux_dirent64`, `timespec`, `pollfd`, `winsize`, packed UTMS7 disk structs.
- `syscall.wh` / `syscall.w` - Raw syscall wrappers and `syscall_error`. Includes Linux-compatible syscalls UTMS7 implements and UTMS7-specific `u_sys_*` wrappers.
- `mem.wh` / `mem.w` - Simple mmap-backed arena allocator (`mem_init`, `malloc`, `calloc`, `mfree`, `mrealloc`, `mfree_all`).
- `string.wh` / `string.w` - Basic string/memory routines.
- `io.wh` / `io.w` - stdout/stderr helpers, basic read helpers, `write_all`/`read_full`, `strerror`.
- `fileio.wh` / `fileio.w` - File operations and buffered reader/writer helpers.
- `process.wh` / `process.w` - fork/exec/wait/exit/kill/getpid/getppid/sleep/yield wrappers.
- `path.wh` / `path.w` - Simple path helpers (`exists`, `is_dir`, `is_file`, `join`, `dirname`, `basename`, `stat`).
- `dir.wh` / `dir.w` - Minimal `getdents64` directory iteration.
- `args.wh` / `args.w` - argv/envp helpers.
- `time.wh` / `time.w` - UTMS7 tick-based time helpers.
- `std.wh` / `std.w` - Minimal std helpers: `exit`, `rand`, `srand`.

## Key ABI Decisions / UTMS7 vs Linux

- **Linux-like numbers where UTMS7 implements them**: read/write/open/close/stat/fstat/lseek/mmap/munmap/mprotect/brk/ioctl/poll/pipe2/dup/dup2/dup3/getpid/fork/execve/exit/wait4/kill/getcwd/chdir/mkdir/rmdir/unlink/rename/symlink/readlink/access/sync/fsync/getdents64/openat/newfstatat/readv/writev/sched_yield/uid/gid/uname/pread64/pwrite64/fcntl.
- **UTMS7-specific syscalls**: exposed as `u_sys_*` wrappers for disk/partition/ps/meminfo/clear/setcolor/sleep/pgid and other UTMS7 1000+ syscalls. These are UTMS-only; do not use in `llibw.86`.
- **Time**: `sys_gettime_ticks()` / `time_ticks()` return UTMS7 tick counts. UTMS7's current `clock_gettime`/`uSYS_gettime` kernel implementation returns ticks, not a Linux `timespec`-filling ABI. No Linux timespec `clock_gettime` wrapper is exposed.
- **Errors**: raw wrappers return `u64`; `syscall_error(ret)` detects Linux-style negative errno encoded in high `u64`.
- **Memory allocator**: `mem.w` uses UTMS7 anonymous `mmap/munmap`, not `brk` freelists. Simple arena with free flag reuse.
- **No Linux-only semantics claimed**: no sockets/epoll/timerfd/signalfd/eventfd wrappers, no Linux-correct `clock_gettime` timespec, no robust pthread API.

## UTMS7 Limitations / Missing Features

- Only syscalls registered in `kernel/core/syscall.c` should be used.
- `clock_gettime` is tick-based in current UTMS7 kernel behavior; Linux timespec semantics are not available.
- `mprotect` is accepted/stub-like in kernel; no strict protection guarantees.
- `access` is mapped to stat-like behavior in kernel syscall table; treat as best-effort existence check.
- Signal handling/futex/clone exist at low level but are not wrapped as a full pthread/signal API here.
- Filesystem/network/device behavior is UTMS7-specific in places; verify before exposing new wrappers.

## Usage

```wand
sc.true
#import <io>
#import <fileio>

fn main() -> i64 {
    print_string("hello from UTMS7\n");
    return(0);
}
```

Compile with `wand2c` using `WAND_LIB_PATH` pointing at this directory, or install the library modules as needed.
