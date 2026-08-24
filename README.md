# wand2c - Wand Version 2 Compiler

wand2c compiles the Wand systems programming language.
It translates source code directly to native x86_64 machine code.
It does not use LLVM or GCC.

The compiler generates ELF64 executables, relocatable object files, flat binary images, and dynamic execution modules.
It adds Standard 4/6 binary metadata to custom ELF sections.

## Architecture

The compiler uses a modular multi-pass pipeline:

* `src/ast.rs`: Defines the Abstract Syntax Tree (AST). Includes data types, pointer modifiers, expressions, and declarations.
* `src/token.rs`: Defines lexical tokens.
* `src/lexer.rs`: Scans source text into tokens. Tracks line and column numbers for error reporting.
* `src/parser.rs`: Parses tokens into the AST. Uses a two-pass system for functions.
* `src/checker.rs`: Checks types. Calculates structure sizes and field offsets.
* `src/optimizer.rs`: Optimizes the AST. Folds constants, removes dead code, applies algebraic simplifications, and performs strength reduction.
* `src/safety.rs`: Checks memory safety. Detects memory leaks, use-after-free errors, uninitialized variables, and null pointer dereferences.
* `src/codegen.rs`: Generates x86_64 machine code. Patches call sites and string addresses. Builds the final binary.
* `src/abi.rs`: Generates Standard 4/6 metadata. Builds string tables and type records.
* `src/main.rs`: Manages the command line interface. Resolves imports and installs libraries.

## Build the Compiler

Use Cargo to build the compiler:

```bash
cargo build --release
```

The binary is located at `target/release/wand2c`.

## Standard Library Installation

The compiler has a package manager for the standard library (`libw`).

Install all libraries:

```bash
wand2c -il libw
```

Install one library:

```bash
wand2c -il libw/io
```

Install a directory of libraries:

```bash
wand2c -il sfa/
```

The compiler validates the source code before installation.
It copies the files to `~/.local/lib/libw/`.

## Compilation Commands

Compile a source file to an executable program:

```bash
wand2c main.w -o main -fp
```

Compile with verbose output:

```bash
wand2c main.w -o main -fp -v
```

Compile to a relocatable object file:

```bash
wand2c module.w -o module.o -fo
```

Compile to a flat binary:

```bash
wand2c boot.w -o boot.bin -fr --entry=start
```

Compile to a freestanding kernel image:

```bash
wand2c kernel.w -o kernel.kbin -fk --entry=kmain
```

Compile to a dynamic execution module:

```bash
wand2c module.w -o module.wexp -fw
```

## Output Formats

| Flag | Format | Description |
|---|---|---|
| `-fp` | program | Hosted ELF64 executable. Requires `sc.true`. |
| `-fo` | object | Relocatable ELF object file. |
| `-fr` | raw | Flat binary image. No ELF header. |
| `-fk` | kernel | Freestanding kernel image. Requires `sc.false`. |
| `-fw` | wexp | Dynamic execution module. |

## Compiler Flags

| Flag | Description |
|---|---|
| `-o <file>` | Set output file path |
| `-v, --verbose` | Show detailed compilation pipeline output |
| `-fp, -fo, -fr, -fk, -fw` | Set output format |
| `--entry <name>` | Set entry function (raw and kernel formats only) |
| `-il <path>` | Install a library or library directory |

## Language Features

### Environment Tokens

Every Wand source file starts with an environment token:

* `sc.true`: Hosted environment. The program runs under an operating system. Requires a `main` function. Allows system calls.
* `sc.false`: Freestanding environment. The program runs on bare metal. Requires a `kmain` function. Allows `critical` blocks and IRQ handlers.

### Import System

System modules use angle brackets. Local modules use double quotes.

```
#import <io>
#import <mem>
#import "my_module"
```

### Pointer Modifiers

WandC uses explicit pointer modifiers to declare data flow direction:

* `*i`: The function reads through this pointer.
* `*o`: The function writes through this pointer.
* `*io`: The function reads and writes through this pointer.
* `*adr`: Takes the address of a variable.

Multi-level pointers (`u8**`) are not allowed.

### Control Flow

WandC supports `if`, `else`, `while`, `for`, `match`, `continue`, and `break`.

### Compound Assignment

