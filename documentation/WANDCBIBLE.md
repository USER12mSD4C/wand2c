# WandC Bible

A complete guide from zero to systems programming.

This book assumes you know nothing about programming.
It teaches you WandC from your first terminal command to writing kernel modules and GPU compute shaders.

---

## Part 1: Foundations

### Chapter 1: What is WandC

WandC is a systems programming language.
It compiles directly to x86_64 machine code.
It does not use LLVM, GCC, or any other compiler backend.

WandC is designed for:

- Operating system development
- Device drivers
- Low-level utilities (coreutils, binutils)
- Kernel modules
- Bare-metal firmware
- GPU compute programming

What makes WandC different from C:

- Compile-time memory safety analysis (leaks, use-after-free, null dereference)
- Struct and field versioning for stable binary interfaces
- Explicit pointer data flow modifiers (`*i`, `*o`, `*io`)
- Built-in optimizer with algebraic simplifications
- Custom binary format (.wexp) with type metadata
- No undefined behavior by design

What WandC does NOT do:

- No garbage collector
- No runtime type information
- No dynamic dispatch
- No multi-level pointers (no `u8**`)
- No implicit type conversions

### Chapter 2: Installing the Compiler

This chapter assumes you have a Linux system.
If you just installed Linux Mint, open the terminal from the application menu.

#### Step 1: Install Rust toolchain

WandC compiler is written in Rust. You need `cargo` to build it.

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

Verify installation:

```
cargo --version
```

#### Step 2: Get the compiler source

```
git clone https://github.com/user12msd4c/wand2c.git
cd wand2c
```

#### Step 3: Build

```
cargo build --release
```

The compiler binary is at `target/release/wand2c`.

#### Step 4: Install to PATH

```
sudo cp target/release/wand2c /usr/local/bin/
```

Verify:

```
wand2c --help
```

#### Step 5: Install the standard library

```
wand2c -il libw
```

This copies all library files to `~/.local/lib/libw/`.

Available modules:

| Module | Purpose |
|--------|---------|
| `<io>` | Console and file I/O |
| `<mem>` | Heap memory allocation |
| `<string>` | C-style string operations |
| `<syscall>` | Linux system calls |
| `<args>` | Command-line argument parsing |
| `<path>` | File path utilities |
| `<fileio>` | Line-by-line file reader |
| `<vector>` | Dynamic arrays |
| `<unistd>` | Process and directory helpers |
| `<math>` | Floating-point math |
| `<fpmath>` | Fixed-point math |
| `<keyboard>` | Terminal key input |
| `<tui>` | Terminal user interface |
| `<std>` | Basic utilities (rand, exit) |

### Chapter 3: Your First Program

Create a file named `hello.w`:

```
sc.true

#import <io>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    print_string("Hello, WandC!\n");
    return(0);
}
```

Compile it:

```
wand2c hello.w -o hello -fp
```

Run it:

```
./hello
```

Output:

```
Hello, WandC!
```

Let's break down every line.

**Line 1: `sc.true`**

Every WandC file starts with an environment token.

- `sc.true` means the program runs under an operating system. It requires a `main` function. System calls are available.
- `sc.false` means the program runs on bare metal (no OS). It requires a `kmain` function. Critical sections and IRQ handlers are available.

For now, always use `sc.true`.

**Line 3: `#import <io>`**

This imports the `io` module from the standard library.
It gives you access to `print_string`, `print_number`, `print_char`, and other I/O functions.

System modules use angle brackets: `#import <io>`.
Local modules use double quotes: `#import "mymodule"`.

**Line 5: `fn main(u64 argc, u64 argv, u64 envp) -> u64`**

This declares the entry point function.

- `fn` declares a function.
- `main` is the function name.
- `u64 argc` is the argument count.
- `u64 argv` is the argument vector (array of string pointers).
- `u64 envp` is the environment pointer array.
- `-> u64` means the function returns a 64-bit unsigned integer.

**Line 6: `print_string("Hello, WandC!\n");`**

This calls the `print_string` function from the `io` module.
The `\n` is a newline character.

**Line 7: `return(0);`**

This returns 0 to the operating system.
A return value of 0 means success.

#### Compile with verbose output

Add `-v` to see the full compilation pipeline:

```
wand2c hello.w -o hello -fp -v
```

This shows every stage: parsing, optimization, type checking, code generation, and linking.

### Chapter 4: File Types

WandC uses two file types.

#### Source Files (.w)

Source files contain executable code.
They hold function implementations.

```
sc.true

#import <io>

fn add(u64 a, u64 b) -> u64 {
    return(a + b);
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u64 result = add(3, 4);
    print_number(result);
    print_char(10);
    return(0);
}
```

#### Header Files (.wh)

Header files contain declarations only.
They define interfaces without implementation.

A header file declares:

- Function signatures (no body)
- Structure definitions
- Constants
- Enumerations

```
sc.true

fn add(u64 a, u64 b) -> u64;
fn subtract(u64 a, u64 b) -> u64;

struct Config version 1 {
    u64 timeout version 1;
    u8* path version 1;
}
```

#### Rules

1. Never put function bodies in `.wh` files.
2. Never put implementation logic in `.wh` files.
3. Use `.wh` for public API.
4. Use `.w` for implementation.

#### Multi-file project

```
project/
├── main.w          (entry point)
├── math_utils.wh   (declarations)
├── math_utils.w    (implementation)
├── parser.wh       (declarations)
└── parser.w        (implementation)
```

Compile only the entry file. The compiler resolves dependencies automatically:

```
wand2c main.w -o program -fp
```

### Chapter 5: Compilation Flags and Output Formats

The compiler supports five output formats.

| Flag | Format | Use case |
|------|--------|----------|
| `-fp` | Program | Normal Linux executable. Requires `sc.true`. |
| `-fo` | Object | Relocatable ELF object for linking with other tools. |
| `-fr` | Raw | Flat binary image. No headers. For firmware. |
| `-fk` | Kernel | Freestanding kernel image. Requires `sc.false`. |
| `-fw` | Wexp | Dynamic execution module with ABI metadata. |

#### Examples

Compile a normal program:

```
wand2c main.w -o main -fp
```

Compile a relocatable object:

```
wand2c module.w -o module.o -fo
```

Compile a flat binary for bare metal:

```
wand2c boot.w -o boot.bin -fr --entry=start
```

Compile a kernel:

```
wand2c kernel.w -o kernel.img -fk --entry=kmain
```

Compile a dynamic module:

```
wand2c plugin.w -o plugin.wexp -fw
```

#### Other flags

| Flag | Description |
|------|-------------|
| `-o <file>` | Set output file path |
| `-v, --verbose` | Show detailed compilation output |
| `--entry <name>` | Set entry function (raw and kernel only) |
| `-il <path>` | Install a library |

---

## Part 2: Language Core

### Chapter 6: Primitive Types and Literals

WandC has fixed-size integer types.

| Type | Size | Range |
|------|------|-------|
| `u8` | 1 byte | 0 to 255 |
| `u16` | 2 bytes | 0 to 65535 |
| `u32` | 4 bytes | 0 to 4294967295 |
| `u64` | 8 bytes | 0 to 18446744073709551615 |
| `i8` | 1 byte | -128 to 127 |
| `i16` | 2 bytes | -32768 to 32767 |
| `i32` | 4 bytes | -2147483648 to 2147483647 |
| `i64` | 8 bytes | -9223372036854775808 to 9223372036854775807 |
| `f64` | 8 bytes | IEEE 754 double-precision float |
| `void` | 0 bytes | No value |

#### Literals

Decimal numbers:

```
u64 a = 4096;
u64 b = 0;
```

Hexadecimal numbers:

```
u64 addr = 0x400078;
u8 magic = 0xFF;
```

Floating-point numbers:

```
f64 pi = 3.14159;
f64 neg = -1.5;
```

String literals:

```
u8* msg = "hello world";
```

Null pointer:

```
u8* ptr = null;
```

### Chapter 7: Variables and Constants

#### Variables

Declare a variable with a type and a name:

```
u64 x = 10;
i64 temperature = -5;
u8* name = "wand";
```

You can declare without initialization, but the compiler will warn if you read it before writing:

```
u64 y;
y = 42;
```

#### Constants

Constants are compile-time values. They cannot change.

```
const MAX_SIZE = 4096;
const MAGIC = 0xDEADBEEF;
```

Constants can be used in array sizes:

```
u8 buffer[MAX_SIZE];
```

Constants must be declared before use.

#### Multiple assignment

```
u64 a = 1, b = 2, c = 3;
```

Wait, actually WandC does not support this syntax. Each variable needs its own declaration:

```
u64 a = 1;
u64 b = 2;
u64 c = 3;
```

### Chapter 8: Operators and Expressions

#### Arithmetic operators

| Operator | Meaning | Example |
|----------|---------|---------|
| `+` | Addition | `a + b` |
| `-` | Subtraction | `a - b` |
| `*` | Multiplication | `a * b` |
| `/` | Division | `a / b` |
| `%` | Modulo | `a % b` |

#### Bitwise operators

| Operator | Meaning | Example |
|----------|---------|---------|
| `&` | Bitwise AND | `a & b` |
| `\|` | Bitwise OR | `a \| b` |
| `^` | Bitwise XOR | `a ^ b` |
| `~` | Bitwise NOT | `~a` |
| `<<` | Shift left | `a << 3` |
| `>>` | Shift right | `a >> 2` |

