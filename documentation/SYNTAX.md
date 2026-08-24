# WandC Language Syntax

## 1. Source Files
Every file must start with an environment token.
Use `sc.true` for programs that run in an operating system.
Use `sc.false` for bare-metal code like kernels or drivers.

## 2. Comments
Use two slash characters to write a comment.
```
// This is a comment.
```
The compiler ignores comments.

## 3. Literals
Numbers:
```
u64 a = 4096;
```
Hexadecimal numbers:
```
u64 b = 0x1000;
```
Text strings:
```
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
```
u8* p;
```
Use brackets for arrays.
```
u8 buffer[256];
```

## 5. Constants
Constants do not change.
```
const MAX_TASKS = 256;
```

## 6. Enums
Enums assign names to numbers.
```
enum State version 1 {
    OFF = 0 version 1;
    ON = 1 version 1;
}
```
Read the value like this: `State:ON`.

## 7. Variables
Declare a variable with a type and a name.
```
u64 x = 10;
```

### Pointers
Use modifiers to show how a pointer operates.
- The `*i` modifier reads data from the pointer.
- The `*o` modifier writes data to the pointer.
- The `*io` modifier reads and writes data.

## Pointer Rules

WandC pointers point to variables.
Multi-level pointers are not allowed.

Use pointer modifiers to define data flow.

- The `*i` modifier reads the pointed variable.
- The `*o` modifier writes the pointed variable.
- The `*io` modifier reads and writes the pointed variable.

Pass an array to a function with the `*adr` operator.

```
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
```
if (x == 10) {
    x = 0;
}
```

Use `while` to repeat code.
```
while (x < 10) {
    x = x + 1;
}
```

Use `for` to repeat code with a counter.
```
for (u64 i = 0; i < 10; i = i + 1) {
}
```

Use `match` to check multiple values.
```
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
```
fn add(u64 a, u64 b) -> u64 {
    return(a + b);
}
```

The main function starts the program.
```
fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    return(0);
}
```

### Multiple Return Values
Functions can return multiple values using tuple syntax.
```
fn get_coords() -> (u64, u64) {
    return(10, 20);
}

fn main() -> u64 {
    u64 x, y;
    [x, y] = get_coords();
    return(0);
}
```

## 10. Structures
A structure holds multiple variables.
```
struct Task version 1 {
    u64 id version 1;
    u64 state version 1;
}
```
Read a field like this: `task.id`.
Read a pointer field like this: `task_ptr->id`.

### Packed Structures
Use the `packed` keyword to remove padding.
Use the `align(N)` modifier to set custom alignment.
```
packed align(1) struct Packet {
    u8 type version 1;
    u16 length version 1;
}
```

## 11. Unions
A union stores different data types in the same memory location.
```
union Data version 1 {
    u64 as_u64 version 1;
    f64 as_f64 version 1;
}
```

## 12. Typedef
Use `typedef` to create type aliases.
```
typedef u8[256] Buffer;
typedef i64 Result;
```

## 13. Global Sections
Put global variables in a section.
```
sect.state
    u64 ticks = 0;
EOS
```
Read the variable like this: `state:ticks`.

### Section Modifiers
Use `align(N)` to set section alignment.
Use `ro` to make the section read-only.
Use `noinit` to exclude the section from initialization data.
```
align(4096) ro sect.config
    u64 magic = 0x1234;
EOS
```

## 14. Volatile and Atomic Variables
Use `volatile` and `atomic` on section variables and structure fields.

Section example:
```
sect.state
    volatile i64 flag = 0;
