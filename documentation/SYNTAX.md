# WandC Language Syntax Specification

WandC is a systems programming language. It targets operating systems, device drivers, boot code, kernel modules, and hosted user-space programs. WandC provides direct hardware access, explicit memory layout control, strict compile-time safety checks, and Standard 4/6 binary metadata.

---

## 1. Source Files

Every WandC source file must start with an environment token.

```c
sc.true
```

or

```c
sc.false
```

### sc.true

Use `sc.true` for hosted environments. A hosted environment provides operating system services.

Examples:
- Linux user-space programs
- Init systems
- Daemons
- Service managers

`sc.true` enables hosted compiler built-ins:
- `syscall0` through `syscall6`

### sc.false

Use `sc.false` for bare-metal and kernel code.

Examples:
- Kernels
- Schedulers
- Interrupt handlers
- Hardware drivers
- Bootloader modules

`sc.false` forbids hosted built-ins. Use `bmloc`, port I/O, inline assembly, and custom kernel services.

---

## 2. Comments

WandC supports single-line comments only.

```c
// This is a comment.
```

Multi-line comments are not supported.

---

## 3. Literals

### Integer literals

Decimal:
```c
u64 a = 4096;
```

Hexadecimal:
```c
u64 b = 0x1000;
```

### Float literals

```c
f64 pi = 3.14159;
```

### String literals

```c
u8* msg = "boot complete\n";
```

Supported escape sequences:
- `\n` (newline)
- `\t` (tab)
- `\r` (carriage return)
- `\"` (double quote)

---

## 4. Primitive Types

| Type | Size |
|---|---:|
| `u8` | 1 byte |
| `u16` | 2 bytes |
| `u32` | 4 bytes |
| `u64` | 8 bytes |
| `i8` | 1 byte |
| `i16` | 2 bytes |
| `i32` | 4 bytes |
| `i64` | 8 bytes |
| `f64` | 8 bytes |
| `void` | 0 bytes |

Pointers use the `*` symbol.

```c
u8* p;
u64* counter;
```

Arrays use brackets.

```c
u8 buffer[256];
```

Alternative array syntax:

```c
array:u8[256] buffer;
```

Type aliases use `typedef`.

```c
typedef u8[256] SectorBuffer;
```

---

## 5. Constants

Constants are compile-time values.

Syntax:
```c
const NAME = expression;
```

Examples:
```c
const MAX_TASKS = 256;
const TASK_STACK_SIZE = 0x4000;
const FRAMEBUFFER_SIZE = SCREEN_WIDTH * SCREEN_HEIGHT * 4;
```

Constants may use numbers, other constants, arithmetic, enums, and compile-time reflection functions.

Rules:
1. A constant expression must be known at compile time.
2. A constant must not depend on a runtime variable.
3. A constant must not call a user function.
4. A constant may use `sizeof`, `alignof`, `offsetof`, `versionof`, `fieldsof`.

---

## 6. Enums

Enums define named integer values.

Syntax:
```c
enum Name {
    VALUE_A = 0;
    VALUE_B = 1;
}
```

If a value has no explicit number, the compiler assigns the previous value plus one.

Enums can have versions.

```c
enum GpuState version 2 {
    OFF = 0 version 1;
    IDLE = 1 version 1;
    ACTIVE = 2 version 2;
}
```

Enum values are unsigned 64-bit integers. Access uses the colon syntax: `EnumName:ValueName`.

---

## 7. Variables and Pointers

Local variable declaration:
```c
u64 x = 10;
i64 delta = -5;
```

### Pointer access modifiers

WandC provides three pointer modifiers to define data flow semantics.

- `*i` means input pointer.
- `*o` means output pointer.
- `*io` means input-output pointer.

Input pointer:
```c
u64 value*i;
```
- Reading the variable loads the pointed value.
- Assigning to the variable changes the pointer address itself.

Output pointer:
```c
u64 slot*o;
```
- Reading the variable reads the pointer address.
- Assigning to the variable writes through the pointer.

Input-output pointer:
```c
u64 data*io;
```
- Reading the variable loads the pointed value (like `*i`).
- Assigning to the variable writes through the pointer (like `*o`).

Example:
```c
fn modify(u64 x*io) {
    x = x + 1;
}
```

---

## 8. Control Flow

### if / else

```c
if (x == 10) {
    x = 0;
} else {
    x = 1;
}
```

### while

```c
u64 i = 0;
while (i < 10) {
    i = i + 1;
}
```

### for

```c
for (u64 i = 0; i < 10; i = i + 1) {
    outb(0x3F8, i);
}
```

### match

