# WandC Getting Started Guide

This guide helps you start with WandC.
You will learn the basic workflow and file structure.

---

## File Types

WandC uses two file types.

### Source Files (.w)

Source files contain executable code.
Use `.w` files for implementation.

Example structure:

```
sc.true

#import <io>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    print_string("Hello, WandC!\n");
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

```
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

```
wand2c program.w -o program -fp
```

### Multi-File Project

For larger projects, split code into modules:

```
project/
├── main.w          (main program)
├── utils.wh        (utility declarations)
├── utils.w         (utility implementation)
├── parser.wh       (parser declarations)
└── parser.w        (parser implementation)
```

Compile with:

```
wand2c main.w -o program -fp
```

The compiler automatically loads `.wh` and `.w` dependencies.

---

## Import System

Use `#import` to include modules.

### System Libraries

Import system libraries with angle brackets:

```
#import <io>
#import <mem>
#import <string>
#import <syscall>
```

### Local Modules

Import local modules with double quotes:

```
#import "utils"
#import "parser"
```

The compiler searches for:
1. `module.wh` (header)
2. `module.w` (source)

---

## Correct Practices

### Do: Separate Interface and Implementation

Create a header file for public API:

```
// api.wh
sc.true

export fn public_function(u64 value) -> u64;
```

Implement in source file:

```
// api.w
sc.true

export fn public_function(u64 value) -> u64 {
    return(value * 2);
}
```

### Do: Use Explicit Exports

Mark public functions with `export`:

```
export fn visible_function() -> u64 {
    return(1);
}

fn internal_function() -> u64 {
    return(0);
}
```

### Do: Initialize Memory Before Use

Always call `mem_init` before `malloc`:

```
#import <mem>

fn main() -> u64 {
    mem_init(1048576);
    u8* ptr = malloc(256);
    mfree(ptr);
    return(0);
}
```

### Do: Check Pointers for Null

Always verify allocation success:

```
u8* buffer = malloc(1024);
if (buffer == null) {
    print_string("Allocation failed\n");
    return(1);
}
```

### Do: Free Allocated Memory

Always free memory before function exit:

```
u8* data = malloc(512);
// Use data
mfree(data);
```

---

## Incorrect Practices

### Do Not: Put Implementation in Headers

Incorrect:

```
// bad.wh
sc.true

fn calculate(u64 a, u64 b) -> u64 {
    return(a + b);  // Wrong: implementation in header
}
```

Correct:

```
// good.wh
sc.true

fn calculate(u64 a, u64 b) -> u64;
```

```
// good.w
sc.true

fn calculate(u64 a, u64 b) -> u64 {
    return(a + b);
}
```

### Do Not: Use Memory Without Initialization

Incorrect:

```
#import <mem>

fn main() -> u64 {
    u8* ptr = malloc(256);  // Wrong: mem_init not called
    return(0);
}
```

### Do Not: Forget to Free Memory

Incorrect:

```
fn process_data() {
    u8* buffer = malloc(1024);
    // Use buffer
    // Wrong: buffer never freed
}
```

### Do Not: Use Freed Pointers

Incorrect:

```
u8* data = malloc(512);
mfree(data);
data[0] = 42;  // Wrong: use-after-free
```

---

## Compilation Flags

### Program Format (-fp)

Creates a hosted executable file:

```
wand2c main.w -o program -fp
```

### Object Format (-fo)

Creates a relocatable ELF object:

```
wand2c module.w -o module.o -fo
```

### Raw Format (-fr)

Creates a flat binary image:

```
wand2c kernel.w -o kernel.bin -fr
```

### Kernel Format (-fk)

Creates a freestanding kernel or kernel module image:

```
wand2c kernel.w -o kernel.img -fk
```

### Wexp Format (-fw)

Creates a dynamic execution module:

```
wand2c module.w -o module.wexp -fw
```

---

## Standard Library

Install the standard library:

```
wand2c -il libw
```

Available modules:
- `<io>` - Console and file I/O
- `<mem>` - Memory allocation
- `<string>` - String operations
- `<syscall>` - System calls
- `<args>` - Command-line arguments
- `<path>` - Path utilities
- `<fileio>` - File reader
- `<vector>` - Dynamic arrays
- `<unistd>` - Process utilities
- `<math>` - Floating-point math
- `<fpmath>` - Fixed-point math
- `<keyboard>` - Terminal input
- `<tui>` - Terminal UI
- `<std>` - Basic utilities

---

## Complete Example

Create a simple calculator:

```
// calc.wh
sc.true

fn add(u64 a, u64 b) -> u64;
fn subtract(u64 a, u64 b) -> u64;
```

```
// calc.w
sc.true

fn add(u64 a, u64 b) -> u64 {
    return(a + b);
}

fn subtract(u64 a, u64 b) -> u64 {
    return(a - b);
}
```

```
// main.w
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

```
wand2c main.w -o calc -fp
./calc
```

Output:

```
10 + 20 = 30
50 - 25 = 25
```

---

## Next Steps

1. Read SYNTAX.md for complete language reference
2. Read STDLIB.md for standard library functions
3. Read EXAMPLES.md for more code examples
4. Read standart-46.md for binary interface specification

---

## Summary

- Use `.w` files for implementation
- Use `.wh` files for declarations
- Import system libraries with `<module>`
- Import local modules with `<module>`
- Always initialize memory before allocation
- Always check pointers for null
- Always free allocated memory
- Use `export` to mark public functions
