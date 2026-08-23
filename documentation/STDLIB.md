# WandC Standard Library (libw) Reference

The standard library provides system call wrappers, memory management, string operations, and I/O functions. The compiler core does not include these functions. You must import the required modules.

Install the library:

```bash
wand2c -il libw
```

Import modules in your source file:

```c
sc.true

#import <syscall>
#import <io>
#import <mem>
#import <string>
#import <path>
#import <args>
```

---

## Module: syscall

The `syscall` module wraps the `syscall0` through `syscall6` compiler built-ins into named functions. All functions return the raw system call result. Use `syscall_error` to check for errors.

### Error Checking

```c
fn syscall_error(u64 ret) -> u64;
```

Returns 1 if `ret` is an error code. Returns 0 if `ret` is a valid result.

Example:

```c
u64 fd = sys_open("/etc/hostname", O_RDONLY, 0);
if (syscall_error(fd) == 1) {
    printf("error: cannot open file\n");
    return(1);
}
```

### File Operations

```c
fn sys_open(u8* path, u64 flags, u64 mode) -> u64;
fn sys_close(u64 fd) -> u64;
fn sys_read(u64 fd, u8* buf, u64 size) -> u64;
fn sys_write(u64 fd, u8* buf, u64 size) -> u64;
fn sys_lseek(u64 fd, u64 offset, u64 whence) -> u64;
fn sys_ioctl(u64 fd, u64 request, u64 arg) -> u64;
fn sys_dup2(u64 oldfd, u64 newfd) -> u64;
fn sys_unlink(u8* path) -> u64;
fn sys_readlink(u8* path, u8* buf, u64 size) -> u64;
fn sys_getdents64(u64 fd, u8* dirp, u64 count) -> u64;
```

### File Status

```c
fn sys_stat(u8* path, u8* statbuf) -> u64;
fn sys_fstat(u64 fd, u8* statbuf) -> u64;
fn sys_lstat(u8* path, u8* statbuf) -> u64;
fn sys_chmod(u8* path, u64 mode) -> u64;
fn sys_chown(u8* path, u64 uid, u64 gid) -> u64;
fn sys_mkdir(u8* path, u64 mode) -> u64;
fn sys_rmdir(u8* path) -> u64;
```

### Process Control

```c
fn sys_fork() -> u64;
fn sys_execve(u8* path, u8* argv, u8* envp) -> u64;
fn sys_wait4(u64 pid, u64* status, u64 options, u8* rusage) -> u64;
fn sys_getpid() -> u64;
fn sys_kill(u64 pid, u64 sig) -> u64;
fn sys_setsid() -> u64;
fn sys_setpgid(u64 pid, u64 pgid) -> u64;
fn sys_exit(u64 code);
fn sys_exit_group(u64 code);
```

### User and Group

```c
fn sys_getuid() -> u64;
fn sys_getgid() -> u64;
fn sys_geteuid() -> u64;
fn sys_getegid() -> u64;
fn sys_setuid(u64 uid) -> u64;
fn sys_setgid(u64 gid) -> u64;
```

### Signals

```c
fn sys_rt_sigaction(u64 signum, u8* act, u8* oldact, u64 sigsetsize) -> u64;
fn sys_rt_sigprocmask(u64 how, u8* set, u8* oldset, u64 sigsetsize) -> u64;
```

### Time

```c
fn sys_nanosleep(u8* req, u8* rem) -> u64;
fn sys_clock_gettime(u64 clockid, u8* tp) -> u64;
```

### Network

```c
fn sys_socket(u64 domain, u64 type, u64 protocol) -> u64;
fn sys_bind(u64 sockfd, u8* addr, u64 addrlen) -> u64;
fn sys_listen(u64 sockfd, u64 backlog) -> u64;
fn sys_accept(u64 sockfd, u8* addr, u64* addrlen) -> u64;
fn sys_connect(u64 sockfd, u8* addr, u64 addrlen) -> u64;
fn sys_sendto(u64 sockfd, u8* buf, u64 len, u64 flags, u8* dest_addr, u64 addrlen) -> u64;
fn sys_recvfrom(u64 sockfd, u8* buf, u64 len, u64 flags, u8* src_addr, u64* addrlen) -> u64;
```

### Event Polling

```c
fn sys_epoll_create(u64 flags) -> u64;
fn sys_epoll_ctl(u64 epfd, u64 op, u64 fd, u8* event) -> u64;
fn sys_epoll_wait(u64 epfd, u8* events, u64 maxevents, u64 timeout) -> u64;
```

### System Control

