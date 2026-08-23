# WandC Language Syntax

## 1. Source Files
Every file must start with an environment token.
Use `sc.true` for programs that run in an operating system.
Use `sc.false` for bare-metal code like kernels or drivers.

## 2. Comments
Use two slash characters for a comment.
```wandc
// This is a comment.
```
The compiler ignores comments.

## 3. Literals
Numbers:
```wandc
u64 a = 4096;
```
Hexadecimal numbers:
```wandc
u64 b = 0x1000;
```
Text strings:
```wandc
u8* msg = "hello";
```

## 4. Primitive Types
| Type | Size |
|---|---|
| `u8` | 1 byte |
| `u16` | 2 bytes |
| `u32` | 4 bytes |
| `u64` | 8 bytes |
| `i8` | 1 byte (signed) |
| `i16` | 2 bytes (signed) |
| `i32` | 4 bytes (signed) |
| `i64` | 8 bytes (signed) |
| `f64` | 8 bytes (float) |
| `void` | 0 bytes |

Use `*` for pointers.
```wandc
u8* p;
```
Use brackets for arrays.
```wandc
u8 buffer[256];
```

## 5. Constants
Constants do not change.
```wandc
const MAX_TASKS = 256;
```

## 6. Enums
Enums give names to numbers.
```wandc
enum State {
    OFF = 0;
    ON = 1;
}
```
Read the value like this: `State:ON`.

## 7. Variables
Declare a variable with a type and a name.
```wandc
u64 x = 10;
```

### Pointers
Use modifiers to show how a pointer works.
- `*i` reads data from the pointer.
- `*o` writes data to the pointer.
- `*io` reads and writes data.

## Pointer Rules

WandC pointers point to variables.
Multi-level pointers are not allowed.

Use pointer modifiers to define data flow.

- `*i` reads the pointed variable.
- `*o` writes the pointed variable.
- `*io` reads and writes the pointed variable.

To pass an array to a function, use `*adr`.

```wandc
fn run(u8* argv);

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u8* args[2];

    args[0] = "sh";
    args[1] = null;

    run(args*adr);

    return(0);
}
```

## 8. Control Flow
Use `if` and `else` to make choices.
```wandc
if (x == 10) {
    x = 0;
}
```

Use `while` to repeat code.
```wandc
while (x < 10) {
    x = x + 1;
}
```

Use `for` to repeat code with a counter.
```wandc
for (u64 i = 0; i < 10; i = i + 1) {
}
```

Use `match` to check multiple values.
```wandc
match (state) {
    case 1 {
        print_string("One");
    }
    default {
        print_string("Other");
    }
}
```

## 9. Functions
A function groups code together.
```wandc
fn add(u64 a, u64 b) -> u64 {
    return(a + b);
}
```

The main function starts the program.
```wandc
fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    return(0);
}
```

## 10. Structures
A structure holds multiple variables.
```wandc
struct Task {
    u64 id;
    u64 state;
}
```
Read a field like this: `task.id`.
Read a pointer field like this: `task_ptr->id`.

## 11. Global Sections
Put global variables in a section.
```wandc
sect.state
    u64 ticks = 0;
EOS
```
Read the variable like this: `state:ticks`.

## 12. Compile-Time Reflection
The compiler knows type sizes.
- `sizeof(Type)` gives the size in bytes.
- `alignof(Type)` gives the alignment.

## 13. Built-In Functions
The compiler has built-in tools.
Use `inb` and `outb` for hardware ports.
Use `atomic_add` for safe math.

## 14. Standard Library
Import modules to get more functions.
```wandc
#import <io>
```

## 15. Inline Assembly
Write CPU instructions in a block.
```wandc
fn halt() {
    ::nasm::{
        hlt
    }
}
```

## 16. Imports
Use `#import` to load files.
```wandc
#import <string>
```

## 17. Memory Safety
The compiler checks your code for safety.
1. Initialize variables before use.
2. Check pointers for `null`.
3. Free memory when done.

## 18. Compiler Formats
Tell the compiler what to make.
- `-fp` makes a normal program.
- `-fk` makes a kernel.
```

### STDLIB.md

```markdown
# WandC Standard Library

The standard library gives you ready functions.
You must import the modules you need.

## Setup
Install the library:
```bash
wand2c -il libw
```

Import modules in your code:
```wandc
#import <io>
#import <mem>
```

