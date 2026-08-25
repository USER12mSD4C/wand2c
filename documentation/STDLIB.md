# WandC Standard Library Reference

This document describes the WandC standard library.
The library contains ready functions for common tasks.
You must import the necessary modules in your source file.

---

## Install the Library

Run this command to install the library:

```text
wand2c -il libw
```

---

## Import Modules

Add import lines at the top of your source file.
Use angle brackets for system modules.
Use double quotes for local modules.

```wandc
sc.true
#import <syscall>
#import <io>
#import <mem>
#import <string>
#import <args>
#import <path>
#import <fileio>
#import <vector>
#import <unistd>
#import <math>
#import <fpmath>
#import <keyboard>
#import <tui>
#import <std>
#import <process>
#import <signal>
```

---

## Module: syscall

This module interfaces with the Linux operating system through raw system calls.

### Error Checking

```wandc
fn syscall_error(u64 ret) -> u64;
```

This function returns 1 if `ret` is an error code.
It returns 0 if `ret` is valid.
The error threshold is `ret > 0xFFFFFFFFFFFFF000` (-4096).

### Raw Memory Allocation

```wandc
fn mloc(u64 hint, u64 size) -> u8*;
fn mfree(u8* ptr);
```

The `mloc` function allocates memory through the `mmap` system call.
It stores the block size in an 8-byte header before the returned pointer.
The `mfree` function reads the header and calls `munmap`.

Do not mix `mloc`/`mfree` with the `<mem>` arena allocator.

### File Operations

```wandc
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

```wandc
fn sys_stat(u8* path, u8* statbuf) -> u64;
fn sys_fstat(u64 fd, u8* statbuf) -> u64;
fn sys_lstat(u8* path, u8* statbuf) -> u64;
fn sys_chmod(u8* path, u64 mode) -> u64;
fn sys_chown(u8* path, u64 uid, u64 gid) -> u64;
fn sys_mkdir(u8* path, u64 mode) -> u64;
fn sys_rmdir(u8* path) -> u64;
```

### Process Control

```wandc
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

```wandc
fn sys_getuid() -> u64;
fn sys_getgid() -> u64;
fn sys_geteuid() -> u64;
fn sys_getegid() -> u64;
fn sys_setuid(u64 uid) -> u64;
fn sys_setgid(u64 gid) -> u64;
```

### Signals

```wandc
fn sys_rt_sigaction(u64 signum, u8* act, u8* oldact, u64 sigsetsize) -> u64;
fn sys_rt_sigprocmask(u64 how, u8* set, u8* oldset, u64 sigsetsize) -> u64;
```

### Time

```wandc
fn sys_nanosleep(u8* req, u8* rem) -> u64;
fn sys_clock_gettime(u64 clockid, u8* tp) -> u64;
```

### Network

```wandc
fn sys_socket(u64 domain, u64 type, u64 protocol) -> u64;
fn sys_bind(u64 sockfd, u8* addr, u64 addrlen) -> u64;
fn sys_listen(u64 sockfd, u64 backlog) -> u64;
fn sys_accept(u64 sockfd, u8* addr, u64* addrlen) -> u64;
fn sys_connect(u64 sockfd, u8* addr, u64 addrlen) -> u64;
fn sys_sendto(u64 sockfd, u8* buf, u64 len, u64 flags, u8* dest_addr, u64 addrlen) -> u64;
fn sys_recvfrom(u64 sockfd, u8* buf, u64 len, u64 flags, u8* src_addr, u64* addrlen) -> u64;
```

### Event Polling

```wandc
fn sys_epoll_create(u64 flags) -> u64;
fn sys_epoll_ctl(u64 epfd, u64 op, u64 fd, u8* event) -> u64;
fn sys_epoll_wait(u64 epfd, u8* events, u64 maxevents, u64 timeout) -> u64;
```

### System Control