#### Comparison operators

| Operator | Meaning |
|----------|---------|
| `==` | Equal |
| `!=` | Not equal |
| `<` | Less than |
| `<=` | Less or equal |
| `>` | Greater than |
| `>=` | Greater or equal |

#### Logical operators

| Operator | Meaning |
|----------|---------|
| `&&` | Logical AND |
| `\|\|` | Logical OR |

#### Assignment operators

| Operator | Meaning | Equivalent |
|----------|---------|------------|
| `=` | Assign | |
| `+=` | Add and assign | `x = x + val` |
| `-=` | Subtract and assign | `x = x - val` |
| `*=` | Multiply and assign | `x = x * val` |
| `/=` | Divide and assign | `x = x / val` |
| `%=` | Modulo and assign | `x = x % val` |
| `&=` | AND and assign | `x = x & val` |
| `\|=` | OR and assign | `x = x \| val` |
| `^=` | XOR and assign | `x = x ^ val` |
| `<<=` | Shift left and assign | `x = x << val` |
| `>>=` | Shift right and assign | `x = x >> val` |

#### Increment and decrement

```
x++;
x--;
```

These are equivalent to `x = x + 1` and `x = x - 1`.

#### Type casting

Cast between integer and float:

```
u64 int_val = 42;
f64 float_val = (f64)int_val;
u64 back = (u64)float_val;
```

### Chapter 9: Control Flow

#### If and else

```
if (x > 10) {
    print_string("big\n");
} else {
    print_string("small\n");
}
```

Chained conditions:

```
if (x == 0) {
    print_string("zero\n");
} else if (x == 1) {
    print_string("one\n");
} else {
    print_string("other\n");
}
```

#### While loop

```
u64 i = 0;
while (i < 10) {
    print_number(i);
    print_char(10);
    i++;
}
```

#### For loop

```
for (u64 i = 0; i < 10; i++) {
    print_number(i);
    print_char(32);
}
print_char(10);
```

The `for` loop has three parts:

1. Init: `u64 i = 0` (runs once)
2. Condition: `i < 10` (checked before each iteration)
3. Post: `i++` (runs after each iteration)

#### Continue and break

`continue` skips to the next iteration:

```
for (u64 i = 0; i < 10; i++) {
    if (i == 5) {
        continue;
    }
    print_number(i);
    print_char(32);
}
```

`break` exits the loop:

```
u64 i = 0;
while (1) {
    if (i >= 10) {
        break;
    }
    i++;
}
```

#### Match statement

`match` checks a value against multiple cases:

```
match (state) {
    case 0 {
        print_string("idle\n");
    }
    case 1 {
        print_string("running\n");
    }
    case 2 {
        print_string("stopped\n");
    }
    default {
        print_string("unknown\n");
    }
}
```

### Chapter 10: Functions and Multiple Return Values

#### Basic function

```
fn add(u64 a, u64 b) -> u64 {
    return(a + b);
}
```

- `fn` declares a function.
- Parameters are declared as `type name`.
- `-> u64` specifies the return type.
- `return(value)` returns a value.

#### Void function (no return value)

```
fn print_hello() {
    print_string("hello\n");
}
```

A bare `return;` is equivalent to `return(0);`.

#### Multiple return values

Functions can return tuples:

```
fn divmod(u64 a, u64 b) -> (u64, u64) {
    return(a / b, a % b);
}
```

Destructure the result:

```
u64 quotient, remainder;
[quotient, remainder] = divmod(17, 5);
```

Use `_` to ignore a value:

```
[_, remainder] = divmod(17, 5);
```

#### Recursion

```
fn factorial(u64 n) -> u64 {
    if (n <= 1) {
        return(1);
    }
    return(n * factorial(n - 1));
}
```

### Chapter 11: Pointers and Modifiers

Pointers store memory addresses.

#### Declaration

```
u8* ptr;
u64* number_ptr;
```

#### The *adr operator

`*adr` takes the address of a variable:

```
u64 x = 42;
u64* ptr = x*adr;
```

#### Pointer modifiers

WandC uses explicit modifiers to declare data flow direction:

| Modifier | Meaning |
|----------|---------|
| `*i` | Function reads through this pointer |
| `*o` | Function writes through this pointer |
| `*io` | Function reads and writes |

Example:

```
fn read_value(u64* out*o) {
    out = 42;
}

fn modify_value(u64* val*io) {
    val = val + 10;
}

fn main() -> u64 {
    u64 x = 0;
    read_value(x*adr);
    modify_value(x*adr);
    print_number(x);
    print_char(10);
    return(0);
}
```

Output: `52`

#### Passing arrays

Pass an array address with `*adr`:

```
fn process(u8* data*o, u64 size) {
    for (u64 i = 0; i < size; i++) {
        data[i] = i;
    }
}

fn main() -> u64 {
    u8 buffer[256];
    process(buffer*adr, 256);
    return(0);
}
```

#### Multi-level pointers are forbidden

WandC does not allow `u8**`. Use single pointers and pass addresses with `*adr`.

Incorrect:

```
u8** ptr;
```

Correct:

```
u8* ptr;
u8 buffer[256];
ptr = buffer*adr;
```

### Chapter 12: Structures, Unions, Enums, Typedef

#### Structures

A struct groups multiple fields:

```
struct Point version 1 {
    i64 x version 1;
    i64 y version 1;
}
```

Access fields with `.`:

```
Point pt;
pt.x = 10;
pt.y = 20;
```

Access through a pointer with `->`:

```
fn move_point(Point* p, i64 dx, i64 dy) {
    p->x = p->x + dx;
    p->y = p->y + dy;
}
```

#### Versioning

The `version` keyword tracks ABI compatibility.
When you add a field in version 2, old code that only knows version 1 will still work.

```
struct Config version 2 {
    u32 flags version 1;
    u64 timeout version 1;
    u8* name version 2;
}
```

#### Packed structures

Use `packed` to remove padding:

```
packed align(1) struct Packet {
    u8 type version 1;
    u16 length version 1;
    u32 data version 1;
}
```

`sizeof(Packet)` is 7 bytes (no padding).

#### Unions

A union stores different types in the same memory:

```
union Data version 1 {
    u64 as_u64 version 1;
    f64 as_f64 version 1;
    u8 bytes[8] version 1;
}
```

#### Enums

Enums assign names to numbers:

```
enum State version 1 {
    Idle = 0 version 1;
    Running = 1 version 1;
    Stopped = 2 version 1;
}
```

Read values with `EnumName:Value`:

```
State s = State:Running;
if (s == State:Running) {
    print_string("running\n");
}
```

#### Typedef

Create type aliases:

```
typedef u8[256] Buffer;
typedef i64 Result;
```

### Chapter 13: Global Sections and EOS

Global variables live in named sections.
Sections are terminated by `EOS` (End Of Section).

```
sect.state
    u64 ticks = 0;
    u8* device_name = "gpu0";
EOS
```

Access section variables with `section:variable`:

```
state:ticks = state:ticks + 1;
print_string(state:device_name);
```

#### Section modifiers

```
align(4096) ro sect.config
    u64 magic = 0x1234;
EOS
```

- `align(N)`: Set section alignment (must be power of two).
- `ro`: Read-only section.
- `noinit`: Exclude from initialization data.

#### Array initialization in sections

```
sect.shader_data
    u8 code[16] = {
        0x00, 0x00, 0x84, 0xBE,
        0x01, 0x00, 0x84, 0xBE,
        0x02, 0x00, 0x84, 0xBE,
        0x03, 0x00, 0x84, 0xBE
    };
EOS
```

### Chapter 14: Volatile and Atomic Variables

#### Volatile

Use `volatile` for memory-mapped I/O and hardware registers.
The compiler never optimizes reads or writes to volatile variables.
A memory fence is emitted after every store.

```
sect.hardware
    volatile u64 status_register = 0;
EOS

fn poll_device() -> u64 {
    while (hardware:status_register == 0) {
        // spin
    }
    return(hardware:status_register);
}
```

#### Atomic

Use `atomic` for lock-free shared data between threads or cores.

```
sect.shared
    atomic u64 counter = 0;
EOS

fn increment() {
    atomic_add(shared:counter*adr, 1);
}
```

#### Atomic built-in functions

| Function | Operation |
|----------|-----------|
| `atomic_load(ptr)` | Read atomically |
| `atomic_store(ptr, val)` | Write atomically |
| `atomic_add(ptr, val)` | Add atomically |
| `atomic_sub(ptr, val)` | Subtract atomically |
| `atomic_inc(ptr)` | Increment atomically |
| `atomic_dec(ptr)` | Decrement atomically |
| `atomic_swap(ptr, val)` | Swap atomically |
| `atomic_cas(ptr, exp, des)` | Compare-and-swap |
| `memory_barrier()` | Full memory fence |
| `compiler_barrier()` | Prevent compiler reordering |

## Part 3: Memory and Safety

### Chapter 15: Stack vs Heap

Every program uses two kinds of memory.

#### Stack memory

Stack memory is automatic.
Variables declared inside a function live on the stack.
They are created when the function starts and destroyed when the function returns.

```
fn example() -> u64 {
    u64 x = 10;
    u64 y = 20;
    u8 buffer[256];
    return(x + y);
}
```

