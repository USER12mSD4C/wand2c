# WandC agent reference (compact)

Authoritative source: `SYNTAX.md`. This file is a dense, self-contained index for AI agents: syntax, semantics, flags, validation, and known sharp edges. No new syntax is defined here.

## 1. Program model

| File | Role |
|---|---|
| `.w` | implementation |
| `.wh` | declarations / interfaces |

Every file starts with one environment token:

```wand
sc.true                         // hosted; requires main; syscalls allowed; critical forbidden
// sc.false                     // freestanding; main by default, --entry kmain for kmain; critical/irq allowed
```

Imports:

```wand
#import <io>                    // system module: logical name, no extension or path separator
#import "utils"                 // local module: quoted, no extension or path separator
```

Skeleton:

```wand
sc.true
#import <io>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    return(0);
}
```

Comments are `//`. Identifiers start with a letter or `_`, then letters/digits/`_`. Keywords are reserved.

## 2. Literals and primitives

- Integers: `4096`, `0x1000`; floats: `3.14159`; strings: `"hello"` (read-only bytes); null pointer: `null`.
- Primitives: `u8 u16 u32 u64 i8 i16 i32 i64 f64 void`. Narrow signed values sign-extend when promoted to 64 bits.

## 3. Pointers, arrays, addresses

```wand
u8* p;                          // one level only; u8** is forbidden
u8 buffer[256];                 // size must be compile-time constant
u8 data[4] = { 1, 2, 3, 4 };
u64 x = 0;
u64* px = x*adr;                // explicit address-of; arrays decay to first element
fn f(u8* data, u64 n) -> u64;   // pass buffer*adr for arrays
```

Pointer arithmetic is allowed; bounds and alignment are the programmer's job.

Pointer data-flow modifiers on parameters/variables:

| Modifier | Meaning |
|---|---|
| `*i` | read |
| `*o` | write |
| `*io` | read and write |

```wand
fn set(u64* out*o) { out = 1; }
u64 n = 0;
set(n*adr);
```

In value context, `*i` and `*io` read the pointee. An `*i` **local** used as a
pointer walker keeps its address in pointer arithmetic, comparisons, and bare
truthiness, so `p = p + 1`, `p != end`, `if (p)`, and
`match (p) { case end ... }` advance/compare/test addresses rather than pointee
bytes. Copying a pointer into a plain pointer variable preserves the address.

Do not combine `volatile` or `atomic` with `*i/*o/*io`.

## 4. User types

```wand
const MAX = 256;                // compile-time only; no calls or runtime variables

enum State version 1 {
    Idle = 0 version 1;
    Running = 1 version 1;
}
State s = State:Running;        // Enum:Value access; versions default to 1

struct Point version 1 {        // fields may be volatile/atomic
    i64 x version 1;
    i64 y version 1;
}
Point pt; pt.x = 10; pt.y = 20;
fn move(Point* p, i64 dx) { p->x = p->x + dx; }

packed struct Header version 1 { u32 magic version 1; u16 len version 1; }
align(1) packed struct Packet version 1 { u8 kind version 1; u16 len version 1; }

union Data version 1 { u64 as_u64 version 1; f64 as_f64 version 1; }

typedef u8[256] Buffer;
typedef i64 Result;
```

`align(N)` must be a power of two and appears before `packed`.

## 5. Operators and casts

Arithmetic `+ - * / %`; bitwise `& | ^ ~ << >>`; comparisons `== != < <= > >=`; logical `&& ||`; assignment `= += -= *= /= %= &= |= ^= <<= >>=`; postfix `x++` and `x--`.

All narrowing/conversion boundaries should be explicit:

```wand
u8 small = (u8)big;
f64 f = (f64)int_val;
u64 addr = (u64)p;
u8* p2 = (u8*)addr;
```

The language forbids implicit type conversions; use explicit casts at every narrowing or widening boundary. Do not rely on compiler tolerance for implicit narrowing.