```c
fn sys_mount(u8* source, u8* target, u8* fstype, u64 flags, u8* data) -> u64;
fn sys_umount2(u8* target, u64 flags) -> u64;
fn sys_reboot(u64 magic1, u64 magic2, u64 cmd, u8* arg) -> u64;
fn sys_prctl(u64 option, u64 a2, u64 a3, u64 a4, u64 a5) -> u64;
fn sys_syslog(u64 kind, u8* buf, u64 len) -> u64;
fn sys_getrlimit(u64 resource, u8* rlim) -> u64;
fn sys_setrlimit(u64 resource, u8* rlim) -> u64;
fn sys_unshare(u64 flags) -> u64;
fn sys_setns(u64 fd, u64 nstype) -> u64;
```

### Memory Allocation

```c
fn mloc(u64 hint, u64 size) -> u8*;
fn mfree(u8* ptr);
```

`mloc` allocates memory through the `mmap` system call. It stores the block size in a header. The returned pointer is offset by 8 bytes. `mfree` reads the header and unmaps the block.

---

## Module: io

The `io` module provides console output, console input, and file operations.

### Console Output

```c
fn print_char(u8 c);
fn print_string(u8* s);
fn print_number(u64 num);
fn print_signed_number(i64 num);
fn printf(u8* format, u64 arg1, u64 arg2, u64 arg3);
```

`printf` supports three format specifiers:

| Specifier | Output |
|---|---|
| `%v` | Unsigned integer |
| `%d` | Signed integer |
| `%s` | String |

Example:

```c
printf("name: %s, count: %v\n", name, count);
```

### Console Input

```c
fn read_char() -> u8;
fn read_string(u8* buf, u64 max_size);
fn read_integer() -> i64;
fn read_float() -> f64;
```

### File Operations

```c
fn file_open(u8* path, u64 flags, u64 mode) -> i64;
fn file_close(u64 fd) -> i64;
fn file_read(u64 fd, u8* buf, u64 size) -> i64;
fn file_write(u64 fd, u8* buf, u64 size) -> i64;
fn file_remove(u8* path) -> i64;
```

### Parsing

```c
fn parse_float(u8* s) -> f64;
```

---

## Module: mem

The `mem` module provides a heap allocator on top of `mloc`. It uses a first-fit strategy with block headers.

### Initialization

```c
fn mem_init(u64 initial_size);
```

Call `mem_init` before `malloc`, `calloc`, or `mrealloc`. It allocates an arena of `initial_size` bytes.

### Allocation

```c
fn malloc(u64 size) -> void*;
fn calloc(u64 num, u64 size) -> void*;
fn mrealloc(u8* ptr, u64 new_size) -> u8*;
fn mfree(u8* ptr);
fn mfree_all();
```

`malloc` returns a pointer to a block of at least `size` bytes. `calloc` allocates and fills the block with zeros. `mrealloc` changes the size of a block and copies the data. `mfree` marks a block as free. `mfree_all` resets the arena offset.

Example:

```c
mem_init(1048576);
u8* buf = malloc(256);
if (buf != null) {
    mfree(buf);
}
```

---

## Module: string

The `string` module provides C-style string and memory operations.

```c
fn strlen(u8* s) -> u64;
fn strcmp(u8* s1, u8* s2) -> i64;
fn strcpy(u8* dest, u8* src) -> u8*;
fn strcat(u8* dest, u8* src) -> u8*;
fn memcpy(u8* dest, u8* src, u64 n) -> void*;
fn memset(u8* s, u8 c, u64 n) -> void*;
fn atoi(u8* s) -> u64;
fn itoa(i64 num, u8* buf) -> u8*;
```

`strcmp` returns 0 if the strings are equal. It returns -1 if `s1` is less than `s2`. It returns 1 if `s1` is greater than `s2`.

---

## Module: path

The `path` module provides file path utilities.

```c
fn path_exists(u8* path) -> u64;
fn path_is_dir(u8* path) -> u64;
fn path_join(u8* dest, u8* a, u8* b);
fn path_dirname(u8* path, u8* dest);
fn path_basename(u8* path) -> u8*;
```

`path_exists` returns 1 if the path exists. `path_is_dir` returns 1 if the path is a directory. `path_join` writes `a/b` into `dest`.

Example:

```c
u8 full_path[512];
path_join(full_path*adr, "/etc", "hostname");
```

---

## Module: args

The `args` module provides command-line argument parsing. It works with the `argv` and `argc` values from `main`.

```c
fn get_arg(u64 argv, u64 index) -> u8*;
fn arg_equals(u64 argv, u64 index, u8* expected) -> u64;
fn find_arg(u64 argc, u64 argv, u8* name) -> u64;
fn get_arg_value(u64 argc, u64 argv, u8* name) -> u8*;
```

`get_arg` returns the string at `index`. `arg_equals` returns 1 if the argument matches `expected`. `find_arg` returns the index of `name` or 0. `get_arg_value` returns the argument after `name`.

Example:

