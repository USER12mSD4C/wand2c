[
  (compound_statement)
  (argument_list)
  (parameter_list)
  (enumerator_list)
  (field_declaration_list)
  (initializer_list)
  (preproc_if)
  (preproc_ifdef)
] @indent

; Used by decrease_indent_patterns in config.toml so `else` aligns with `if`.
(if_statement) @start.if
