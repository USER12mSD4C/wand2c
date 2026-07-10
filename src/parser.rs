use crate::ast::*;
use crate::lexer::Lexer;
use crate::token::Token;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

pub struct Parser {
    lexer: Lexer,
    current_token: Token,
    current_span: Span,
    peek_token: Token,
    peek_span: Span,
}

impl Parser {
    pub fn new(lexer: Lexer) -> Self {
        let mut p = Self {
            lexer,
            current_token: Token::EOF,
            current_span: Span {
                line: 1,
                col: 1,
                start: 0,
                end: 0,
            },
            peek_token: Token::EOF,
            peek_span: Span {
                line: 1,
                col: 1,
                start: 0,
                end: 0,
            },
        };
        p.step();
        p.step();
        p
    }

    fn step(&mut self) {
        self.current_token = self.peek_token.clone();
        self.current_span = self.peek_span;

        let (tok, span) = self.lexer.next_token_with_span();
        self.peek_token = tok;
        self.peek_span = span;
    }

    fn err(&self, msg: &str) -> ParseError {
        ParseError {
            message: msg.to_string(),
            span: self.current_span,
        }
    }

    fn is_current_token_type(&self) -> bool {
        match &self.current_token {
            Token::TypeU8
            | Token::TypeU16
            | Token::TypeU32
            | Token::TypeU64
            | Token::TypeI8
            | Token::TypeI16
            | Token::TypeI32
            | Token::TypeI64
            | Token::TypeF64
            | Token::TypeVoid => true,
            Token::Ident(name) => match &self.peek_token {
                Token::Ident(_)
                | Token::PtrInputModifier(_)
                | Token::PtrOutputModifier(_)
                | Token::OpMul => true,
                _ => false,
            },
            _ => false,
        }
    }

    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut program = Program {
            use_os: true,
            imports: Vec::new(),
            typedefs: Vec::new(),
            structs: Vec::new(),
            sections: Vec::new(),
            functions: Vec::new(),
        };

        if self.current_token == Token::ScTrue {
            program.use_os = true;
            self.step();
        } else if self.current_token == Token::ScFalse {
            program.use_os = false;
            self.step();
        } else {
            return Err(self.err("Program must start with sc.true or sc.false"));
        }

