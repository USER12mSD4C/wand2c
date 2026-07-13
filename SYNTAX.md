# WandC v2.0 Language & Syntax Specification

WandC is a systems programming language designed for bare-metal applications, custom kernel development, and hosted system utilities. It features direct hardware access, explicit stack frame layout control, pointer modifications on variable scopes, and strict conformance to the **Standard 4/6 Binary Interface Specification**.

---

## 1. Lexical and Grammar Fundamentals

### 1.1 Source Files and Execution Preambles
Every WandC file (`.w` or `.wexp`) must begin with an environment assertion token on its first non-comment line. This tells the parser whether hosted operating system interfaces are accessible:

*   `sc.true` — **Hosted Mode**: Enables high-level virtual allocations via system calls (`sys_mmap`, `sys_munmap`) and sets the runtime entry wrapper to pass arguments and collect exit codes.
*   `sc.false` — **Bare-Metal Mode**: Disables kernel system calls. Built-in functions like `mloc` and `mfree` are disabled or act as direct physical address mapping wrappers.

### 1.2 Comments
WandC supports only single-line comments. Multi-line comments are invalid.
```c
// This is a comment. It runs until the end of the line.
```

### 1.3 Literal Formats
*   **Integer Literals**: Decimal and hexadecimal notations are parsed directly into 64-bit unsigned representations by default:
    ```c
    u64 dec_val = 4096;
    u64 hex_val = 0x1000;
    ```
*   **String Literals**: Enclosed in double quotes. Backslash escape sequences (`\n`, `\t`, `\r`, `\"`) are escaped by the compiler and written directly to the read-only segment of the global data section.
    ```c
    string message = "Initialization successful\n";
    ```

---

## 2. Type System and Memory Alignment

WandC defines explicit primitive types, arrays, pointers, and user-defined aliases.

### 2.1 Primitive Types
Primitive types specify fixed physical bit widths independent of the target machine:

| Type | Size (Bytes) | Range / Interpretation |
|------|--------------|------------------------|
| `u8`  | 1            | Unsigned 8-bit integer |
| `u16` | 2            | Unsigned 16-bit integer |
| `u32` | 4            | Unsigned 32-bit integer |
| `u64` | 8            | Unsigned 64-bit integer |
| `i8`  | 1            | Signed 8-bit integer |
| `i16` | 2            | Signed 16-bit integer |
| `i32` | 4            | Signed 32-bit integer |
| `i64` | 8            | Signed 64-bit integer |
| `void`| 0            | Empty/Unmapped type |

### 2.2 Pointer Types
Pointer types can be represented using the postfix `*` type syntax:
```c
u8*  char_ptr;  // Pointer to a u8
u64* data_ptr;  // Pointer to a u64
```
A pointer occupies exactly `pointer_size` bytes (statically configured in the binary header; default is 8 bytes for 64-bit systems).

### 2.3 Array Declarations
Arrays represent contiguous allocations of a uniform base type. The array length must be a constant integer declared inside square brackets `[...]`.

There are two equivalent array declaration syntaxes:
1.  **C-Style brackets**:
    ```c
    u8 buffer[256]; // Allocates 256 bytes on the stack frame
    ```
2.  **Explicit `array` prefix notation**:
    ```c
    array:u8[256] buffer; // Identical allocation
    ```

### 2.4 User-Defined Aliases (`typedef`)
WandC supports type aliasing using the `typedef` keyword. Aliases are registered in the global type table and exported in the `.p46_types` SHT metadata section:

```c
typedef u8[256] SectorBuffer;
typedef Vector3D* PositionPointer;
```

---

## 3. Variable Declarations and Pointer Access Modifiers

WandC does not treat pointer dereferencing as an operator inside expressions. Instead, dereferencing constraints and behaviors are governed by postfix access modifiers applied to variable names on their stack frame scopes.

### 3.1 Pointer Modifiers (`*i` and `*o`)
*   **`*i` (Input/Read-Only Pointer)**: Tells the compiler that the pointer is used for reading.
*   **`*o` (Output/Write-Only Pointer)**: Tells the compiler that the pointer is used for writing.

```c
u64 ptr*i; // ptr is a stack variable holding the address of a u64 (Input access)
u64 ptr*o; // ptr is a stack variable holding the address of a u64 (Output access)
```

