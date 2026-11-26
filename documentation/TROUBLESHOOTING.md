# Troubleshooting

This page lists common compiler errors, runtime failures, and environment problems. For language rules, use [SYNTAX.md](SYNTAX.md). For guided examples, use [LEARN.md](LEARN.md).

## Build environment

### `wand2c: command not found`

Use the binary from the repository root:

```bash
target/release/wand2c FILE.w -o OUT -fp
```

Or install it:

```bash
cargo build --release
sudo cp target/release/wand2c /usr/local/bin/
```

### Missing standard library module

Install `libw`:

```bash
wand2c -il libw/llibw.86
```

Files are copied to:

```text
~/.local/lib/libw/
```

### Local import cannot be found

Local imports are resolved from the current working directory. Run the compiler from the directory that contains the imported `.w` or `.wh` file.

```bash
cd documentation/examples
../../target/release/wand2c numbers.w -o numbers -fp
```

## Compiler errors

### Program must start with `sc.true` or `sc.false`

The environment token must be the first token in every Wand source file.

```wand
sc.true
```

Use `sc.true` for hosted programs and `sc.false` for freestanding code.

### `[checker error] critical requires sc.false`

`critical { }` is only valid in freestanding code.

Replace `sc.true` with `sc.false`, compile as a kernel or raw image, and provide `main`, or pass `--entry kmain` to use `kmain`. Otherwise remove the `critical` block.

### `entry function 'main' not found`

A hosted program requires `main`:

```wand
fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    return(0);
}
```

A raw or kernel image uses `main` by default. Pass `--entry kmain` to select `kmain`, or another function name if required.

### `system import must be a logical module name without path separators or extensions`

System imports use logical names. They must not contain extensions or path separators.

```wand
#import <io>        // correct
#import <io.w>      // wrong
```

Quoted local imports may contain relative subdirectories. They must not contain a file extension.

```wand
#import "parser"      // correct
#import "sub/parser"  // correct when the path exists
#import "parser.w"    // wrong
```

### `multi-level pointers are not allowed in WandC; use a single pointer and pass array addresses with *adr`

Wand allows one pointer level.

```wand
u8 buffer[16];
u8* p = buffer*adr;
```

`u8** p;` is not valid.

### `program format requires sc.true.`

`-fp` requires a hosted source file. Change the token to `sc.true` or compile with `-fk`/`-fr`.

### `kernel format requires sc.false.`

`-fk` requires a freestanding source file. Change the token to `sc.false` or compile with `-fp`.

### `--entry is not allowed for program or wexp format`

`--entry` is valid only with `-fr` and `-fk`. Hosted programs and wexp modules use `main`.

### Function `version N` has no effect

Function versions are recorded in the exports and type records of `-fw`
continuation objects, and `version N` is accepted before or after the return
type. They do not change the exported symbol name: `fn add version 3` and
`fn add version 4` still collide. Use distinct names for different exports.

### `warning: unsupported ::nasm:: line ignored`

`::nasm::{}` is a small built-in encoder, not a full NASM assembler. It only
knows the instructions listed in `SYNTAX.md` section 28. Unknown lines emit no
bytes, so a program may build and then misbehave. Check compiler warnings and
replace unsupported instructions with Wand builtins (`atomic_*`, `inb/outb`,
`memory_barrier()`) or a supported `mov` form through a local variable.

### `Unexpected primary expression` on `*(u32*)addr`

Wand has no raw pointer dereference expression. Use the raw-memory builtins
instead: `mem_load_u32(addr)`, `mem_store_u32(addr, val)`, and the
`volatile_load_*` / `volatile_store_*` forms for MMIO registers. They take a
`u64` address and emit exact-width x86 accesses without a C shim. See
`SYNTAX.md`, section 25.

### `jmpto` returns 0 or leaves its result variable unchanged

If the worker source is not next to the host at compile time, `jmpto` uses the
dynamic loader. Register the module before calling it:

```c
wand_p46_register_path("worker.wexp", "/path/to/worker.wexp", &options);
```

Without a registration (or with more than six arguments / four results) the
loader returns 0. If the worker source *is* present, the body is inlined and
the optimizer can fold reads of locals written only by the inlined code; use
the dynamic path, or a section global, when the caller needs the result.

