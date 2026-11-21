# Learning Wand

This tutorial explains Wand in the order needed to write and compile small programs. It is not a complete syntax reference. Full rules are in [SYNTAX.md](SYNTAX.md). Runnable source files are in [examples/](examples/README.md).

Wand is also called WandC. The compiler is `wand2c`.

## Prerequisites

Build the compiler from the repository root:

```bash
cargo build --release
```

Install the standard library:

```bash
target/release/wand2c -il libw/llibw.86
```

Run the examples from their directory because local imports are resolved from the current working directory:

```bash
cd documentation/examples
../../target/release/wand2c hello.w -o hello -fp
./hello
```

The runnable files and their topics are listed in [examples/README.md](examples/README.md). Each example is compiled with `-fp` except `bare.w`, which uses `-fr --entry kmain`.

---

## 1. Program structure

File: [examples/hello.w](examples/hello.w).

```wand
sc.true
#import <io>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    print_string("Hello, Wand!\n");
    return(0);
}
```

Compile and run:

```bash
../../target/release/wand2c hello.w -o hello -fp
./hello
```

Output:

```text
Hello, Wand!
```

The first token selects the execution environment:

| Token | Environment | Required entry |
|---|---|---|
| `sc.true` | Hosted, with an operating system | `main` |
| `sc.false` | Freestanding, without an operating system | `main` by default; `--entry kmain` selects `kmain` |

`#import <io>` makes I/O functions available. `fn main(...) -> u64` declares the entry function. The body is the block between `{` and `}`. `return(0);` returns success to the operating system.

