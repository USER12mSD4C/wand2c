# STANDARD 4/6 - BINARY INTERFACE SPECIFICATION

**Version 1.6.0 (2026)**
*Copyright (c) 2026 The Standard 4/6 Authors*
*Licensed under MIT License*

---

## 1. Introduction

### 1.1. Purpose
Standard 4/6 defines a binary interface for low-level code that includes:
- Type versioning (backward and forward compatibility)
- Binary reflection (runtime type information)
- Standardized module loading
- Cross-platform and cross-architecture portability
- Mandatory loader safety validation

The primary goal is to enable **understandable and readable code** written with Protocol 4/6, without workarounds or hacks - everything is implemented natively in the compiler and toolchain.

### 1.2. Scope
This standard applies to:
- Compiled binaries (executables, libraries, modules)
- Compilers generating code for any architecture
- Operating systems loading Standard 4/6 modules
- Tools that inspect or manipulate Standard 4/6 binaries

### 1.3. Conformance
A binary conforms to Standard 4/6 if it:
- Contains the required metadata sections
- Follows the type layout rules
- Implements the versioning semantics
- Provides reflection data as specified
- Passes loader validation rules (see section 10)

A compiler conforms if it can generate Standard 4/6 binaries in at least one embedding format from section 3.1.
An operating system conforms if it can load and execute Standard 4/6 modules in at least one embedding format from section 3.1 - it does not need to support all of them. A from-scratch OS with no ELF/PE legacy to interoperate with is fully conformant supporting Raw Binary alone.

---

## 2. Versioning Model

### 2.1. Module Version
Every module has a version number: `MAJOR.MINOR.PATCH.BUILD`

| Field | Meaning |
|-------|---------|
| MAJOR | Complete redesign, breaking changes (entire codebase reconsidered) |
| MINOR | Major feature addition, new functionality, may add new types/functions |
| PATCH | Minor updates, bug fixes |
| BUILD | Ultra-minor changes (typos in comments, metadata only, republishing without functional changes) |

**Example:** `3.2.1.5` - third major version, second major update, first patch, fifth republish with minor fixes.

### 2.2. Type Versioning
Each exported structure has its own version. Fields have:
- `version_added` - when this field appeared
- `version_removed` - when this field was removed (`0xFFFFFFFF` = still present)

```
struct Foo version 2 {
    u32 x version 1;      // added in v1
    u64 y version 2;      // added in v2
}
```

### 2.3. Enum Versioning
Enums can also be versioned. Enum size is the smallest that fits all values (1, 2, 4, or 8 bytes). Default is 4 bytes unless specified.

```
enum Error version 3 {
    None = 0 version 1;
    NotFound = 1 version 1;
    PermissionDenied = 2 version 2;
    Timeout = 3 version 3;
}
```

### 2.4. Function Versioning
Each exported function has a version. Multiple versions of the same function may coexist:

```
fn open(path: string, flags: u32) -> i32 version 2;
fn open(path: string, mode: u16) -> i32 version 1;
```

### 2.5. Compatibility Rules
- MAJOR must match exactly: a module with MAJOR version N is compatible only with code expecting MAJOR version N. This is the precondition for every rule below.
- Given matching MAJOR: a module is backward compatible if its MINOR >= the expected MINOR
- PATCH and BUILD never affect compatibility (given matching MAJOR and sufficient MINOR)
- For individual types (independent of module-level MAJOR/MINOR): code expecting a field with `version_added <= current_version <= version_removed` works
- `version_removed = 0xFFFFFFFF` means the field is still present
- Function symbol imports (section 6.3) and type field imports (section 5.4) use different compatibility checks - a function import matches by comparing the whole function's `version` against `required_version`; a type import matches per-field via `version_added`/`version_removed`. Don't conflate the two.

---

## 3. Binary Layout

### 3.1. Supported Formats
Standard 4/6 can be embedded in:
- **ELF** (Linux, Unix-like) - sections: `.p46_header`, `.p46_types`, `.p46_exports`, `.p46_imports`, `.p46_deps`, `.p46_reflect`, `.p46_strtab`
- **PE/COFF** (Windows, UEFI) - sections: `.p46$H`, `.p46$T`, `.p46$E`, `.p46$I`, `.p46$D`, `.p46$R`, `.p46$STR`
- **Raw binary** (bare metal, embedded, or any OS with no existing container format) - layout defined in Appendix A. `offset` fields here are relative to wherever the loader placed the blob's first byte - that address might be a physical/virtual load address in RAM, a flash/ROM offset, a position in a boot image, or an offset in an ordinary file; there doesn't have to be a "file" at all. This is unlike ELF/PE, where `offset` is a file position resolved through that format's own program-header/section-table indirection before anything is loaded into memory.
- **Custom formats** - any format that implements the required sections

A conforming OS is not required to support all four of these - only whichever embedding its own loader actually targets. An OS built from scratch with no pre-existing binary ecosystem to interoperate with has no reason to implement ELF or PE/COFF parsing at all; Raw Binary alone is a fully conformant target (section 1.3).

When embedding in existing formats, Standard 4/6 adds custom sections without breaking existing tools.

### 3.2. Required Sections
Every Standard 4/6 binary must contain:

| Section | Content |
|---------|---------|
| `.p46_header` | Magic, version, endianness, pointer size, address size, section descriptors, string table location |
| `.p46_types` | Type descriptors (structs, enums, unions, functions, arrays, strings, pointers, typedefs) - TLV record sequence, sections 3.6-3.7 |
| `.p46_exports` | Exported symbols with versions - flat structure, section 5.1 |
| `.p46_imports` | Imported symbols with required versions - flat structure, section 5.2 |
| `.p46_deps` | Module dependencies - flat structure, section 5.3 |
| `.p46_reflect` | Qualified-name lookup index (name -> type_id / symbol), used by `reflect_get_qualified_type` and `module_get_qualified_symbol` - flat structure, section 7.5 |
| `.p46_strtab` | String table (null-terminated UTF-8 strings for names); no section descriptor, located via `strtab_offset`/`strtab_size` |