```c
fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    if (arg_equals(argv, 1, "build") == 1) {
        u8* target = get_arg_value(argc, argv, "-t");
        if (target == null) {
            printf("error: no target\n");
            return(1);
        }
        printf("building: %s\n", target);
    }
    return(0);
}
```

---

## Module: math

The `math` module provides floating-point math functions.

```c
fn abs(f64 x) -> f64;
fn sqrt(f64 x) -> f64;
fn sin(f64 x) -> f64;
fn cos(f64 x) -> f64;
fn tan(f64 x) -> f64;
fn print_float(f64 x);
```

---

## Module: fpmath

The `fpmath` module provides fixed-point math functions. Values use a scale factor of 1000000.

### Constants

```c
sect.math_const
    i64 PI = 3141592;
    i64 TWO_PI = 6283185;
    i64 HALF_PI = 1570796;
    i64 FIXED_ONE = 1000000;
EOS
```

### Functions

```c
fn abs(i64 x) -> i64;
fn pow(i64 base, u64 exp) -> i64;
fn sqrt(i64 x) -> i64;
fn sin(i64 rad) -> i64;
fn cos(i64 rad) -> i64;
fn tan(i64 rad) -> i64;
fn print_fixed(i64 x);
```

---

## Module: keyboard

The `keyboard` module provides terminal key input with escape sequence parsing.

```c
fn char_available() -> u64;
fn read_key() -> u64;
```

`read_key` returns the key code. Printable characters return their ASCII value. Special keys return codes from the `keys` section.

### Key Constants

```c
sect.keys
    u64 arrow_up = 1000;
    u64 arrow_down = 1001;
    u64 arrow_right = 1002;
    u64 arrow_left = 1003;
    u64 key_delete = 1004;
    u64 page_up = 1005;
    u64 page_down = 1006;
    u64 key_home = 1010;
    u64 key_end = 1011;
    u64 key_f1 = 1101;
    u64 key_f2 = 1102;
    u64 key_f3 = 1103;
    u64 key_f4 = 1104;
    u64 key_esc = 27;
    u64 key_backspace = 127;
    u64 key_enter = 10;
    u64 key_tab = 9;
    u64 key_space = 32;
EOS
```

---

## Module: tui

The `tui` module provides a terminal user interface with double buffering.

```c
fn get_terminal_size(u64* out_rows*o, u64* out_cols*o);
fn tui_init();
fn tui_clear_physical();
fn tui_move_cursor_physical(u64 row, u64 col);
fn tui_clear();
fn tui_draw_char(u64 r, u64 c, u8 ch);
fn tui_draw_string(u64 r, u64 c, u8* s);
fn tui_set_cursor(u64 r, u64 c);
fn tui_render();
```

Call `tui_init` before other `tui` functions. Draw to the screen buffer with `tui_draw_char` and `tui_draw_string`. Call `tui_render` to update the terminal.

Example:

```c
tui_init();
tui_clear();
tui_draw_string(0, 0, "Hello TUI");
tui_render();
```

---

## Module: std

The `std` module provides basic utility functions.

```c
fn exit(u64 code);
fn srand(u64 seed);
fn rand() -> u64;
```

`rand` uses a linear congruential generator. Call `srand` to set the seed.

---

## Constants

Import `<syscall>` to use these constants.

### File Open Flags

| Constant | Value | Meaning |
|---|---:|---|
| `O_RDONLY` | 0 | Open for reading |
| `O_WRONLY` | 1 | Open for writing |
| `O_RDWR` | 2 | Open for reading and writing |
| `O_CREAT` | 64 | Create file if it does not exist |
| `O_EXCL` | 128 | Fail if file exists |
| `O_NOCTTY` | 256 | Do not assign controlling terminal |
| `O_TRUNC` | 512 | Truncate file to zero length |
| `O_APPEND` | 1024 | Append on each write |
| `O_NONBLOCK` | 2048 | Non-blocking mode |
| `O_DSYNC` | 4096 | Synchronized I/O data integrity |
| `O_DIRECT` | 16384 | Direct disk access |
| `O_LARGEFILE` | 32768 | Allow large files |
| `O_DIRECTORY` | 65536 | Must be a directory |
| `O_NOFOLLOW` | 131072 | Do not follow symlinks |
| `O_CLOEXEC` | 524288 | Close on exec |

### Signals

