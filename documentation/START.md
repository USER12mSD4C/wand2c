# WandC Getting Started Guide

This guide helps you start with WandC.
It shows the basic workflow and file structure.

---

## Environment Token

Every WandC file must start with an environment token.

Use `sc.true` for programs that run under an operating system.
Use `sc.false` for bare metal code.

`sc.true` requires a `main` function.
`sc.false` requires a `kmain` function.

Example:

```wandc
sc.true
```

---

## File Types

WandC uses two file types.

### Source Files (.w)

Source files contain executable code.
Use `.w` files for implementation.

Example:

```wandc
sc.true
#import <io>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    print_string("Hello, WandC!");
    print_char(10);
    return(0);
}
```

### Header Files (.wh)

Header files contain declarations.
Use `.wh` files for interfaces.

Header files declare:

- Function signatures
- Structure definitions
- Constants
- Enumerations

Example header:

```wandc
sc.true
#import <syscall>

fn file_open(u8* path, u64 flags, u64 mode) -> i64;
fn file_close(u64 fd) -> i64;

struct Config version 1 {
    u64 timeout version 1;
    u8* path version 1;
}
```

---

## Project Structure

Organize your project with separate header and source files.

### Single File Project

For small programs, use one `.w` file:

```text
wand2c program.w -o program -fp
```

### Multi-File Project

For larger projects, split code into modules:

```text
project/
    main.w
    utils.wh
    utils.w
    parser.wh
    parser.w
```

Compile with:

```text
wand2c main.w -o program -fp
```

The compiler loads `.wh` and `.w` dependencies.

---

## Import System

Use `#import` to include modules.

### System Libraries

Import system libraries with angle brackets:

```wandc
#import <io>
#import <mem>
#import <string>
#import <syscall>
```

### Local Modules

Import local modules with double quotes:

```wandc
#import "utils"
#import "parser"
```

The compiler searches for:

1. `module.wh`
2. `module.w`

Import paths must not contain file extensions.

Correct:

```wandc
#import <io>
#import "utils"
```

Incorrect:

```wandc
#import <io.w>
#import "utils.wh"
```

---

## Correct Practices

### Separate Interface and Implementation

Create a header file for the public API.

Header file:

```wandc
sc.true

export fn public_function(u64 value) -> u64;
```

Source file:

```wandc
sc.true

export fn public_function(u64 value) -> u64 {
    return(value * 2);
}
```

### Use Explicit Exports

Mark public functions with `export`:

```wandc
export fn visible_function() -> u64 {
    return(1);
}

fn internal_function() -> u64 {
    return(0);
}
```

### Initialize Memory Before Use

Call `mem_init` before `malloc`:

```wandc
sc.true
#import <mem>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    mem_init(1048576);
    u8* ptr = (u8*)malloc(256);
    if (ptr != null) {
        mfree(ptr);
    }
    return(0);
}
```

### Check Pointers for Null

Always verify allocation success:

```wandc
u8* buffer = (u8*)malloc(1024);
if (buffer == null) {
    print_string("Allocation failed");
    print_char(10);
    return(1);
}
```

### Free Allocated Memory

Free allocated memory before function exit:

```wandc
u8* data = (u8*)malloc(512);
if (data != null) {
    mfree(data);
}
```

---

## Incorrect Practices

### Do Not Put Implementation in Headers

Incorrect header:

```wandc
sc.true

fn calculate(u64 a, u64 b) -> u64 {
    return(a + b);
}
```

Correct header:

```wandc
sc.true

fn calculate(u64 a, u64 b) -> u64;
```

Correct source file:

```wandc
sc.true

fn calculate(u64 a, u64 b) -> u64 {
    return(a + b);
}
```

### Do Not Use Memory Without Initialization

Incorrect:

```wandc
sc.true
#import <mem>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u8* ptr = (u8*)malloc(256);
    return(0);
}
```

Correct:

```wandc
sc.true
#import <mem>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    mem_init(1048576);
    u8* ptr = (u8*)malloc(256);
    if (ptr != null) {
        mfree(ptr);
    }
    return(0);
}
```

### Do Not Forget to Free Memory

Incorrect:

```wandc
fn process_data() {
    u8* buffer = (u8*)malloc(1024);
}
```

Correct:

```wandc
fn process_data() {
    u8* buffer = (u8*)malloc(1024);
    if (buffer != null) {
        mfree(buffer);
    }
}
```

### Do Not Use Freed Pointers

Incorrect:

```wandc
u8* data = (u8*)malloc(512);
if (data != null) {
    mfree(data);
    data[0] = 42;
}
```

Correct:

```wandc
u8* data = (u8*)malloc(512);
if (data != null) {
    mfree(data);
    data = null;
}
```

---

## Compilation Flags

### Program Format (-fp)

Create a hosted executable file:

```text
wand2c main.w -o program -fp
```

### Object Format (-fo)

Create a relocatable object file:

```text
wand2c module.w -o module.o -fo
```

### Raw Format (-fr)

Create a flat binary image:

```text
wand2c kernel.w -o kernel.bin -fr
```

### Kernel Format (-fk)

Create a freestanding kernel image:

```text
wand2c kernel.w -o kernel.img -fk
```

### Wexp Format (-fw)

Create a dynamic execution module:

```text
wand2c module.w -o module.wexp -fw
```

---

## Standard Library

Install the standard library:

```text
wand2c -il libw
```

Available modules:

- `<io>`: Console and file I/O
- `<mem>`: Memory allocation
- `<string>`: String operations
- `<syscall>`: System calls
- `<args>`: Command-line arguments
- `<path>`: Path utilities
- `<fileio>`: File reader
- `<vector>`: Dynamic arrays
- `<unistd>`: Process utilities
- `<math>`: Floating-point math
- `<fpmath>`: Fixed-point math
- `<keyboard>`: Terminal input
- `<tui>`: Terminal UI
- `<std>`: Basic utilities

---

## Complete Example

Create a simple calculator.

File `calc.wh`:

```wandc
sc.true

fn add(u64 a, u64 b) -> u64;
fn subtract(u64 a, u64 b) -> u64;
```

File `calc.w`:

```wandc
sc.true

fn add(u64 a, u64 b) -> u64 {
    return(a + b);
}

fn subtract(u64 a, u64 b) -> u64 {
    return(a - b);
}
```

File `main.w`:

```wandc
sc.true
#import <io>
#import "calc"

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u64 result = add(10, 20);
    print_string("10 + 20 = ");
    print_number(result);
    print_char(10);

    result = subtract(50, 25);
    print_string("50 - 25 = ");
    print_number(result);
    print_char(10);

    return(0);
}
```

Compile and run:

```text
wand2c main.w -o calc -fp
./calc
```

Output:

```text
10 + 20 = 30
50 - 25 = 25
```

---

## Next Steps

1. Read `SYNTAX.md` for the language reference.
2. Read `STDLIB.md` for the standard library reference.
3. Read `EXAMPLES.md` for more code examples.
4. Read `STANDARD_46.md` for the binary interface specification.

---

## Summary

- Use `.w` files for implementation.
- Use `.wh` files for declarations.
- Import system libraries with `<module>`.
- Import local modules with `"module"`.
- Initialize memory before allocation.
- Check pointers for null.
- Free allocated memory.
- Use `export` to mark public functions.