### 3.2 Evaluation Rules (LHS vs RHS)

The access modifier dictates how the compiler generates machine instructions depending on whether the variable is evaluated as a source (Right-Hand Side) or a destination (Left-Hand Side) of an expression:

#### Input Pointers (`*i`)
*   **Right-Hand Side (RHS / Reading)**: Evaluating `ptr` automatically generates a dereferenced load instruction.
    ```c
    y = ptr; // Generates: mov rax, [rbp - ptr_offset]; mov rax, [rax]; mov [rbp - y_offset], rax
    ```
*   **Left-Hand Side (LHS / Writing)**: Assigning a value to `ptr` modifies the **pointer itself** (the address stored on the stack).
    ```c
    ptr = ptr + 1; // Evaluates as: ptr_address_on_stack = ptr_address_on_stack + 1 (Pointer Arithmetic)
    ```

#### Output Pointers (`*o`)
*   **Right-Hand Side (RHS / Reading)**: Evaluating `ptr` reads the raw pointer address from the stack.
    ```c
    y = ptr; // Generates: mov rax, [rbp - ptr_offset]; mov [rbp - y_offset], rax
    ```
*   **Left-Hand Side (LHS / Writing)**: Assigning a value to `ptr` automatically dereferences the pointer, writing the RHS value directly to the memory address it points to.
    ```c
    ptr = 100; // Generates: mov rbx, [rbp - ptr_offset]; mov rax, 100; mov [rbx], rax (*ptr = 100)
    ```

### 3.3 Function Argument Passing Rules
When passing pointer-modified variables to functions, the compiler resolves automatic dereferencing by inspecting the destination parameter type in the target's signature:
*   If the target parameter is a **Pointer Type** (e.g., `u8*`), the argument is passed as a raw memory address (no auto-dereferencing occurs).
*   If the target parameter is a **Non-Pointer Type** (e.g., `u8`), the compiler automatically dereferences the input pointer argument first, passing its loaded value in the target register.

---

## 4. Control Flow and Statements

WandC enforces structured scopes. Body blocks of control flow structures must always be enclosed in braces `{}`.

### 4.1 Conditional Statements (`if` / `else`)
```c
if (x == y) {
    print_string("Equal\n");
} else {
    print_string("Not Equal\n");
}
```

### 4.2 Loops (`while` and `for`)
*   **`while` Loop**: Runs while the expression evaluates to non-zero.
    ```c
    u64 count = 0;
    while (count < 10) {
        print_number(count);
        count = count + 1;
    }
    ```
*   **`for` Loop**: Consists of an initialization statement, a condition expression, and a post-loop expression statement.
    ```c
    for (u64 i = 0; i < 100; i = i + 1) {
        print_number(i);
    }
    ```

### 4.3 Syntactic Constraints on Statement Operators
Operators like `++` (`Token::OpInc`) and `--` (`Token::OpDec`) are **only** valid as standalone expression statements (e.g., inside `for` post-actions or as a separate line). They cannot be nested inside larger assignments or expressions.
```c
i++;           // VALID
for (u64 i=0; i<10; i++) { ... } // VALID
u64 x = i++;   // SYNTAX ERROR
```

---

## 5. Functions and Multi-Value Returns

Functions in WandC support type inference for their return values and can return up to four values simultaneously.

### 5.1 Declaring Signatures
Return types are inferred from the function body and are not declared in the function signature header.

```c
fn process_packet(u8* data, u64 size) {
    // Body statements
}
```

### 5.2 Multi-Value Returns
To return multiple values, specify their types and values within a single `return` statement:
```c
fn get_limits() {
    return(u64 0, u64 1024, u64 2048);
}
```

### 5.3 Destructuring Assignments
To capture multiple values returned by a function call, use square brackets `[...]` enclosing the target destination variables on the left-hand side of the assignment:
```c
u64 min_val;
u64 mid_val;
u64 max_val;
[min_val, mid_val, max_val] = get_limits();
```

---

## 6. Structures and Memory Layout Rules

### 6.1 Structure Declarations
Structures are defined using the `struct` keyword, followed by an optional name and a version specifier for binary interface versioning.

```c
struct Vector version 1 {
    u32 x version 1;
    u32 y version 1;
    u64 z version 2;
}
```