Stack memory is fast.
No allocation or deallocation is needed.
But stack memory is limited.
Large buffers or deeply nested calls can overflow the stack.

#### Heap memory

Heap memory is manual.
You request a block of memory and you must return it when done.

WandC provides two heap allocators.

**Arena allocator** (from the `mem` module):

```
sc.true

#import <io>
#import <mem>
#import <string>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    mem_init(1048576);

    u8* buffer = malloc(256);
    if (buffer != null) {
        strcpy(buffer, "Hello from heap");
        print_string(buffer);
        print_char(10);
        mfree(buffer);
    }

    return(0);
}
```

Rules for the arena allocator:

1. Call `mem_init(size)` before any allocation.
2. Check every pointer for `null` after allocation.
3. Call `mfree(ptr)` for every block you no longer need.
4. Never use a pointer after calling `mfree` on it.

**Raw allocator** (from the `syscall` module):

```
sc.true

#import <syscall>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u8* block = mloc(0, 4096);
    if (block != null) {
        block[0] = 0xFF;
        mfree(block);
    }
    return(0);
}
```

The `mloc` function allocates memory through the `mmap` system call.
It stores the block size in an 8-byte header before the returned pointer.
The `mfree` function reads the header and calls `munmap`.

### Chapter 16: Pointer Safety Rules

WandC enforces pointer safety at compile time.
The compiler tracks the state of every pointer through the function body.

#### Rule 1: Initialize before use

```
u64 x;
print_number(x);
```

This produces an error:

```
error: use of potentially uninitialized variable 'x'
  note: initialize it: 'u64 x = null;'
```

Fix:

```
u64 x = 0;
print_number(x);
```

#### Rule 2: Check allocation results

```
u8* ptr = malloc(1024);
ptr[0] = 42;
```

This produces an error:

```
error: potential null pointer dereference of 'ptr'
  note: wrap in null check: if (ptr != null) { ... }
```

Fix:

```
u8* ptr = malloc(1024);
if (ptr != null) {
    ptr[0] = 42;
    mfree(ptr);
}
```

#### Rule 3: Never use freed memory

```
u8* data = malloc(512);
mfree(data);
data[0] = 42;
```

This produces an error:

```
error: use-after-free violation on pointer 'data'
```

Fix: remove the access after `mfree`, or re-allocate.

#### Rule 4: Free before return

```
fn process() -> u64 {
    u8* buffer = malloc(1024);
    return(0);
}
```

This produces a warning:

```
warning: potential memory leak in function 'process': pointer 'buffer' was never freed via 'mfree()'
  note: call mfree(ptr) before the function returns, or document that ownership is transferred
```

Fix:

```
fn process() -> u64 {
    u8* buffer = malloc(1024);
    if (buffer != null) {
        mfree(buffer);
    }
    return(0);
}
```

#### Rule 5: Null-check before free

```
u8* ptr = malloc(64);
mfree(ptr);
```

This produces a warning:

```
warning: freeing potentially null pointer 'ptr'
  note: pointer 'ptr' was allocated on line 1 but never checked for null
```

Fix:

```
u8* ptr = malloc(64);
if (ptr != null) {
    mfree(ptr);
}
```

### Chapter 17: Compile-Time Memory Safety Analysis

The compiler runs memory safety analysis during Stage 3.
It does not add runtime checks.
It analyzes the control flow graph of every function.

What it detects:

| Error | Severity | Description |
|-------|----------|-------------|
| Uninitialized variable | Error | Reading a variable before any assignment |
| Uninitialized struct field | Error | Reading a struct field before assignment |
| Use-after-free | Error | Accessing a pointer after `mfree` |
| Null pointer dereference | Error | Accessing through a pointer that was never null-checked |
| Memory leak | Warning | Allocated pointer never freed before function exit |
| Freeing null pointer | Warning | Calling `mfree` on a pointer that was never null-checked |

Errors abort compilation.
Warnings are reported but do not stop the build.

The analyzer tracks state through `if` branches:

```
u8* ptr = malloc(256);
if (ptr != null) {
    ptr[0] = 1;
    mfree(ptr);
}
```

Inside the `if` block, the analyzer knows `ptr` is not null.
The access `ptr[0]` is allowed.
After `mfree`, any further access to `ptr` is an error.

### Chapter 18: Common Mistakes and Fixes

#### Mistake 1: Forgetting mem_init

```
fn main() -> u64 {
    u8* ptr = malloc(256);
    return(0);
}
```

This crashes at runtime because the arena is not initialized.

Fix:

```
fn main() -> u64 {
    mem_init(1048576);
    u8* ptr = malloc(256);
    if (ptr != null) {
        mfree(ptr);
    }
    return(0);
}
```

#### Mistake 2: Buffer overflow

```
u8 buffer[16];
for (u64 i = 0; i < 100; i++) {
    buffer[i] = i;
}
```

WandC does not perform bounds checking at compile time for dynamic indices.
This writes past the end of the buffer and corrupts memory.

Fix: ensure the loop bound matches the array size.

```
u8 buffer[16];
for (u64 i = 0; i < 16; i++) {
    buffer[i] = i;
}
```

#### Mistake 3: Dangling pointer after realloc

```
u8* ptr = malloc(64);
u8* new_ptr = mrealloc(ptr, 128);
ptr[0] = 42;
```

After `mrealloc`, the old pointer `ptr` may be invalid.

Fix: use the new pointer.

```
u8* ptr = malloc(64);
u8* new_ptr = mrealloc(ptr, 128);
if (new_ptr != null) {
    new_ptr[0] = 42;
    mfree(new_ptr);
}
```

#### Mistake 4: Double free

```
u8* ptr = malloc(64);
if (ptr != null) {
    mfree(ptr);
    mfree(ptr);
}
```

The second `mfree` corrupts the allocator state.

Fix: set the pointer to null after freeing, or restructure the code.

```
u8* ptr = malloc(64);
if (ptr != null) {
    mfree(ptr);
    ptr = null;
}
```

---

## Part 4: Standard Library

### Chapter 19: io - Printing and Reading

Import with `#import <io>`.

#### Console output

```
fn print_char(u8 c);
fn print_string(u8* s);
fn print_number(u64 num);
fn print_signed_number(i64 num);
fn printf(u8* format, u64 arg1, u64 arg2, u64 arg3);
```

The `printf` function supports three format specifiers:

| Specifier | Output |
|-----------|--------|
| `%v` | Unsigned integer |
| `%d` | Signed integer |
| `%s` | String |

Example:

```
printf("name: %s, count: %v, diff: %d\n", name, count, diff);
```

#### Console input

```
fn read_char() -> u8;
fn read_string(u8* buf, u64 max_size);
fn read_integer() -> i64;
fn read_float() -> f64;
```

Example:

```
u8 input[256];
print_string("Enter your name: ");
read_string(input*adr, 256);
printf("Hello, %s\n", input*adr);
```

#### File operations

```
fn file_open(u8* path, u64 flags, u64 mode) -> i64;
fn file_close(u64 fd) -> i64;
fn file_read(u64 fd, u8* buf, u64 size) -> i64;
fn file_write(u64 fd, u8* buf, u64 size) -> i64;
fn file_remove(u8* path) -> i64;
```

Example:

```
i64 fd = file_open("/tmp/test.txt", O_WRONLY | O_CREAT, 0o644);
if (syscall_error(fd) == 0) {
    file_write(fd, "hello\n"*adr, 6);
    file_close(fd);
}
```

### Chapter 20: mem - Heap Allocation

Import with `#import <mem>`.

```
fn mem_init(u64 initial_size);
fn malloc(u64 size) -> void*;
fn calloc(u64 num, u64 size) -> void*;
fn mrealloc(u8* ptr, u64 new_size) -> u8*;
fn mfree(u8* ptr);
fn mfree_all();
```

The allocator uses a first-fit strategy with block headers.

| Function | Description |
|----------|-------------|
| `mem_init` | Allocates an arena of the given size. Call once before any allocation. |
| `malloc` | Returns a block of at least `size` bytes. Returns `null` on failure. |
| `calloc` | Like `malloc` but fills the block with zeros. |
| `mrealloc` | Changes the block size. Copies existing data. Returns a new pointer. |
| `mfree` | Marks a block as free. Does not return memory to the OS. |
| `mfree_all` | Resets the entire arena. All previous pointers become invalid. |

### Chapter 21: string - C-Style String Operations

Import with `#import <string>`.

```
fn strlen(u8* s) -> u64;
fn strcmp(u8* s1, u8* s2) -> i64;
fn strcpy(u8* dest, u8* src) -> u8*;
fn strcat(u8* dest, u8* src) -> u8*;
fn memcpy(u8* dest, u8* src, u64 n) -> void*;
fn memset(u8* s, u8 c, u64 n) -> void*;
fn atoi(u8* s) -> u64;
fn itoa(i64 num, u8* buf) -> u8*;
```

The `strcmp` function returns:

| Return value | Meaning |
|--------------|---------|
| 0 | Strings are equal |
| -1 | `s1` is less than `s2` |
| 1 | `s1` is greater than `s2` |

Example:

```
u8 a[64];
u8 b[64];
strcpy(a*adr, "hello");
strcpy(b*adr, "world");
strcat(a*adr, " ");
strcat(a*adr, b*adr);
print_string(a*adr);
print_char(10);
```

