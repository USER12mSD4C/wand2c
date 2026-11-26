# WandC Language Syntax

This document defines the syntax of WandC.

---

## 1. Source Model

A WandC program is a set of text files.

There are two file types:

| Extension | Purpose |
|---|---|
| `.w` | Implementation |
| `.wh` | Declarations |

Use `.wh` files for interfaces.
Use `.w` files for function bodies.

The compiler reads one entry file and resolves imported modules.

---

## 2. Environment Token

Every WandC file must start with one environment token.

| Token | Environment | Required entry function |
|---|---|---|
| `sc.true` | Hosted | `main` |
| `sc.false` | Freestanding | `main` by default; `--entry kmain` selects `kmain` |

Hosted entry function:

```wandc
fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    return(0);
}
```

Freestanding entry function:

```wandc
fn kmain() {
}
```

Rules:

1. `sc.true` allows system call functions.
2. `sc.true` forbids `critical`.
3. `sc.false` allows `critical` and `irq`.
4. `sc.false` forbids system call functions.

---

## 3. Module Imports

Use `#import` to import a module.

System module:

```wandc
#import <io>
```

Local module:

```wandc
#import "utils"
```

Rules:

1. System modules use angle brackets.
2. Local modules use double quotes.
3. Import paths must not contain file extensions.
4. Import paths must not contain path separators.

Correct:

```wandc
#import <io>
#import "parser"
```

Incorrect:

```wandc
#import <io.w>
#import "parser.wh"
```

---

## 4. Comments

Use two slash characters to start a line comment.

The compiler ignores comments.

---

## 5. Identifiers

An identifier starts with a letter or underscore.
It can contain letters, digits, and underscores.

Keywords are reserved.
Do not use keywords as identifiers.

---

## 6. Literals

### Integer Literals

Decimal:

```wandc
u64 a = 4096;
```

Hexadecimal:

```wandc
u64 b = 0x1000;
```

### Floating-Point Literals

```wandc
f64 x = 3.14159;
```

### String Literals

```wandc
u8* msg = "hello";
```

A string literal is a read-only byte sequence.
Do not write through a string literal pointer.

### Null Pointer

```wandc
u8* p = null;
```

`null` is the null pointer constant.

---

## 7. Primitive Types

| Type | Size |
|---|---|
| `u8` | 1 byte |
| `u16` | 2 bytes |
| `u32` | 4 bytes |
| `u64` | 8 bytes |
| `i8` | 1 byte |
| `i16` | 2 bytes |
| `i32` | 4 bytes |
| `i64` | 8 bytes |
| `f64` | 8 bytes |
| `void` | no value |

Signed types use two's complement representation. When a narrow signed value
(`i8`, `i16`, `i32`) is promoted into a 64-bit expression, it is sign-extended;
the unsigned types zero-extend.

---

## 8. Pointers

Use `*` after a type to declare a pointer.

```wandc
u8* p;
u64* counter;
void* generic;
```

Rules:

1. Multi-level pointers are forbidden.
2. Pointer arithmetic is allowed between a pointer and an integer.
3. Pointer bounds are not checked.
4. Pointer alignment is the responsibility of the programmer.

Forbidden:

```wandc
u8** p;
```

Allowed:

```wandc
u8* p;
```

---

## 9. Arrays

Use brackets after a type or variable name.

```wandc
u8 buffer[256];
```

Array size must be a compile-time constant.

Allowed:

```wandc
u8 buffer[4096];
```

```wandc
const MAX_NAME = 256;
u8 name[MAX_NAME];
```

Forbidden:

```wandc
u64 n = read_size();
u8 buffer[n];
```

Array indexing starts at zero.

```wandc
buffer[0] = 1;
```

An array name can be passed to a pointer parameter. `*adr` is the explicit address-taking form and is also valid.

```wandc
fn process(u8* data, u64 size) {
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u8 buffer[256];
    process(buffer*adr, 256);
    return(0);
}
```

Array initializer:

```wandc
u8 data[4] = { 1, 2, 3, 4 };
```

The array size must be present.

---

## 10. Constants

Use `const` to declare a compile-time constant.

```wandc
const MAX_TASKS = 256;
const MAGIC = 0xDEADBEEF;
```

Rules:

1. A constant expression must be evaluable at compile time.
2. Constants must appear before use.
3. Constants must not call functions.
4. Constants must not use run-time variables.