### Syntax errors

The compiler may report the line after the actual mistake. Check the previous line for:

- missing `;`
- unmatched `{` or `}`
- unmatched `(` or `)`
- missing `EOS`
- `=` used in a condition where `==` is required

## Runtime failures

### A `.wexp` binary will not run directly on Linux

`-fw` builds a continuation object, not a stand-alone program. It is a flat P46
Raw Binary image with no ELF header and no process entry stub, so a shell or the
Linux kernel cannot execute it. Use `-fp` for hosted executables, or load the
`.wexp` module through a Standard 4/6 loader (`include/wand_p46.h`).

### Number printing crashes

Some installed copies of `libw/io.w` declare buffers as:

```wand
u8[32] buf;
```

That form can crash at runtime. Use the name-before-brackets form:

```wand
u8 buf[32];
```

The examples use the shared helper in `documentation/examples/numio.w`, which uses the correct form.

### `print_string` prints extra bytes

`print_string` reads until a zero byte. String literals already end in zero. A character array does not unless you add it:

```wand
u8 name[4] = { 'A', 'B', 'C', 0 };
print_string(name*adr);
```

### Implicit integer narrowing is accepted

The checker currently accepts assignments between some integer types without a cast. The value is truncated to the target width.

```wand
u64 big = 300;
u8 small = big; // compiles; small is 44
```

Use an explicit cast at every narrowing boundary. Explicit casts state the intended truncation and remain correct if checker behavior changes.

### Multiple return values give the same result (historical)

Fixed in the current compiler. Older builds assigned the last returned value
to every target because each returned expression clobbered `rax` before the
destructuring targets were populated. The regression probes
`probes/tuple_return.w` and `probes/tuple_return_multi.w` in the experiment
directory now produce `7 9` and `11 22 33` / `7 10 13`.

The old failure looked like this:

```wand
fn pair() -> (u64, u64) {
    return(7, 9);
}

u64 a;
u64 b;
[a, b] = pair(); // a = 9, b = 9
```

Older builds needed output pointers as a workaround; this is no longer
required:

```wand
fn pair(u64* a*o, u64* b*o) {
    a = 7;
    b = 9;
}

u64 a;
u64 b;
pair(a*adr, b*adr); // a = 7, b = 9
```

### A loop does not stop

Check that the loop variable changes and that the comparison is correct. Also check the difference between `<` and `<=`.

### Integer division result is unexpected

Wand integer division truncates. Use `%` for the remainder.

```wand
u64 q = 23 / 5; // 4
u64 r = 23 % 5; // 3
```

## Memory issues

### Null pointer dereference

Check allocation results before use:

```wand
u8* p = malloc(64);
if (p != null) {
    p[0] = 1;
    mfree(p);
}
```

### Use after free

Do not access a pointer after `mfree`:

```wand
mfree(p);
p[0] = 1; // wrong
```

### Possible leak

Free allocated memory before the function returns, or transfer ownership explicitly.

The compiler reports many unsafe patterns, but it cannot prove all pointer properties. Bounds, alignment, pointer arithmetic, and allocator discipline remain programmer responsibilities. See [SYNTAX.md, section 31](SYNTAX.md#31-memory-safety-analysis).

## Sections and globals

### Global variable is not visible

Section variables use `section:variable`.

```wand
sect.state
    u64 ticks = 0;
EOS
```

```wand
state:ticks = 1;
```

A section must end with `EOS`.

## QEMU kernel images

A raw `-fr` or `-fk` image is flat machine code. QEMU `-kernel` expects a bootable header or a suitable ELF image. Use a loader or a small trampoline to map the raw image and jump to the Wand entry function, commonly `kmain`.

The kernel logic can remain entirely in Wand. The trampoline is only a test harness.

## Zed editor

To enable `.w` and `.wh` recognition, install the dev extension in `zed-wand/`:

1. Open Extensions.
2. Run `zed: install dev extension`.
3. Select the `zed-wand` directory.

See [../zed-wand/README.md](../zed-wand/README.md).

## Reporting a problem

Include:

1. exact source
2. exact command
3. exact compiler output
4. expected result
5. actual result

Use the smallest file that reproduces the problem.