Output: `hello world`

### Chapter 22: syscall - Talking to the Kernel

Import with `#import <syscall>`.

This module provides direct Linux system calls.

#### Error checking

```
fn syscall_error(u64 ret) -> u64;
```

Returns 1 if `ret` is an error code, 0 otherwise.

#### File operations

```
fn sys_open(u8* path, u64 flags, u64 mode) -> u64;
fn sys_close(u64 fd) -> u64;
fn sys_read(u64 fd, u8* buf, u64 size) -> u64;
fn sys_write(u64 fd, u8* buf, u64 size) -> u64;
fn sys_lseek(u64 fd, u64 offset, u64 whence) -> u64;
fn sys_ioctl(u64 fd, u64 request, u64 arg) -> u64;
fn sys_stat(u8* path, u8* statbuf) -> u64;
fn sys_fstat(u64 fd, u8* statbuf) -> u64;
fn sys_mkdir(u8* path, u64 mode) -> u64;
fn sys_unlink(u8* path) -> u64;
```

#### Process control

```
fn sys_fork() -> u64;
fn sys_execve(u8* path, u8* argv, u8* envp) -> u64;
fn sys_wait4(u64 pid, u64* status, u64 options, u8* rusage) -> u64;
fn sys_getpid() -> u64;
fn sys_kill(u64 pid, u64 sig) -> u64;
fn sys_exit(u64 code);
```

#### Time

```
fn sys_nanosleep(u8* req, u8* rem) -> u64;
fn sys_clock_gettime(u64 clockid, u8* tp) -> u64;
```

#### Network

```
fn sys_socket(u64 domain, u64 type, u64 protocol) -> u64;
fn sys_bind(u64 sockfd, u8* addr, u64 addrlen) -> u64;
fn sys_listen(u64 sockfd, u64 backlog) -> u64;
fn sys_accept(u64 sockfd, u8* addr, u64* addrlen) -> u64;
fn sys_connect(u64 sockfd, u8* addr, u64 addrlen) -> u64;
```

#### Event polling

```
fn sys_epoll_create(u64 flags) -> u64;
fn sys_epoll_ctl(u64 epfd, u64 op, u64 fd, u8* event) -> u64;
fn sys_epoll_wait(u64 epfd, u8* events, u64 maxevents, u64 timeout) -> u64;
```

#### Constants

File open flags:

| Constant | Value | Meaning |
|----------|-------|---------|
| `O_RDONLY` | 0 | Open for reading |
| `O_WRONLY` | 1 | Open for writing |
| `O_RDWR` | 2 | Open for reading and writing |
| `O_CREAT` | 64 | Create file if it does not exist |
| `O_TRUNC` | 512 | Truncate file to zero length |
| `O_APPEND` | 1024 | Append on each write |

Signals:

| Constant | Value |
|----------|-------|
| `SIGHUP` | 1 |
| `SIGINT` | 2 |
| `SIGKILL` | 9 |
| `SIGTERM` | 15 |

### Chapter 23: args, path, fileio - Working with Files

#### args module

Import with `#import <args>`.

```
fn get_arg(u64 argv, u64 index) -> u8*;
fn arg_equals(u64 argv, u64 index, u8* expected) -> u64;
fn find_arg(u64 argc, u64 argv, u8* name) -> u64;
fn get_arg_value(u64 argc, u64 argv, u8* name) -> u8*;
```

Example:

```
fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    if (argc < 2) {
        print_string("Usage: program <name>\n");
        return(1);
    }
    u8* name = get_arg(argv, 1);
    printf("Hello, %s\n", name);
    return(0);
}
```

#### path module

Import with `#import <path>`.

```
fn path_exists(u8* path) -> u64;
fn path_is_dir(u8* path) -> u64;
fn path_join(u8* dest, u8* a, u8* b);
fn path_dirname(u8* path, u8* dest);
fn path_basename(u8* path) -> u8*;
```

Example:

```
u8 full[512];
path_join(full*adr, "/etc", "hostname");
if (path_exists(full*adr) == 1) {
    printf("found: %s\n", full*adr);
}
```

#### fileio module

Import with `#import <fileio>`.

Reads files line by line using a buffered reader.

```
struct FileReader {
    i64 fd;
    u8 buf[4096];
    u64 pos;
    u64 len;
}

fn file_reader_init(FileReader* r, i64 fd);
fn file_reader_next_line(FileReader* r, u8* out, u64 max_size) -> i64;
```

Example:

```
i64 fd = file_open("data.txt", O_RDONLY, 0);
if (syscall_error(fd) == 0) {
    FileReader reader;
    file_reader_init(reader*adr, fd);
    u8 line[4096];
    while (file_reader_next_line(reader*adr, line*adr, 4096) >= 0) {
        printf("line: %s\n", line*adr);
    }
    file_close(fd);
}
```

### Chapter 24: vector, unistd - Dynamic Arrays and Processes

#### vector module

Import with `#import <vector>`.

Provides a dynamic array of string pointers.

```
struct StrVec {
    u64 items;
    u64 count;
    u64 capacity;
}

fn strvec_init(StrVec* v);
fn strvec_add(StrVec* v, u64 str_ptr);
fn strvec_contains(StrVec* v, u64 str_ptr) -> i64;
fn strvec_free(StrVec* v);
fn strvec_clear(StrVec* v);
fn strvec_pop(StrVec* v);
```

Helper functions:

```
fn xmalloc(u64 size) -> u64;
fn xstrdup(u64 s) -> u64;
```

Example:

```
StrVec list;
strvec_init(list*adr);
u64 s = xstrdup("hello");
strvec_add(list*adr, s);
if (strvec_contains(list*adr, s) == 1) {
    print_string("found\n");
}
strvec_free(list*adr);
```

#### unistd module

Import with `#import <unistd>`.

```
fn get_cpu_count() -> i64;
fn popen(u8* command) -> i64;
fn pclose(i64 fd) -> i64;
fn opendir(u8* path) -> DIR*;
fn readdir(DIR* dir) -> u8*;
fn closedir(DIR* dir);
```

Example:

```
DIR* d = opendir("/tmp");
if (d != null) {
    u8* name = readdir(d);
    while (name != null) {
        printf("file: %s\n", name);
        name = readdir(d);
    }
    closedir(d);
}
```

### Chapter 25: math, fpmath - Floating Point and Fixed Point

#### math module

Import with `#import <math>`.

```
fn abs(f64 x) -> f64;
fn sqrt(f64 x) -> f64;
fn sin(f64 x) -> f64;
fn cos(f64 x) -> f64;
fn tan(f64 x) -> f64;
fn print_float(f64 x);
```

Example:

```
f64 result = sqrt(2.0);
print_float(result);
print_char(10);
```

#### fpmath module

Import with `#import <fpmath>`.

Uses fixed-point arithmetic with a scale factor of 1000000.
This avoids floating-point hardware requirements.

```
fn abs(i64 x) -> i64;
fn pow(i64 base, u64 exp) -> i64;
fn sqrt(i64 x) -> i64;
fn sin(i64 rad) -> i64;
fn cos(i64 rad) -> i64;
fn tan(i64 rad) -> i64;
fn print_fixed(i64 x);
```

Constants:

| Constant | Value | Meaning |
|----------|-------|---------|
| `PI` | 3141592 | 3.141592 in fixed-point |
| `TWO_PI` | 6283185 | 6.283185 |
| `HALF_PI` | 1570796 | 1.570796 |
| `FIXED_ONE` | 1000000 | 1.0 |

Example:

```
i64 angle = PI / 4;
i64 s = sin(angle);
print_fixed(s);
print_char(10);
```

### Chapter 26: keyboard, tui - Terminal Interface

#### keyboard module

Import with `#import <keyboard>`.

```
fn char_available() -> u64;
fn read_key() -> u64;
```

Printable characters return their ASCII value.
Special keys return codes:

| Key | Code |
|-----|------|
| Arrow up | 1000 |
| Arrow down | 1001 |
| Arrow right | 1002 |
| Arrow left | 1003 |
| Delete | 1004 |
| Page up | 1005 |
| Page down | 1006 |
| Home | 1010 |
| End | 1011 |
| F1 | 1101 |
| F2 | 1102 |
| F3 | 1103 |
| F4 | 1104 |
| Escape | 27 |
| Backspace | 127 |
| Enter | 10 |
| Tab | 9 |
| Space | 32 |

Example:

```
u64 key = read_key();
if (key == keys:arrow_up) {
    print_string("up\n");
}
```

#### tui module

Import with `#import <tui>`.

Provides a double-buffered terminal interface.