Allowed constant operators:

```text
+ - * / % & | ^ << >> == != < <= > >= && || ~
```

---

## 11. Enumerations

Use `enum` to define named integer values.

```wandc
enum State version 1 {
    Idle = 0 version 1;
    Running = 1 version 1;
    Stopped = 2 version 1;
}
```

Access an enum value with `EnumName:Value`.

```wandc
State s = State:Running;

if (s == State:Running) {
    print_string("running");
}
```

Rules:

1. Enum values are integer literals.
2. Enum value names are scoped by the enum name.
3. Enum version is optional. Default is 1.
4. Enum field version is optional. Default is 1.

---

## 12. Structures

Use `struct` to group fields.

```wandc
struct Point version 1 {
    i64 x version 1;
    i64 y version 1;
}
```

Create and use a structure:

```wandc
Point pt;
pt.x = 10;
pt.y = 20;
```

Use `->` through a pointer:

```wandc
fn move_point(Point* p, i64 dx, i64 dy) {
    p->x = p->x + dx;
    p->y = p->y + dy;
}
```

### Structure Versioning

A structure has a version.
Each field has a version.

```wandc
struct Config version 2 {
    u32 revision version 1;
    u64 flags version 1;
    u8* name version 2;
}
```

Default version is 1.

### Packed Structures

Use `packed` to remove padding.

```wandc
packed struct Header version 1 {
    u32 magic version 1;
    u16 size version 1;
}
```

Use `align(N)` before `packed` to set alignment.

```wandc
align(1) packed struct Packet version 1 {
    u8 type version 1;
    u16 length version 1;
    u32 data version 1;
}
```

Rules:

1. `align(N)` requires a power of two.
2. `align(N)` appears before `packed`.
3. `packed` appears before `struct`.

### Structure Field Restrictions

1. Field names must be identifiers.
2. Field names must not be keywords.
3. Structure fields can be volatile.
4. Structure fields can be atomic.

---

## 13. Unions

Use `union` to store different fields in the same memory.

```wandc
union Data version 1 {
    u64 as_u64 version 1;
    f64 as_f64 version 1;
}
```

All fields in a union start at offset zero.

---

## 14. Typedef

Use `typedef` to create a type alias.

```wandc
typedef u8[256] Buffer;
typedef i64 Result;
```

Use the alias as a type name.

```wandc
Buffer b;
Result r = 0;
```

---

## 15. Variables

Declare a variable with type, name, and optional initializer.

```wandc
u64 x = 10;
i64 y = -5;
u8* name = "wand";
```

A variable without an initializer has no defined value until assignment.

```wandc
u64 x;
x = 0;
```

Use of an uninitialized variable is an error.

---

## 16. Pointer Modifiers

Pointer modifiers declare data flow through pointers.

| Modifier | Meaning |
|---|---|
| `*i` | Read |
| `*o` | Write |
| `*io` | Read and write |

Example:

```wandc
fn write_one(u64* out*o) {
    out = 1;
}

fn add_ten(u64* value*io) {
    value = value + 10;
}
```

Call with `*adr`:

```wandc
u64 num = 5;
write_one(num*adr);
add_ten(num*adr);
```

Rules:

1. Pointer modifiers apply to pointer parameters and pointer variables.
2. A pointer parameter with `*o` or `*io` names the pointed object.
3. Do not combine `volatile` or `atomic` with `*i`, `*o`, or `*io`.

Forbidden:

```wandc
volatile u64* p*i;
```

Allowed:

```wandc
volatile u64* p;
```

---

## 17. The *adr Operator

The `*adr` operator takes an address.

Scalar:

```wandc
u64 x = 0;
u64* p = x*adr;
```

Array:

```wandc
u8 buffer[256];
u8* p = buffer*adr;
```

Structure:

```wandc
Point pt;
Point* p = pt*adr;
```

Section variable:

```wandc
sect.state
u64 ticks = 0;
EOS

u64* p = state:ticks*adr;
```

Rules:

1. `expr*adr` produces a pointer.
2. For an array, `array*adr` produces a pointer to the first element.
3. Use `*adr` when a function expects a pointer.
4. Do not use `*adr` when passing a value.

---

## 18. Operators

### Arithmetic Operators

| Operator | Meaning | Example |
|---|---|---|
| `+` | Addition | `a + b` |
| `-` | Subtraction | `a - b` |
| `*` | Multiplication | `a * b` |
| `/` | Division | `a / b` |
| `%` | Remainder | `a % b` |