| Constant | Value |
|---|---:|
| `SIGHUP` | 1 |
| `SIGINT` | 2 |
| `SIGQUIT` | 3 |
| `SIGILL` | 4 |
| `SIGTRAP` | 5 |
| `SIGABRT` | 6 |
| `SIGBUS` | 7 |
| `SIGFPE` | 8 |
| `SIGKILL` | 9 |
| `SIGUSR1` | 10 |
| `SIGSEGV` | 11 |
| `SIGUSR2` | 12 |
| `SIGPIPE` | 13 |
| `SIGALRM` | 14 |
| `SIGTERM` | 15 |
| `SIGSTKFLT` | 16 |
| `SIGCHLD` | 17 |
| `SIGCONT` | 18 |
| `SIGSTOP` | 19 |
| `SIGTSTP` | 20 |
| `SIGTTIN` | 21 |
| `SIGTTOU` | 22 |
| `SIGURG` | 23 |
| `SIGXCPU` | 24 |
| `SIGXFSZ` | 25 |
| `SIGVTALRM` | 26 |
| `SIGPROF` | 27 |
| `SIGWINCH` | 28 |
| `SIGIO` | 29 |
| `SIGPWR` | 30 |
| `SIGSYS` | 31 |

### Wait Options

| Constant | Value | Meaning |
|---|---:|---|
| `WNOHANG` | 1 | Return immediately if no child exited |
| `WUNTRACED` | 2 | Report stopped children |
| `WCONTINUED` | 8 | Report continued children |

### Directory Entry Types

| Constant | Value |
|---|---:|
| `DT_UNKNOWN` | 0 |
| `DT_FIFO` | 1 |
| `DT_CHR` | 2 |
| `DT_DIR` | 4 |
| `DT_BLK` | 6 |
| `DT_REG` | 8 |
| `DT_LNK` | 10 |
| `DT_SOCK` | 12 |

### Epoll Events

| Constant | Value |
|---|---:|
| `EPOLLIN` | 1 |
| `EPOLLPRI` | 2 |
| `EPOLLOUT` | 4 |
| `EPOLLERR` | 8 |
| `EPOLLHUP` | 16 |
| `EPOLLRDNORM` | 64 |
| `EPOLLRDBAND` | 128 |
| `EPOLLWRNORM` | 256 |
| `EPOLLWRBAND` | 512 |
| `EPOLLMSG` | 1024 |
| `EPOLLRDHUP` | 8192 |
| `EPOLLONESHOT` | 1073741824 |
| `EPOLLET` | 2147483648 |

### Epoll Operations

| Constant | Value |
|---|---:|
| `EPOLL_CTL_ADD` | 1 |
| `EPOLL_CTL_DEL` | 2 |
| `EPOLL_CTL_MOD` | 3 |

### Fcntl Commands

| Constant | Value |
|---|---:|
| `F_GETFD` | 1 |
| `F_SETFD` | 2 |
| `F_GETFL` | 3 |
| `F_SETFL` | 4 |
| `F_GETLK` | 5 |
| `F_SETLK` | 6 |
| `F_SETLKW` | 7 |
| `F_GETOWN` | 9 |
| `F_SETOWN` | 8 |
| `FD_CLOEXEC` | 1 |

---

## Structures

Import `<syscall>` to use these structures.

### linux_dirent64

Directory entry from `sys_getdents64`.

```c
struct linux_dirent64 version 1 {
    u64 d_ino version 1;
    u64 d_off version 1;
    u16 d_reclen version 1;
    u8 d_type version 1;
    u8 d_name[256] version 1;
}
```

### timespec

Time value for `sys_nanosleep` and `sys_clock_gettime`.

```c
struct timespec version 1 {
    i64 tv_sec version 1;
    i64 tv_nsec version 1;
}
```

### sigaction

Signal handler configuration for `sys_rt_sigaction`.

```c
struct sigaction version 1 {
    u64 sa_handler version 1;
    u64 sa_flags version 1;
    u64 sa_restorer version 1;
    u64 sa_mask version 1;
}
```

### stat

File status from `sys_stat`, `sys_fstat`, and `sys_lstat`.

```c
struct stat version 1 {
    u64 st_dev version 1;
    u64 st_ino version 1;
    u64 st_nlink version 1;
    u32 st_mode version 1;
    u32 st_uid version 1;
    u32 st_gid version 1;
    u32 __pad0 version 1;
    u64 st_rdev version 1;
    i64 st_size version 1;
    i64 st_blksize version 1;
    i64 st_blocks version 1;
    u64 st_atime_sec version 1;
    u64 st_atime_nsec version 1;
    u64 st_mtime_sec version 1;
    u64 st_mtime_nsec version 1;
    u64 st_ctime_sec version 1;
    u64 st_ctime_nsec version 1;
    u64 __unused[3] version 1;
}
```

### epoll_event

Event structure for `sys_epoll_ctl` and `sys_epoll_wait`.

```c
struct epoll_event version 1 {
    u32 events version 1;
    u64 data version 1;
}
```

### pollfd

Poll descriptor.

```c
struct pollfd version 1 {
    i32 fd version 1;
    i16 events version 1;
    i16 revents version 1;
}
```

### timeval

Time value with microsecond precision.

```c
struct timeval version 1 {
    i64 tv_sec version 1;
    i64 tv_usec version 1;
}
```