```
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

Workflow:

1. Call `tui_init()` once.
2. Draw to the buffer with `tui_draw_char` and `tui_draw_string`.
3. Call `tui_render()` to flush the buffer to the terminal.

Example:

```
tui_init();
tui_clear();
tui_draw_string(0, 0, "Hello TUI");
tui_draw_string(2, 4, "Row 2, Col 4");
tui_render();
```

---

## Part 5: Systems Programming

### Chapter 27: Inline Assembly

WandC allows raw x86_64 instructions inside `::nasm::{}` blocks.

```
fn halt() {
    ::nasm::{
        hlt
    }
}
```

The assembler supports a subset of NASM syntax.
Local variables can be accessed by name inside square brackets.

```
fn get_ticks() -> u64 {
    u64 low = 0;
    u64 high = 0;
    ::nasm::{
        rdtsc
        mov [low], eax
        mov [high], edx
    }
    return((high << 32) | low);
}
```

Supported instructions:

| Instruction | Encoding |
|-------------|----------|
| `hlt` | 0xF4 |
| `cli` | 0xFA |
| `sti` | 0xFB |
| `nop` | 0x90 |
| `ret` | 0xC3 |
| `syscall` | 0x0F 0x05 |
| `rdtsc` | 0x0F 0x31 |
| `push rbp` | 0x55 |
| `pop rbp` | 0x5D |
| `mov reg, reg` | Register to register move |
| `mov [var], reg` | Store register to local variable |
| `mov reg, [var]` | Load local variable to register |
| `mov reg, imm` | Immediate to register |

### Chapter 28: Critical Sections and IRQ Handlers

Critical sections and IRQ handlers are only available in `sc.false` (freestanding) code.

#### Critical sections

A `critical` block disables interrupts for the duration of the block.

```
sc.false

sect.kernel
    u64 tick_count = 0;
EOS

fn update_ticks() {
    critical {
        kernel:tick_count = kernel:tick_count + 1;
    }
}

fn kmain() {
    update_ticks();
}
```

The compiler generates:

1. Push flags (`pushfq`)
2. Pop flags to a register (`pop rax`)
3. Save the register (`push rax`)
4. Disable interrupts (`cli`)
5. Execute the block body
6. Restore flags (`pop rax` then `push rax` / `popfq`)

#### IRQ handlers

Use the `irq` keyword to define an interrupt handler.
The compiler generates a full register save/restore prologue and epilogue.

```
sc.false

irq fn timer_interrupt() {
    // Handle timer interrupt
}

fn kmain() {
    // Configure the interrupt vector to point to timer_interrupt
}
```

The `irq` prologue saves all general-purpose registers (rax, rcx, rdx, rbx, rsi, rdi, r8-r15).
The epilogue restores them and executes `iretq`.

### Chapter 29: Bare Metal - sc.false and kmain

Freestanding code runs without an operating system.
Use `sc.false` as the environment token.
The entry point must be named `kmain`.

```
sc.false

sect.bss
    u8 stack[8192];
EOS

fn kmain() {
    // Initialize hardware
    // Enter main loop
    ::nasm::{
        cli
        hlt
    }
}
```

Compile with:

```
wand2c kernel.w -o kernel.img -fk --entry=kmain
```

Differences from `sc.true`:

| Feature | sc.true | sc.false |
|---------|---------|----------|
| Entry point | `main` | `kmain` |
| System calls | Allowed | Forbidden |
| Critical sections | Forbidden | Allowed |
| IRQ handlers | Forbidden | Allowed |
| Port I/O | Allowed | Allowed |
| Inline assembly | Allowed | Allowed |

### Chapter 30: Hardware Port I/O

WandC provides built-in functions for x86 port I/O.

| Function | Description |
|----------|-------------|
| `inb(port)` | Read a byte from a port |
| `outb(port, value)` | Write a byte to a port |
| `inw(port)` | Read a word (16-bit) from a port |
| `outw(port, value)` | Write a word to a port |
| `inl(port)` | Read a long (32-bit) from a port |
| `outl(port, value)` | Write a long to a port |

Example: reading the CMOS clock

```
sc.false

fn read_cmos_register(u8 reg) -> u8 {
    outb(0x70, reg);
    return(inb(0x71));
}

fn kmain() {
    u8 seconds = read_cmos_register(0x00);
    u8 minutes = read_cmos_register(0x02);
    u8 hours = read_cmos_register(0x04);
}
```

Example: writing to a VGA port

```
sc.false

fn vga_write_index(u16 index) {
    outb(0x3D4, (index >> 8) & 0xFF);
    outb(0x3D5, index & 0xFF);
}
```

---

## Part 6: Modules and ABI

### Chapter 31: Export and Extern Functions

#### Export

Mark a function as visible to other modules:

```
export fn public_api(u64 value) -> u64 {
    return(value * 2);
}
```

If any function in a file has `export`, only exported functions appear in the export table.
Non-exported functions become local symbols.

#### Extern

Declare a function that is defined in another module:

```
extern fn external_func(u64 a, u64 b) -> u64;
```

The compiler records this as an import in the `.p46_imports` section.
The loader resolves the symbol at load time.

### Chapter 32: The .wexp Format

The `.wexp` format is a dynamic execution module.
Compile with `-fw`:

```
wand2c module.w -o module.wexp -fw
```

A `.wexp` file uses the ELF container with Standard 4/6 metadata sections:

| Section | Content |
|---------|---------|
| `.text` | Executable code |
| `.p46_header` | ABI magic and metadata |
| `.p46_types` | TLV type descriptors |
| `.p46_exports` | Exported symbols |
| `.p46_imports` | Imported symbols |
| `.p46_deps` | Module dependencies |
| `.p46_reflect` | Qualified-name lookup index |
| `.p46_strtab` | String table |

The `.wexp` format requires a `main` function.
The `--entry` flag is not allowed for this format.

### Chapter 33: jmpto - Dynamic Module Invocation

The `jmpto` statement invokes another module at the language level.

```
jmpto "worker.wexp" {
    u64 task_id = 42;
    return(task_id);
}
```

The compiler attempts to inline the target module at compile time.
If the source file is found, the compiler:

1. Parses the target module.
2. Locates the `main` function.
3. Compiles the argument statements in the caller context.
4. Inlines the body of the target `main`.
5. Converts `return(expr)` into a store to the caller variable.

If the source file is not found, the compiler generates a call to `__wand_jmpto_loader`.
The runtime provides this function.
It loads the `.wexp` file, validates metadata, resolves imports, and calls the entry point.

Example with compile-time inlining:

```
fn main() -> u64 {
    u64 result = 0;
    jmpto "compute.w" {
        u64 input = 100;
        return(input);
    }
    print_number(result);
    return(0);
}
```

### Chapter 34: Standard 4/6 Binary Interface

Standard 4/6 is the binary metadata format used by WandC.
It provides:

- Type descriptors with version tracking
- Function signatures with parameter types
- Module dependency resolution
- Qualified-name reflection

The header magic is `P46\0` (bytes 0x50, 0x34, 0x36, 0x00).
Version is 1.6.
Pointer size is 8 bytes.

The loader performs these steps when loading a `.wexp` module:

1. Validate the header magic and version.
2. Resolve dependencies from `.p46_deps`.
3. Resolve imports from `.p46_imports`.
4. Map the `.text` section into memory.
5. Transfer control to the entry point.

---

## Part 7: Optimization and Internals

### Chapter 35: How the Optimizer Works

The optimizer runs during Stage 2, after parsing and before type checking.
It operates on the AST directly.
It runs up to 10 iterations per function.

#### Constant folding

```
u64 x = 2 + 3;
```

Becomes:

```
u64 x = 5;
```

#### Dead variable elimination

```
u64 unused = 42;
```

If `unused` is never read, the statement is removed.

#### Constant propagation

```
u64 limit = 100;
while (count < limit) {
    count++;
}
```

The `limit` is replaced with `100` in the condition.

#### Algebraic simplifications

| Expression | Result |
|------------|--------|
| `x + 0` | `x` |
| `x - 0` | `x` |
| `x - x` | `0` |
| `x * 1` | `x` |
| `x * 0` | `0` |
| `x / 1` | `x` |
| `x / x` | `1` |
| `x & 0` | `0` |
| `x & x` | `x` |
| `x \| 0` | `x` |
| `x \| x` | `x` |
| `x ^ 0` | `x` |
| `x ^ x` | `0` |
| `x << 0` | `x` |
| `x >> 0` | `x` |
| `~~x` | `x` |
| `x == x` | `1` |
| `x != x` | `0` |
| `x < x` | `0` |
| `x <= x` | `1` |
| `x > x` | `0` |
| `x >= x` | `1` |
| `0 && x` | `0` |
| `1 && x` | `x` |
| `1 \|\| x` | `1` |
| `0 \|\| x` | `x` |

#### Strength reduction

Multiplication and division by powers of two become shifts:

```
x * 8
```

Becomes:

```
x << 3
```

```
x / 16
```

Becomes:

```
x >> 4
```

#### Branch elimination

```
if (1) {
    print_string("always");
} else {
    print_string("never");
}
```

Only the `then` branch is emitted.

```
while (0) {
    // removed entirely
}
```

#### Volatile and atomic exemption

Variables with `volatile` or `atomic` modifiers are never optimized.
All reads and writes are preserved.

### Chapter 36: Reading Generated Assembly

The compiler generates x86_64 machine code directly.
You can inspect the output with `objdump`:

```
objdump -d -M intel ./program
```

The entry point for `sc.true` programs is at virtual address `0x400078`.

The calling convention:

| Register | Purpose |
|----------|---------|
| `rdi` | First argument |
| `rsi` | Second argument |
| `rdx` | Third argument |
| `rcx` | Fourth argument |
| `r8` | Fifth argument |
| `r9` | Sixth argument |
| `rax` | Return value (first) |
| `rdx` | Return value (second) |
| `rcx` | Return value (third) |
| `r8` | Return value (fourth) |

### Chapter 37: Performance Tuning for Low-Level Code

#### Use compound assignment

```
x += 1;
```

The optimizer may reduce this to `inc rax` instead of `add rax, 1`.

#### Use shifts for powers of two

```
x = x * 8;
```

The optimizer converts this to `x << 3` automatically.
But writing the shift explicitly makes intent clear.

#### Avoid volatile when not needed

Every write to a `volatile` variable emits a memory fence (`mfence`).
Only use `volatile` for hardware registers and memory-mapped I/O.

#### Prefer stack allocation

Stack variables are free.
Heap allocation has overhead from the allocator.
Use the stack for small, short-lived buffers.

#### Use packed structures for wire formats

```
packed align(1) struct Packet {
    u8 type version 1;
    u16 length version 1;
    u32 data version 1;
}
```

This eliminates padding and produces the exact wire layout.

#### Minimize function call depth

Each function call pushes a return address and sets up a stack frame.
For tight loops, consider inlining manually or restructuring to reduce call depth.

---

## Part 8: Projects

### Chapter 38: Building a Command-Line Tool

Build a tool that counts lines in a file.

```
sc.true