The compiler supports compound assignment operators:

`+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=`, `>>=`

### Array Initialization

Arrays in global sections can be initialized with brace syntax:

```
sect.data
    u8 buffer[4] = {
        0x00, 0x01, 0x02, 0x03
    };
EOS
```

### Multiple Return Values

Functions can return tuples:

```
fn divmod(u64 a, u64 b) -> (u64, u64) {
    return(a / b, a % b);
}

u64 q, r;
[q, r] = divmod(17, 5);
```

Use `_` to ignore a return value:

```
[_, r] = divmod(17, 5);
```

### Structures with Versioning

Structures and fields carry version metadata for ABI stability:

```
struct Config version 2 {
    u32 flags version 1;
    u8* name version 2;
}
```

### Global Sections

Global variables live in named sections, terminated by `EOS`:

```
sect.state
    u64 ticks = 0;
    volatile u64 status = 0;
EOS
```

Access section variables with `section:variable` syntax.

### Inline Assembly

Use `::nasm::{}` blocks for raw x86_64 instructions:

```
fn halt() {
    ::nasm::{
        hlt
    }
}
```

### Compile-Time Reflection

Built-in functions inspect types at compile time:

* `sizeof(Type)`: Size in bytes.
* `alignof(Type)`: Alignment requirement.
* `offsetof(Struct:field)`: Field byte offset.
* `fieldsof(Type)`: Number of fields.
* `versionof(Type)`: Type version number.
* `nameof(Type)`: Type name as a string.

## Memory Safety Analysis

The compiler performs static memory safety analysis during Stage 3. It detects:

* Use of uninitialized variables and struct fields.
* Use-after-free violations.
* Potential null pointer dereferences.
* Potential memory leaks.
* Freeing potentially null pointers.

Errors abort compilation. Warnings are reported but do not stop the build.

## Optimizer

The optimizer runs up to 10 iterations per function. It performs:

* Constant folding and propagation.
* Dead variable elimination.
* Algebraic simplifications (`x + 0`, `x * 1`, `x ^ x`, etc.).
* Power-of-two strength reduction (multiply and divide to shifts).
* Commutative chain folding.
* Loop and branch elimination for constant conditions.

Volatile and atomic variables are never optimized.

## Memory Allocation

The standard library provides two allocation systems:

### Arena Allocator (`mem` module)

Call `mem_init(size)` to allocate an arena. Then use `malloc`, `calloc`, `mrealloc`, and `mfree`.

### Raw Allocator (`syscall` module)

The `mloc` function allocates memory through the `mmap` system call.
It stores the block size in an 8-byte header before the returned pointer.
The `mfree` function reads the header and calls `munmap`.

## Hardware Support

The compiler supports direct hardware access on x86_64:

* **Port I/O**: `inb`, `outb`, `inw`, `outw`, `inl`, `outl`.
* **Inline Assembly**: `::nasm::{}` blocks for raw machine instructions.
* **Atomics**: `atomic_load`, `atomic_store`, `atomic_add`, `atomic_sub`, `atomic_inc`, `atomic_dec`, `atomic_swap`, `atomic_cas`.
* **Barriers**: `memory_barrier`, `compiler_barrier`.
* **Critical Sections**: `critical { }` blocks disable interrupts in `sc.false` code.
* **IRQ Handlers**: `irq fn handler() { }` generates interrupt-safe function prologues and epilogues.

## Binary Metadata

The compiler adds custom sections to the ELF file:

| Section | Content |
|---|---|
| `.text` | Executable machine code |
| `.p46_header` | Standard 4/6 magic and metadata |
| `.p46_types` | TLV structure and type definitions |
| `.p46_exports` | Exported function signatures |
| `.p46_imports` | External symbol dependencies |
| `.p46_deps` | Module dependency list |
| `.p46_reflect` | Qualified-name lookup index |
| `.p46_strtab` | Metadata string table |

## Dynamic Modules (.wexp)

The `-fw` flag produces a dynamic execution module. The module contains a `main` entry point and Standard 4/6 metadata. The runtime loads `.wexp` modules through the Standard 4/6 loader API.

The `jmpto` statement invokes dynamic modules at the language level. If the source file is available, the compiler inlines the target. If not, it generates a call to `__wand_jmpto_loader`.
