# wand2c — Wand Version 2 Compiler

`wand2c` is a compiler for the Wand systems programming language, tailored for operating systems, device drivers, and bare-metal application development.

The compiler translates WandC source code directly into native x86_64 machine code and packages it into ELF64 executables. The generated binaries conform to the **Standard 4/6 Binary Interface Specification (v1.5.0)**, embedding type layouts, exports, imports, and reflection metadata directly within custom ELF Section Header Table (SHT) sections.

---

## Codebase Architecture

The compiler is organized as a modular, multi-pass pipeline. The core components of the system include:

*   **`src/ast.rs`**  
    Defines the Abstract Syntax Tree (AST), including data types (`DataType`), pointer access modifiers (`PtrAccess`), expressions (`Expr`), statements (`Stmt`), and declarations for functions, structs, and sections.
*   **`src/token.rs`**  
    Contains the `Token` enum, representing lexical units of the language (keywords, types, operators, and assembly blocks).
*   **`src/lexer.rs`**  
    The lexical analyzer (tokenizer). It scans source text into tokens, tracking exact character coordinates (`Span`) for compiler error reporting. It parses decimal and hexadecimal numeric literals, string constants, pointer modifiers (`*i`, `*o`, `*adr`), and inline assembly blocks (`::nasm::{}`).
*   **`src/parser.rs`**  
    The syntax analyzer. It parses control structures, destructuring assignments, data sections, and implements a two-pass parser for functions (gathering signatures on the first pass and parsing function bodies on the second).
*   **`src/checker.rs`**  
    The type checker. It calculates the physical size of data types and determines struct field offsets in memory according to strict alignment rules (field alignment is capped at a maximum of 8 bytes).
*   **`src/optimizer.rs`**  
    The AST optimizer. It performs constant folding on binary expressions during compilation.
*   **`src/safety.rs`**  
    The static memory leak detector. It tracks calls to built-in allocators (`mloc`, `bmloc`) and outputs compile-time warnings if an allocated pointer is not freed via `mfree()`.
*   **`src/abi.rs`**  
    Generates Standard 4/6 binary metadata. It constructs the string table (`Strtab`) and packages types, exports, imports, and reflection indices into flat or TLV (Type-Length-Value) structures.
*   **`src/codegen.rs`**  
    The x86_64 machine code generator and ELF64 packager. It directly emits native x86_64 instructions from the AST, performs call-site and string constant address patching (backpatching), and constructs the final ELF64 image containing the following custom sections:
    *   `.text` — Executive payload with the entry point.
    *   `.p46_header` — Standard 4/6 header (magic sequence `P46\0`, pointer size, endianness).
    *   `.p46_types` — Type declarations (TLV sequence).
    *   `.p46_exports` — Exported symbols and their type signatures.
    *   `.p46_imports` — Imported external symbols.
    *   `.p46_deps` — Declared module dependencies.
    *   `.p46_reflect` — Lookup index mapping qualified names to their internal indices.
    *   `.p46_strtab` — Metadata string table.
*   **`src/main.rs`**  
    The compiler entry point. It parses CLI arguments, manages multi-file compilation, handles automatic resolution and loading of `.w`/`.wh` library files, validates libraries during installation, and links unresolved calls.

---

## Building the Compiler

The project uses **Nix** for package management and reproducible builds:

```bash
# Build the project
nix build

# Install the compiler into the user profile
nix profile install .
```

Alternatively, the compiler can be built using standard Cargo:
```bash
cargo build --release
```

---

## Library Installation and Usage

`wand2c` features an integrated package manager for standard libraries (`libw`).

### System Library Installation
To validate and install libraries (e.g., `io`, `mem`, `string`, `std`) into the default directory (`~/.local/lib/libw` or specified via `WAND_LIB_PATH`):

```bash
# Install all libraries in the libw directory
- wand2c --install-library libw

or you can just:

- wand2c -il libw

# Install a single library module
- wand2c --install-library libw/io
```
*Before copying, the compiler validates the source code and headers of the target libraries.*

### Compiling Programs
To compile a WandC source file into an executable binary:

```bash
# Compile main.w with an auto-generated output name
wand2c main.w

# Compile with a custom output filepath
wand2c main.w -o my_program
```

---

## Memory Allocation Runtime (mloc / mfree)

To prevent memory leaks under lightweight or bare-metal execution environments where a heavy user-space allocator is unavailable, the compiler implements a **size-prefix header**:

1.  **Allocation (`mloc`)**: When a block is requested, the compiler implicitly requests 8 additional bytes (`size + 8`) via the `sys_mmap` system call. The total size is stored in the first 8 bytes of the mapped block, and the pointer returned to the user code is offset by 8 bytes (`allocated_address + 8`).
2.  **Deallocation (`mfree`)**: When `mfree(ptr)` is called, the compiled assembly automatically subtracts 8 bytes from the pointer to locate the start of the block, loads the original size from the header, and passes the original address and size to the `sys_munmap` system call.

---

## Direct Assembly and Port I/O Support

`wand2c` targets native machine instructions and supports port mapping under x86 architectures through specialized operations. 
*   **Port instructions**: Statements like `inb`, `outb`, `inw`, `outw`, `inl`, and `outl` translate directly to hardware-level port operations, safely routing parameters through System V registers (`rcx` / `rdx` for port configuration, `rax` for value assignment).
*   **Inline NASM Blocks**: Hardware-specific contexts are handled via raw `::nasm::` blocks, where scope-defined variables can be safely loaded.

---

## Target Configurations and Packaging

```bash
# Custom system compilation runs can specify target ABI behaviors using system profiles
wand2c main.w -o file
```
The packaging pass appends section tables containing structural layouts of variables, allowing the loader to determine type sizes, pointer constraints, and structure alignment profiles dynamically during execution.