EOS
```

Structure example:
```
struct Ctx {
    volatile i64 interrupted;
}
```

A write to a `volatile` target emits a memory fence after the store.

## 15. Compile-Time Reflection
The compiler provides built-in tools to inspect types.
- `sizeof(Type)` provides the size in bytes.
- `alignof(Type)` provides the alignment.
- `offsetof(Struct:field)` provides the field offset.
- `fieldsof(Type)` provides the number of fields.
- `versionof(Type)` provides the type version.
- `nameof(Type)` provides the type name as a string.

## 16. Atomic Built-in Functions
Use atomic functions for lock-free thread safety.
- `atomic_load(ptr)`
- `atomic_store(ptr, val)`
- `atomic_add(ptr, val)`
- `atomic_sub(ptr, val)`
- `atomic_inc(ptr)`
- `atomic_dec(ptr)`
- `atomic_swap(ptr, val)`
- `atomic_cas(ptr, expected, desired)`
- `memory_barrier()`
- `compiler_barrier()`

## 17. Critical Sections (Bare Metal)
Use `critical` blocks to disable interrupts in `sc.false` code.
```
critical {
    state:ticks = state:ticks + 1;
}
```

## 18. IRQ Handlers
Use the `irq` keyword to define interrupt handlers.
```
irq fn timer_handler() {
    // Handle interrupt
}
```

## 19. Export and Extern Functions
Use `export` to make a function visible to other modules.
```
export fn public_api() -> u64 {
    return(1);
}
```

Use `extern` to declare an external function.
```
extern fn external_func() -> u64;
```

## 20. Built-In Functions
Use `inb` and `outb` for hardware ports.

## 21. Standard Library
Import modules to get more functions.
```
#import <io>
```

## 22. Inline Assembly
Write CPU instructions in a block.
```
fn halt() {
    ::nasm::{
        hlt
    }
}
```

## 23. Return Statement
Use `return(value);` to return a value.
```
fn add(u64 a, u64 b) -> u64 {
    return(a + b);
}
```

A bare `return;` is allowed.
It is identical to `return(0);`.
```
fn stop() {
    return;
}
```

## 24. Array Sizes
The size of an array must be a compile-time constant.
You can use a number literal.
```
u8 buffer[4096];
```
You can also use a constant.
```
const MAX_NAME = 256;
struct User {
    u8 name[MAX_NAME];
}
```
Constants must appear before you use them in array sizes.

## 25. Compiler Formats
Tell the compiler what to generate.
- The `-fp` flag generates a normal program.
- The `-fk` flag generates a kernel.
- The `-fo` flag generates a relocatable object file.
- The `-fr` flag generates a flat raw binary.
- The `-fw` flag generates a dynamic execution module.


---

## 26. Language Restrictions

WandC enforces the following restrictions at compile time.

### Multi-Level Pointers Are Not Allowed

WandC does not support pointers to pointers. Use a single pointer and pass array addresses with `*adr`.

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

The compiler emits this error for multi-level pointers:

```
error: multi-level pointers are not allowed in WandC; use a single pointer and pass array addresses with *adr
```

### sc.true and sc.false Restrictions

The environment token controls which built-in functions are available.

- `sc.true` (hosted): allows `syscall0` through `syscall6`. Requires `main` function.
- `sc.false` (freestanding): allows `critical` blocks. Requires `kmain` function.

Using `syscall0` through `syscall6` in `sc.false` code produces:

```
error: 'syscallN' requires sc.true
```

Using `critical` in `sc.true` code produces:

```
error: critical requires sc.false
```

### Import Path Restrictions

Import paths must not contain file extensions.

Incorrect:

```
#import <io.w>
#import <io.h>
```

Correct:

```
#import <io>
#import "mymodule"
```

System modules use angle brackets.
Local modules use double quotes.

### Array Size Must Be Compile-Time Constant

Array sizes must be numeric literals or `const` values.

Correct:

```
u8 buffer[4096];

const MAX_NAME = 256;
u8 name[MAX_NAME];
```

### align Values Must Be Powers of Two

The `align(N)` modifier requires N to be a power of two.

Correct: `align(1)`, `align(2)`, `align(4)`, `align(8)`, `align(16)`, `align(4096)`

Incorrect: `align(3)`, `align(0)`, `align(12)`

### volatile and atomic Cannot Combine with *i or *o

Incorrect:

```
volatile u64* ptr*i;
```

Correct:

```
volatile u64* ptr;
```

---

## 27. The *adr Operator

The `*adr` operator takes the address of a variable or expression. It produces a pointer value.

### Syntax

```
variable_name*adr
(expression)*adr
```

### Rules

1. For scalar variables, `*adr` returns the stack address of the variable.
2. For arrays, the array name without `*adr` already decays to a pointer. Use `*adr` only when you need the address of the array variable itself.
3. For struct fields, use `(expr.field)*adr` or `(expr->field)*adr`.
4. `*adr` is required when passing a variable to a function that expects a pointer.

### When to Use *adr

Pass a scalar to a pointer parameter:

```
fn read_value(u64* out*o) {
    out = 42;
}