```wandc
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

---

## Module: io

This module provides console and file input/output.

### Console Output

```wandc
fn print_char(u8 c);
fn print_string(u8* s);
fn print_number(u64 num);
fn print_signed_number(i64 num);
fn printf(u8* format, u64 arg1, u64 arg2, u64 arg3);
fn snprintf(u8* buf, u64 size, u8* fmt, u64 arg1, u64 arg2, u64 arg3) -> i64;
```

The `printf` function supports three format specifiers.

| Specifier | Output |
|---|---|
| `%v` | Unsigned integer |
| `%d` | Signed integer |
| `%s` | String |

The `snprintf` function writes formatted output into a buffer of `size` bytes.
It returns the number of characters written.

Pass pointer arguments as `u64` casts.

```wandc
printf("name: %s, count: %v\n", (u64)name, count, 0);
```

### Console Input

```wandc
fn read_char() -> u8;
fn read_string(u8* buf, u64 max_size);
fn read_integer() -> i64;
fn read_float() -> f64;
```

### File Operations

```wandc
fn file_open(u8* path, u64 flags, u64 mode) -> i64;
fn file_close(u64 fd) -> i64;
fn file_read(u64 fd, u8* buf, u64 size) -> i64;
fn file_write(u64 fd, u8* buf, u64 size) -> i64;
fn file_remove(u8* path) -> i64;
```

### Parsing

```wandc
fn parse_float(u8* s) -> f64;
```

---

## Module: mem

This module provides an arena heap allocator with first-fit block management.

### Initialization

```wandc
fn mem_init(u64 initial_size);
```

Call `mem_init` before any allocation.
This function allocates an arena through `mloc`.

### Allocation

```wandc
fn malloc(u64 size) -> void*;
fn calloc(u64 num, u64 size) -> void*;
fn mrealloc(u8* ptr, u64 new_size) -> u8*;
fn mfree(u8* ptr);
fn mfree_all();
```

Rules:

1. `malloc` returns at least `size` bytes, aligned to 8 bytes.
2. `calloc` returns zero-filled memory.
3. `mrealloc` preserves existing data up to the smaller old and new size.
4. `mfree` releases a block back to the arena free list.
5. `mfree_all` resets the arena and invalidates all arena pointers.

---

## Module: string

This module provides C-style string and memory operations.

```wandc
fn strlen(u8* s) -> u64;
fn strcmp(u8* s1, u8* s2) -> i64;
fn strncmp(u8* s1, u8* s2, u64 n) -> i64;
fn strcpy(u8* dest, u8* src) -> u8*;
fn strcat(u8* dest, u8* src) -> u8*;
fn memcpy(u8* dest, u8* src, u64 n) -> void*;
fn memset(u8* s, u8 c, u64 n) -> void*;
fn memcmp(u8* s1, u8* s2, u64 n) -> i64;
fn atoi(u8* s) -> u64;
fn itoa(i64 num, u8* buf) -> u8*;
```

The `strcmp` and `strncmp` functions return:

| Return value | Meaning |
|---|---|
| 0 | Strings are equal |
| -1 | `s1` is less than `s2` |
| 1 | `s1` is greater than `s2` |

The `memcmp` function returns the same values.

---

## Module: path

This module provides file path utilities.

```wandc
fn path_exists(u8* path) -> u64;
fn path_is_dir(u8* path) -> u64;
fn path_join(u8* dest, u8* a, u8* b);
fn path_dirname(u8* path, u8* dest);
fn path_basename(u8* path) -> u8*;
```

- `path_exists` returns 1 if the path exists.
- `path_is_dir` returns 1 if the path is a directory.
- `path_join` writes `a/b` into `dest`.
- `path_dirname` writes the directory part into `dest`.
- `path_basename` returns a pointer to the filename part within `path`.

---

## Module: args

This module parses command-line arguments.

```wandc
fn get_arg(u64 argv, u64 index) -> u8*;
fn arg_equals(u64 argv, u64 index, u8* expected) -> u64;
fn find_arg(u64 argc, u64 argv, u8* name) -> u64;
fn get_arg_value(u64 argc, u64 argv, u8* name) -> u8*;
```

- `get_arg` returns the string pointer at `index`.
- `arg_equals` returns 1 if the argument matches `expected`.
- `find_arg` returns the index of `name` or 0 if not found.
- `get_arg_value` returns the argument after `name`, or `null`.

---

## Module: fileio

This module reads files line by line with internal buffering.

### Structure

```wandc
struct FileReader {
    i64 fd;
    u8 buf[4096];
    u64 pos;
    u64 len;
}
```

### Functions

```wandc
fn file_reader_init(FileReader* r, i64 fd);
fn file_reader_next_line(FileReader* r, u8* out, u64 max_size) -> i64;
```

The `file_reader_next_line` function returns the line length.
It returns -1 at the end of the file.

---

## Module: vector

This module provides a dynamic array of string pointers.

### Structure

```wandc
struct StrVec {
    u64 items;
    u64 count;
    u64 capacity;
}
```

### Functions

```wandc
fn strvec_init(StrVec* v);
fn strvec_add(StrVec* v, u64 str_ptr);
fn strvec_contains(StrVec* v, u64 str_ptr) -> i64;
fn strvec_free(StrVec* v);
fn strvec_clear(StrVec* v);
fn strvec_pop(StrVec* v);
```

The `strvec_add` function duplicates the string internally.
The `strvec_free` and `strvec_clear` functions free all duplicated strings.

### Helpers

```wandc
fn xmalloc(u64 size) -> u64;
fn xstrdup(u64 s) -> u64;
```

The `xmalloc` function allocates memory or exits on failure.
The `xstrdup` function copies a string to new memory.

---

## Module: unistd

This module provides process and directory helpers.

```wandc
fn get_cpu_count() -> i64;
fn popen(u8* command) -> i64;
fn pclose(i64 fd) -> i64;
fn opendir(u8* path) -> DIR*;
fn readdir(DIR* dir) -> u8*;
fn closedir(DIR* dir);
```

- `get_cpu_count` reads `/proc/cpuinfo` and returns the CPU count.
- `popen` runs a shell command and returns a read pipe file descriptor.
- `pclose` closes the pipe and waits for the child process.
- `opendir` opens a directory for reading.
- `readdir` returns the next file name. It skips `.` and `..`.
- `closedir` closes the directory and frees memory.

### Structure

```wandc
struct DIR {
    i64 fd;
    u8 data[2048];
    u64 data_pos;
    u64 data_len;
    u8 name[256];
}
```

---

## Module: process

This module provides process management with group support.

### Structures

```wandc
struct ProcessInfo {
    i64 pid;
    i64 status;
    i64 exit_code;
}