```c
match (state) {
    case 1 {
        print_string("State 1\n");
    }
    case 2 {
        print_string("State 2\n");
    }
    default {
        print_string("Unknown\n");
    }
}
```

Increment and decrement operators (`++`, `--`) are allowed only as full standalone statements.

---

## 9. Functions

Function syntax:
```c
fn name(u64 a, u8* ptr) {
}
```

Function with return type:
```c
fn add(u64 a, u64 b) -> u64 {
    return(a + b);
}
```

Function with multiple return values:
```c
fn range() -> (u64, u64) {
    return(u64 0, u64 1024);
}
```

Destructuring assignment:
```c
u64 low;
u64 high;
[low, high] = range();
```

Function modifiers:
- `extern`: Declares a function without a body.
- `export`: Marks a function for export in the binary metadata.
- `irq`: Marks a function as an interrupt handler (saves and restores all registers).

### Hosted Entry Point

For `sc.true` programs, the entry point must accept three arguments:

```c
fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    return(0);
}
```

- `argc`: Number of command-line arguments.
- `argv`: Pointer to the array of argument strings.
- `envp`: Pointer to the array of environment variables.

---

## 10. Structures and Unions

Structure syntax:
```c
struct Task version 1 {
    u64 pid version 1;
    u64 state version 1;
    u64 stack version 1;
}
```

Field access:
```c
Task t;
t.pid = 1;
```

Pointer field access:
```c
Task* tp = t*adr;
tp->pid = 2;
```

Packed structure:
```c
packed struct HardwareRegister version 1 {
    u8 control version 1;
    u32 value version 1;
}
```

Union syntax:
```c
union PacketView version 1 {
    u64 raw version 1;
    u8 bytes[8] version 1;
}
```

Structure layout rules:
1. Fields keep declaration order.
2. Field alignment equals field size, capped at a maximum of 8 bytes.
3. Packed structures use alignment 1.
4. Total structure size is padded to maximum field alignment.

---

## 11. Global Sections

Global variables live inside `sect` blocks.

```c
sect.kernel_state
    u64 ticks = 0;
    u64 active_cpu = 0;
EOS
```

Access syntax:
```c
kernel_state:ticks = kernel_state:ticks + 1;
```

A section ends with the `EOS` token.

### Section Attributes

Attributes are placed before the `sect` keyword.

```c
align(4096) ro sect.vga_config
    u64 width = 1920;
EOS

align(64) noinit sect.per_cpu
    u64 ticks;
EOS
```

| Attribute | Meaning |
|---|---|
| `align(N)` | Section alignment in memory. N must be a power of two. |
| `ro` | Read-only section. Placed in `.rodata`. |
| `noinit` | Section has no initial data in the binary. Analogous to `.bss`. |

### Volatile and Atomic Section Variables

```c
sect.mmio_regs
    volatile u32 status = 0;
    atomic u64 counter = 0;
EOS
```

Reads and writes to `volatile` variables generate real memory operations. A write to a `volatile` variable emits an `MFENCE` instruction after the store.

---

## 12. Compile-Time Reflection

WandC provides compile-time type inspection.

- `sizeof(Type)`: Returns type size in bytes.
- `alignof(Type)`: Returns type alignment in bytes.
- `offsetof(Type:field)`: Returns field offset in bytes.
- `versionof(Type)`: Returns structure version.
- `fieldsof(Type)`: Returns number of fields.
- `nameof(Type)`: Returns type name as a string pointer.

Reflection is evaluated strictly at compile time.

---

## 13. Built-In Functions

The compiler provides low-level hardware and system primitives.

### Port I/O (x86)

```c
u8 value = inb(0x3F8);
outb(0x3F8, value);
u16 w = inw(0x1F0);
outw(0x1F0, w);
u32 d = inl(0xCF8);
outl(0xCF8, d);
```

### Memory Allocation

Bare-metal mode allocation:
```c
u64 phys = 0x100000;
u64 mapped = bmloc(phys);
```

### Raw System Calls

The compiler exposes raw system call instructions for hosted environments:
```c
syscall0(number);
syscall1(number, arg1);
syscall2(number, arg1, arg2);
syscall3(number, arg1, arg2, arg3);
syscall4(number, arg1, arg2, arg3, arg4);
syscall5(number, arg1, arg2, arg3, arg4, arg5);
syscall6(number, arg1, arg2, arg3, arg4, arg5, arg6);
```

### Atomics and Barriers

```c
u64 val = atomic_load(ptr);
atomic_store(ptr, val);
atomic_add(ptr, 1);
atomic_sub(ptr, 1);
atomic_inc(ptr);
atomic_dec(ptr);
atomic_swap(ptr, new_val);
atomic_cas(ptr, expected, desired);
memory_barrier();
compiler_barrier();
```

