; Wand highlighting on top of Zed's built-in C grammar.
; The C parser supplies the skeleton; these queries rename the shared tokens
; and catch Wand-only words that the C grammar reports as identifiers.

; ---------------------------------------------------------------------------
; Comments and literals
; ---------------------------------------------------------------------------

(comment) @comment

[
  (string_literal)
  (concatenated_string)
  (system_lib_string)
  (char_literal)
] @string

(escape_sequence) @string.escape
(number_literal) @number

; #import <io>, #import "module"
(preproc_directive) @keyword.preproc @preproc
(preproc_arg) @string.special

; ---------------------------------------------------------------------------
; Tokens shared by C and Wand
; ---------------------------------------------------------------------------

[
  "const"
  "enum"
  "extern"
  "struct"
  "typedef"
  "union"
  "volatile"
] @keyword

[
  "break"
  "case"
  "continue"
  "default"
  "else"
  "for"
  "if"
  "return"
  "while"
] @keyword.control

; Reflection builtins that are real tokens in the C grammar.
[
  "sizeof"
  "alignof"
  "offsetof"
] @function.builtin

[
  "="
  "+="
  "-="
  "*="
  "/="
  "%="
  "&="
  "|="
  "^="
  "<<="
  ">>="
  "++"
  "--"
  "+"
  "-"
  "*"
  "/"
  "%"
  "~"
  "&"
  "|"
  "^"
  "<<"
  ">>"
  "!"
  "&&"
  "||"
  "=="
  "!="
  "<"
  ">"
  "<="
  ">="
  "->"
] @operator

[
  "."
  ";"
  ","
  ":"
] @punctuation.delimiter

[
  "{"
  "}"
  "("
  ")"
  "["
  "]"
] @punctuation.bracket

; ---------------------------------------------------------------------------
; Generic fallbacks first, Wand-specific overrides below
; ---------------------------------------------------------------------------

(identifier) @variable
(type_identifier) @type
(primitive_type) @type

; ---------------------------------------------------------------------------
; Wand-only keywords (the C grammar sees these as identifiers)
; ---------------------------------------------------------------------------

; Environment token and null pointer constant.
((identifier) @constant.builtin
  (#match? @constant.builtin "^(sc|null)$"))
((type_identifier) @constant.builtin
  (#match? @constant.builtin "^(sc|null)$"))
((field_identifier) @constant.builtin
  (#match? @constant.builtin "^(true|false)$"))

[
  (true)
  (false)
  (null)
] @constant.builtin

; Wand primitive types.
((identifier) @type.builtin
  (#match? @type.builtin "^(u8|u16|u32|u64|i8|i16|i32|i64|f64|void)$"))
((type_identifier) @type.builtin
  (#match? @type.builtin "^(u8|u16|u32|u64|i8|i16|i32|i64|f64|void)$"))

; Pointer flow modifiers: *i, *o, *io, *adr.
; `adr` is unambiguous enough to catch everywhere; i/o/io are caught only
; when they appear on the right of a `*` binary expression.
((identifier) @operator
  (#eq? @operator "adr"))
((binary_expression
    operator: "*"
    right: (identifier) @operator)
  (#match? @operator "^(i|o|io|adr)$"))

; ---------------------------------------------------------------------------
; Functions, calls, and builtins
; ---------------------------------------------------------------------------

(call_expression
  function: (identifier) @function)

(function_declarator
  declarator: (identifier) @function)

((call_expression
    function: (identifier) @function.builtin)
  (#match? @function.builtin "^(fieldsof|versionof|nameof|syscall[0-6]|atomic_[a-z_]+|memory_barrier|compiler_barrier|inb|outb|print_[a-z_]+|read_[a-z_]+|file_[a-z_]+|sys_[a-z_]+)$"))

; ---------------------------------------------------------------------------
; Fields and members
; ---------------------------------------------------------------------------

(field_expression
  field: (field_identifier) @property)

(field_identifier) @property

; ---------------------------------------------------------------------------
; Late Wand keyword override.
; Lines such as `fn main(...)` and `EOS` are aliased to type/field/statement
; identifiers by the C grammar, so repeat the keyword match over every alias
; after the generic rules above.
; ---------------------------------------------------------------------------

([
  (identifier)
  (type_identifier)
  (field_identifier)
  (statement_identifier)
] @keyword
  (#match? @keyword "^(fn|sect|EOS|jmpto|packed|export|atomic|critical|irq|align|ro|noinit|match|version|nasm)$"))
