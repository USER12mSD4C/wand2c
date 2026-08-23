# WandC Code Examples

This document gives practical examples of WandC code.
Use these examples to learn the language syntax.

---

## Hello World

This example prints text to the console.

```wandc
sc.true

#import <io>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    print_string("Hello, WandC!\n");
    return(0);
}
```

---

## Variables and Control Flow

This example shows variables, `if`, and `while` loops.

```wandc
sc.true

#import <io>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u64 count = 0;
    i64 limit = 5;

    while (count < 5) {
        if (count == 3) {
            print_string("Three\n");
        }
        count = count + 1;
    }

    print_number(count);
    print_char(10);

    return(0);
}
```

---

## For Loops

This example shows a `for` loop.

```wandc
sc.true

#import <io>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    for (u64 i = 0; i < 10; i = i + 1) {
        print_number(i);
        print_char(32);
    }
    print_char(10);
    return(0);
}
```

---

## Pointers and Modifiers

WandC uses pointer modifiers to show data flow.
- `*i` reads data.
- `*o` writes data.
- `*io` reads and writes data.

```wandc
sc.true

#import <io>

fn add_ten(u64 value*io) {
    value = value + 10;
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u64 num = 5;
    add_ten(num*adr);
    print_number(num);
    print_char(10);
    return(0);
}
```

---

## Structures

This example defines a structure and changes its fields.

```wandc
sc.true

#import <io>

struct Point version 1 {
    i64 x version 1;
    i64 y version 1;
}

fn move_point(Point* p, i64 dx, i64 dy) {
    p->x = p->x + dx;
    p->y = p->y + dy;
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    Point pt;
    pt.x = 10;
    pt.y = 20;

    move_point(pt*adr, 5, 5);

    print_signed_number(pt.x);
    print_char(44);
    print_signed_number(pt.y);
    print_char(10);

    return(0);
}
```

---

## Memory Allocation

This example allocates heap memory and frees it.
You must call `mem_init` before you use `malloc`.

```wandc
sc.true

#import <io>
#import <mem>
#import <string>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    mem_init(1048576);

    u8* buffer = malloc(64);
    if (buffer != null) {
        strcpy(buffer, "Dynamic string");
        print_string(buffer);
        print_char(10);
        mfree(buffer);
    }

    return(0);
}
```

---

## File I/O

This example opens a file, reads data, and closes the file.

```wandc
sc.true

#import <io>
#import <syscall>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    i64 fd = file_open("/etc/hostname", O_RDONLY, 0);
    if (syscall_error(fd) == 1) {
        print_string("Cannot open file\n");
        return(1);
    }

    u8 buffer[256];
    i64 bytes = file_read(fd, buffer*adr, 255);
    if (bytes > 0) {
        buffer[bytes] = 0;
        print_string(buffer);
    }

    file_close(fd);
    return(0);
}
```

---

## Command-Line Arguments

This example reads arguments from the command line.

```wandc
sc.true

#import <io>
#import <args>
#import <string>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    if (argc < 2) {
        print_string("Usage: program <name>\n");
        return(1);
    }

    u8* name = get_arg(argv, 1);
    print_string("Hello, ");
    print_string(name);
    print_char(10);

    return(0);
}
```

---

## Global Sections

This example uses a global section to store state.

```wandc
sc.true

#import <io>

sect.counter
    u64 value = 0;
EOS

fn increment() {
    counter:value = counter:value + 1;
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    increment();
    increment();
    increment();

    print_number(counter:value);
    print_char(10);

    return(0);
}
```

---

## Match Statement

This example uses `match` to check multiple values.

```wandc
sc.true

#import <io>

fn check_state(u64 state) {
    match (state) {
        case 1 {
            print_string("State is one\n");
        }
        case 2 {
            print_string("State is two\n");
        }
        default {
            print_string("Unknown state\n");
        }
    }
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    check_state(2);
    return(0);
}
```

---

## Inline Assembly

This example uses inline assembly to read a CPU timestamp.

```wandc
sc.false

#import <io>

fn get_ticks() -> u64 {
    u64 low = 0;
    u64 high = 0;
    ::nasm::{
        rdtsc
        mov [low], eax
        mov [high], edx
    }
    return((high << 32) | low);
}

fn kmain() {
    u64 ticks = get_ticks();
}
```
