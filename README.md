# wand2c - Wand Version 2 Compiler

wand2c compiles the Wand systems programming language. 
It translates source code directly to native x86_64 machine code. 
It does not use LLVM or GCC.

The compiler generates ELF64 executables, relocatable object files, and flat binary images. 
It adds Standard 4/6 binary metadata to custom ELF sections.

## Architecture

The compiler uses a modular multi-pass pipeline:

* `src/ast.rs`: Defines the Abstract Syntax Tree (AST). Includes data types, pointer modifiers, expressions, and declarations.
* `src/token.rs`: Defines lexical tokens.
* `src/lexer.rs`: Scans source text into tokens. Tracks line and column numbers for error reporting.
* `src/parser.rs`: Parses tokens into the AST. Uses a two-pass system for functions.
* `src/checker.rs`: Checks types. Calculates structure sizes and field offsets.
* `src/optimizer.rs`: Optimizes the AST. Folds constants and removes dead code.
* `src/safety.rs`: Checks memory safety. Detects memory leaks, use-after-free errors, and uninitialized variables.
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

The compiler validates the source code before installation. 
It copies the files to `~/.local/lib/libw/`.

## Compilation Commands

Compile a source file to an executable program:

```bash
wand2c main.w -o main -fp
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

## Output Formats

| Flag | Format | Description |
|---|---|---|
| `-fp` | program | Hosted ELF64 executable. Requires `sc.true`. |
| `-fo` | object | Relocatable ELF object file. |
| `-fr` | raw | Flat binary image. No ELF header. |
| `-fk` | kernel | Freestanding kernel image. Requires `sc.false`. |
| `-fw` | wexp | Dynamic execution module. |

## Memory Allocation

The compiler uses a size-prefix header for memory allocation.

1. **Allocation (`mloc`)**: The compiler requests 8 extra bytes. It stores the block size in the first 8 bytes. It returns the address plus 8.
2. **Deallocation (`mfree`)**: The compiler subtracts 8 from the pointer. It reads the size from the header. It calls the unmap system call.

## Hardware Support

The compiler supports direct hardware access on x86_64.

* **Port I/O**: `inb`, `outb`, `inw`, `outw`, `inl`, `outl`.
* **Inline Assembly**: Use `::nasm::{}` blocks for raw machine instructions.
* **Atomics**: `atomic_load`, `atomic_store`, `atomic_cas`, `memory_barrier`.

## Binary Metadata

The compiler adds custom sections to the ELF file:

* `.text`: The executable machine code.
* `.p46_header`: Standard 4/6 magic and metadata.
* `.p46_types`: Structure and type definitions.
* `.p46_exports`: Exported function signatures.
* `.p46_imports`: External dependencies.
* `.p46_strtab`: Metadata string table.