### 6.2 Structural Alignment and Size Calculations
The compiler implements strict, architecture-independent layout calculation rules:
1.  Fields are laid out in their exact order of declaration.
2.  Each field is aligned to an offset that is a multiple of its size (up to 8 bytes).
3.  For fields larger than 8 bytes, the alignment constraint is clamped at 8 bytes.
4.  The total structural size is rounded up to the maximum alignment factor of its fields.

### 6.3 Instantiation and Field Access
*   **Direct Access (`.`)**: Used on standard instances of structures allocated on the stack.
    ```c
    Vector v;
    v.x = 10;
    ```
*   **Pointer Access (`->`)**: Used when accessing structure fields through an address pointer.
    ```c
    Vector* ptr = v*adr;
    ptr->z = 100;
    ```

---

## 7. Global Data Sections (`sect`)

Shared and global variables are grouped inside `sect` blocks. These variables are placed in the unit's global data segment. Functions cannot be declared inside `sect` blocks.

```c
sect.system_state
    u64 uptime_ticks = 0;
    u8  interrupt_level = 0;
EOS
```

Variables from a global section are referenced using the namespace colon `:` operator:
```c
system_state:uptime_ticks = system_state:uptime_ticks + 1;
```

---

## 8. Allocation Built-ins, System Calls, and Port I/O

WandC incorporates hardware operations, kernel memory interfaces, and native kernel system calls as compiler primitives.

### 8.1 Memory Allocations (`mloc`, `mfree`, `bmloc`)
*   `mloc(owner, size)`: Allocates `size + 8` bytes of virtual memory via `sys_mmap`. The compiler writes the total allocation size to the first 8 bytes of the block as metadata and returns the pointer offset by 8 bytes (`ptr + 8`) to prevent memory leaks during freeing.
*   `mfree(ptr)`: Reads the size prefix from `[ptr - 8]`, shifts the address back by 8 bytes, and deallocates the entire block using `sys_munmap`.
*   `bmloc(address, size)`: Designed for mapping physical addresses in bare-metal targets. Acts as a passive address evaluator in hosted modes.

```c
void* block = mloc(null, 2048);
mfree(block);
```

### 8.2 Port I/O Primitives
Under `sc.false` on x86 architectures, port I/O built-ins map directly to hardware assembly instructions:

```c
u8  val8  = inb(0x3F8);  // in al, dx
outb(0x3F8, val8);       // out dx, al

u16 val16 = inw(0x1F0);  // in ax, dx
outw(0x1F0, val16);      // out dx, ax

u32 val32 = inl(0xCF8);  // in eax, dx
outl(0xCF8, val32);      // out dx, eax
```

### 8.3 Compiler System Call Built-ins
When compiled under hosted mode (`sc.true`), WandC supports direct low-level kernel system call wrappers as first-class primitives. These are compiled directly into raw `syscall` instructions without any external runtime dependencies:

*   `sys_read(u64 fd, u8* buf, u64 size)`: Invokes the read system call (RAX = 0) with arguments mapped to RDI, RSI, RDX.
*   `sys_write(u64 fd, u8* buf, u64 size)`: Invokes the write system call (RAX = 1) with arguments mapped to RDI, RSI, RDX.
*   `sys_open(u8* path, u64 flags, u64 mode)`: Invokes the open system call (RAX = 2) with arguments mapped to RDI, RSI, RDX.
*   `sys_close(u64 fd)`: Invokes the close system call (RAX = 3) with the argument mapped to RDI.
*   `sys_unlink(u8* path)`: Invokes the unlink system call (RAX = 87) with the argument mapped to RDI.
*   `sys_ioctl(u64 fd, u64 req, u64 arg)`: Invokes the ioctl system call (RAX = 16) with arguments mapped to RDI, RSI, RDX.
*   `sys_exit(u64 code)`: Invokes the exit system call (RAX = 60) with the argument mapped to RDI.
*   `sys_fork()`: Invokes the fork system call (RAX = 57). Returns the child process PID in the parent process and 0 in the child process.
*   `sys_execve(u8* path, u64* argv, u64* envp)`: Invokes the execve system call (RAX = 59) with arguments mapped to RDI, RSI, RDX.
*   `sys_wait4(i64 pid, u32* wstatus, u64 options, u64* rusage)`: Invokes the wait4 system call (RAX = 61) with arguments mapped to RDI, RSI, RDX, RCX.