### 3.3. Header Structure

```
struct p46_header {
    u8  magic[4];            // fixed byte sequence: 0x50 '4' '6' 0x00 ("P46\\0")
    u8  format_major;        // e.g. 1
    u8  format_minor;        // e.g. 6
    u8  format_patch;        // e.g. 0
    u8  endianness;          // 0x01 = little-endian, 0x02 = big-endian
    u8  pointer_size;        // byte width of a native pointer on the target: 1, 2, 4, or 8. NOT a 32/64 flag.
    u8  address_size;        // byte width of address fields in exports: 1, 2, 4, or 8. May differ from pointer_size.
    u8  reserved[2];
    u32 module_version;      // packed: (MAJOR<<24) | (MINOR<<16) | (PATCH<<8) | BUILD
    u32 section_count;       // number of section descriptors following (>= 5, see 3.2)
    u64 strtab_offset;       // offset to string table
    u64 strtab_size;         // size of string table in bytes
    // followed by section descriptors
}
```

**Magic:** `magic` is a raw 4-byte sequence, **not** an integer. It must be compared byte-for-byte (e.g. `memcmp`), never loaded as a `u32` and compared against a constant - a `u32` load is endianness-dependent, but `endianness` itself hasn't been validated yet at the point the magic is checked. Byte-wise comparison sidesteps this chicken-and-egg problem entirely.

**Format version:** split into three explicit `u8` fields. `format_major`/`format_minor`/`format_patch` mirror the SemVer-style versioning of this document (section 9.1) directly, byte for byte, with no encoding step to get wrong.

**Module version:** `module_version` is a packed `u32`: bits 31-24 = MAJOR, 23-16 = MINOR, 15-8 = PATCH, 7-0 = BUILD. Example: `0x01020304` means v1.2.3.4. This allows single-integer version comparisons.

**Endianness:** narrowed from `u16` to `u8` (values 1/2 never needed more than a byte) and moved next to `pointer_size` so the header no longer has a stray 2-byte field forcing implicit padding.

**Pointer size:** the old `0 = 32-bit, 1 = 64-bit` flag baked in an assumption that only two pointer widths will ever exist - true for the x86/ARM world this borrowed conventions from, not guaranteed for a from-scratch CPU design where the native word size isn't decided by someone else. `pointer_size` is now a literal byte count (`1`, `2`, `4`, `8` are the values this document defines; any custom architecture with a byte-addressable, <=64-bit pointer just states its width directly, no flag lookup table to keep in sync).

**Address size:** new in v1.6.0. Explicit byte width for address fields in `.p46_exports` (section 5.1). May differ from `pointer_size` - for example, a 32-bit module (`pointer_size = 4`) may use 64-bit metadata addresses (`address_size = 8`) when running in a larger address space, or a 64-bit module may use 32-bit compact addresses (`address_size = 4`) if all modules are loaded below 4GB. The `address` field in exports uses this width on disk, zero-extended to `u64` when loaded.

The `address` field in exports (section 5.1) and the fixed-width fields used for pointer/string types elsewhere stay `u64` in the in-memory API regardless - this is a deliberate, documented ceiling: **this standard supports address spaces up to 64 bits (`pointer_size <= 8` and `address_size <= 8`)**. Wider pointers are a MAJOR-version concern (would require widening `address` and every `type_id`-adjacent fixed field), not something to half-support with variable-width records everywhere. If a future architecture genuinely needs it, that's a v2.0 problem, called out explicitly rather than silently assumed away.

Alignment (sections 4.4/8.4) already capped field alignment at 8 bytes independent of native word size, so this ceiling was implicitly already assumed throughout the layout rules - it's now just stated once, honestly, at the field that actually encodes it.

**Section Descriptor:**

```
struct p46_section {
    u64 offset;             // offset from start of file
    u64 size;               // size of section in bytes
    u32 type;               // section type (see table below)
}
```

**Section Types:**

| Type | Meaning |
|------|---------|
| 1 | P46_SECTION_TYPES |
| 2 | P46_SECTION_EXPORTS |
| 3 | P46_SECTION_REFLECT |
| 4 | P46_SECTION_IMPORTS |
| 5 | P46_SECTION_DEPENDENCIES |
| 100-65535 | Reserved for vendor/tool-specific sections (must be ignored by conforming loaders if unrecognized) |

`.p46_strtab` is **not** listed here and has no section descriptor of its own - its location is given directly by `strtab_offset`/`strtab_size` in the header (section 3.3), since it's the one section every other section depends on to resolve names and must be locatable without walking the descriptor array first. `.p46_header` likewise has no descriptor; it's always at file offset 0.

Consequently `section_count` is exactly **5** for a minimal conforming binary (types, exports, imports, deps, reflect - all five are mandatory per section 3.2, even if empty with `size = 0`), plus one descriptor for each vendor section present. Appendix B's worked example uses `section_count: 5` accordingly.

### 3.4. String Table Format
The string table (`.p46_strtab`) is a contiguous block of null-terminated UTF-8 strings. Offsets in other sections refer to positions within this table.

```
// Example string table layout:
// offset 0: "Foo\\0"
// offset 4: "bar\\0"
// offset 8: "libc.ko\\0"
```

Loaders do not validate UTF-8 correctness; string table entries are byte sequences with null terminators. Consumers must handle invalid UTF-8 sequences gracefully.

### 3.5. Section Format (TLV-based)
Only `.p46_types` (`P46_SECTION_TYPES`) uses this repeated-record format. It contains a sequence of TLV (Type-Length-Value) records - one per type descriptor (struct/enum/union/function/array/string/pointer/typedef, section 3.6) - parsed by reading records sequentially until the section size is exhausted. Any number of type descriptors may appear in one section, in any order, mixed freely by kind.