fn main() -> u64 {
    u64 val = 0;
    read_value(val*adr);
    return(0);
}
```

Pass an array to a function:

```
fn process(u8* data, u64 size);

fn main() -> u64 {
    u8 buffer[256];
    process(buffer*adr, 256);
    return(0);
}
```

Pass a struct to a pointer parameter:

```
fn init_point(Point* p*o);

fn main() -> u64 {
    Point pt;
    init_point(pt*adr);
    return(0);
}
```

### When *adr Is Not Needed

When the parameter is not a pointer:

```
fn add(u64 a, u64 b) -> u64;

u64 x = 10;
u64 result = add(x, 5);
```

When the variable is already a pointer:

```
u8* ptr;
u8* copy = ptr;
```

---

## 28. jmpto Statement

The `jmpto` statement transfers control to another module. The compiler attempts to inline the target module at compile time. If the source is not available, it generates a dynamic call through the Standard 4/6 ABI loader.

### Syntax

```
jmpto module_name {
    statement1;
    statement2;
    return(expr);
}
```

The module name can be a string literal or an identifier with extension.

```
jmpto "worker.wexp" {
    u64 task_id = 42;
    return(task_id);
}

jmpto worker.wexp {
    u64 task_id = 42;
    return(task_id);
}
```

### Compile-Time Inlining

When the compiler finds the source file of the target module, it:
1. Parses the target module.
2. Locates the `main` function in the target module.
3. Compiles the argument statements in the caller context.
4. Inlines the body of the target `main` function.
5. Converts `return(expr)` in the target into a store to the caller variable.

The compiler searches for the source file in this order:
1. The exact module name as a file path.
2. The module name with `.wexp` replaced by `.w`.

### Dynamic Loading

When the source file is not found, the compiler generates a call to `__wand_jmpto_loader`. This function is provided by the runtime or the operating system loader.

### Variable Isolation

Variables declared inside the `jmpto` body are local to the inlined code. The compiler prefixes internal variable names to avoid conflicts with the caller scope.

### Return Value Handling

The `return(expr)` inside `jmpto` does not exit the current function. It stores the value of `expr` into the variable specified in the caller.

Example:

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

If the target module `compute.w` contains:

```
sc.true

fn main() -> u64 {
    u64 output = 0;
    output = input * 2;
    return(output);
}
```

The compiler inlines the body and the `return(output)` stores the value into the caller context.

---

## 29. Memory Safety Analysis

The compiler performs static memory safety analysis during Stage 3. It detects the following classes of errors.

### Use of Uninitialized Variables

The compiler tracks variable initialization state. Reading a variable before assignment produces an error.

```
fn main() -> u64 {
    u64 x;
    print_number(x);
    return(0);
}
```

Produces:

```
error: use of potentially uninitialized variable 'x'
  --> line 3
   |
 3 |     print_number(x);
   |                  ^
   |
  note: initialize it: 'u64 x = null;'
```

### Use of Uninitialized Struct Fields

```
fn main() -> u64 {
    Point pt;
    print_signed_number(pt.x);
    return(0);
}
```

Produces:

```
error: use of uninitialized field 'x' of struct 'pt'
```

### Use-After-Free

The compiler tracks `mfree` calls. Accessing a pointer after `mfree` produces an error.

```
fn main() -> u64 {
    mem_init(1048576);
    u8* ptr = malloc(64);
    mfree(ptr);
    u8 val = ptr[0];
    return(0);
}
```

Produces:

```
error: use-after-free violation on pointer 'ptr'
```

### Potential Null Pointer Dereference

Accessing a field through a pointer that was allocated but not checked for null produces an error.

```
fn main() -> u64 {
    mem_init(1048576);
    Point* p = malloc(sizeof(Point));
    p->x = 10;
    return(0);
}
```

Produces:

```
error: potential null pointer dereference of 'p' when accessing field 'x'
  note: wrap in null check: if (p != null) { ... }
```

### Potential Memory Leak

A pointer that is allocated but never freed before function exit produces a warning.

```
warning: potential memory leak in function 'main': pointer 'ptr' was never freed via 'mfree()'
  note: call mfree(ptr) before the function returns, or document that ownership is transferred
