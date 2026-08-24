# WandC Code Examples

This document contains practical examples of WandC code.
Use these examples to learn the language syntax.

---

## Hello World

This example prints text to the console.

```
sc.true

#import <io>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    print_string("Hello, WandC!\n");
    return(0);
}
```

---

## Variables and Control Flow

This example shows variables, if statements, and while loops.

```
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

This example shows a for loop.

```
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
The `*i` modifier reads data.
The `*o` modifier writes data.
The `*io` modifier reads and writes data.

```
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

```
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

## Packed Structures

This example shows a packed structure with custom alignment.

```
sc.true

#import <io>

packed align(1) struct Packet {
    u8 type version 1;
    u16 length version 1;
    u32 data version 1;
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    Packet pkt;
    pkt.type = 1;
    pkt.length = 10;
    pkt.data = 0x12345678;
    
    print_number(sizeof(Packet));
    print_char(10);
    
    return(0);
}
```

---

## Unions

This example shows a union type.

```
sc.true

#import <io>

union Data version 1 {
    u64 as_u64 version 1;
    f64 as_f64 version 1;
    u8 bytes[8] version 1;
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    Data d;
    d.as_u64 = 0x1234567890ABCDEF;
    
    print_number(d.as_u64);
    print_char(10);
    
    return(0);
}
```

---

## Enums

This example shows enum usage with versioning.

```
sc.true

#import <io>

enum State version 1 {
    Idle = 0 version 1;
    Running = 1 version 1;
    Stopped = 2 version 1;
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    State s = State:Running;
    
    if (s == State:Running) {
        print_string("Running\n");
    }
    
    return(0);
}
```

---

## Typedef

This example shows type aliases.

```
sc.true

#import <io>

typedef u8[256] Buffer;
typedef i64 Result;

fn process(Buffer buf) -> Result {
    return(0);
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    Buffer b;
    Result r = process(b);
    print_signed_number(r);
    print_char(10);
    return(0);
}
```

---

## Memory Allocation

This example allocates heap memory and frees it.
You must call `mem_init` before you use `malloc`.

```
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

```
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

```
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

```
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

## Volatile and Atomic Variables

This example shows volatile variables for memory-mapped I/O.

```
sc.true

#import <io>

sect.hardware
    volatile u64 status = 0;
    volatile u8* mmio_base = 0;
EOS

fn check_status() -> u64 {
    return(hardware:status);
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    hardware:status = 1;
    u64 s = check_status();
    print_number(s);
    print_char(10);
    return(0);
}
```

---

## Match Statement

This example uses `match` to check multiple values.

```
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

## Multiple Return Values

This example shows functions that return multiple values.

```
sc.true

#import <io>

fn divmod(u64 a, u64 b) -> (u64, u64) {
    u64 quotient = a / b;
    u64 remainder = a % b;
    return(quotient, remainder);
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u64 q, r;
    [q, r] = divmod(17, 5);
    
    print_string("Quotient: ");
    print_number(q);
    print_char(10);
    print_string("Remainder: ");
    print_number(r);
    print_char(10);
    
    return(0);
}
```

---

## Compile-Time Reflection

This example shows compile-time type information.

```
sc.true

#import <io>

struct Config version 2 {
    u32 version version 1;
    u64 flags version 1;
    u8* name version 2;
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    print_string("Config size: ");
    print_number(sizeof(Config));
    print_char(10);
    
    print_string("Config alignment: ");
    print_number(alignof(Config));
    print_char(10);
    
    print_string("Config fields: ");
    print_number(fieldsof(Config));
    print_char(10);
    
    print_string("flags offset: ");
    print_number(offsetof(Config:flags));
    print_char(10);
    
    return(0);
}
```

---

## Atomic Operations

This example shows atomic operations for thread safety.

```
sc.true

#import <io>

sect.shared
    atomic u64 counter = 0;
EOS

fn increment_atomic() {
    atomic_add(shared:counter*adr, 1);
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    increment_atomic();
    increment_atomic();
    increment_atomic();
    
    u64 val = atomic_load(shared:counter*adr);
    print_number(val);
    print_char(10);
    
    return(0);
}
```

---

## Inline Assembly

This example uses inline assembly to read a CPU timestamp.

```
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

---

## Critical Sections (Bare Metal)

This example shows critical sections for interrupt-safe code.

```
sc.false

sect.kernel
    u64 tick_count = 0;
EOS

fn update_ticks() {
    critical {
        kernel:tick_count = kernel:tick_count + 1;
    }
}

fn kmain() {
    update_ticks();
}
```

---

## IRQ Handlers

This example shows interrupt handler functions.

```
sc.false

irq fn timer_interrupt() {
    // Handle timer interrupt
}

fn kmain() {
    // Setup interrupt vector to point to timer_interrupt
}
```

---

## Export Functions

This example shows exported functions for modules.

```
sc.true

#import <io>

export fn public_api(u64 value) -> u64 {
    return(value * 2);
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u64 result = public_api(21);
    print_number(result);
    print_char(10);
    return(0);
}
```

---

## Extern Functions

This example shows external function declarations.

```
sc.true

#import <io>

extern fn external_lib_func(u64 a, u64 b) -> u64;

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    print_string("External function declared\n");
    return(0);
}
```