#import <io>
#import <syscall>
#import <args>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    if (argc < 2) {
        print_string("Usage: wc <file>\n");
        return(1);
    }

    u8* filename = get_arg(argv, 1);
    i64 fd = file_open(filename, O_RDONLY, 0);
    if (syscall_error(fd) == 1) {
        printf("error: cannot open %s\n", filename);
        return(1);
    }

    u8 buffer[4096];
    u64 line_count = 0;
    u64 byte_count = 0;

    while (1) {
        i64 bytes = file_read(fd, buffer*adr, 4096);
        if (bytes <= 0) {
            break;
        }
        byte_count += bytes;
        for (u64 i = 0; i < bytes; i++) {
            if (buffer[i] == 10) {
                line_count++;
            }
        }
    }

    file_close(fd);

    printf("%v lines, %v bytes\n", line_count, byte_count);
    return(0);
}
```

Compile and run:

```
wand2c wc.w -o wc -fp
./wc /etc/hostname
```

### Chapter 39: Writing a Simple Shell

```
sc.true

#import <io>
#import <mem>
#import <string>
#import <syscall>
#import <unistd>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    mem_init(1048576);
    u8 line[4096];

    while (1) {
        print_string("wand$ ");
        read_string(line*adr, 4096);

        u64 len = strlen(line*adr);
        if (len == 0) {
            continue;
        }

        if (strcmp(line*adr, "exit") == 0) {
            break;
        }

        if (strcmp(line*adr, "pwd") == 0) {
            i64 fd = popen("pwd");
            if (fd >= 0) {
                u8 buf[256];
                i64 n = sys_read(fd, buf*adr, 255);
                if (n > 0) {
                    buf[n] = 0;
                    print_string(buf*adr);
                }
                pclose(fd);
            }
            continue;
        }

        printf("unknown command: %s\n", line*adr);
    }

    print_string("goodbye\n");
    return(0);
}
```

### Chapter 40: Writing a Kernel Module

A minimal freestanding kernel that writes to the VGA text buffer.

```
sc.false

sect.vga
    volatile u16* vram = 0xB8000;
EOS

fn vga_put(u64 index, u8 ch, u8 attr) {
    vga:vram[index] = (attr << 8) | ch;
}

fn vga_clear() {
    for (u64 i = 0; i < 2000; i++) {
        vga_put(i, 0, 0x07);
    }
}

fn vga_write(u8* s) {
    u64 i = 0;
    while (s[i] != 0) {
        vga_put(i, s[i], 0x0F);
        i++;
    }
}

fn kmain() {
    vga_clear();
    vga_write("Hello from WandC kernel!");

    ::nasm::{
        cli
        hlt
    }
}
```

Compile:

```
wand2c kernel.w -o kernel.img -fk --entry=kmain
```

## Appendix A: Complete Operator Reference

| Category | Operators |
|----------|-----------|
| Arithmetic | `+` `-` `*` `/` `%` |
| Bitwise | `&` `\|` `^` `~` `<<` `>>` |
| Comparison | `==` `!=` `<` `<=` `>` `>=` |
| Logical | `&&` `\|\|` |
| Assignment | `=` `+=` `-=` `*=` `/=` `%=` `&=` `\|=` `^=` `<<=` `>>=` |
| Increment | `++` `--` |
| Pointer | `*adr` |
| Access | `.` `->` `:` |

## Appendix B: Complete Keyword Reference

| Keyword | Purpose |
|---------|---------|
| `sc.true` | Hosted environment |
| `sc.false` | Freestanding environment |
| `fn` | Function declaration |
| `struct` | Structure definition |
| `union` | Union definition |
| `enum` | Enumeration definition |
| `version` | ABI version annotation |
| `sect` | Global section |
| `EOS` | End of section |
| `if` / `else` | Conditional |
| `while` | Loop |
| `for` | Loop with counter |
| `match` / `case` / `default` | Multi-way branch |
| `continue` | Skip to next iteration |
| `break` | Exit loop |
| `return` | Return from function |
| `null` | Null pointer literal |
| `typedef` | Type alias |
| `const` | Compile-time constant |
| `packed` | Remove struct padding |
| `align(N)` | Set alignment |
| `ro` | Read-only section |
| `noinit` | No initialization data |
| `volatile` | Prevent optimization |
| `atomic` | Atomic variable |
| `critical` | Interrupt-safe block |
| `irq` | Interrupt handler |
| `export` | Make function visible |
| `extern` | Declare external function |
| `jmpto` | Dynamic module invocation |
| `#import` | Module import |

## Appendix C: Compile-Time Reflection Functions

| Function | Returns |
|----------|---------|
| `sizeof(Type)` | Size in bytes |
| `alignof(Type)` | Alignment requirement |
| `offsetof(Struct:field)` | Field byte offset |
| `fieldsof(Type)` | Number of fields |
| `versionof(Type)` | Type version number |
| `nameof(Type)` | Type name as string |

## Appendix D: Atomic Built-in Functions

| Function | Operation |
|----------|-----------|
| `atomic_load(ptr)` | Read atomically |
| `atomic_store(ptr, val)` | Write atomically |
| `atomic_add(ptr, val)` | Add atomically |
| `atomic_sub(ptr, val)` | Subtract atomically |
| `atomic_inc(ptr)` | Increment atomically |
| `atomic_dec(ptr)` | Decrement atomically |
| `atomic_swap(ptr, val)` | Swap atomically |
| `atomic_cas(ptr, exp, des)` | Compare-and-swap |
| `memory_barrier()` | Full memory fence |
| `compiler_barrier()` | Prevent compiler reordering |

### Chapter 41: Writing a TUI Text Editor

This project builds a minimal text editor using the `tui` and `keyboard` modules.

