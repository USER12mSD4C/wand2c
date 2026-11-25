# llibw.86 - Linux Wand Userspace Library

`llibw.86` is a Wand-native userspace library set for **Linux x86_64**. It provides syscall wrappers and common utilities for hosted Linux programs written in Wand.

## Target

- **Target OS**: Linux x86_64
- **ABI**: Linux syscall numbers and calling convention as used by x86_64 Linux
- **Language**: Wand (`.w`/`.wh`) only

## Modules

- `syscall_constants.wh` - Linux constants (open flags, errno, signals, poll/epoll, mmap, stat mode bits, etc.).
- `syscall_structs.wh` - Linux-compatible userspace struct layouts (`stat`, `linux_dirent64`, `timespec`, `sigaction`, `pollfd`, `epoll_event`, etc.).
- `syscall.wh` / `syscall.w` - Raw Linux syscall wrappers and `syscall_error`.
- `string.wh` / `string.w` - String/memory helpers (`strlen`, `strcmp`, `memcpy`, `itoa`, parsing helpers, etc.).
- `mem.wh` / `mem.w` - mmap-backed arena allocator (`mem_init`, `malloc`, `calloc`, `mrealloc`, `mfree`, `mfree_all`).
- `io.wh` / `io.w` - Console/file I/O helpers, formatted output, buffered stdout helpers, `write_all`/`read_full`, `strerror`.
- `fileio.wh` / `fileio.w` - Buffered file reader/writer utilities.
- `process.wh` / `process.w` - fork/exec/wait/kill/process group helpers.
- `path.wh` / `path.w` - Path utilities (`exists`, `is_dir`, `join`, `basename`, `dirname`, stat helpers).
- `args.wh` / `args.w` - argv/envp helpers.
- `time.wh` / `time.w` - Time formatting/conversion helpers.
- `math.wh` / `math.w`, `fpmath.wh` / `fpmath.w` - Math helpers.
- `signal.wh` / `signal.w` - Signal helpers.
- `unistd.wh` / `unistd.w` - POSIX-like helpers.
- `keyboard.wh` / `keyboard.w`, `tui.wh` / `tui.w` - Terminal/input helpers.
- `vector.wh` / `vector.w` - Simple vector/string-vector utilities.
- `std.wh` / `std.w` - Minimal std helpers (`exit`, `rand`, `srand`).

## Error Convention

Most raw syscall wrappers return Linux syscall return values as `u64`. Use `syscall_error(ret)` to detect encoded negative errno values. Higher-level modules may return `i64` negative errno or module-specific status; check the module header.

## Notes

- This library targets Linux semantics and Linux syscall ABI. Do not use it for UTMS7-specific syscalls; use `../ulibw.86` for UTMS7.
- Some modules are convenience wrappers over raw syscalls and may evolve; prefer checking the `.wh` header for the current API.
