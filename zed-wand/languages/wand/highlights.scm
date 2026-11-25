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

; Some real-world Wand sources use "# ..." comment lines. The C parser reads
; them as preprocessor calls, so dim them as comments instead. `#import` is
; excluded because its directive has no space after '#'. The compiler accepts
; both `//` and `# ...` comments; this rule only fixes editor highlighting.
((preproc_directive) @hash.directive @comment
  (#match? @hash.directive "^#[ \\t]"))

((preproc_call
   directive: (preproc_directive) @hash.directive
   argument: (preproc_arg) @comment)
  (#match? @hash.directive "^#[ \\t]"))

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

; Pointer flow modifiers: *i, *o, *io, *adr.
; Require the `*` to touch the modifier: `4 * i` is ordinary multiplication,
; while `value*io` and `buffer*adr` are Wand address/modifier syntax.
((identifier) @operator
  (#eq? @operator "adr"))
((binary_expression
    operator: "*"
    right: (identifier) @operator) @modifier.form
  (#match? @modifier.form "[*](i|o|io|adr)$")
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
  (#match? @function.builtin "^(fieldsof|versionof|nameof|syscall[0-6]|bmloc|atomic_[a-z_]+|memory_barrier|compiler_barrier|in[bwl]|out[bwl]|print_[a-z_]+|read_[a-z_]+|file_[a-z_]+|sys_[a-z_]+)$"))

; ---------------------------------------------------------------------------
; Fields and members
; ---------------------------------------------------------------------------

(field_expression
  field: (field_identifier) @property)

(field_identifier) @property

; ---------------------------------------------------------------------------
; Late overrides.
; Lines such as `fn main(...)` and `EOS` are aliased to type/field/statement
; identifiers by the C grammar. Error recovery also aliases keywords (including
; C keywords Wand shares) to plain identifiers. Repeat these matches after the
; generic rules so the Wand scope wins over `(identifier) @variable`.
; ---------------------------------------------------------------------------

([
  (identifier)
  (type_identifier)
  (field_identifier)
  (statement_identifier)
] @type.builtin
  (#match? @type.builtin "^(u8|u16|u32|u64|i8|i16|i32|i64|f64|void)$"))

; `sc.true` / `sc.false` are parsed as a field expression; the generic
; `(field_identifier) @property` rule must not win over the builtin constant.
([
  (identifier)
  (type_identifier)
  (field_identifier)
  (statement_identifier)
] @constant.builtin
  (#match? @constant.builtin "^(sc|null|true|false)$"))

([
  (identifier)
  (type_identifier)
  (field_identifier)
  (statement_identifier)
] @keyword.control
  (#match? @keyword.control "^(break|case|continue|default|else|for|if|return|while)$"))

([
  (identifier)
  (type_identifier)
  (field_identifier)
  (statement_identifier)
] @keyword
  (#match? @keyword "^(fn|sect|EOS|jmpto|packed|export|atomic|critical|irq|align|ro|noinit|match|version|nasm|const|enum|extern|struct|typedef|union|volatile)$"))
