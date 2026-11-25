(function_definition
  declarator: (function_declarator
    declarator: (identifier) @name)) @item

(function_declarator
  declarator: (identifier) @name) @item

(struct_specifier
  name: (type_identifier) @name) @item

(union_specifier
  name: (type_identifier) @name) @item

(enum_specifier
  name: (type_identifier) @name) @item

(enumerator
  name: (identifier) @name) @item

; Only file-scope variables belong in the outline. An unscoped
; `(init_declarator)` rule would list every local in every function body,
; which drowns out functions and types in real Wand programs.
(translation_unit
  (declaration
    declarator: (identifier) @name) @item)

(translation_unit
  (declaration
    declarator: (init_declarator
      declarator: (identifier) @name)) @item)

(translation_unit
  (declaration
    declarator: (array_declarator
      declarator: (identifier) @name)) @item)