For `f64* x*i`, `x` names the `f64` object when an f64 value is required, so `x + 1.0` and `x + x` read the pointee. It remains a pointer in address context: `y = x` copies the address, `x != end` compares addresses, and `x = x + 8` advances the pointer. Pointer casts are address-context uses too: `(u64)x`, `(f64)x`, and `(u8*)x` start from the address, not the pointee.

## 6. Control flow

```wand
if (x == 1) { } else if (x == 2) { } else { }
while (x < 10) { x++; }
for (u64 i = 0; i < 10; i++) { }
match (state) {
    case 1 { }                  // no fallthrough
    default { }                 // optional, at most one
}
break; continue;
```

## 7. Functions

```wand
fn add(u64 a, u64 b) -> u64 { return(a + b); }
fn stop() { }                   // bare return == return(0) in u64 functions
fn decl(u64 a) -> u64;          // body-less declaration for .wh files

fn divmod(u64 a, u64 b) -> (u64, u64) { return(a / b, a % b); }
u64 q; u64 r;
[q, r] = divmod(17, 5);

export fn api(u64 x) -> u64;    // visible to other modules
extern fn other(u64 x) -> u64;  // defined elsewhere
```

A pointer- or array-typed return slot preserves pointer identity; scalar return
slots read through `*i`/`*io` value context as usual.

Function `version N` suffixes are parsed but not implemented for ABI versioning; two definitions differing only in version collide. Struct/union `versionof` works.

## 8. Global sections and concurrency

```wand
sect.state
u64 ticks = 0;
EOS

align(4096) ro sect.config
u64 magic = 0x1234;
EOS

noinit sect.bss
u8 work[1024];
EOS

state:ticks = state:ticks + 1;
u64* p = state:ticks*adr;
```

Modifiers: `align(N)`, `ro`, `noinit`, combinable before `sect`.

```wand
volatile u64 status;            // MMIO; not optimized away
atomic u64 counter;             // concurrent state
atomic_add(shared:counter*adr, 1);
```

Atomic builtins: `atomic_load/store/add/sub/inc/dec/swap/cas`, `memory_barrier()`, `compiler_barrier()`. RMW builtins return the old value except `inc`/`dec`, which return the new value. Atomic width follows the pointee type (`u8/u16/u32/u64`).

Raw MMIO builtins: `mem_load_*`, `mem_store_*`, `volatile_load_*`, `volatile_store_*` for `u8/u16/u32/u64` and `i8/i16/i32/i64`. Loads zero/sign-extend to 64 bits; stores truncate. `volatile_*` adds fences before and after.

`critical { ... }` disables interrupts and requires `sc.false`. `irq fn handler() { ... }` also requires `sc.false`; the compiler saves/restores registers.

## 9. Inline assembly and jmpto

```wand
::nasm::{
    pause
    rdtsc
    mov [low], eax
}
```

This is a tiny built-in encoder, **not** NASM. Supported: `pause hlt nop rdtsc mfence lfence sfence cli sti cld syscall ret push rbp pop rbp rep movsb rep stosb`, `mov reg,reg`, `mov [local],reg`, `mov reg,[local]`, `mov reg,imm`. Unknown lines are ignored with a warning. The compiler does not save/restore registers around the block; use locals for values.

```wand
jmpto "worker.wexp" {
    u64 task_id = 42;
    return(result);
}
```

`jmpto` calls a separate `.wexp` module through `__wand_jmpto_loader` (register with `wand_p46_register_path()`); up to six prepared argument values and four result values. Source-available modules may be inlined.

## 10. Reflection

`sizeof(Type)`, `alignof(Type)`, `fieldsof(Type)`, `offsetof(Type:field)`, `versionof(Type)`, `nameof(Type)`.

## 11. Safety analysis (what the checker enforces)

The analyzer is conservative. It reports:

- use of uninitialized variables, uninitialized struct fields, and uninitialized pointer-array elements;
- null or unchecked-allocation pointer dereferences, including literal `null` / numeric-zero aggregate leaves and dynamically collapsed array element states;
- use-after-free (`Freed` is the strongest collapsed state);
- probable leaks and freeing a possibly-null pointer.

