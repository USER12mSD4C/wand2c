# wand2c

wand2c compiles Wand source files to native x86_64 code.

## Overview

Wand is a systems programming language.
wand2c is the compiler for Wand.
The compiler emits machine code from an in-tree x86_64 backend.

wand2c writes these outputs:

- ELF64 executables with `-fp`.
- Relocatable ELF64 objects with `-fo`.
- Flat binaries with `-fr` or `-fk`.
- `.wexp` continuation modules with `-fw`.

A `.wexp` file is a flat Standard 4/6 image called a continuation module.
A loader maps the image and calls its `main` function.
Standard 4/6 is a binary interface for typed and versioned modules.
ELF and `.wexp` outputs carry Standard 4/6 metadata.
Read [standart-46.md](documentation/standart-46.md) for the binary layout.

## Documentation

| Need | Document |
|---|---|
| Tutorial and runnable concepts | [LEARN.md](documentation/LEARN.md) |
| Document map | [documentation/README.md](documentation/README.md) |
| Command and syntax index | [CHEATSHEET.md](documentation/CHEATSHEET.md) |
| Authoritative language rules | [SYNTAX.md](documentation/SYNTAX.md) |
| Standard library | [STDLIB.md](documentation/STDLIB.md) |
| Example catalog | [EXAMPLES.md](documentation/EXAMPLES.md) |
| Errors and known limits | [TROUBLESHOOTING.md](documentation/TROUBLESHOOTING.md) |
| Binary interface | [standart-46.md](documentation/standart-46.md) |

## Requirements

| Item | Requirement |
|---|---|
| Target platform | Linux on x86_64 |
| Build tools | Cargo and a Rust toolchain with edition 2021 support |
| C compiler | Required by the `cc` build script for the C backend |
| Optional Nix shell | The flake provides `cargo`, `rustc`, and `nasm` |

The compiler emits x86_64 machine code.
The package definition declares Linux support.

## Build and install

Build the compiler from the repository root:

```bash
cargo build --release
```

The binary is at `target/release/wand2c`.

Install the compiler with the repository script:

```bash
./install.sh
```

The script runs `cargo build --release`.
The script then runs `sudo install target/release/wand2c /usr/bin/`.
The script installs the binary at `/usr/bin/wand2c`.
The script does not install the standard library.

Install the standard library separately:

```bash
target/release/wand2c -il libw/llibw.86
```

Run this command from the repository root.
The install flag must be the first argument.
Use `--install-library` in place of `-il`.
The tool validates `.w` and `.wh` files before copying them.
The tool copies files to `$HOME/.local/lib/libw/`.
The tool uses `$WAND_LIB_PATH` when that variable is set.
Directory installs are not recursive.

## Quick start

1. Build the compiler and install the standard library as shown above.
2. Create `hello.w` with this text:

   ```wand
   sc.true
   #import <io>

   fn main(u64 argc, u64 argv, u64 envp) -> u64 {
       print_string("Hello, Wand!\n");
       return(0);
   }
   ```

3. Compile the file:

   ```bash
   target/release/wand2c hello.w -o hello -fp
   ```

4. Run the program:

   ```bash
   ./hello
   ```

The program prints `Hello, Wand!`.
The program exits with status 0.

## Command line

Use this form for compilation:

```text
wand2c <input_files> [options]
```

Put `--install-library` or `-il` first when you install a library.

| Option | Meaning |
|---|---|
| `-o <file>` | Set the output path. |
| `-f <name>` | Set the output format by name. |
| `-f=<name>` | Set the output format by name. |
| `--format <name>` | Set the output format by name. |
| `--format=<name>` | Set the output format by name. |
| `-fp` | Use the `program` format. |
| `-fo` | Use the `object` format. |
| `-fr` | Use the `raw` format. |
| `-fk` | Use the `kernel` format. |
| `-fw` | Use the `wexp` format. |
| `--entry <name>` | Set the entry function for `raw` or `kernel`. |
| `-v`, `--verbose` | Print compiler stages. |
| `-il <path>`, `--install-library <path>` | Install one library or a library directory. |

Valid format names are `program`, `object`, `raw`, `kernel`, and `wexp`.
You must not combine two different format options.

## Output formats

| Option | Format | Output | Requirements |
|---|---|---|---|
| `-fp` | `program` | Hosted ELF64 executable | `sc.true` and `main` |
| `-fo` | `object` | Relocatable ELF64 object | None |
| `-fr` | `raw` | Flat binary without an ELF header | `sc.false`, optional `--entry` |
| `-fk` | `kernel` | Freestanding flat binary | `sc.false`, optional `--entry` |
| `-fw` | `wexp` | Flat Standard 4/6 continuation module | `main`, a loader at run time |

Raw and kernel formats use `main` as the default entry.
Use `--entry <name>` to select another function.

## Language outline