```
[type: u16][length: u32][value: byte[length]]   // one type descriptor, per section 3.7
[type: u16][length: u32][value: byte[length]]
...
```

`.p46_exports`, `.p46_imports`, `.p46_deps` and `.p46_reflect` are **not** TLV-record sequences - each is a single flat structure (a count followed by a fixed-shape array), defined in full in sections 5.1-5.3 and 7.5 respectively. They don't need TLV type/length framing at the record level: the section descriptor already gives the exact `offset`/`size` of the whole structure, and every element within it has a statically known shape. Wrapping them in an outer TLV record on top of that would be redundant.

### 3.6. Type IDs for TLV

| Type ID | Meaning |
|---------|---------|
| 1 | P46_TYPE_STRUCT |
| 2 | P46_TYPE_ENUM |
| 3 | P46_TYPE_UNION |
| 4 | P46_TYPE_FUNCTION |
| 5 | P46_TYPE_PRIMITIVE |
| 6 | P46_TYPE_ARRAY |
| 7 | P46_TYPE_STRING |
| 8 | P46_TYPE_POINTER |
| 9 | P46_TYPE_TYPEDEF |
| 10-99 | Reserved for future core revisions of this standard |
| 100-65535 | Vendor/language-specific type kinds (e.g. generics, tagged unions, traits - anything a language built on top of this standard adds that isn't one of the 9 core kinds). A loader that doesn't recognize a type ID in this range must skip the record using its `length` field and keep parsing, not abort - the TLV framing (section 3.5) exists precisely so unknown records are safely skippable. |

This field is `u16` (section 3.5), so the ceiling is 65535, not a small closed set - a language still being designed on top of this standard isn't limited to the 9 kinds that exist today.

**Primitive Type IDs (for TYPE_PRIMITIVE):**

| ID | Type | Size (bytes) |
|----|------|--------------|
| 1 | u8 | 1 |
| 2 | u16 | 2 |
| 3 | u32 | 4 |
| 4 | u64 | 8 |
| 5 | i8 | 1 |
| 6 | i16 | 2 |
| 7 | i32 | 4 |
| 8 | i64 | 8 |
| 9 | f32 | 4 |
| 10 | f64 | 8 |
| 11 | ptr | pointer_size from header |

**Note:** The `ptr` type size is determined by the `pointer_size` field in the header at load time.

### 3.7. Type Encoding Format

**STRUCT:**
```
type = 1
value = [
    name_offset: u32,      // offset in string table
    version: u32,
    size: u32,
    field_count: u32,
    fields: [
        name_offset: u32,  // offset in string table
        type_id: u32,      // reference to another type
        offset: u32,
        version_added: u32,
        version_removed: u32  // 0xFFFFFFFF = still present
    ] * field_count
]
```

**ENUM:**
```
type = 2
value = [
    name_offset: u32,
    version: u32,
    size: u32,             // bytes occupied (1, 2, 4, or 8)
    value_count: u32,
    values: [
        name_offset: u32,
        value: u64,
        version_added: u32,
        version_removed: u32
    ] * value_count
]
```

**UNION:**
```
type = 3
value = [
    name_offset: u32,
    version: u32,
    size: u32,
    field_count: u32,
    fields: [ (same as struct fields) ]  // offset is always 0 for every field
]
```

**FUNCTION:**
```
type = 4
value = [
    name_offset: u32,
    version: u32,
    return_type: u32,      // type_id
    param_count: u32,
    param_types: [u32] * param_count
]
```

**ARRAY:**
```
type = 6
value = [
    element_type: u32,     // type_id
    count: u32
]
// Arrays can be named by wrapping in TYPEDEF
```

**STRING:**
```
type = 7
value = [
    // No additional data - string is a pointer to null-terminated UTF-8
]
// Size is pointer_size from header
```

**POINTER:**
```
type = 8
value = [
    pointee_type: u32      // type_id (0 = void)
]
// Size is pointer_size from header
```

**TYPEDEF:**
```
type = 9
value = [
    name_offset: u32,      // alias name
    underlying_type: u32   // type_id being aliased
]
```

---

## 4. Type System

### 4.1. Primitive Types
Fixed-size types regardless of architecture:

| Type | Size (bytes) | Type ID |
|------|--------------|---------|
| u8 | 1 | 1 |
| u16 | 2 | 2 |
| u32 | 4 | 3 |
| u64 | 8 | 4 |
| i8 | 1 | 5 |
| i16 | 2 | 6 |
| i32 | 4 | 7 |
| i64 | 8 | 8 |
| f32 | 4 | 9 |
| f64 | 8 | 10 |
| ptr | pointer_size | 11 |

### 4.2. String Type
The `string` type is a pointer to null-terminated UTF-8 data. In memory, it occupies `pointer_size` bytes. The referenced data is not owned by the string type.

### 4.3. Pointer Type
Pointers are explicitly typed with `pointer<T>` syntax. The size is determined by the `pointer_size` field in the header at load time.

### 4.4. Structures
Layout rules (architecture-independent):
- Fields are laid out in order of declaration
- Each field is aligned to an offset multiple of its size (up to 8 bytes)
- For fields with size > 8 bytes, alignment is 8 bytes
- Total structure size is rounded up to the maximum field alignment

```
struct Example version 1 {
    u8 a;     // offset 0
    u64 b;    // offset 8 (padding 7 bytes)
    u32 c;    // offset 16
} // total size = 24 (padding 4 bytes after c to 8-byte alignment)
```

### 4.5. Arrays
```
array: u8[256]  // type = array, element_type = u8, count = 256
```

Arrays can be named using TYPEDEF:
```
typedef u8[256] MyBuffer;
```

### 4.6. Global Variables
Variables are exported with:
- Name (string table offset)
- Type (type_id)
- Address (relative to module base)
- Version
- Size

---

## 5. Symbol Tables

### 5.1. Exports Section Format
Section descriptor `type` field = `P46_SECTION_EXPORTS` (2). Content (flat structure, not TLV):
```
export_count: u32,
exports: [
    name_offset: u32,      // in string table
    module_name_offset: u32,
    version: u32,
    kind: u8,              // P46_EXPORT_KIND_*, see below
    address: u8/u16/u32/u64 (width = header.address_size),
                           // relative to module base
    type_id: u32,          // For variables and functions: type of the symbol
                           // For types: type being exported
    // For functions only (kind = P46_EXPORT_KIND_FUNCTION):
    param_count: u32,
    return_type: u32,
    param_types: [u32] * param_count
    // For variables (kind = P46_EXPORT_KIND_VARIABLE): param_count = 0, return_type = 0
    // For types (kind = P46_EXPORT_KIND_TYPE): address = 0, param_count = 0, return_type = 0
] * export_count
```

```
#define P46_EXPORT_KIND_FUNCTION 1
#define P46_EXPORT_KIND_VARIABLE 2
#define P46_EXPORT_KIND_TYPE     3
```
This is a distinct namespace from `P46_TYPE_*` (section 3.6) and `REFLECT_KIND_*` (section 7.1) - it classifies *what an export points at*, not a type descriptor's own shape.

**Address encoding:** The `address` field occupies exactly `header.address_size` bytes on disk (1, 2, 4, or 8). The loader zero-extends this to `u64` for its internal representation. For 32-bit modules using `address_size = 4`, high 4 bytes are zero. For 8-bit modules using `address_size = 1`, the address is a single byte offset.

### 5.2. Imports Section Format
Section descriptor `type` field = `P46_SECTION_IMPORTS` (4). Content (flat structure, not TLV):
```
import_count: u32,
imports: [
    name_offset: u32,
    module_name_offset: u32,
    required_version: u32,
    required_type_version: u32  // for imported types
] * import_count
```

### 5.3. Dependencies Section Format
Section descriptor `type` field = `P46_SECTION_DEPENDENCIES` (5). Content (flat structure, not TLV):
```
dep_count: u32,
dependencies: [
    module_name_offset: u32,
    required_major: u32,
    required_minor: u32,
    required_patch: u32,
    required_build: u32
] * dep_count
```

### 5.4. Import Type Versioning
When importing a type, the module specifies the version it expects. The loader must verify compatibility:
- If imported version <= exported version, and field layout is compatible (no removed fields that the importer uses), the import succeeds
- Otherwise, the import fails

---

## 6. Module Loading

### 6.1. Loader API
Standard 4/6 defines a minimal loader interface. `module_load` (by path) is a convenience binding for hosted environments with a filesystem - it is **not** the primitive. `module_load_from_memory` is: every path-based loader is just "read the file into a buffer, then call this." An OS with no filesystem yet at the point modules are loaded (early boot, a driver embedded in a boot image, a module received over a network stack) implements only the memory variant and doesn't need to fake a path.

Error reporting is an explicit out-parameter, not an implicit global (no `errno`-style state): a custom OS/kernel loader may be reentrant or running before any per-thread state exists, and a hidden global error variable would be a silent multithreading hazard baked into the standard itself.

```
typedef enum {
    P46_OK = 0,
    P46_ERR_BAD_MAGIC,
    P46_ERR_UNSUPPORTED_FORMAT_MAJOR,
    P46_ERR_UNSUPPORTED_POINTER_SIZE,
    P46_ERR_UNSUPPORTED_ADDRESS_SIZE,
    P46_ERR_ENDIANNESS_MISMATCH,      // loader chose not to convert, see section 8.3
    P46_ERR_MISSING_DEPENDENCY,
    P46_ERR_VERSION_MISMATCH,
    P46_ERR_TRUNCATED,                // section/strtab offset+size exceeds buffer
    P46_ERR_TRUNCATED_SECTION,        // individual section extends past file bounds
    P46_ERR_OVERLAPPING_SECTIONS,     // two sections overlap each other
    P46_ERR_HEADER_TOO_SMALL,         // file smaller than sizeof(p46_header)
    P46_ERR_CORRUPT_TYPES,            // TLV length exceeds section bounds
    P46_ERR_CORRUPT_STRTAB,           // unterminated string or offset out of bounds
    P46_ERR_OUT_OF_MEMORY,
    P46_ERR_IO                        // only meaningful for path-based loading
} p46_status;

// Load a module from a byte buffer already in memory. This is the primitive -
// every other loading path (file, network, boot image, ROM) reduces to this.
module* module_load_from_memory(const void* data, size_t size, p46_status* out_status);

// Convenience binding for hosted environments with a filesystem.
// Equivalent to reading `path` fully into memory and calling module_load_from_memory.
module* module_load(const char* path, p46_status* out_status);

// Get symbol by name from a specific module
void* module_get_symbol(module* m, const char* name, uint32_t required_version);

// Get symbol by qualified name (module:symbol)
void* module_get_qualified_symbol(const char* qualified_name, uint32_t required_version);

// Get type from a specific module
reflect_type* module_get_type(module* m, const char* type_name, uint32_t required_version);

// Get type by qualified name (module:type)
reflect_type* reflect_get_qualified_type(const char* qualified_name, uint32_t required_version);

// Check version compatibility
int module_check_version(module* m, uint32_t required_major, uint32_t required_minor);

// Unload module
void module_unload(module* m);

// Memory management for reflection data
void reflect_free_type(reflect_type* t);
void reflect_free_field(reflect_field* f);
void reflect_free_enum_value(reflect_enum_value* v);

// Vendor section access (section 6.4)
const void* module_get_raw_section(module* m, uint32_t section_type, size_t* out_size);

// Error string for debugging
const char* p46_status_string(p46_status status);
```

`out_status` may be `NULL` if the caller doesn't care why loading failed - a `NULL` return from either loading function is always sufficient to know it failed.

**Module base:** every `address` field in this standard (section 5.1) is relative to "module base" - the address at which the loader placed the start of the module's loadable image. How that address is chosen (fixed, relocated, memory-mapped, position-independent) is entirely host-loader-defined and out of this standard's scope; the only contract is that it's a single, stable base for the lifetime of the loaded module.

### 6.2. Memory Ownership
- `reflect_get_type()`, `module_get_type()`, and `reflect_get_fields()` return newly allocated memory
- Caller must free with `reflect_free_type()` and `reflect_free_field()`
- Module loader owns the underlying metadata; reflection functions copy what is needed
- "Newly allocated" describes an ownership contract, not literally `malloc`/`free`. An implementation may back these calls with an arena/bump allocator (where `reflect_free_*` is a no-op, or just decrements a refcount, and the real reclamation happens in bulk elsewhere) or any other allocation strategy. The only thing a caller may rely on is: the pointer stays valid until it calls the matching `free`, and must not be touched after.

### 6.3. Version Resolution Rules
1. Each module has an **isolated namespace** - symbols from different modules do not conflict
2. Imports are resolved by `(module_name, symbol_name, required_version)`
3. If multiple versions of the same module are required, the loader may load multiple instances
4. If a module requires two different versions of the same symbol, the loader must check if the exported symbol version satisfies both (i.e., `max(required) <= exported`)

### 6.4. Vendor Section Access
Loaders must ignore unrecognized vendor sections (type >= 100) during normal loading, but must expose them via `module_get_raw_section()`. This allows tools and language runtimes to attach custom metadata without breaking standard conformance.

```
const void* module_get_raw_section(module* m, uint32_t section_type, size_t* out_size);
```
- Returns a pointer to the raw section data if `section_type` matches a loaded vendor section
- Returns `NULL` if not found
- `out_size` is set to the section size if non-NULL
- The returned pointer is valid for the lifetime of the module

### 6.5. Dependency Resolution
Loader resolves dependencies recursively, and may load newer compatible versions than requested.

---

## 7. Reflection API

### 7.1. Reflection Kind Constants
`REFLECT_KIND_*` is not a separate numbering - it **is** `P46_TYPE_*` from section 3.6, reused directly, so a wire-format `type` tag can be assigned straight into `reflect_type.kind` with no translation table:

```
#define REFLECT_KIND_STRUCT    1  // = P46_TYPE_STRUCT
#define REFLECT_KIND_ENUM      2  // = P46_TYPE_ENUM
#define REFLECT_KIND_UNION     3  // = P46_TYPE_UNION
#define REFLECT_KIND_FUNCTION  4  // = P46_TYPE_FUNCTION
#define REFLECT_KIND_PRIMITIVE 5  // = P46_TYPE_PRIMITIVE
#define REFLECT_KIND_ARRAY     6  // = P46_TYPE_ARRAY
#define REFLECT_KIND_STRING    7  // = P46_TYPE_STRING
#define REFLECT_KIND_POINTER   8  // = P46_TYPE_POINTER
#define REFLECT_KIND_TYPEDEF   9  // = P46_TYPE_TYPEDEF
```

### 7.2. Type Information Structures
These are **runtime, in-memory** structures - never the on-disk format. The loader builds them on demand from `.p46_types` records (section 3.7) when `reflect_get_type`/`reflect_get_qualified_type`/etc. are called; nothing in a `.p46_*` file is laid out with C pointers, since pointers aren't valid in a serialized, position-independent file. `.p46_types` is the actual reflection data at rest; this struct is its query-time projection.

```
typedef struct reflect_type {
    uint32_t id;            // type_id from TLV
    char* name;             // type name (NULL for anonymous)
    uint32_t version;
    uint32_t size;          // size in bytes
    uint32_t kind;          // one of REFLECT_KIND_*
    uint32_t field_count;
    reflect_field* fields;  // for struct/union
    // For enum:
    uint32_t enum_value_count;
    reflect_enum_value* enum_values;
    // For array:
    struct reflect_type* element_type;
    uint32_t array_count;
    // For pointer:
    struct reflect_type* pointee_type;
} reflect_type;

typedef struct reflect_field {
    char* name;
    reflect_type* type;
    uint32_t offset;
    uint32_t version_added;
    uint32_t version_removed;  // 0xFFFFFFFF = still present
} reflect_field;

typedef struct reflect_enum_value {
    char* name;
    uint64_t value;
    uint32_t version_added;
    uint32_t version_removed;
} reflect_enum_value;
```

### 7.3. API Functions
```
// Get type info by name (allocates memory)
reflect_type* reflect_get_type(const char* name);

// Get type info by qualified name (module:type)
reflect_type* reflect_get_qualified_type(const char* qualified_name, uint32_t required_version);

// Get type info by ID (allocates memory)
reflect_type* reflect_get_type_by_id(uint32_t id);

// Get fields for a type (allocates memory for array)
reflect_field* reflect_get_fields(reflect_type* type, int* count);

// Get enum values (allocates memory for array)
reflect_enum_value* reflect_get_enum_values(reflect_type* type, int* count);

// Free allocated memory
void reflect_free_type(reflect_type* t);
void reflect_free_field(reflect_field* f);
void reflect_free_enum_value(reflect_enum_value* v);
```

### 7.4. API Versioning
The reflection API is versioned. Programs should check `REFLECT_API_VERSION` macro and use appropriate functions.

### 7.5. `.p46_reflect` Section Format
This is the wire format backing `reflect_get_qualified_type` and `module_get_qualified_symbol` (section 6.1): a flat lookup index from qualified name to the entry it resolves to, so those calls don't require a linear scan of `.p46_types`/`.p46_exports`. It is a flat structure (section 3.5), not a TLV sequence:

Section descriptor `type` field = `P46_SECTION_REFLECT` (3). Content (flat structure, not TLV):
```
entry_count: u32,
entries: [
    qualified_name_offset: u32,  // "module:symbol" or "module:Type", in string table
    target_kind: u8,             // 1 = type (resolves into .p46_types), 2 = export (resolves into .p46_exports)
    target_index: u32,           // index of the target record within its section
    version: u32                 // version of the target at the time this entry was built
] * entry_count
```

A loader that omits this index entirely (`entry_count = 0`) must still conform - callers fall back to a linear scan of `.p46_types`/`.p46_exports` - but the section itself remains mandatory per section 3.2 so its presence and shape don't need to be probed for.

---

## 8. Architecture and Platform Considerations

### 8.1. Supported Architectures
This standard is architecture-agnostic by design - any byte-addressable target with a pointer width of 1, 2, 4, or 8 bytes (section 3.3) qualifies, including architectures that don't exist yet. The list below is reference points, not a closed enumeration a new target needs to be added to:
- **x86_64** - reference architecture (pointer_size = 8)
- **ARM64** - supported (pointer_size = 8)
- **RISC-V** - supported (pointer_size = 8)
- **x86_32 / ARM32** - supported (pointer_size = 4)
- **Custom/experimental architectures** (including ones with no existing toolchain, OS, or prior art) - supported as long as they're byte-addressable with `pointer_size <= 8`; nothing in this standard references a specific instruction set, calling convention, or existing OS ABI

### 8.2. Pointer Size Selection
The `pointer_size` field in the header (a byte count, not a flag - see section 3.3) determines the size of pointer types for that module. A loader encountering a `pointer_size` it doesn't natively support MAY reject the module or MAY perform conversion (e.g. widening) - both are conformant; this standard only requires that the loader never silently truncate or misinterpret a pointer.

### 8.3. Endianness
Standard 4/6 uses **little-endian** for all metadata by default, and this governs every multi-byte field in every section (header, types, exports, imports, deps, reflect) - not just the header. The `endianness` field indicates the byte order the file was actually written in. A loader MAY convert on load if its native order differs, or MAY simply refuse to load modules whose declared `endianness` doesn't match its own - both are conformant. What's not conformant is ignoring the field and assuming native order. (An OS that only ever runs on one architecture it controls end-to-end is not obligated to implement a byte-swapping path it will never exercise.)

### 8.4. Alignment Rules (Architecture-Independent)
1. Fields are laid out in order of declaration
2. Each field is aligned to an offset multiple of its size (up to 8 bytes)
3. For fields with size > 8 bytes, alignment is 8 bytes
4. Total structure size is rounded up to the maximum alignment of its fields
5. These rules are identical across all architectures, ensuring binary compatibility

---

## 9. Standard Evolution

### 9.1. Versioning of This Standard
This standard itself is versioned as `MAJOR.MINOR.PATCH (YEAR)`:

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026 | Initial release |
| 1.1 | 2026 | Added: format_version in header, isolated symbol namespaces, detailed type encoding, architecture-independent alignment rules |
| 1.2 | 2026 | Added: string table, ptr32/ptr64 split, version_removed special value, reflection memory ownership, global variable support, type import versioning |
| 1.3 | 2026 | Added: pointer_size field, TYPEDEF type, kind constants, dependencies section, qualified type lookup, enum size rules, section descriptors |
| 1.3.1 | 2026 | Refined: split exports/imports/deps into separate sections, clarified address field for 32-bit, kind semantics in exports, added parser example |
| 1.4.0 | 2026 | Corrections: magic redefined as raw byte sequence; `format_version` replaced with explicit `format_major`/`format_minor`/`format_patch`; `section_count` semantics fixed; resolved TLV vs flat structure contradiction; defined `.p46_reflect` wire format; merged `REFLECT_KIND_*` into `P46_TYPE_*`; named `P46_EXPORT_KIND_*`; clarified UNION field offsets; tightened compatibility rules |
| 1.5.0 | 2026 | Portability corrections: `pointer_size` generalized to byte count; architecture list opened; conformance relaxed to one embedding format; Raw Binary offsets clarified as load-relative; endianness conformance relaxed; loader API given `module_load_from_memory` as primitive; reflection memory ownership decoupled from `malloc`; TLV type IDs given reserved ranges; Appendix C marked as illustration |
| 1.6.0 | 2026 | Added: `u64` section offsets/sizes; `address_size` field; packed `module_version`; `module_get_raw_section`; mandatory loader validation; expanded error codes; `p46_status_string`; variable-width export addresses |

- **MAJOR** increments: breaking changes to the standard
- **MINOR** increments: additions, clarifications, non-breaking changes
- **PATCH** increments: corrections, clarifications, examples
- **YEAR** indicates the approval year

### 9.2. Maintenance
The Standard 4/6 specification is maintained by its original author (and potentially a future committee). Updates are published with clear compatibility notes.

### 9.3. Compatibility Between Standard Versions
- Standard v1.x implementations are forward compatible (v1.0 code works on v1.6.0 loaders)
- Standard v2.0 may introduce breaking changes

---

## 10. Security and Safety

### 10.1. Loader Validation Requirements
A conforming loader **must** perform the following validation before accepting a module. Failure on any check **must** result in the corresponding error code and immediate rejection of the module.

**Header validation:**
- The input buffer must be at least `sizeof(p46_header)` bytes (`P46_ERR_HEADER_TOO_SMALL`)
- `magic` must match byte-for-byte (`P46_ERR_BAD_MAGIC`)
- `format_major` must be supported (`P46_ERR_UNSUPPORTED_FORMAT_MAJOR`)
- `pointer_size` must be 1, 2, 4, or 8 (`P46_ERR_UNSUPPORTED_POINTER_SIZE`)
- `address_size` must be 1, 2, 4, or 8 (`P46_ERR_UNSUPPORTED_ADDRESS_SIZE`)
- `endianness` must match loader native order or loader must convert (`P46_ERR_ENDIANNESS_MISMATCH`)

**Section validation:**
- Every section descriptor's `offset + size` must not exceed the input buffer size (`P46_ERR_TRUNCATED_SECTION`)
- No two sections may overlap in the `[offset, offset + size)` range (`P46_ERR_OVERLAPPING_SECTIONS`)
- The string table (`strtab_offset + strtab_size`) must fit within the buffer (`P46_ERR_TRUNCATED`)

**String table validation:**
- Every string offset referenced by parsed records must be `< strtab_size`
- Every string must be null-terminated within `strtab_size` (`P46_ERR_CORRUPT_STRTAB`)

### 10.2. TLV Parsing Safety
When parsing `.p46_types` (the only TLV-record section), the loader **must**:

1. Read `type` (2 bytes) and `length` (4 bytes)
2. Verify that `current_position + 6 + length <= section_end` before reading any value bytes (`P46_ERR_CORRUPT_TYPES`)
3. If `type` is unrecognized (vendor range 100-65535), skip `6 + length` bytes and continue
4. If `type` is invalid (0 or > 65535), reject (`P46_ERR_CORRUPT_TYPES`)
5. Never trust `length` without bounds checking

This prevents malformed TLV records from causing out-of-bounds reads.

### 10.3. Export Address Safety
When reading export entries, the loader **must**:

1. Read exactly `header.address_size` bytes for each `address` field
2. Zero-extend to `u64` internally
3. Verify that `address` is within the module's loaded image bounds (loader-defined check)

### 10.4. Reflection Index Safety
When using `.p46_reflect`:

1. `target_index` must be validated against the target section's entry count before dereferencing
2. `qualified_name_offset` must be validated against the string table
3. If the index is corrupt, the loader **must** fall back to linear scan or fail safely - never crash

---

## 11. License

### 11.1. This Specification
This specification is licensed under the **MIT License**.

```
Copyright (c) 2026 The Standard 4/6 Authors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this specification to implement, use, and distribute implementations of
this standard, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the specification.

THE SPECIFICATION IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND...
```

### 11.2. Reference Implementations
- **WandC** (reference compiler) - licensed under UOPL
- **UTMS7** (reference OS) - licensed under UOPL

This separation allows the standard to be freely implemented while protecting the author's original implementations.

---

## Appendix A: Raw Binary Format

For raw binaries (bare metal), the layout is:
```
[ p46_header (fixed size, includes section descriptors) ]
[ p46_types   (TLV record sequence, section 3.5) ]
[ p46_exports (flat structure, section 5.1) ]
[ p46_imports (flat structure, section 5.2) ]
[ p46_deps    (flat structure, section 5.3) ]
[ p46_reflect (flat structure, section 7.5) ]
[ p46_strtab  (strings, located via strtab_offset/strtab_size, not a section descriptor) ]
```

The header's section descriptors (one per mandatory section: types, exports, imports, deps, reflect, plus any vendor sections) point to offsets within the file. `p46_strtab` is located directly via the header's `strtab_offset`/`strtab_size` fields, not through a descriptor.

---

## Appendix B: Example Binary Layout (ELF)

```
Section .p46_header:
   magic:            50 34 36 00  ("P46\\0")
   format_major:      1
   format_minor:      6
   format_patch:      0
   endianness:        1  (little-endian)
   pointer_size:      8  (64-bit)
   address_size:      8  (64-bit)
   reserved:          00 00
   module_version:    0x01000000  (v1.0.0.0)
   section_count:     5
   strtab_offset:     0x0000000000002000
   strtab_size:       256

Section .p46_types:  (TLV record sequence)
   [type: 1][length: 24][value: (type descriptor for struct Foo)]
   [type: 9][length: 16][value: (typedef MyBuffer = u8[256])]

Section .p46_exports:  (flat structure, section 5.1)
   export_count: 1
   exports: [ (export entry for function "open") ]

Section .p46_imports:  (flat structure, section 5.2)
   import_count: 1
   imports: [ (import entry for "libc.ko:open") ]

Section .p46_deps:  (flat structure, section 5.3)
   dep_count: 1
   dependencies: [ (dependency entry for "libc.ko") ]

Section .p46_reflect:  (flat structure, section 7.5)
   entry_count: 1
   entries: [ (qualified-name index entry for "libc.ko:open" -> export #0) ]

Section .p46_strtab:
   "Foo\\0x\\0y\\0MyBuffer\\0libc.ko\\0open\\0"
```

---

## Appendix C: Parser Example (C)

This example demonstrates how to read the header and locate sections. It's written against hosted C (`stdio.h`, `malloc`) purely because that's a portable way to show byte layout on paper - it is not part of the standard's contract. A kernel-level loader (no filesystem yet, no heap yet, reading a module out of a boot-time memory blob) replaces `fread`/`fseek` with whatever primitives it already has (raw pointer arithmetic into a mapped image, block-device reads, etc.) and `malloc` with its own allocator or none at all (see section 6.2). Only the byte layout below is normative.

```c
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    uint8_t  magic[4];
    uint8_t  format_major;
    uint8_t  format_minor;
    uint8_t  format_patch;
    uint8_t  endianness;
    uint8_t  pointer_size;
    uint8_t  address_size;
    uint8_t  reserved[2];
    uint32_t module_version;
    uint32_t section_count;
    uint64_t strtab_offset;
    uint64_t strtab_size;
} p46_header;

typedef struct {
    uint64_t offset;
    uint64_t size;
    uint32_t type;
} p46_section;

static const uint8_t P46_MAGIC[4] = { 0x50, '4', '6', 0x00 };

static uint64_t read_address(const uint8_t* data, uint8_t address_size) {
    switch (address_size) {
        case 1: return data[0];
        case 2: return (uint64_t)data[0] | ((uint64_t)data[1] << 8);
        case 4: return (uint64_t)data[0] | ((uint64_t)data[1] << 8) |
                        ((uint64_t)data[2] << 16) | ((uint64_t)data[3] << 24);
        case 8: return (uint64_t)data[0] | ((uint64_t)data[1] << 8) |
                        ((uint64_t)data[2] << 16) | ((uint64_t)data[3] << 24) |
                        ((uint64_t)data[4] << 32) | ((uint64_t)data[5] << 40) |
                        ((uint64_t)data[6] << 48) | ((uint64_t)data[7] << 56);
        default: return 0;
    }
}

int parse_p46(FILE* f) {
    p46_header hdr;
    if (fread(&hdr, sizeof(hdr), 1, f) != 1) return -1;

    // Byte-wise compare: correct regardless of host or file endianness,
    // and doesn't depend on having read hdr.endianness first.
    if (memcmp(hdr.magic, P46_MAGIC, 4) != 0) {
        fprintf(stderr, "Invalid magic: %02x %02x %02x %02x\\n",
                hdr.magic[0], hdr.magic[1], hdr.magic[2], hdr.magic[3]);
        return -1;
    }

    printf("Standard 4/6 v%u.%u.%u\\n",
           hdr.format_major, hdr.format_minor, hdr.format_patch);
    printf("Module version: %u.%u.%u.%u\\n",
           (hdr.module_version >> 24) & 0xFF,
           (hdr.module_version >> 16) & 0xFF,
           (hdr.module_version >> 8) & 0xFF,
           hdr.module_version & 0xFF);
    printf("Pointer size: %u bytes (%u-bit)\\n", hdr.pointer_size, hdr.pointer_size * 8);
    printf("Address size: %u bytes\\n", hdr.address_size);

    // Read section descriptors (section_count >= 5, see section 3.2/3.3)
    p46_section* sections = malloc(hdr.section_count * sizeof(p46_section));
    if (fread(sections, sizeof(p46_section), hdr.section_count, f) != hdr.section_count) {
        free(sections);
        return -1;
    }

    // Locate sections
    for (uint32_t i = 0; i < hdr.section_count; i++) {
        const char* type_name = "";
        switch (sections[i].type) {
            case 1: type_name = "types"; break;
            case 2: type_name = "exports"; break;
            case 3: type_name = "reflect"; break;
            case 4: type_name = "imports"; break;
            case 5: type_name = "dependencies"; break;
            default: type_name = "vendor/unknown"; break;
        }
        printf("Section %u: type=%s offset=0x%llx size=%llu\\n",
               i, type_name, (unsigned long long)sections[i].offset,
               (unsigned long long)sections[i].size);
    }

    // Read string table (located via header fields, not a section descriptor)
    fseek(f, (long)hdr.strtab_offset, SEEK_SET);
    char* strtab = malloc((size_t)hdr.strtab_size);
    fread(strtab, 1, (size_t)hdr.strtab_size, f);

    // Example: read exports - a flat structure (section 5.1), not TLV records.
    // .p46_types is the only section that needs a TLV-record loop (section 3.5).
    for (uint32_t i = 0; i < hdr.section_count; i++) {
        if (sections[i].type == 2) { // exports
            fseek(f, (long)sections[i].offset, SEEK_SET);
            uint32_t export_count;
            fread(&export_count, sizeof(export_count), 1, f);
            printf("Found exports section: %u exports\\n", export_count);
            
            // Example: read first export address with variable width
            uint8_t addr_buf[8] = {0};
            fread(addr_buf, 1, hdr.address_size, f);
            uint64_t addr = read_address(addr_buf, hdr.address_size);
            printf("First export address: 0x%llx\\n", (unsigned long long)addr);
            break;
        }
    }

    free(strtab);
    free(sections);
    return 0;
}
```

---

## Appendix D: Reflection API Usage Example

```c
#include <reflect.h>

void print_struct(void* ptr, reflect_type* t) {
    if (!t) return;

    for (int i = 0; i < t->field_count; i++) {
        reflect_field* f = &t->fields[i];
        printf("%s = ", f->name);

        switch (f->type->kind) {
            case REFLECT_KIND_STRUCT:
                print_struct((char*)ptr + f->offset, f->type);
                break;
            case REFLECT_KIND_PRIMITIVE:
                switch (f->type->id) {
                    case 3: // u32
                        printf("%u\\n", *(uint32_t*)((char*)ptr + f->offset));
                        break;
                    case 4: // u64
                        printf("%llu\\n", *(uint64_t*)((char*)ptr + f->offset));
                        break;
                    // ...
                }
                break;
            case REFLECT_KIND_ARRAY:
                printf("array[%u]\\n", f->type->array_count);
                break;
        }
    }
}

int main() {
    reflect_type* t = reflect_get_qualified_type("mymodule:Foo", 2);
    if (t) {
        void* obj = get_object(); // some pointer
        print_struct(obj, t);
        reflect_free_type(t);
    }
    return 0;
}
```

---

## Appendix E: Glossary

| Term | Definition |
|------|------------|
| ABI | Application Binary Interface - low-level interface between modules |
| Reflection | Ability to inspect types and fields at runtime |
| Versioning | Mechanism to track changes in types and functions over time |
| Qualifying Entity | Term used in UOPL (not part of this standard) |
| TLV | Type-Length-Value - extensible data encoding format |
| Isolated Namespace | Each module's symbols do not conflict with other modules' symbols |
| String Table | Centralized storage for null-terminated strings, referenced by offset |
| Typedef | Named alias for another type |

---

**End of Specification**
