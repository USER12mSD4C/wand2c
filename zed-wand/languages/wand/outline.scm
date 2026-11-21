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

(init_declarator
  declarator: (identifier) @name) @item