---

## 14. Standard Library

High-level system calls, memory management, and I/O are provided by the standard library (`libw`), not the compiler core.

Import required modules:
```c
#import <syscall>
#import <io>
#import <mem>
#import <string>
#import <args>
#import <path>
```

The `<syscall>` library wraps `syscall0`..`syscall6` into named functions like `sys_read`, `sys_write`, `sys_fork`, `sys_execve`, etc.
The `<mem>` library provides `malloc`, `mfree`, `mrealloc`, and `calloc` on top of the raw `mloc` syscall.

---

## 15. Inline Assembly

Inline assembly uses the `::nasm::` block.

```c
fn halt_cpu() {
    ::nasm::{
        cli
        hlt
    }
}
```

Local variables can be accessed with brackets:
```c
fn write_value(u64 value) {
    ::nasm::{
        mov rax, [value]
    }
}
```

Section variables use the section syntax:
```c
fn tick() {
    ::nasm::{
        mov rax, [cpu_state:ticks]
        add rax, 1
        mov [cpu_state:ticks], rax
    }
}
```

---

## 16. Imports

Use `#import` to load library modules.

```c
#import <io>
#import <string>
```

System libraries use angle brackets `<name>`. Local files use plain names `name`. Import paths must not include file extensions.

---

## 17. Dynamic Execution Modules

WandC supports dynamic execution modules with the `.wexp` format.

Use `jmpto` to execute a module dynamically.

```c
fn main(u64 argc, u64 argv, u64 envp) {
    u64 input_val = 100;
    jmpto module.wexp {
        input_val;
    }
}
```

---

## 18. Operator Precedence

Listed from highest to lowest precedence.

| Operator | Meaning |
|---|---|
| `*adr` | Address-of |
| `->` | Pointer member access |
| `.` | Member access |
| `[index]` | Array index |
| `!`, `~` | Logical NOT, Bitwise NOT |
| `*`, `/`, `%` | Multiply, Divide, Modulo |
| `+`, `-` | Add, Subtract |
| `<<`, `>>` | Shift left, Shift right |
| `<`, `<=`, `>`, `>=` | Relational operators |
| `==`, `!=` | Equality operators |
| `&` | Bitwise AND |
| `^` | Bitwise XOR |
| `\|` | Bitwise OR |
| `&&` | Logical AND |
| `\|\|` | Logical OR |
| `=` | Assignment |

---

## 19. Memory Safety Rules

The compiler performs static analysis to enforce memory safety.

1. Local variables must be initialized before use.
2. Structure fields must be initialized before use.
3. Pointers returned by allocators must be checked for `null` before dereference.
4. Freed pointers must not be used (use-after-free detection).
5. Allocated pointers must be freed before the function returns (leak detection).
6. Pointer writes through `*o` are checked for initialization.

Example of a required null check:
```c
void* p = malloc(4096);
if (p != null) {
    mfree(p);
}
```

---

## 20. Compiler Output Formats

The compiler supports multiple target formats.

| Flag | Format | Description |
|---|---|---|
| `-fp` | program | Hosted ELF64 executable. Requires `sc.true`. Entry is `main`. |
| `-fo` | object | Relocatable ELF64 object file. For linking. |
| `-fr` | raw | Flat binary image. No ELF header. Entry set via `--entry`. |
| `-fk` | kernel | Freestanding kernel image. Requires `sc.false`. Entry set via `--entry`. |
| `-fw` | wexp | Dynamic execution module. |

Examples:
```bash
wand2c init.w -o init -fp
wand2c boot.w -o boot.bin -fr --entry=start
wand2c kernel.w -o kernel.kbin -fk --entry=kmain
```

---

## 21. Complete Example

```c
sc.false

const MAX_TASKS = 64;
const STACK_SIZE = 0x4000;

enum TaskState {
    READY = 1;
    RUNNING = 2;
    SLEEPING = 3;
}

struct Task version 1 {
    u64 pid version 1;
    u64 state version 1;
    u64 stack version 1;
}

sect.scheduler_data
    u64 current_task = 0;
    u64 task_count = 0;
EOS

fn task_size() -> u64 {
    return(sizeof(Task));
}

fn kmain() {
    u64 size = sizeof(Task);
    u64 state = TaskState:READY;

    if (state == TaskState:READY) {
        state = TaskState:RUNNING;
    }

    scheduler_data:current_task = 1;
}
```

Compile:
```bash
wand2c kernel.w -o kernel.kbin -fk --entry=kmain
```