struct ProcessGroup {
    i64* pids;
    i64 count;
    i64 capacity;
    i64 running;
}
```

### Functions

```wandc
fn process_fork() -> i64;
fn process_exec(u8* path, u8* argv, u8* envp) -> i64;
fn process_wait(i64 pid, ProcessInfo* info) -> i64;
fn process_wait_all(ProcessInfo* info) -> i64;
fn process_exit(i64 code);
fn process_kill(i64 pid, u64 signum) -> i64;
fn process_get_pid() -> i64;
fn process_exec_shell(u8* cmd) -> i64;
fn process_exec_shell_wait(u8* cmd) -> i64;
```

### Process Group Functions

```wandc
fn process_group_init(ProcessGroup* group);
fn process_group_add(ProcessGroup* group, i64 pid);
fn process_group_wait_any(ProcessGroup* group, ProcessInfo* info) -> i64;
fn process_group_wait_all(ProcessGroup* group);
fn process_group_kill_all(ProcessGroup* group, u64 signum);
fn process_group_free(ProcessGroup* group);
```

---

## Module: signal

This module provides signal handling.

### Structure

```wandc
struct SignalHandler {
    u64 handler;
    u64 flags;
    u64 mask;
}
```

### Functions

```wandc
fn signal_init_handler(SignalHandler* handler);
fn signal_set_handler(SignalHandler* handler, u64 func_ptr);
fn signal_set_flags(SignalHandler* handler, u64 flags);
fn signal_add_to_mask(SignalHandler* handler, u64 signum);
fn signal_install(SignalHandler* handler, u64 signum) -> i64;
fn signal_ignore(u64 signum) -> i64;
fn signal_default(u64 signum) -> i64;
fn signal_send(u64 pid, u64 signum) -> i64;
fn signal_block(u64 signum) -> i64;
fn signal_unblock(u64 signum) -> i64;
```

---

## Module: math

This module provides floating-point math functions.

```wandc
fn abs(f64 x) -> f64;
fn sqrt(f64 x) -> f64;
fn sin(f64 x) -> f64;
fn cos(f64 x) -> f64;
fn tan(f64 x) -> f64;
fn print_float(f64 x);
```

Constants in `math_const` section:

| Constant | Value |
|---|---|
| `PI` | 3.141592653589793 |
| `HALF_PI` | 1.570796326794896 |

---

## Module: fpmath

This module provides fixed-point math functions.
Values use a scale factor of 1000000.

```wandc
fn abs(i64 x) -> i64;
fn pow(i64 base, u64 exp) -> i64;
fn sqrt(i64 x) -> i64;
fn sin(i64 rad) -> i64;
fn cos(i64 rad) -> i64;
fn tan(i64 rad) -> i64;
fn print_fixed(i64 x);
```

Constants in `math_const` section:

| Constant | Value | Meaning |
|---|---|---|
| `PI` | 3141592 | 3.141592 |
| `TWO_PI` | 6283185 | 6.283185 |
| `HALF_PI` | 1570796 | 1.570796 |
| `FIXED_ONE` | 1000000 | 1.0 |

---

## Module: keyboard

This module reads terminal key input.
It parses escape sequences for special keys.

```wandc
fn char_available() -> u64;
fn read_key() -> u64;
```

Printable characters return their ASCII value.
Special keys return codes from the `keys` section.

### Key Constants

```wandc
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
    u64 ctrl_a = 1;
    u64 ctrl_b = 2;
    ...
    u64 ctrl_z = 26;
