(function_definition
  body: (compound_statement) @function.inside) @function.around

(function_declarator) @function.around

(struct_specifier
  body: (field_declaration_list) @class.inside) @class.around

(union_specifier
  body: (field_declaration_list) @class.inside) @class.around

(enum_specifier
  body: (enumerator_list) @class.inside) @class.around

(comment)+ @comment.around