Example usage for library-level file handlers:
```c
fn file_open(u8* path, u64 flags, u64 mode) -> i64 {
    i64 fd = sys_open(path, flags, mode);
    return(fd);
}

fn file_close(u64 fd) -> i64 {
    i64 res = sys_close(fd);
    return(res);
}

fn file_remove(u8* path) -> i64 {
    i64 res = sys_unlink(path);
    return(res);
}
```

---

## 9. Dynamic Execution Modules (`.wexp`) and `jmpto`

WandC supports modular dynamic execution. A `.wexp` file represents a self-contained execution module compiled with its own `main` entry point and full Standard 4/6 symbols.

### 9.1 Loading and Executing with `jmpto`
The `jmpto` statement invokes the dynamic loader (such as `sld` or the operating system's built-in loader) to map the `.wexp` module, load its entry point, and execute it.

```c
fn main() {
    u64 input_val = 500;

    // Load and execute module.wexp
    jmpto module.wexp {
        input_val; // Pass the variable to the module
    }

    // The return value of module.wexp's main is saved in 'result'
    print_number(result);
}
```

### 9.2 Receiving Arguments inside `.wexp`
Inside the target `.wexp` file, passed arguments must be declared as global variables. They are initialized automatically upon module transition:

```c
// module.wexp
sc.true

u64 input_val; // Populated by the loader from jmpto argument state

fn main() {
    return(input_val + 5);
}
```

---

## 10. Inline NASM Assembly (`::nasm::`)

Inline assembly blocks start with the `::nasm::` prefix.

### 10.1 Accessing Stack Variables
Local stack variables are referenced directly inside brackets `[...]`. The compiler resolves variable names to their exact frame displacement relative to RBP and substitutes them:

```c
fn set_rax(u64 value) {
    ::nasm::{
        mov rax, [value] // Compiled to: mov rax, [rbp - offset]
    }
}
```

### 10.2 Accessing Global Sections via `sect:var`
Inside `::nasm::` blocks, variables declared in global sections can be accessed by their qualified names using the `[section_name:variable_name]` bracket syntax. The compiler substitutes these references with the resolved addresses of the global variables:

```c
sect.config
    u64 baudrate = 9600;
EOS

fn set_baudrate(u64 rate) {
    ::nasm::{
        mov rax, [rate]             // Local stack variable
        mov [config:baudrate], rax  // Global section variable
    }
}
```
---

## 11. Complete Operator Precedence Table

The following table lists WandC operators in order of decreasing precedence, along with their associativity and parser token representation:

| Precedence | Operator | Description | Associativity | Token |
|------------|----------|-------------|---------------|-------|
| **1**      | `*adr`   | Address-of | Right | `Token::AddrOf(String)` |
| **2**      | `->`     | Structure field pointer dereference | Left | `Token::Arrow` |
| **2**      | `.`      | Structure field direct access | Left | `Token::Dot` |
| **2**      | `[index]`| Array indexing | Left | `Token::LBracket / RBracket` |
| **3**      | `!`      | Logical negation | Right | `Token::OpNot` |
| **4**      | `*`      | Arithmetic multiplication (formatted with spaces) | Left | `Token::OpMul` |
| **4**      | `/`      | Arithmetic division | Left | `Token::OpDiv` |
| **4**      | `%`      | Arithmetic modulo | Left | `Token::OpMod` |
| **5**      | `+`      | Arithmetic addition | Left | `Token::OpAdd` |
| **5**      | `-`      | Arithmetic subtraction | Left | `Token::OpSub` |
| **6**      | `<`      | Relational less than | Left | `Token::OpLt` |
| **6**      | `<=`     | Relational less than or equal | Left | `Token::OpLtEq` |
| **6**      | `>`      | Relational greater than | Left | `Token::OpGt` |
| **6**      | `>=`     | Relational greater than or equal | Left | `Token::OpGtEq` |
| **7**      | `==`     | Relational equality | Left | `Token::OpEq` |
| **7**      | `!=`     | Relational inequality | Left | `Token::OpNotEq` |
| **8**      | `&&`     | Logical conjunction | Left | `Token::OpAnd` |
| **9**      | `\|\|`   | Logical disjunction | Left | `Token::OpOr` |
| **10**     | `=`      | Variable assignment | Right | `Token::OpAssign` |