EOS
```

---

## Module: tui

This module provides a terminal user interface with double buffering and a widget system.

### Initialization

```wandc
fn tui_init();
fn tui_shutdown();
fn tui_run(u64 root);
fn tui_request_exit();
fn tui_render_frame();
```

### Events

```wandc
fn tui_poll_event(u64 out_type*o, u64 out_a*o, u64 out_b*o, u64 out_c*o) -> u64;
```

### Widget Creation

```wandc
fn tui_panel_new(u8* title, i64 x, i64 y, i64 w, i64 h) -> u64;
fn tui_label_new(u8* text, i64 x, i64 y, i64 w) -> u64;
fn tui_textbox_new(i64 x, i64 y, i64 w, u64 max_len) -> u64;
fn tui_list_new(i64 x, i64 y, i64 w, i64 h) -> u64;
fn tui_textview_new(i64 x, i64 y, i64 w, i64 h) -> u64;
fn tui_button_new(u8* text, i64 x, i64 y, i64 w) -> u64;
```

### Widget Management

```wandc
fn tui_widget_add_child(u64 parent, u64 child);
fn tui_widget_remove_child(u64 parent, u64 child);
fn tui_widget_set_visible(u64 widget, u8 visible);
fn tui_widget_set_anchor(u64 widget, u8 left, u8 top, u8 right, u8 bottom);
fn tui_widget_set_anchor_offsets(u64 widget, i64 l, i64 t, i64 r, i64 b);
fn tui_widget_set_colors(u64 widget, u8 fg, u8 bg);
fn tui_widget_set_attr(u64 widget, u8 attr);
fn tui_widget_set_pos(u64 widget, i64 x, i64 y);
fn tui_widget_set_size(u64 widget, i64 w, i64 h);
fn tui_widget_set_focusable(u64 widget, u8 focusable);
fn tui_widget_set_on_click(u64 widget, u64 callback);
fn tui_widget_set_on_key(u64 widget, u64 callback);
fn tui_widget_destroy(u64 widget);
fn tui_focus_widget(u64 widget);
fn tui_get_focused_widget() -> u64;
```

### Widget-Specific Functions

```wandc
fn tui_panel_set_title(u64 widget, u8* title);
fn tui_panel_set_border(u64 widget, u8 border_style);
fn tui_label_set_text(u64 widget, u8* text);
fn tui_label_get_text(u64 widget) -> u8*;
fn tui_textbox_get_text(u64 widget) -> u8*;
fn tui_textbox_set_text(u64 widget, u8* text);
fn tui_textbox_set_placeholder(u64 widget, u8* text);
fn tui_list_add_item(u64 widget, u8* text);
fn tui_list_get_selected(u64 widget) -> i64;
fn tui_list_set_selected(u64 widget, i64 idx);
fn tui_list_get_item(u64 widget, i64 idx) -> u8*;
fn tui_list_get_count(u64 widget) -> i64;
fn tui_list_clear(u64 widget);
fn tui_list_remove_item(u64 widget, i64 idx);
fn tui_textview_set_text(u64 widget, u8* text);
fn tui_textview_append_line(u64 widget, u8* line);
fn tui_textview_clear(u64 widget);
fn tui_textview_get_line_count(u64 widget) -> i64;
fn tui_button_set_text(u64 widget, u8* text);
```

### Colors

```wandc
fn tui_color_rgb(u8 r, u8 g, u8 b) -> u8;
fn tui_color_index(u8 idx) -> u8;
fn tui_get_cols() -> i64;
fn tui_get_rows() -> i64;
```

---

## Module: std

This module provides basic utility functions.

```wandc
fn exit(u64 code);
fn srand(u64 seed);
fn rand() -> u64;
```

The `rand` function uses a linear congruential generator.

---

## Constants

Import `<syscall>` to use these constants.

### File Open Flags

| Constant | Value | Meaning |
|---|---|---|
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
|---|---|
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
|---|---|---|
| `WNOHANG` | 1 | Return immediately if no child exited |
| `WUNTRACED` | 2 | Report stopped children |
| `WCONTINUED` | 8 | Report continued children |

### Directory Entry Types

| Constant | Value |
|---|---|
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
|---|---|
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
|---|---|
| `EPOLL_CTL_ADD` | 1 |
| `EPOLL_CTL_DEL` | 2 |
| `EPOLL_CTL_MOD` | 3 |

### Poll Events

| Constant | Value |
|---|---|
| `POLLIN` | 1 |
| `POLLPRI` | 2 |
| `POLLOUT` | 4 |
| `POLLERR` | 8 |
| `POLLHUP` | 16 |
| `POLLNVAL` | 32 |

### Fcntl Commands

| Constant | Value |
|---|---|
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

```wandc
struct linux_dirent64 version 1 {
    u64 d_ino version 1;
    u64 d_off version 1;
    u16 d_reclen version 1;
    u8 d_type version 1;
    u8 d_name[256] version 1;
}
```

### timespec

```wandc
struct timespec version 1 {
    i64 tv_sec version 1;
    i64 tv_nsec version 1;
}
```

### sigaction

```wandc
struct sigaction version 1 {
    u64 sa_handler version 1;
    u64 sa_flags version 1;
    u64 sa_restorer version 1;
    u64 sa_mask version 1;
}
```

### stat

```wandc
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