Statements end with `;`. See [SYNTAX.md, sections 2-5](SYNTAX.md#2-environment-token).

---

## 2. Variables and types

File: [examples/numbers.w](examples/numbers.w).

```wand
u64 apples = 3;
u64 bananas = 2;
u64 fruit = apples + bananas;
```

A variable declaration has a type, a name, and an optional initializer. `u64` is an unsigned 64-bit integer. `i64` is signed. `f64` is a double-precision float. `u8` is one byte.

The complete type table is in [SYNTAX.md, section 7](SYNTAX.md#7-primitive-types).

Use an explicit cast when the target type differs:

```wand
u8 small = (u8)big;
```

The current checker also accepts some integer assignments without a cast and truncates the value. See [TROUBLESHOOTING.md](TROUBLESHOOTING.md#implicit-integer-narrowing-is-accepted).

The tutorial examples call `show()` to print integers. It is defined once in [examples/numio.w](examples/numio.w) and imported by examples that need it:

```wand
#import "numio"
```

Do not copy that helper into each program. Import it, or use your own output routine.

Array declarations use the name-before-brackets form:

```wand
u8 buf[32];
```

Use the name-before-brackets form shown here. The reason is documented in [TROUBLESHOOTING.md](TROUBLESHOOTING.md#number-printing-crashes).

---

## 3. Expressions and assignment

Assignment stores a value:

```wand
fruit += 5;
```

Comparison asks a question:

```wand
if (n == 3) {
}
```

`=` assigns. `==` compares. Using `=` in a condition is a common error.

The full operator list is in [SYNTAX.md, section 18](SYNTAX.md#18-operators).

---

## 4. Conditions and loops

File: [examples/control.w](examples/control.w).

### `if`

```wand
if (n == 3) {
    print_string("three\n");
} else {
    print_string("not three\n");
}
```

`else if` chains are supported. See [SYNTAX.md, section 20](SYNTAX.md#20-control-flow).

### `while`

```wand
u64 i = 0;
while (i < 3) {
    show(i);
    print_char(32);
    i++;
}
```

The condition is evaluated before each iteration. The loop variable must change inside the body or in a surrounding expression.

### `for`

```wand
for (u64 j = 0; j < 3; j++) {
    show(j);
    print_char(32);
}
```

The three parts are initialization, condition, and post-expression.

### `match`

```wand
match (n) {
    case 3 {
        print_string("match: three\n");
    }
    default {
        print_string("match: other\n");
    }
}
```

Cases do not fall through. `default` is optional.

`break` exits the nearest loop. `continue` starts the next iteration.

---

## 5. Functions

File: [examples/functions.w](examples/functions.w).

```wand
fn double(u64 x) -> u64 {
    return(x * 2);
}
```

`fn` declares a function. `x` is a parameter. The value passed by the caller is an argument. `-> u64` declares the return type.

Call the function:

```wand
show(double(21));
```

### Multiple results

Use output pointers when a function must produce several independent values:

```wand
fn divmod(u64 a, u64 b, u64* q*o, u64* r*o) {
    q = a / b;
    r = a % b;
}
```

Call it with addresses:

```wand
u64 q;
u64 r;
divmod(23, 5, q*adr, r*adr);
```

`*o` declares an output pointer. `*adr` takes the address of a variable.

Tuple return syntax exists:

```wand
fn divmod(u64 a, u64 b) -> (u64, u64) {
    return(a / b, a % b);
}

u64 q;
u64 r;
[q, r] = divmod(17, 5);
```

Tuple destructuring has a current implementation limitation described in [TROUBLESHOOTING.md](TROUBLESHOOTING.md#multiple-return-values-give-the-same-result).

Function declarations without bodies belong in `.wh` files. See [SYNTAX.md, sections 21-22](SYNTAX.md#21-functions).

---

## 6. Arrays and addresses

File: [examples/arrays.w](examples/arrays.w).

```wand
u8 letters[5];
fill(letters*adr, 5);
```

An array is a fixed-size sequence. Indexing starts at `0`. `letters[0]` is the first element.

The array size must be a compile-time constant. Wand does not check bounds at run time.

The function receives a pointer:

```wand
fn fill(u8* data*o, u64 size) {
    for (u64 i = 0; i < size; i++) {
        data[i] = 65 + i;
    }
}
```

Pointer modifiers declare data flow:

| Modifier | Meaning |
|---|---|
| `*i` | read through the pointer |
| `*o` | write through the pointer |
| `*io` | read and write through the pointer |
| `*adr` | take the address of an expression |

Multi-level pointers such as `u8**` are not allowed. An array name can be passed to a pointer parameter. `array*adr` remains the explicit form and is used in the runnable example.

See [SYNTAX.md, sections 9 and 16-17](SYNTAX.md#9-arrays).

---

## 7. Structures and enumerations

File: [examples/structs.w](examples/structs.w).

```wand
struct Pixel version 1 {
    u8 r version 1;
    u8 g version 1;
    u8 b version 1;
}
```

Create and use a structure:

```wand
Pixel p;
p.r = 255;
p.g = 128;
p.b = 0;
```

Use `->` through a pointer:

```wand
fn set_red(Pixel* p) {
    p->r = 255;
}
```

Enumerations assign names to integer values:

```wand
enum Color version 1 {
    Red = 0 version 1;
    Green = 1 version 1;
    Blue = 2 version 1;
}

Color c = Color:Green;
```

`version` is metadata used by the binary interface. It does not change field access syntax.

Unions store fields in the same memory. `typedef` creates a type alias. Both are defined in [SYNTAX.md, sections 13-14](SYNTAX.md#13-unions).

Packed structures use this order:

```wand
align(1) packed struct Packet version 1 {
    u8 kind version 1;
    u16 length version 1;
}
```

---

## 8. Global sections

File: [examples/globals.w](examples/globals.w).

```wand
sect.state
    u64 ticks = 0;
EOS
```

A section is a named group of global variables. `EOS` terminates it. Access members with `section:variable`:

```wand
state:ticks = state:ticks + 1;
```

Section modifiers are `align(N)`, `ro`, and `noinit`. `volatile` marks memory-mapped state. `atomic` marks shared state.

See [SYNTAX.md, sections 23-25](SYNTAX.md#23-global-sections).

---

## 9. Freestanding code

File: [examples/bare.w](examples/bare.w).

```wand
sc.false

fn kmain() {
    critical {
    }
    ::nasm::{
        hlt
    }
}
```

Compile with:

```bash
../../target/release/wand2c bare.w -o bare.bin -fr --entry kmain
```

Raw and kernel images use `main` as the default entry. This example passes `--entry kmain` to select `kmain`. `sc.false` enables `critical`, `irq`, and direct hardware access. System calls are not available in `sc.false`.

`critical { }` disables interrupts for its body. `irq fn name() { }` declares an interrupt handler. `::nasm::{ }` contains x86_64 assembly and is an optimization barrier.

Port helpers are `inb`, `outb`, `inw`, `outw`, `inl`, and `outl`. See [SYNTAX.md, sections 24-28](SYNTAX.md#26-critical-sections).

---

## 10. Modules

Two file types are used:

| Suffix | Purpose |
|---|---|
| `.w` | Implementation with function bodies |
| `.wh` | Declarations without function bodies |

Imports use logical names without file extensions:

```wand
#import <io>
#import "numio"
```

Angle brackets select system modules. Double quotes select local modules. The compiler follows imports from the entry file.

`export` makes a function visible to other modules. `extern` declares a function defined elsewhere. `jmpto` invokes a dynamic module. See [SYNTAX.md, sections 3, 22, and 29](SYNTAX.md#3-module-imports).

---

## 11. Output formats

| Flag | Output |
|---|---|
| `-fp` | Hosted ELF64 program |
| `-fo` | Relocatable ELF object |
| `-fr` | Flat raw binary |
| `-fk` | Freestanding kernel image |
| `-fw` | Dynamic `.wexp` module |

`--entry` is valid only with `-fr` and `-fk`. `-o` selects the output path. `-v` prints compiler stages. `-il` installs a library.

Full commands are in [../README.md](../README.md#compilation-commands).

---

## 12. Safety model

The compiler checks types, declarations, module rules, and several memory-safety patterns. It reports errors and warnings. Errors stop the build.

The programmer remains responsible for:

- array bounds
- pointer alignment
- pointer arithmetic
- inline assembly effects
- allocator correctness
- explicit casts

Wand has no garbage collector. Explicit casts are recommended at narrowing boundaries.

See [SYNTAX.md, section 31](SYNTAX.md#31-memory-safety-analysis).

---

## 13. Optimizer

The optimizer folds constants, removes dead code, and simplifies expressions. It does not remove `volatile` accesses, `atomic` operations, or `::nasm::{}` blocks.

See [SYNTAX.md, section 32](SYNTAX.md#32-optimizer-rules).

---

## 14. Errors and known limitations

Read [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for compiler messages, fixes, and current implementation limitations.

---

## 15. References

| Need | Document |
|---|---|
| Complete syntax | [SYNTAX.md](SYNTAX.md) |
| Standard library | [STDLIB.md](STDLIB.md) |
| More examples | [EXAMPLES.md](EXAMPLES.md) |
| Compiler flags and architecture | [../README.md](../README.md) |
| Binary interface | [standart-46.md](standart-46.md) |
| Deep reference | [WANDCBIBLE.md](WANDCBIBLE.md) |