```
sc.true

#import <io>
#import <mem>
#import <string>
#import <keyboard>
#import <tui>

const MAX_LINES = 256;
const MAX_LINE_LEN = 512;

struct Editor {
    u8 lines[MAX_LINES][MAX_LINE_LEN];
    u64 line_count;
    u64 cursor_row;
    u64 cursor_col;
    u64 scroll_row;
    u64 term_rows;
    u64 term_cols;
}

fn editor_init(Editor* ed) {
    ed->line_count = 1;
    ed->cursor_row = 0;
    ed->cursor_col = 0;
    ed->scroll_row = 0;
    for (u64 i = 0; i < MAX_LINES; i++) {
        ed->lines[i][0] = 0;
    }
}

fn editor_line_len(Editor* ed, u64 row) -> u64 {
    return(strlen(ed->lines[row]*adr));
}

fn editor_insert_char(Editor* ed, u8 ch) {
    u64 len = editor_line_len(ed, ed->cursor_row);
    if (len >= MAX_LINE_LEN - 1) {
        return;
    }
    u64 i = len;
    while (i > ed->cursor_col) {
        ed->lines[ed->cursor_row][i] = ed->lines[ed->cursor_row][i - 1];
        i--;
    }
    ed->lines[ed->cursor_row][ed->cursor_col] = ch;
    ed->lines[ed->cursor_row][len + 1] = 0;
    ed->cursor_col++;
}

fn editor_delete_char(Editor* ed) {
    if (ed->cursor_col == 0) {
        return;
    }
    u64 len = editor_line_len(ed, ed->cursor_row);
    u64 i = ed->cursor_col - 1;
    while (i < len) {
        ed->lines[ed->cursor_row][i] = ed->lines[ed->cursor_row][i + 1];
        i++;
    }
    ed->cursor_col--;
}

fn editor_newline(Editor* ed) {
    if (ed->line_count >= MAX_LINES) {
        return;
    }
    u64 row = ed->cursor_row;
    u64 col = ed->cursor_col;
    u64 len = editor_line_len(ed, row);

    u64 i = ed->line_count;
    while (i > row + 1) {
        strcpy(ed->lines[i]*adr, ed->lines[i - 1]*adr);
        i--;
    }

    u64 j = 0;
    while (col + j < len) {
        ed->lines[row + 1][j] = ed->lines[row][col + j];
        j++;
    }
    ed->lines[row + 1][j] = 0;
    ed->lines[row][col] = 0;

    ed->line_count++;
    ed->cursor_row++;
    ed->cursor_col = 0;
}

fn editor_render(Editor* ed) {
    tui_clear();

    u64 visible_rows = ed->term_rows - 1;

    if (ed->cursor_row < ed->scroll_row) {
        ed->scroll_row = ed->cursor_row;
    }
    if (ed->cursor_row >= ed->scroll_row + visible_rows) {
        ed->scroll_row = ed->cursor_row - visible_rows + 1;
    }

    u64 screen_row = 0;
    u64 line_idx = ed->scroll_row;
    while (screen_row < visible_rows && line_idx < ed->line_count) {
        tui_draw_string(screen_row, 0, ed->lines[line_idx]*adr);
        screen_row++;
        line_idx++;
    }

    u8 status[128];
    u64 sl = 0;
    status[sl] = 'L'; sl++;
    status[sl] = 'n'; sl++;
    status[sl] = ':'; sl++;
    u8 num_buf[32];
    itoa(ed->cursor_row + 1, num_buf*adr);
    u64 nl = strlen(num_buf*adr);
    for (u64 k = 0; k < nl; k++) {
        status[sl] = num_buf[k];
        sl++;
    }
    status[sl] = ' '; sl++;
    status[sl] = 'C'; sl++;
    status[sl] = 'o'; sl++;
    status[sl] = 'l'; sl++;
    status[sl] = ':'; sl++;
    itoa(ed->cursor_col + 1, num_buf*adr);
    nl = strlen(num_buf*adr);
    for (u64 k = 0; k < nl; k++) {
        status[sl] = num_buf[k];
        sl++;
    }
    status[sl] = 0;
    tui_draw_string(ed->term_rows - 1, 0, status*adr);

    u64 draw_row = ed->cursor_row - ed->scroll_row;
    tui_set_cursor(draw_row, ed->cursor_col);
    tui_render();
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    mem_init(4194304);

    Editor* ed = malloc(sizeof(Editor));
    if (ed == null) {
        print_string("error: cannot allocate editor\n");
        return(1);
    }

    get_terminal_size(ed->term_rows*adr, ed->term_cols*adr);
    tui_init();
    editor_init(ed);

    u64 running = 1;
    while (running == 1) {
        editor_render(ed);
        u64 key = read_key();

        if (key == keys:key_esc) {
            running = 0;
        } else if (key == keys:arrow_up) {
            if (ed->cursor_row > 0) {
                ed->cursor_row--;
                u64 len = editor_line_len(ed, ed->cursor_row);
                if (ed->cursor_col > len) {
                    ed->cursor_col = len;
                }
            }
        } else if (key == keys:arrow_down) {
            if (ed->cursor_row < ed->line_count - 1) {
                ed->cursor_row++;
                u64 len = editor_line_len(ed, ed->cursor_row);
                if (ed->cursor_col > len) {
                    ed->cursor_col = len;
                }
            }
        } else if (key == keys:arrow_left) {
            if (ed->cursor_col > 0) {
                ed->cursor_col--;
            }
        } else if (key == keys:arrow_right) {
            u64 len = editor_line_len(ed, ed->cursor_row);
            if (ed->cursor_col < len) {
                ed->cursor_col++;
            }
        } else if (key == keys:key_enter) {
            editor_newline(ed);
        } else if (key == keys:key_backspace) {
            editor_delete_char(ed);
        } else if (key >= 32 && key < 127) {
            editor_insert_char(ed, key);
        }
    }

    tui_clear_physical();
    print_string("editor exited\n");
    mfree(ed);
    return(0);
}
```

Compile and run:

```
wand2c editor.w -o editor -fp
./editor
```

Controls:

| Key | Action |
|-----|--------|
| Arrow keys | Move cursor |
| Printable characters | Insert text |
| Backspace | Delete character before cursor |
| Enter | Insert newline, split current line |
| Escape | Quit editor |

---

### Chapter 42: Writing a Network Echo Server

This project builds a TCP echo server using the `syscall` module directly.

```
sc.true

#import <io>
#import <syscall>
#import <string>

const AF_INET = 2;
const SOCK_STREAM = 1;
const INADDR_ANY = 0;
const PORT = 8080;

struct sockaddr_in version 1 {
    u16 sin_family version 1;
    u16 sin_port version 1;
    u32 sin_addr version 1;
    u8 sin_zero[8] version 1;
}

fn htons(u16 val) -> u16 {
    return((val >> 8) | (val << 8));
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u64 server_fd = sys_socket(AF_INET, SOCK_STREAM, 0);
    if (syscall_error(server_fd) == 1) {
        print_string("error: cannot create socket\n");
        return(1);
    }

    sockaddr_in addr;
    memset(addr*adr, 0, sizeof(sockaddr_in));
    addr.sin_family = AF_INET;
    addr.sin_port = htons(PORT);
    addr.sin_addr = INADDR_ANY;

    u64 bind_ret = sys_bind(server_fd, addr*adr, sizeof(sockaddr_in));
    if (syscall_error(bind_ret) == 1) {
        print_string("error: cannot bind\n");
        sys_close(server_fd);
        return(1);
    }

    u64 listen_ret = sys_listen(server_fd, 16);
    if (syscall_error(listen_ret) == 1) {
        print_string("error: cannot listen\n");
        sys_close(server_fd);
        return(1);
    }

    printf("echo server listening on port %v\n", PORT);

    while (1) {
        sockaddr_in client_addr;
        u64 client_len = sizeof(sockaddr_in);
        u64 client_fd = sys_accept(server_fd, client_addr*adr, client_len*adr);
        if (syscall_error(client_fd) == 1) {
            continue;
        }

        print_string("client connected\n");

        u8 buf[1024];
        while (1) {
            u64 bytes = sys_read(client_fd, buf*adr, 1024);
            if (bytes == 0 || syscall_error(bytes) == 1) {
                break;
            }
            sys_write(client_fd, buf*adr, bytes);
        }

        print_string("client disconnected\n");
        sys_close(client_fd);
    }

    sys_close(server_fd);
    return(0);
}
```

Compile:

```
wand2c echo_server.w -o echo_server -fp
./echo_server
```

Test from another terminal:

```
echo "hello" | nc 127.0.0.1 8080
```

The server echoes back every byte it receives.
The `htons` function converts a 16-bit value from host byte order to network byte order (big-endian).
This is required because network protocols use big-endian format.

---

### Chapter 43: Writing a Custom Memory Allocator

This project implements a linked-list heap allocator from scratch.
It demonstrates how memory management works under the hood.

```
sc.true

#import <io>
#import <syscall>
#import <string>

const HEAP_SIZE = 1048576;
const BLOCK_MAGIC = 0xB10CB10C;
const MIN_SPLIT_SIZE = 32;

struct BlockHeader version 1 {
    u32 magic version 1;
    u32 size version 1;
    u64 is_free version 1;
    u64 next version 1;
}

sect.heap
    u8 heap_arena[HEAP_SIZE];
    u64 heap_end = 0;
EOS

fn heap_init() {
    heap:heap_end = 0;
}

fn heap_alloc(u64 size) -> u8* {
    u64 total_size = size + sizeof(BlockHeader);
    total_size = (total_size + 15) & ~(15);

    u64 current = 0;
    while (current < heap:heap_end) {
        u8* block_ptr = heap:heap_arena*adr + current;
        BlockHeader* hdr = block_ptr;
        if (hdr->magic != BLOCK_MAGIC) {
            break;
        }
        if (hdr->is_free == 1 && hdr->size >= size) {
            hdr->is_free = 0;
            return(block_ptr + sizeof(BlockHeader));
        }
        current += sizeof(BlockHeader) + hdr->size;
    }

    if (heap:heap_end + total_size > HEAP_SIZE) {
        return(null);
    }

    u8* block_ptr = heap:heap_arena*adr + heap:heap_end;
    BlockHeader* hdr = block_ptr;
    hdr->magic = BLOCK_MAGIC;
    hdr->size = total_size - sizeof(BlockHeader);
    hdr->is_free = 0;
    hdr->next = 0;

    heap:heap_end += total_size;

    return(block_ptr + sizeof(BlockHeader));
}

fn heap_free(u8* ptr) {
    if (ptr == null) {
        return;
    }
    u8* block_ptr = ptr - sizeof(BlockHeader);
    BlockHeader* hdr = block_ptr;
    if (hdr->magic != BLOCK_MAGIC) {
        return;
    }
    hdr->is_free = 1;
}

fn heap_stats() {
    u64 total_blocks = 0;
    u64 free_blocks = 0;
    u64 used_bytes = 0;
    u64 free_bytes = 0;

    u64 current = 0;
    while (current < heap:heap_end) {
        u8* block_ptr = heap:heap_arena*adr + current;
        BlockHeader* hdr = block_ptr;
        if (hdr->magic != BLOCK_MAGIC) {
            break;
        }
        total_blocks++;
        if (hdr->is_free == 1) {
            free_blocks++;
            free_bytes += hdr->size;
        } else {
            used_bytes += hdr->size;
        }
        current += sizeof(BlockHeader) + hdr->size;
    }

    printf("blocks: %v total, %v free\n", total_blocks, free_blocks);
    printf("memory: %v used, %v free\n", used_bytes, free_bytes);
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    heap_init();

    u8* a = heap_alloc(128);
    u8* b = heap_alloc(256);
    u8* c = heap_alloc(64);

    if (a == null || b == null || c == null) {
        print_string("error: allocation failed\n");
        return(1);
    }

    memset(a, 0x41, 128);
    memset(b, 0x42, 256);
    memset(c, 0x43, 64);

    print_string("after 3 allocations:\n");
    heap_stats();

    heap_free(b);
    print_string("after freeing b:\n");
    heap_stats();

    u8* d = heap_alloc(200);
    if (d == null) {
        print_string("error: reuse failed\n");
        return(1);
    }
    print_string("after allocating d (reuses freed block):\n");
    heap_stats();

    heap_free(a);
    heap_free(c);
    heap_free(d);
    print_string("after freeing all:\n");
    heap_stats();

    return(0);
}
```