```wandc
struct epoll_event version 1 {
    u32 events version 1;
    u64 data version 1;
}
```

### pollfd

```wandc
struct pollfd version 1 {
    i32 fd version 1;
    i16 events version 1;
    i16 revents version 1;
}
```

### timeval

```wandc
struct timeval version 1 {
    i64 tv_sec version 1;
    i64 tv_usec version 1;
}
```

### wait_status

```wandc
struct wait_status version 1 {
    u32 status version 1;
}
```

### siginfo

```wandc
struct siginfo version 1 {
    i32 si_signo version 1;
    i32 si_errno version 1;
    i32 si_code version 1;
    u64 si_addr version 1;
}
```

### fd_set

```wandc
struct fd_set version 1 {
    u64 fds_bits[16] version 1;
}
```

---

## Complete Example

This example reads a file and prints it.

```wandc
sc.true
#import <io>
#import <syscall>
#import <args>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    if (argc < 2) {
        print_string("usage: cat file\n");
        return(1);
    }

    u8* filename = get_arg(argv, 1);
    i64 fd = file_open(filename, O_RDONLY, 0);
    if (syscall_error(fd) == 1) {
        print_string("cannot open file\n");
        return(1);
    }

    u8 buffer[4096];
    while (1) {
        i64 bytes = file_read(fd, buffer*adr, 4096);
        if (bytes <= 0) {
            break;
        }

        u64 count = (u64)bytes;
        for (u64 i = 0; i < count; i++) {
            print_char(buffer[i]);
        }
    }

    file_close(fd);
    return(0);
}
```

Compile:

```text
wand2c cat.w -o cat -fp
```

Run:

```text
./cat /etc/hostname
```