### Bitwise Operators

| Operator | Meaning | Example |
|---|---|---|
| `&` | Bitwise AND | `a & b` |
| `\|` | Bitwise OR | `a \| b` |
| `^` | Bitwise XOR | `a ^ b` |
| `~` | Bitwise NOT | `~a` |
| `<<` | Shift left | `a << 1` |
| `>>` | Shift right | `a >> 1` |

### Comparison Operators

| Operator | Meaning |
|---|---|
| `==` | Equal |
| `!=` | Not equal |
| `<` | Less than |
| `<=` | Less than or equal |
| `>` | Greater than |
| `>=` | Greater than or equal |

### Logical Operators

| Operator | Meaning |
|---|---|
| `&&` | Logical AND |
| `\|\|` | Logical OR |

### Assignment Operators

| Operator | Meaning |
|---|---|
| `=` | Assign |
| `+=` | Add and assign |
| `-=` | Subtract and assign |
| `*=` | Multiply and assign |
| `/=` | Divide and assign |
| `%=` | Remainder and assign |
| `&=` | Bitwise AND and assign |
| `\|=` | Bitwise OR and assign |
| `^=` | Bitwise XOR and assign |
| `<<=` | Shift left and assign |
| `>>=` | Shift right and assign |

### Increment and Decrement

```wandc
x++;
x--;
```

These are postfix operators.

---

## 19. Type Conversions

Use explicit casts at narrowing boundaries.