Compile and run:

```
wand2c allocator.w -o allocator -fp
./allocator
```

Key concepts in this allocator:

- Each block has a `BlockHeader` that stores the block size, a magic number for validation, and a free flag.
- Allocation scans the heap for the first free block that is large enough (first-fit strategy).
- If no free block is found, a new block is carved from the end of the arena.
- Deallocation sets the free flag. It does not return memory to the OS.
- The magic number detects corruption and double-free attempts.

---

### Chapter 44: Writing a Dynamic Module Loader

This project demonstrates the `.wexp` format and the `jmpto` statement.

First, create a module that will be loaded dynamically.

File `worker.w`:

```
sc.true

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u64 result = input_value * 2;
    return(result);
}
```

Compile it as a dynamic module:

```
wand2c worker.w -o worker.wexp -fw
```

Now create a host program that invokes the module.

File `host.w`:

```
sc.true

#import <io>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u64 result = 0;

    jmpto "worker.wexp" {
        u64 input_value = 21;
        return(input_value);
    }

    printf("result from module: %v\n", result);
    return(0);
}
```

Compile the host:

```
wand2c host.w -o host -fp
./host
```

Output:

```
result from module: 42
```

If the compiler finds `worker.w` or `worker.wexp` source at compile time, it inlines the module body directly into the host.
If the source is not found, the compiler emits a call to `__wand_jmpto_loader`.
The runtime loads the `.wexp` file, validates the Standard 4/6 metadata, resolves imports, and calls the module entry point.

To force dynamic loading at runtime, compile the host without the worker source present, then place `worker.wexp` next to the host binary.

---

## Appendix A: Complete Operator Reference

| Category | Operators |
|----------|-----------|
| Arithmetic | `+` `-` `*` `/` `%` |
| Bitwise | `&` `\|` `^` `~` `<<` `>>` |
| Comparison | `==` `!=` `<` `<=` `>` `>=` |
| Logical | `&&` `\|\|` |
| Assignment | `=` `+=` `-=` `*=` `/=` `%=` `&=` `\|=` `^=` `<<=` `>>=` |
| Increment | `++` `--` |
| Pointer | `*adr` |
| Access | `.` `->` `:` |

## Appendix B: Complete Keyword Reference

| Keyword | Purpose |
|---------|---------|
| `sc.true` | Hosted environment |
| `sc.false` | Freestanding environment |
| `fn` | Function declaration |
| `struct` | Structure definition |
| `union` | Union definition |
| `enum` | Enumeration definition |
| `version` | ABI version annotation |
| `sect` | Global section |
| `EOS` | End of section |
| `if` / `else` | Conditional |
| `while` | Loop |
| `for` | Loop with counter |
| `match` / `case` / `default` | Multi-way branch |
| `continue` | Skip to next iteration |
| `break` | Exit loop |
| `return` | Return from function |
| `null` | Null pointer literal |
| `typedef` | Type alias |
| `const` | Compile-time constant |
| `packed` | Remove struct padding |
| `align(N)` | Set alignment |
| `ro` | Read-only section |
| `noinit` | No initialization data |
| `volatile` | Prevent optimization |
| `atomic` | Atomic variable |
| `critical` | Interrupt-safe block |
| `irq` | Interrupt handler |
| `export` | Make function visible |
| `extern` | Declare external function |
| `jmpto` | Dynamic module invocation |
| `#import` | Module import |

## Appendix C: Compile-Time Reflection Functions

| Function | Returns |
|----------|---------|
| `sizeof(Type)` | Size in bytes |
| `alignof(Type)` | Alignment requirement |
| `offsetof(Struct:field)` | Field byte offset |
| `fieldsof(Type)` | Number of fields |
| `versionof(Type)` | Type version number |
| `nameof(Type)` | Type name as string |

## Appendix D: Atomic Built-in Functions

| Function | Operation |
|----------|-----------|
| `atomic_load(ptr)` | Read atomically |
| `atomic_store(ptr, val)` | Write atomically |
| `atomic_add(ptr, val)` | Add atomically |
| `atomic_sub(ptr, val)` | Subtract atomically |
| `atomic_inc(ptr)` | Increment atomically |
| `atomic_dec(ptr)` | Decrement atomically |
| `atomic_swap(ptr, val)` | Swap atomically |
| `atomic_cas(ptr, exp, des)` | Compare-and-swap |
| `memory_barrier()` | Full memory fence |
| `compiler_barrier()` | Prevent compiler reordering |

## Appendix E: Standard Library Module Index

| Module | Import | Purpose |
|--------|--------|---------|
| io | `#import <io>` | Console and file I/O |
| mem | `#import <mem>` | Arena heap allocation |
| string | `#import <string>` | C-style string operations |
| syscall | `#import <syscall>` | Linux system calls |
| args | `#import <args>` | Command-line argument parsing |
| path | `#import <path>` | File path utilities |
| fileio | `#import <fileio>` | Line-by-line file reader |
| vector | `#import <vector>` | Dynamic string arrays |
| unistd | `#import <unistd>` | Process and directory helpers |
| math | `#import <math>` | Floating-point math |
| fpmath | `#import <fpmath>` | Fixed-point math |
| keyboard | `#import <keyboard>` | Terminal key input |
| tui | `#import <tui>` | Terminal user interface |
| std | `#import <std>` | Basic utilities |

## Appendix F: Compiler Flag Reference

| Flag | Description |
|------|-------------|
| `-o <file>` | Set output file path |
| `-v, --verbose` | Show detailed compilation output |
| `-fp` | Hosted ELF64 executable |
| `-fo` | Relocatable ELF object |
| `-fr` | Flat binary image |
| `-fk` | Freestanding kernel image |
| `-fw` | Dynamic execution module |
| `--entry <name>` | Set entry function (raw and kernel only) |
| `-il <path>` | Install a library |

## Appendix G: Error Message Reference

| Error | Cause | Fix |
|-------|-------|-----|
| `use of potentially uninitialized variable` | Reading a variable before assignment | Initialize the variable |
| `use of uninitialized field` | Reading a struct field before assignment | Initialize the field |
| `use-after-free violation` | Accessing a pointer after `mfree` | Remove the access or re-allocate |
| `potential null pointer dereference` | Accessing through a pointer without null check | Add `if (ptr != null)` guard |
| `potential memory leak` | Allocated pointer never freed | Call `mfree` before function exit |
| `freeing potentially null pointer` | Calling `mfree` without null check | Add null check before `mfree` |
| `multi-level pointers are not allowed` | Declaring `u8**` or deeper | Use single pointer with `*adr` |
| `critical requires sc.false` | Using `critical` in hosted code | Use `sc.false` or remove `critical` |
| `syscallN requires sc.true` | Using system calls in freestanding code | Use `sc.true` or remove the call |
| `array size must be a compile-time constant` | Array size is not a literal or `const` | Use a numeric literal or `const` value |
| `align value must be a power of two` | `align(N)` where N is not power of two | Use 1, 2, 4, 8, 16, 4096, etc. |
| `'_' cannot be used as a variable name` | Using `_` as identifier | Use `_` only in destructuring |

# Chapter 39: Writing a vim-like text editor

This chapter walks through building a modal text editor using the TUI library.

## Architecture

The editor uses three modes:
- Normal mode: navigation and commands (default)
- Insert mode: text input
- Command mode: colon commands (:w, :q, :wq)

## Buffer model

Text is stored as a dynamic array of lines. Each line has:
- `data`: heap-allocated byte buffer
- `len`: current text length
- `cap`: allocated capacity

Lines are inserted, deleted, and modified in-place.

## Widget layout

The editor creates widgets at init time:
- Panel (root container, borderless)
- Label array for line numbers (cyan text)
- Label array for line text (white text)
- 1x1 Label for cursor (reverse video)
- Status bar Label (black on white)
- Command/message Label (bottom row)
- Mode indicator Label (bold, yellow on black)

## Key bindings

Normal mode:
- h/j/k/l: cursor movement
- w/b: word forward/backward
- 0/$: line start/end
- gg/G: file start/end
- i/a/I/A: enter insert mode
- o/O: open line below/above
- x: delete character
- X: delete line
- dd: delete line
- D: delete to end of line
- :: enter command mode

Insert mode:
- Printable characters: insert at cursor
- Backspace: delete before cursor
- Enter: split line
- Escape: return to normal mode
- Arrow keys: move cursor

Command mode:
- :w — save file
- :q — quit (fails if modified)
- :q! — quit without saving
- :wq — save and quit
- :w filename — save as

## Building and running