```

### Freeing Potentially Null Pointer

Calling `mfree` on a pointer that was allocated but not null-checked produces a warning.

```
warning: freeing potentially null pointer 'ptr'
  note: pointer 'ptr' was allocated on line 3 but never checked for null, add if (ptr != null) before mfree
```

---

## 30. Optimizer Behavior

The compiler runs an AST optimization pass during Stage 2. The optimizer performs constant folding, dead code elimination, and algebraic simplification.

### Constant Folding

The optimizer evaluates constant expressions at compile time.

```
u64 x = 2 + 3;
```

Becomes:

```
u64 x = 5;
```

The optimizer handles: `+`, `-`, `*`, `/`, `%`, `&`, `|`, `^`, `<<`, `>>`, comparisons, logical operators, and bitwise NOT.

### Dead Variable Elimination

Variables that are assigned but never read are removed.

```
u64 unused = 42;
```

If `unused` is never read, the statement is removed.

### Constant Propagation

Variables initialized with constants are replaced at use sites.

```
u64 limit = 100;
while (count < limit) {
    count = count + 1;
}
```

The `limit` is replaced with `100` in the condition.

### Algebraic Simplifications

The optimizer applies these rules:

| Expression | Result |
|---|---|
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

### Power-of-Two Strength Reduction

Multiplication and division by powers of two are converted to shifts.

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

### Commutative Chain Folding

Chains of commutative operations fold multiple constants into one.

```
x + 10 + 20 + 30
```

Becomes:

```
x + 60
```

### Loop Optimization

The optimizer removes loop bodies when the condition is a constant zero.

```
while (0) {
    // removed entirely
}
```

Variables assigned inside loops are removed from the constant table to prevent incorrect propagation.

### If-Branch Elimination

When an `if` condition is a compile-time constant, only the taken branch is emitted.

```
if (1) {
    print_string("always runs");
} else {
    print_string("removed");
}
```

Becomes only the `then` branch code.

### Volatile and Atomic Variables Are Not Optimized

Variables with `volatile` or `atomic` modifiers are never constant-propagated or eliminated. The optimizer preserves all reads and writes to these variables.

### Iteration Limit

The optimizer runs up to 10 iterations per function. Each iteration re-scans the AST for new optimization opportunities created by previous passes.

---

## 31. Wexp Format and Dynamic Modules

The `.wexp` format produces a dynamic execution module. Use the `-fw` flag to compile.

```
wand2c module.w -o module.wexp -fw
```

### Entry Point

The `.wexp` format requires a `main` function. The compiler rejects `--entry` for this format.

### Binary Layout

A `.wexp` file uses the ELF container with Standard 4/6 metadata sections. The layout is:

```
.text          (0x400078) -> executable code
.p46_header    -> ABI magic and metadata
.p46_types     -> TLV type descriptors
.p46_exports   -> exported symbols
.p46_imports   -> imported symbols
.p46_deps      -> module dependencies
.p46_reflect   -> qualified-name lookup index
.p46_strtab    -> string table
```

### Loading

The target operating system or runtime loads `.wexp` modules through the Standard 4/6 loader API. The loader:
1. Validates the header magic and version.
2. Resolves dependencies from `.p46_deps`.
3. Resolves imports from `.p46_imports`.
4. Maps the `.text` section into memory.
5. Transfers control to the entry point.

### Relationship to jmpto

The `jmpto` statement is the language-level mechanism to invoke `.wexp` modules. When the source file of the target module is not available at compile time, the compiler emits a call to `__wand_jmpto_loader` with the module name as a string argument.

The runtime provides `__wand_jmpto_loader`. This function:
1. Loads the `.wexp` file from disk or memory.
2. Validates the Standard 4/6 metadata.
3. Resolves imports.
4. Calls the module entry point.
5. Returns the result to the caller.

### Exporting Functions

Mark functions with `export` to make them visible to other modules.

```
export fn public_api(u64 value) -> u64 {
    return(value * 2);
}
```

If any function in a module has `export`, only `export` functions are included in the export table. Non-exported functions become local symbols.

### Importing Functions

Declare external functions with `extern`. The compiler records these as imports in the `.p46_imports` section.

```
extern fn external_func(u64 a, u64 b) -> u64;
```