## Module: io
This module prints text and reads input.

### Print text
```wandc
fn print_string(u8* s);
fn print_number(u64 num);
fn printf(u8* format, u64 arg1, u64 arg2, u64 arg3);
```

Example:
```wandc
printf("Hello %s\n", name);
```

### Read input
```wandc
fn read_char() -> u8;
fn read_string(u8* buf, u64 max_size);
```

### File operations
```wandc
fn file_open(u8* path, u64 flags, u64 mode) -> i64;
fn file_close(u64 fd) -> i64;
fn file_read(u64 fd, u8* buf, u64 size) -> i64;
fn file_write(u64 fd, u8* buf, u64 size) -> i64;
```

## Module: mem
This module manages memory.

### Start the memory system
Call this function first:
```wandc
fn mem_init(u64 initial_size);
```

### Allocate memory
```wandc
fn malloc(u64 size) -> void*;
fn calloc(u64 num, u64 size) -> void*;
```

### Free memory
```wandc
fn mfree(u8* ptr);
```

Example:
```wandc
mem_init(1048576);
u8* buf = malloc(256);
mfree(buf);
```

## Module: string
This module changes text and memory.

```wandc
fn strlen(u8* s) -> u64;
fn strcmp(u8* s1, u8* s2) -> i64;
fn strcpy(u8* dest, u8* src) -> u8*;
fn memcpy(u8* dest, u8* src, u64 n) -> void*;
fn memset(u8* s, u8 c, u64 n) -> void*;
```

`strcmp` returns 0 if the strings are equal.

## Module: syscall
This module talks to the operating system.

### Open a file
```wandc
fn sys_open(u8* path, u64 flags, u64 mode) -> u64;
```

### Check for errors
```wandc
fn syscall_error(u64 ret) -> u64;
```
It returns 1 if there is an error.

Example:
```wandc
u64 fd = sys_open("file.txt", 0, 0);
if (syscall_error(fd) == 1) {
    print_string("Error");
}
```

### Process control
```wandc
fn sys_fork() -> u64;
fn sys_exit(u64 code);
```

## Module: args
This module reads command arguments.

```wandc
fn get_arg(u64 argv, u64 index) -> u8*;
fn arg_equals(u64 argv, u64 index, u8* expected) -> u64;
```

Example:
```wandc
if (arg_equals(argv, 1, "build") == 1) {
    print_string("Building");
}
```

## Module: math
This module does float math.

```wandc
fn sqrt(f64 x) -> f64;
fn sin(f64 x) -> f64;
fn cos(f64 x) -> f64;
```

## Module: path
This module joins file paths.

```wandc
fn path_exists(u8* path) -> u64;
fn path_join(u8* dest, u8* a, u8* b);
```

## Constants
The `syscall` module has standard constants.

### File flags
| Name | Value | Meaning |
|---|---|---|
| `O_RDONLY` | 0 | Read only |
| `O_WRONLY` | 1 | Write only |
| `O_RDWR` | 2 | Read and write |
| `O_CREAT` | 64 | Make a new file |

### Signals
| Name | Value |
|---|---|
| `SIGINT` | 2 |
| `SIGKILL` | 9 |
| `SIGTERM` | 15 |

## Structures
The `syscall` module has standard structures.

### stat
This structure holds file data.
```wandc
struct stat {
    u64 st_dev;
    u64 st_ino;
    u32 st_mode;
    i64 st_size;
}
```

## Return Statement

Use `return(value);` to return a value.

```wandc
fn add(u64 a, u64 b) -> u64 {
    return(a + b);
}
```

A bare `return;` is allowed.
It is the same as `return(0);`.

```wandc
fn stop() {
    return;
}
```

## Array Sizes

The size of an array must be a compile-time constant.

You can use a number literal.

```wandc
u8 buffer[4096];
```

You can also use a constant.

```wandc
const MAX_NAME = 256;

struct User {
    u8 name[MAX_NAME];
}
```

Constants must appear before you use them in array sizes.

## Volatile and Atomic Variables

Use `volatile` and `atomic` on section variables and structure fields.

Section example:

```wandc
sect.state
    volatile i64 flag = 0;
EOS
```

Structure example:

```wandc
struct Ctx {
    volatile i64 interrupted;
}
```

A write to a `volatile` target emits a memory fence after the store.