Wand uses `.w` files for implementation and `.wh` files for declarations.
Every source file must start with `sc.true` or `sc.false`.
Use `sc.true` for hosted code.
Use `sc.false` for freestanding code.
Hosted code runs under an operating system.
Freestanding code runs without an operating system.
Use `#import <name>` for system modules.
Use `#import "name"` for local modules.
Import paths must not include file extensions.
Local imports resolve from the current working directory.

| Topic | Terms | Reference |
|---|---|---|
| Environment | `sc.true`, `sc.false` | [SYNTAX.md, section 2](documentation/SYNTAX.md#2-environment-token) |
| Functions | `fn`, `export`, `extern`, multiple results | [SYNTAX.md, sections 21 and 22](documentation/SYNTAX.md#21-functions) |
| Types | `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, `i64`, `f64`, `void` | [SYNTAX.md, section 7](documentation/SYNTAX.md#7-primitive-types) |
| Arrays | `u8 buf[32]`, brace initializers | [SYNTAX.md, section 9](documentation/SYNTAX.md#9-arrays) |
| User types | `struct`, `enum`, `union`, `typedef`, `version` | [SYNTAX.md, section 12](documentation/SYNTAX.md#12-structures) |
| Pointer data flow | `*i`, `*o`, `*io` | [SYNTAX.md, section 16](documentation/SYNTAX.md#16-pointer-modifiers) |
| Addresses | `*adr` | [SYNTAX.md, section 17](documentation/SYNTAX.md#17-the-adr-operator) |
| Sections | `sect`, `EOS`, `section:variable`, `volatile`, `atomic` | [SYNTAX.md, sections 23 to 25](documentation/SYNTAX.md#23-global-sections) |
| Reflection | `sizeof`, `alignof`, `offsetof`, `fieldsof`, `versionof`, `nameof` | [SYNTAX.md, section 30](documentation/SYNTAX.md#30-compile-time-reflection) |
| Raw memory | `mem_load_u32`, `mem_store_u32`, `volatile_load_u32`, `volatile_store_u32` | [SYNTAX.md, section 25](documentation/SYNTAX.md#25-atomic-variables) |
| Atomics | `atomic_load`, `atomic_store`, `atomic_add`, `atomic_cas`, `memory_barrier` | [SYNTAX.md, section 25](documentation/SYNTAX.md#25-atomic-variables) |
| Hardware | `inb`, `outb`, `critical`, `irq`, `::nasm::{}` | [SYNTAX.md, sections 26 to 28](documentation/SYNTAX.md#26-critical-sections) |
| Continuations | `jmpto`, `.wexp`, `wand_p46_load` | [SYNTAX.md, section 29](documentation/SYNTAX.md#29-jmpto-statement) |

The [loader API](include/wand_p46.h) declares `wand_p46_load` and `wand_p46_call`.
Build the loader library with `make -C loader`.
Use [LEARN.md](documentation/LEARN.md) for a tutorial.
Use [SYNTAX.md](documentation/SYNTAX.md) for all language rules.

## Testing

Run all Cargo tests from the repository root:

```bash
cargo test --release
```

The regression probes and benchmark suite live outside this checkout.
The paths below assume this checkout stays at `/home/user12ms/Projects/wand2c`.
Run one probe with the compiler built above:

```bash
(cd ../../experiment/probes && /home/user12ms/Projects/wand2c/target/release/wand2c f64_cast.w -o /tmp/f64_cast_wand -fp && /tmp/f64_cast_wand)
```

Run the continuation probes with the same compiler:

```bash
(cd ../../experiment/probes/wexp && WAND2C_CHECKOUT=/home/user12ms/Projects/wand2c ./run_all.sh)
```

Run the benchmark suite:

```bash
(cd ../../experiment && python3 bench.py all --reps 5)
```

Set `WAND2C` when `wand2c` is not on `PATH`.

## Known limits

- Multi-level pointers are not allowed.
- The language requires explicit casts at narrowing boundaries.
- The checker accepts some integer narrowing and truncates the value.
- Array sizes must be compile-time constants.
- System imports must be logical names without extensions or path separators.
- Local imports resolve from the current working directory.
- `critical` and `irq` require `sc.false`.
- System calls require `sc.true`.
- `::nasm::{}` supports a small instruction set. The compiler warns and drops unknown lines.
- Wand has no raw pointer dereference expression. Use raw-memory builtins.
- A `.wexp` file does not run directly on Linux. Use a Standard 4/6 loader.
- `jmpto` passes at most six arguments and receives at most four results.
- Function `version` suffixes do not change exported symbol names.

Read [TROUBLESHOOTING.md](documentation/TROUBLESHOOTING.md) for the complete list.

## Development

You must pass all tests before you merge or install a change.
Run the commands in [Testing](#testing) before you merge.
Use [SYNTAX.md](documentation/SYNTAX.md) for language rules.

## Editor support

The [Zed extension](zed-wand/README.md) adds `.w` and `.wh` support.
Install it in Zed with `zed: install dev extension` and select `zed-wand`.