Important behaviors for agent-authored code:

- arrays are writable storage, not pointers; indexing an uninitialized array is not itself an error;
- constant array elements keep distinct state; dynamic indices collapse to the strongest shared element state;
- explicit null checks refine a pointer on the taken/fallthrough branch (tracked as a checked-pointer state);
- `mfree(p)` requires `p` to be initialized; re-check before reuse;
- scalar stores to pointer-array elements update that element's pointer state.

The programmer is still responsible for bounds, alignment, pointer arithmetic, inline-asm effects, and allocator correctness.

## 12. Optimizer

The AST optimizer folds constants, propagates constants/immutable copies, removes dead variables/stores, simplifies algebra, strength-reduces unsigned division/modulo by powers of two, folds nested shifts, and removes unreachable branches. `x * 0`, `x - x`, `x ^ x`, and `x & 0` simplify only when `x` has no observable effects.

Barriers/observability: `volatile`, `atomic`, inline assembly, calls, and memory loads are not dropped or duplicated. Signed division/modulo are not strength-reduced.

## 13. Compiler flags

```bash
wand2c main.w -o program  -fp              # hosted program
wand2c mod.w   -o mod.o   -fo              # relocatable object
wand2c boot.w  -o boot.bin -fr --entry NAME # flat raw binary
wand2c kern.w  -o kern.img -fk --entry NAME # kernel image
wand2c svc.w   -o svc.wexp -fw              # Standard 4/6 continuation module
wand2c -il libw/llibw.86                    # install standard library
```

The real format flags are `-fp`, `-fo`, `-fr`, `-fk`, `-fw`. **There is no `-fb`.** `sc.false` sources need `-fo`/`-fw`; hosted `-fp` needs `sc.true`.

`.wexp` is a flat Standard 4/6 Raw Binary continuation blob, not an executable. Section `100` is `P46_SECTION_WEXP_IMAGE`, section `101` is `P46_SECTION_WEXP_RELOCS`; export addresses are module-relative and the loader applies relocations. `jmpto` uses a non-executable loader library.

Hosted programs use the default/`llibw.86` library. Do not set `WAND_LIB_PATH` to `ulibw.86` for `printf` programs; that produces `checker error: call to undeclared function 'printf'`. `printf`/`snprintf` accept up to eight argument values; unused slots are ignored.

## 14. Validation for agents

```bash
CARGO_BUILD_JOBS=1 cargo build --release -j 1
CARGO_BUILD_JOBS=1 cargo test --release -j 1
WAND2C=/path/to/wand2c python3 bench.py verify
WAND2C=/path/to/wand2c WAND2C_CHECKOUT=/path/to/checkout sh probes/wexp/run_all.sh
```

Checksums must agree across Wand/C/Rust before interpreting timings. C reference builds use `-ffp-contract=off`; do not use `-march=native`. Reference checksums:

| Task | Checksum |
|---|---:|
| integer_loop | 4960954624949299676 |
| vector_add | 2078837758754809018 |
| matrix_mul | 17773690160839997510 |
| sieve | 8198169831448681064 |
| quicksort | 616315348270837778 |
| fibonacci | 7004899911075702360 |
| nbody | 10885431617142060451 |
| hash_bytes | 13272412666920075123 |
| monte_carlo | 7325106639165441300 |
| fixed_dot | 4591103294081032387 |

## 15. Sharp edges

1. A single-element struct array whose first field is an aggregate and whose first initializer is a brace/string is an intentional **FIELD LIST**; it is not C brace elision.
2. `u8**` and other multi-level pointers are forbidden.
3. Import paths contain no extensions or path separators.
4. `align(N)` requires a power of two; `critical`/`irq` require `sc.false`; syscalls require `sc.true`.
5. String literals are read-only.
6. Inline assembly is a limited encoder, and unknown lines are ignored.
7. Optimization is internal; Wand syntax must not change when fixing the compiler.