        while self.current_token != Token::EOF {
            match &self.current_token {
                Token::Import => {
                    self.step(); // consume 'import'

                    let mut import_path = String::new();
                    while self.current_token != Token::Semicolon && self.current_token != Token::EOF
                    {
                        match &self.current_token {
                            Token::Ident(name) => import_path.push_str(name),
                            Token::Dot => import_path.push('.'),
                            Token::Colon => import_path.push(':'),
                            Token::OpDiv => import_path.push('/'),
                            Token::OpLt => import_path.push('<'),
                            Token::OpGt => import_path.push('>'),
                            _ => break,
                        }
                        self.step();
                    }

                    if import_path.is_empty() {
                        return Err(self.err("Expected import path after #import"));
                    }

                    if import_path.contains('.') {
                        return Err(self.err(
                                                    "Import path must not contain file extensions (such as .h, .w or .wlib). \
                                                    WandC expects logical module names."
                                                ));
                    }

                    program.imports.push(import_path);

                    if self.current_token == Token::Semicolon {
                        self.step();
                    }
                }
                Token::Union => {
                    self.step(); // consume 'union'
                    let name = match &self.current_token {
                        Token::Ident(n) => n.clone(),
                        _ => return Err(self.err("Expected union name")),
                    };
                    self.step();

                    let mut version = 1;
                    if self.current_token == Token::Version {
                        self.step();
                        if let Token::Number(v) = self.current_token {
                            version = v as u32;
                            self.step();
                        }
                    }

                    if self.current_token != Token::LBrace {
                        return Err(self.err("Expected '{' after union declaration"));
                    }
                    self.step();

                    let mut fields = Vec::new();
                    while self.current_token != Token::RBrace && self.current_token != Token::EOF {
                        let field_type = self.parse_data_type()?;
                        let field_name = match &self.current_token {
                            Token::Ident(n) => n.clone(),
                            _ => return Err(self.err("Expected field name")),
                        };
                        self.step();

                        let mut f_version_added = 1;
                        let f_version_removed = 0xFFFFFFFF;

                        if self.current_token == Token::Version {
                            self.step();
                            if let Token::Number(v) = self.current_token {
                                f_version_added = v as u32;
                                self.step();
                            }
                        }

                        fields.push(FieldDecl {
                            name: field_name,
                            data_type: field_type,
                            version_added: f_version_added,
                            version_removed: f_version_removed,
                        });

                        if self.current_token == Token::Semicolon {
                            self.step();
                        }
                    }
                    self.step();

                    program.structs.push(StructDecl {
                        name,
                        version,
                        fields,
                        is_union: true, // Указываем, что это объединение
                    });
                }
                Token::Enum => {
                    self.step(); // consume 'enum'
                    let _name = match &self.current_token {
                        Token::Ident(n) => n.clone(),
                        _ => return Err(self.err("Expected enum name")),
                    };
                    self.step();

                    if self.current_token == Token::Version {
                        self.step();
                        if let Token::Number(_) = self.current_token {
                            self.step();
                        }
                    }

                    if self.current_token != Token::LBrace {
                        return Err(self.err("Expected '{' after enum name"));
                    }
                    self.step();

                    while self.current_token != Token::RBrace && self.current_token != Token::EOF {
                        let _val_name = match &self.current_token {
                            Token::Ident(n) => n.clone(),
                            _ => return Err(self.err("Expected enum value identifier")),
                        };
                        self.step();

                        if self.current_token == Token::OpAssign {
                            self.step();
                            if let Token::Number(_) = self.current_token {
                                self.step();
                            }
                        }

                        if self.current_token == Token::Version {
                            self.step();
                            if let Token::Number(_) = self.current_token {
                                self.step();
                            }
                        }

                        if self.current_token == Token::Comma {
                            self.step();
                        }
                    }
                    self.step();
                }
                Token::Typedef => {
                    self.step();
                    let underlying = self.parse_data_type()?;
                    let alias_name = match &self.current_token {
                        Token::Ident(n) => n.clone(),
                        _ => return Err(self.err("Expected alias name in typedef")),
                    };
                    self.step();
                    if self.current_token == Token::Semicolon {
                        self.step();
                    }
                    program.typedefs.push((alias_name, underlying));
                }
                Token::Struct => {
                    let s_decl = self.parse_struct_decl()?;
                    program.structs.push(s_decl);
                }
                Token::Sect => {
                    let sect_decl = self.parse_section_decl()?;
                    program.sections.push(sect_decl);
                }
                Token::Fn => {
                    let func_decl = self.parse_function_signature()?;
                    program.functions.push(func_decl);
                }
                _ => self.step(),
            }
        }