The checker currently accepts some integer assignments without a cast and truncates the value. See [TROUBLESHOOTING.md](TROUBLESHOOTING.md#implicit-integer-narrowing-is-accepted).

Integer cast:

```wandc
u64 big = 300;
u8 small = (u8)big;
```

Float cast:

```wandc
u64 int_val = 42;
f64 float_val = (f64)int_val;
```

Pointer to integer:

```wandc
u8* p = null;
u64 addr = (u64)p;
```

Integer to pointer:

```wandc
u64 addr = 0;
u8* p = (u8*)addr;
```

Pointer to pointer:

```wandc
void* generic = malloc(64);
u8* bytes = (u8*)generic;
```

Rules:

1. Every conversion must be explicit.
2. The target type must be written in parentheses.
3. Casting does not change the value representation by itself.
4. Pointer casts do not change alignment.

---

## 20. Control Flow

### If Statement

```wandc
if (x == 10) {
    x = 0;
}
```

With else:

```wandc
if (x == 10) {
    x = 0;
} else {
    x = 1;
}
```

Else-if chain:

```wandc
if (x == 0) {
    print_string("zero");
} else if (x == 1) {
    print_string("one");
} else {
    print_string("other");
}
```

### While Loop

```wandc
while (x < 10) {
    x++;
}
```

### For Loop

```wandc
for (u64 i = 0; i < 10; i++) {
    print_number(i);
}
```

A `for` statement has three parts:

1. Init statement
2. Condition expression
3. Post statement

### Match Statement

```wandc
match (state) {
    case 1 {
        print_string("one");
    }
    case 2 {
        print_string("two");
    }
    default {
        print_string("other");
    }
}
```

Rules:

1. Cases do not fall through.
2. `default` is optional.
3. At most one `default` is allowed.

### Break and Continue

`break` exits the nearest loop.

```wandc
while (1) {
    break;
}
```

`continue` jumps to the next loop iteration.

```wandc
for (u64 i = 0; i < 10; i++) {
    if (i == 5) {
        continue;
    }
}
```

---

## 21. Functions

### Function Declaration

```wandc
fn add(u64 a, u64 b) -> u64 {
    return(a + b);
}
```

A function with no return value:

```wandc
fn stop() {
}
```

### Function Declaration Without Body

```wandc
fn add(u64 a, u64 b) -> u64;
```

Use this in `.wh` files.

### Multiple Return Values

```wandc
fn divmod(u64 a, u64 b) -> (u64, u64) {
    return(a / b, a % b);
}
```

Destructure the result:

```wandc
u64 q;
u64 r;
[q, r] = divmod(17, 5);
```

> Fixed: multi-value returns now assign each result to the correct target. See [TROUBLESHOOTING.md](TROUBLESHOOTING.md#multiple-return-values-give-the-same-result) for the historical bug.

### Return Statement

Return one value:

```wandc
return(1);
```

Return multiple values:

```wandc
return(a, b);
```

Bare return:

```wandc
return;
```

In a function that returns `u64`, bare return is equivalent to:

```wandc
return(0);
```

### Function Versions

> Current limitation: the parser accepts the `version N` suffix but does not
> implement it. The emitted symbol is the plain function name (`open`, not
> `open@2`), and two definitions differing only in version collide. Do not use
> function versions for ABI versioning yet. Struct/union version reflection
> (`versionof`) is implemented; function versioning is not.

---

## 22. Export and Extern Functions

Use `export` to make a function visible to other modules.

```wandc
export fn public_api(u64 value) -> u64 {
    return(value * 2);
}
```

Use `extern` to declare a function from another module.

```wandc
extern fn external_func(u64 a, u64 b) -> u64;
```

> Function `version` suffixes are recorded in the exports and type records of
> continuation objects. The compiler accepts `version N` before or after the
> return type. See [section 21](#21-functions).

---

## 23. Global Sections

Global variables live in named sections.

```wandc
sect.state
u64 ticks = 0;
EOS
```

`EOS` terminates the section.

Access a section variable with `section:variable`.

```wandc
state:ticks = state:ticks + 1;
```

Take the address of a section variable:

```wandc
u64* p = state:ticks*adr;
```

### Section Modifiers

Use `align(N)` to set section alignment.

```wandc
align(4096) sect.buffer
u8 data[4096];
EOS
```

Use `ro` for read-only sections.

```wandc
ro sect.config
u64 magic = 0x1234;
EOS
```

Use `noinit` for sections that must not be initialized.

```wandc
noinit sect.bss
u8 work_area[1024];
EOS
```

Combine modifiers:

```wandc
align(4096) ro sect.config
u64 magic = 0x1234;
EOS
```

---

## 24. Volatile Variables

Use `volatile` for memory-mapped hardware.

```wandc
sect.hardware
volatile u64 status = 0;
EOS
```

Rules:

1. The compiler must not remove volatile reads.
2. The compiler must not remove volatile writes.
3. The compiler must not reorder volatile accesses across sequence points.
4. Use `memory_barrier()` when CPU ordering is required.

---

## 25. Atomic Variables

Use `atomic` for shared data modified by multiple contexts.

```wandc
sect.shared
atomic u64 counter = 0;
EOS
```

Atomic built-in functions:

| Function | Operation |
|---|---|
| `atomic_load(ptr)` | Read |
| `atomic_store(ptr, value)` | Write |
| `atomic_add(ptr, value)` | Add |
| `atomic_sub(ptr, value)` | Subtract |
| `atomic_inc(ptr)` | Increment |
| `atomic_dec(ptr)` | Decrement |
| `atomic_swap(ptr, value)` | Swap |
| `atomic_cas(ptr, expected, desired)` | Compare and swap |
| `memory_barrier()` | CPU memory fence |
| `compiler_barrier()` | Compiler ordering fence |

Example:

```wandc
atomic_add(shared:counter*adr, 1);
```

Implementation notes:

- These builtins work on the pointed-to type: `atomic u8`, `atomic u16`,
  `atomic u32` and `atomic u64` emit 1-, 2-, 4- and 8-byte operations. Narrow
  operations never touch adjacent bytes.
- `atomic_add`, `atomic_sub`, `atomic_swap` and `atomic_cas` return the **old**
  value; `atomic_inc` and `atomic_dec` return the **new** value.
- `atomic_load` is a plain load, `atomic_store` is a store followed by
  `memory_barrier()`, and the read-modify-write builtins use locked
  instructions (`xchg` or `lock xadd`/`lock cmpxchg`).

A spinlock can therefore be written entirely in Wand:

```wandc
sect.shared
atomic u32 lock_word = 0;
EOS

fn lock() -> u64 {
    while (atomic_swap(shared:lock_word*adr, 1) != 0) {
        while (atomic_load(shared:lock_word*adr) != 0) {
            ::nasm::{
                pause
            }
        }
    }
    return(0);
}

fn unlock() -> u64 {
    atomic_store(shared:lock_word*adr, 0);
    return(0);
}
```

#### Raw address loads and stores (MMIO)

Low-level drivers can read and write a device register or packed structure at
an arbitrary address without a C shim. These are built-in calls: no new cast or
dereference syntax is involved.

| Function | Operation |
|---|---|
| `mem_load_u8/u16/u32/u64(addr)` | Read 1/2/4/8 bytes at `addr`, zero-extended to 64 bits |
| `mem_load_i8/i16/i32/i64(addr)` | Read 1/2/4/8 bytes at `addr`, sign-extended to 64 bits |
| `mem_store_u8/u16/u32/u64(addr, val)` | Write 1/2/4/8 bytes at `addr` (returns 0) |
| `mem_store_i8/i16/i32/i64(addr, val)` | Write the low 1/2/4/8 bytes of `val` (returns 0) |
| `volatile_load_u8/u16/u32/u64(addr)` | Read with a full fence before and after, zero-extended |
| `volatile_load_i8/i16/i32/i64(addr)` | Read with a full fence before and after, sign-extended |
| `volatile_store_u8/u16/u32/u64(addr, val)` | Write with a full fence before and after (returns 0) |
| `volatile_store_i8/i16/i32/i64(addr, val)` | Write with a full fence before and after (returns 0) |

```wandc
u32 status = volatile_load_u32(0x1000);
mem_store_u32(0x1004, status);
```

Rules:

1. Loads zero-extend to 64 bits; stores truncate `val` to the named width.
2. Accesses are unaligned-safe on x86; alignment is still the caller's
   responsibility on other targets.
3. Calls are never removed by the optimizer. The `volatile_*` forms add
   `mfence` before and after the access.
4. Device memory must be mapped non-cacheable by the OS. These builtins
   control instruction width and ordering, not page caching.

---

## 26. Critical Sections

`critical` is allowed only in `sc.false`.

```wandc
critical {
    state:ticks = state:ticks + 1;
}
```

A critical section must disable interrupts for its body.

---

## 27. IRQ Handlers

Use `irq` to define an interrupt handler.

```wandc
irq fn timer_interrupt() {
}
```

Rules:

1. `irq` is allowed only in `sc.false`.
2. The compiler saves registers on entry.
3. The compiler restores registers before return.

---

## 28. Inline Assembly

Use a `nasm` block for inline assembly.

```wandc
fn halt() {
    ::nasm::{
        hlt
    }
}
```

Local variables can be used by name.

```wandc
fn read_timestamp() -> u64 {
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

Supported instructions (this is a small built-in encoder, **not** a full NASM
assembler):

| Instruction | Notes |
|---|---|
| `pause`, `hlt`, `nop`, `rdtsc` | No operands. |
| `mfence`, `lfence`, `sfence` | No operands. |
| `cli`, `sti`, `cld` | No operands. |
| `syscall`, `ret`, `push rbp`, `pop rbp` | Fixed encodings. |
| `rep movsb`, `rep stosb` | Fixed encodings. |
| `mov reg, reg` | `rax/rbx/rcx/rdx/rsi/rdi` and 8/16/32-bit aliases (`al`, `ax`, `eax`, …). |
| `mov [local], reg`, `mov reg, [local]` | Stack slot of a local variable; width follows the register name. |
| `mov reg, imm` | Signed decimal immediate, truncated to the register width. |

Rules and limitations:

1. Inline assembly is unsafe and the programmer is responsible for side effects.
2. The block statement itself is kept by the optimizer. Locals named as
   `[name]` inside the block are kept in memory and are not constant-propagated.
3. The compiler does **not** save or restore registers around the block.
   Do not clobber registers that hold live values; prefer passing values
   through local variables.
4. Unknown lines are not assembled. The compiler prints
   `warning: unsupported ::nasm:: line ignored` and emits no bytes for them,
   so check compiler output when using unfamiliar instructions.
5. This is not an optimization or CPU memory barrier by itself. Use explicit
   `memory_barrier()` if ordering is required.

---

## 29. jmpto Statement

Use `jmpto` to call another module.

```wandc
jmpto "worker.wexp" {
    u64 task_id = 42;
}
```

The block contains statements that prepare arguments. This is how a Wand host
calls a separately built `.wexp` module at runtime: the module is registered
with `wand_p46_register_path()` and the generated code calls
`__wand_jmpto_loader` from the Standard 4/6 loader library.

Rules:

1. The module name is a string literal or module identifier.
2. Variables prepared by the block are passed as continuation arguments (at
   most six values, in their order of definition/assignment).
3. A `return(var)` entry in the block names a variable that receives the
   corresponding continuation result (at most four values). Without such an
   entry the first result remains in `rax`.
4. If the module source is available, the compiler may inline it. In that case
   `main`'s `return(expr)` stores into the caller variable named by `expr`.
5. If the module source is not available, the compiler emits a call to
   `__wand_jmpto_loader`, supplied by the Standard 4/6 loader library.

---

## 30. Compile-Time Reflection

Built-in reflection functions:

| Function | Result |
|---|---|
| `sizeof(Type)` | Size in bytes |
| `alignof(Type)` | Alignment in bytes |
| `fieldsof(Type)` | Number of fields |
| `offsetof(Type:field)` | Field offset in bytes |
| `versionof(Type)` | Type version |
| `nameof(Type)` | Type name string |

Example:

```wandc
u64 size = sizeof(Packet);
u64 align = alignof(Packet);
u64 fields = fieldsof(Config);
u64 offset = offsetof(Config:flags);
```

---

## 31. Memory Safety Analysis

The compiler performs static memory analysis.

It should report:

1. Use of uninitialized variables.
2. Use of uninitialized structure fields.
3. Use after free.
4. Probable null pointer dereference.
5. Probable memory leak.
6. Free of a possibly null pointer.

The analysis is conservative.
It may report warnings.
It cannot prove all pointer safety problems.

WandC is a low-level language.
It does not remove all undefined behavior.

The programmer is responsible for:

1. Array bounds.
2. Pointer alignment.
3. Pointer arithmetic.
4. Inline assembly effects.
5. Correct allocator usage.

---

## 32. Optimizer Rules

The compiler optimizes the AST before code generation.

It may perform:

1. Constant folding.
2. Constant propagation.
3. Copy propagation for immutable variables.
4. Dead variable and dead store elimination.
5. Algebraic simplification.
6. Strength reduction for unsigned division and modulo by powers of two.
7. Nested shift folding.
8. Branch elimination and unreachable statement removal.

Rules:

1. `volatile` variables are not optimized.
2. `atomic` variables are not optimized.
3. Inline assembly blocks are optimization barriers.
4. Calls and memory loads are observable; the optimizer does not drop or duplicate them.
5. Signed division and modulo are not strength-reduced to shifts or masks.

Examples:

| Expression | Result | Condition |
|---|---|---|
| `x + 0` | `x` | none |
| `x * 1` | `x` | none |
| `x * 0` | `0` | `x` has no observable effects |
| `x - x` | `0` | `x` has no observable effects |
| `x / 1` | `x` | none |
| `x ^ x` | `0` | `x` has no observable effects |
| `x & 0` | `0` | `x` has no observable effects |
| `x \| 0` | `x` | none |
| `x << 0` | `x` | none |
| `x >> 0` | `x` | none |
| `u64 x % 8` | `x & 7` | `x` is unsigned |
| `(x << a) << b` | `x << (a + b)` | both shifts use the same direction |

---

## 33. Compiler Output Formats

| Flag | Output |
|---|---|
| `-fp` | Hosted program |
| `-fo` | Relocatable object |
| `-fr` | Flat raw binary |
| `-fk` | Freestanding kernel |
| `-fw` | Dynamic module |

Examples:

```text
wand2c main.w -o program -fp
wand2c module.w -o module.o -fo
wand2c boot.w -o boot.bin -fr
wand2c kernel.w -o kernel.img -fk
wand2c module.w -o module.wexp -fw
```

The `-fw` format requires a `main` function.

---

## 34. Language Restrictions

1. Multi-level pointers are forbidden.
2. Implicit type conversions are forbidden.
3. Array size must be a compile-time constant.
4. Import paths must not contain file extensions.
5. `align(N)` requires a power of two.
6. `critical` requires `sc.false`.
7. System calls require `sc.true`.
8. `irq` requires `sc.false`.
9. `volatile` and `atomic` cannot combine with `*i`, `*o`, or `*io`.
10. Keywords cannot be used as identifiers.

---

## 35. Summary

WandC syntax is explicit.

Use:

1. `sc.true` for hosted programs.
2. `sc.false` for bare-metal code.
3. `#import` for modules.
4. `struct`, `union`, and `enum` for data types.
5. `*adr` for addresses.
6. `*i`, `*o`, and `*io` for pointer data flow.
7. `sect` and `EOS` for global state.
8. `volatile` for hardware memory.
9. `atomic` for concurrent state.
10. Explicit casts for all type conversions.