        Ok(program)
    }

    pub fn seek_to_function(&mut self, name: &str) -> Result<(), ParseError> {
        while self.current_token != Token::EOF {
            if self.current_token == Token::Fn {
                self.step();
                if let Token::Ident(n) = &self.current_token {
                    if n == name {
                        while self.current_token != Token::LBrace
                            && self.current_token != Token::EOF
                        {
                            self.step();
                        }
                        return Ok(());
                    }
                }
            }
            self.step();
        }
        Err(self.err(&format!("Function {} not found for second pass", name)))
    }

    pub fn parse_function_body(&mut self) -> Result<Vec<Stmt>, ParseError> {
        if self.current_token != Token::LBrace {
            return Err(self.err("Expected '{' to start function body"));
        }
        self.step();

        let mut stmts = Vec::new();
        while self.current_token != Token::RBrace && self.current_token != Token::EOF {
            stmts.push(self.parse_statement()?);
        }

        if self.current_token == Token::RBrace {
            self.step();
        }

        Ok(stmts)
    }

    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        match &self.current_token {
            Token::If => self.parse_if_statement(),
            Token::While => self.parse_while_statement(),
            Token::For => self.parse_for_statement(),
            Token::Jmpto => self.parse_jmpto_statement(),

            Token::TypeU8
            | Token::TypeU16
            | Token::TypeU32
            | Token::TypeU64
            | Token::TypeI8
            | Token::TypeI16
            | Token::TypeI32
            | Token::TypeI64
            | Token::TypeVoid => {
                let dt = self.parse_data_type()?;
                let var_decl = self.parse_var_decl_tail(dt)?;
                if self.current_token == Token::Semicolon {
                    self.step();
                }
                Ok(Stmt::VarDefinition(var_decl))
            }

            Token::Ident(ref name) => {
                if name == "array" {
                    let dt = self.parse_data_type()?;
                    let var_decl = self.parse_var_decl_tail(dt)?;
                    if self.current_token == Token::Semicolon {
                        self.step();
                    }
                    return Ok(Stmt::VarDefinition(var_decl));
                }

                let is_decl = match &self.peek_token {
                    Token::Ident(_)
                    | Token::PtrInputModifier(_)
                    | Token::PtrOutputModifier(_)
                    | Token::OpMul => true,
                    _ => false,
                };

                if is_decl {
                    let dt = self.parse_data_type()?;
                    let var_decl = self.parse_var_decl_tail(dt)?;
                    if self.current_token == Token::Semicolon {
                        self.step();
                    }
                    Ok(Stmt::VarDefinition(var_decl))
                } else {
                    self.parse_expr_statement()
                }
            }

            Token::Return => {
                self.step();
                if self.current_token != Token::LParen {
                    return Err(self.err("Expected '(' after return"));
                }
                self.step();
                let mut return_vals = Vec::new();
                while self.current_token != Token::RParen && self.current_token != Token::EOF {
                    let dt = if self.is_current_token_type() {
                        self.parse_data_type()?
                    } else {
                        DataType::Void
                    };

                    let val_expr = self.parse_expr()?;
                    return_vals.push((dt, val_expr));
                    if self.current_token == Token::Comma {
                        self.step();
                    }
                }
                self.step();
                if self.current_token == Token::Semicolon {
                    self.step();
                }
                Ok(Stmt::Return(return_vals))
            }
            Token::LBracket => {
                self.step();
                let mut targets = Vec::new();
                while self.current_token != Token::RBracket && self.current_token != Token::EOF {
                    let expr = self.parse_expr()?;
                    targets.push(expr);
                    if self.current_token == Token::Comma {
                        self.step();
                    }
                }
                self.step();
                if self.current_token != Token::OpAssign {
                    return Err(self.err("Expected '=' after destructuring targets"));
                }
                self.step();
                let expr = self.parse_expr()?;
                if self.current_token == Token::Semicolon {
                    self.step();
                }
                Ok(Stmt::Assignment {
                    targets,
                    value: expr,
                })
            }
            Token::NasmBlock(code) => {
                let asm_code = code.clone();
                self.step();
                Ok(Stmt::Nasm(asm_code))
            }
            _ => self.parse_expr_statement(),
        }
    }

    fn parse_jmpto_statement(&mut self) -> Result<Stmt, ParseError> {
        self.step(); // consume 'jmpto'

        let mut module_name = String::new();
        match &self.current_token {
            Token::StringLiteral(s) => {
                module_name = s.clone();
                self.step();
            }
            Token::Ident(n) => {
                module_name.push_str(n);
                self.step();
                if self.current_token == Token::Dot {
                    self.step();
                    module_name.push('.');
                    if let Token::Ident(ref ext) = self.current_token {
                        module_name.push_str(ext);
                        self.step();
                    }
                }
            }
            _ => return Err(self.err("Expected module name or string literal after 'jmpto'")),
        }

        if self.current_token != Token::LBrace {
            return Err(self.err("Expected '{' after jmpto module name"));
        }
        self.step();

        let mut args = Vec::new();
        while self.current_token != Token::RBrace && self.current_token != Token::EOF {
            args.push(self.parse_statement()?);
        }

        if self.current_token == Token::RBrace {
            self.step();
        }

        Ok(Stmt::Jmpto { module_name, args })
    }

    fn parse_if_statement(&mut self) -> Result<Stmt, ParseError> {
        self.step();
        if self.current_token != Token::LParen {
            return Err(self.err("Expected '(' after 'if'"));
        }
        self.step();
        let cond = self.parse_expr()?;
        if self.current_token != Token::RParen {
            return Err(self.err("Expected ')' after 'if' condition"));
        }
        self.step();

        if self.current_token != Token::LBrace {
            return Err(self.err("Expected '{' to start 'if' block"));
        }
        self.step();

        let mut then_branch = Vec::new();
        while self.current_token != Token::RBrace && self.current_token != Token::EOF {
            then_branch.push(self.parse_statement()?);
        }
        if self.current_token == Token::RBrace {
            self.step();
        }

        let mut else_branch = None;
        if self.current_token == Token::Else {
            self.step();
            if self.current_token == Token::LBrace {
                self.step();
                let mut else_stmts = Vec::new();
                while self.current_token != Token::RBrace && self.current_token != Token::EOF {
                    else_stmts.push(self.parse_statement()?);
                }
                if self.current_token == Token::RBrace {
                    self.step();
                }
                else_branch = Some(else_stmts);
            } else {
                let stmt = self.parse_statement()?;
                else_branch = Some(vec![stmt]);
            }
        }

        Ok(Stmt::If {
            cond,
            then_branch,
            else_branch,
        })
    }

    fn parse_while_statement(&mut self) -> Result<Stmt, ParseError> {
        self.step();
        if self.current_token != Token::LParen {
            return Err(self.err("Expected '(' after 'while'"));
        }
        self.step();
        let cond = self.parse_expr()?;
        if self.current_token != Token::RParen {
            return Err(self.err("Expected ')' after 'while' condition"));
        }
        self.step();

        if self.current_token != Token::LBrace {
            return Err(self.err("Expected '{' to start 'while' block"));
        }
        self.step();

        let mut body = Vec::new();
        while self.current_token != Token::RBrace && self.current_token != Token::EOF {
            body.push(self.parse_statement()?);
        }
        if self.current_token == Token::RBrace {
            self.step();
        }

        Ok(Stmt::While { cond, body })
    }

    fn parse_for_statement(&mut self) -> Result<Stmt, ParseError> {
        self.step();
        if self.current_token != Token::LParen {
            return Err(self.err("Expected '(' after 'for'"));
        }
        self.step();

        let mut init = None;
        if self.current_token != Token::Semicolon {
            init = Some(Box::new(self.parse_statement()?));
        } else {
            self.step();
        }

        let cond = if self.current_token != Token::Semicolon {
            let e = self.parse_expr()?;
            self.step();
            e
        } else {
            self.step();
            Expr::Number(1)
        };

        let mut post = None;
        if self.current_token != Token::RParen {
            let stmt = self.parse_expr_statement()?;
            post = Some(Box::new(stmt));
        }
        if self.current_token != Token::RParen {
            return Err(self.err("Expected ')' at the end of 'for' clause"));
        }
        self.step();

        if self.current_token != Token::LBrace {
            return Err(self.err("Expected '{' after 'for' clause"));
        }
        self.step();

        let mut body = Vec::new();
        while self.current_token != Token::RBrace && self.current_token != Token::EOF {
            body.push(self.parse_statement()?);
        }
        if self.current_token == Token::RBrace {
            self.step();
        }

        Ok(Stmt::For {
            init,
            cond,
            post,
            body,
        })
    }

    fn parse_expr_statement(&mut self) -> Result<Stmt, ParseError> {
        let left = self.parse_expr()?;

        if self.current_token == Token::OpInc {
            self.step();
            if self.current_token == Token::Semicolon {
                self.step();
            }
            return Ok(Stmt::Assignment {
                targets: vec![left.clone()],
                value: Expr::Binary {
                    left: Box::new(left.clone()),
                    op: "OpAdd".to_string(),
                    right: Box::new(Expr::Number(1)),
                },
            });
        }

        if self.current_token == Token::OpDec {
            self.step();
            if self.current_token == Token::Semicolon {
                self.step();
            }
            return Ok(Stmt::Assignment {
                targets: vec![left.clone()],
                value: Expr::Binary {
                    left: Box::new(left.clone()),
                    op: "OpSub".to_string(),
                    right: Box::new(Expr::Number(1)),
                },
            });
        }

        if self.current_token == Token::OpAssign {
            self.step();
            let right = self.parse_expr()?;
            if self.current_token == Token::Semicolon {
                self.step();
            }
            return Ok(Stmt::Assignment {
                targets: vec![left],
                value: right,
            });
        }
        if self.current_token == Token::Semicolon {
            self.step();
        }
        Ok(Stmt::Expr(left))
    }

    fn parse_struct_decl(&mut self) -> Result<StructDecl, ParseError> {
        self.step();
        let name = match &self.current_token {
            Token::Ident(n) => n.clone(),
            _ => return Err(self.err("Expected struct name")),
        };
        self.step();

        let mut version = 1;
        if self.current_token == Token::Version {
            self.step();
            if let Token::Number(v) = self.current_token {
                version = v as u32;
                self.step();
            }
        }

        if self.current_token != Token::LBrace {
            return Err(self.err("Expected '{' after struct declaration"));
        }
        self.step();

        let mut fields = Vec::new();
        while self.current_token != Token::RBrace && self.current_token != Token::EOF {
            let field_type = self.parse_data_type()?;
            let field_name = match &self.current_token {
                Token::Ident(n) => n.clone(),
                _ => return Err(self.err("Expected field name")),
            };
            self.step();

            let mut f_version_added = 1;
            let f_version_removed = 0xFFFFFFFF;

            if self.current_token == Token::Version {
                self.step();
                if let Token::Number(v) = self.current_token {
                    f_version_added = v as u32;
                    self.step();
                }
            }

            fields.push(FieldDecl {
                name: field_name,
                data_type: field_type,
                version_added: f_version_added,
                version_removed: f_version_removed,
            });

            if self.current_token == Token::Semicolon {
                self.step();
            }
        }
        self.step();

        Ok(StructDecl {
            name,
            version,
            fields,
            is_union: false,
        })
    }

    fn parse_section_decl(&mut self) -> Result<SectionDecl, ParseError> {
        self.step();
        if self.current_token == Token::Dot {
            self.step();
        }
        let name = match &self.current_token {
            Token::Ident(n) => n.clone(),
            _ => return Err(self.err("Expected section name")),
        };
        self.step();

        let mut variables = Vec::new();
        while self.current_token != Token::Eos && self.current_token != Token::EOF {
            let var_type = self.parse_data_type()?;
            let var_decl = self.parse_var_decl_tail(var_type)?;
            variables.push(var_decl);
            if self.current_token == Token::Semicolon {
                self.step();
            }
        }
        self.step();

        Ok(SectionDecl { name, variables })
    }

    fn parse_function_signature(&mut self) -> Result<FuncDecl, ParseError> {
        self.step();
        let name = match &self.current_token {
            Token::Ident(n) => n.clone(),
            _ => return Err(self.err("Expected function name")),
        };
        self.step();

        if self.current_token != Token::LParen {
            return Err(self.err("Expected '(' after function name"));
        }
        self.step();

        let mut params = Vec::new();
        while self.current_token != Token::RParen && self.current_token != Token::EOF {
            let p_type = self.parse_data_type()?;
            let (p_name, p_access) = match &self.current_token {
                Token::Ident(n) => (n.clone(), PtrAccess::Normal),
                Token::PtrInputModifier(n) => (n.clone(), PtrAccess::Input),
                Token::PtrOutputModifier(n) => (n.clone(), PtrAccess::Output),
                _ => return Err(self.err("Expected parameter name")),
            };
            self.step();

            params.push((p_type, p_name, p_access));

            if self.current_token == Token::Comma {
                self.step();
            }
        }
        self.step();

        let mut return_types = Vec::new();
        if self.current_token == Token::Arrow {
            self.step();
            if self.current_token == Token::LParen {
                self.step();
                while self.current_token != Token::RParen && self.current_token != Token::EOF {
                    return_types.push(self.parse_data_type()?);
                    if self.current_token == Token::Comma {
                        self.step();
                    }
                }
                self.step();
            } else {
                return_types.push(self.parse_data_type()?);
            }
        }

        if self.current_token == Token::Semicolon {
            self.step();
            return Ok(FuncDecl {
                name,
                params,
                return_types,
                body: None,
            });
        }

        if self.current_token == Token::LBrace {
            self.read_until_matching_brace()?;
        }

        Ok(FuncDecl {
            name,
            params,
            return_types,
            body: None,
        })
    }

    fn parse_var_decl_tail(&mut self, base_type: DataType) -> Result<VarDecl, ParseError> {
        let (name, modifier) = match &self.current_token {
            Token::Ident(n) => (n.clone(), PtrAccess::Normal),
            Token::PtrInputModifier(n) => (n.clone(), PtrAccess::Input),
            Token::PtrOutputModifier(n) => (n.clone(), PtrAccess::Output),
            _ => return Err(self.err("Expected variable name")),
        };
        self.step();

        let mut initial_value = None;
        if self.current_token == Token::OpAssign {
            self.step();
            initial_value = Some(Box::new(self.parse_expr()?));
        }

        Ok(VarDecl {
            name,
            data_type: base_type,
            modifier,
            initial_value,
        })
    }

    fn parse_data_type(&mut self) -> Result<DataType, ParseError> {
        if let Token::Ident(ref name) = self.current_token {
            if name == "array" && self.peek_token == Token::Colon {
                self.step(); // array
                self.step(); // :
                let elem_type = self.parse_data_type()?;
                if self.current_token != Token::LBracket {
                    return Err(self.err("Expected '[' in array declaration"));
                }
                self.step();
                let count = match self.current_token {
                    Token::Number(n) => n as usize,
                    _ => return Err(self.err("Expected array count")),
                };
                self.step();
                if self.current_token != Token::RBracket {
                    return Err(self.err("Expected ']' in array declaration"));
                }
                self.step();
                return Ok(DataType::Array(Box::new(elem_type), count));
            }
        }

        let mut dt = match self.current_token {
            Token::TypeU8 => DataType::U8,
            Token::TypeU16 => DataType::U16,
            Token::TypeU32 => DataType::U32,
            Token::TypeU64 => DataType::U64,
            Token::TypeI8 => DataType::I8,
            Token::TypeI16 => DataType::I16,
            Token::TypeI32 => DataType::I32,
            Token::TypeI64 => DataType::I64,
            Token::TypeF64 => DataType::F64,
            Token::TypeVoid => DataType::Void,
            Token::Ident(ref name) => DataType::Struct(name.clone()),
            _ => return Err(self.err("Unexpected token for data type")),
        };
        self.step();

        if self.current_token == Token::LBracket {
            self.step(); // [
            let count = match self.current_token {
                Token::Number(n) => n as usize,
                _ => return Err(self.err("Expected array size number")),
            };
            self.step();
            if self.current_token != Token::RBracket {
                return Err(self.err("Expected ']' after array size"));
            }
            self.step();
            dt = DataType::Array(Box::new(dt), count);
        }

        if self.current_token == Token::OpMul {
            self.step();
            dt = DataType::Pointer(Box::new(dt));
        }
        Ok(dt)
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_binary_expr(0)
    }

    fn parse_binary_expr(&mut self, prec: u8) -> Result<Expr, ParseError> {
        let mut left = self.parse_primary_expr()?;

        while let Some(op_prec) = self.get_tok_precedence() {
            if op_prec < prec {
                break;
            }

            let op = format!("{:?}", self.current_token);
            self.step();
            let right = self.parse_binary_expr(op_prec + 1)?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        match &self.current_token {
            Token::OpBitNot => {
                self.step();
                let expr = self.parse_primary_expr()?;
                Ok(Expr::Binary {
                    left: Box::new(expr),
                    op: "OpBitNot".to_string(),
                    right: Box::new(Expr::Number(0)),
                })
            }
            Token::Number(val) => {
                let v = *val;
                self.step();
                Ok(Expr::Number(v))
            }
            Token::StringLiteral(s) => {
                let s_val = s.clone();
                self.step();
                Ok(Expr::StringLit(s_val))
            }
            Token::LParen => {
                self.step(); // consume '('

                // Проверяем явное приведение типов: (Type)value
                if self.is_current_token_type() {
                    let cast_type = self.parse_data_type()?;
                    if self.current_token != Token::RParen {
                        return Err(self.err("Expected ')' after cast type"));
                    }
                    self.step(); // consume ')'
                    let inner_expr = self.parse_primary_expr()?;
                    return Ok(Expr::Binary {
                        left: Box::new(inner_expr),
                        op: "OpCast".to_string(),
                        right: Box::new(Expr::Number(0)),
                    });
                }

                let expr = self.parse_expr()?;
                if self.current_token != Token::RParen {
                    return Err(self.err("Expected ')' after parenthesized expression"));
                }
                self.step(); // consume ')'
                Ok(expr)
            }
            Token::Ident(name) => {
                let name_val = name.clone();
                self.step();

                if self.current_token == Token::Colon {
                    self.step();
                    if let Token::Ident(var) = &self.current_token {
                        let var_name = var.clone();
                        self.step();
                        return Ok(Expr::SectionAccess {
                            section: name_val,
                            variable: var_name,
                        });
                    }
                }

                if self.current_token == Token::LParen {
                    self.step();
                    let mut args = Vec::new();
                    while self.current_token != Token::RParen && self.current_token != Token::EOF {
                        args.push(self.parse_expr()?);
                        if self.current_token == Token::Comma {
                            self.step();
                        }
                    }
                    self.step();
                    return Ok(Expr::Call {
                        name: name_val,
                        args,
                    });
                }

                let mut current_expr = Expr::Variable(name_val);
                while self.current_token == Token::Arrow
                    || self.current_token == Token::Dot
                    || self.current_token == Token::LBracket
                {
                    if self.current_token == Token::LBracket {
                        self.step();
                        let index = self.parse_expr()?;
                        if self.current_token != Token::RBracket {
                            return Err(self.err("Expected ']' after array index"));
                        }
                        self.step();
                        current_expr = Expr::Index {
                            expr: Box::new(current_expr),
                            index: Box::new(index),
                        };
                    } else {
                        let is_arrow = self.current_token == Token::Arrow;
                        self.step();
                        if let Token::Ident(member) = &self.current_token {
                            current_expr = Expr::MemberAccess {
                                expr: Box::new(current_expr),
                                member: member.clone(),
                                is_arrow,
                            };
                            self.step();
                        } else {
                            return Err(self.err("Expected member identifier"));
                        }
                    }
                }

                Ok(current_expr)
            }
            Token::AddrOf(name) => {
                let name_val = name.clone();
                self.step();
                Ok(Expr::AddrOf(name_val))
            }
            Token::FloatLiteral(s) => {
                let s_val = s.clone();
                self.step();
                Ok(Expr::FloatLit(s_val))
            }
            Token::Null => {
                self.step();
                Ok(Expr::Null)
            }
            _ => Err(self.err("Unexpected primary expression")),
        }
    }

    fn get_tok_precedence(&self) -> Option<u8> {
        match self.current_token {
            Token::OpMul | Token::OpDiv | Token::OpMod => Some(4),
            Token::OpAdd | Token::OpSub => Some(5),
            Token::OpShl | Token::OpShr => Some(5),
            Token::OpLt | Token::OpLtEq | Token::OpGt | Token::OpGtEq => Some(6),
            Token::OpEq | Token::OpNotEq => Some(7),
            Token::OpBitAnd => Some(8),
            Token::OpBitXor => Some(9),
            Token::OpBitOr => Some(10),
            Token::OpAnd => Some(11),
            Token::OpOr => Some(12),
            _ => None,
        }
    }

    fn read_until_matching_brace(&mut self) -> Result<(), ParseError> {
        let mut depth = 0;
        loop {
            match self.current_token {
                Token::LBrace => depth += 1,
                Token::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        self.step();
                        break;
                    }
                }
                Token::EOF => return Err(self.err("Unbalanced braces")),
                _ => {}
            }
            self.step();
        }
        Ok(())
    }
}
