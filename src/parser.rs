use crate::ast::*;
use crate::lexer::Lexer;
use crate::token::Token;
use std::collections::HashMap;

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
    constants: HashMap<String, u64>,
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
            constants: HashMap::new(),
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

    fn parse_const_decl(&mut self) -> Result<ConstDecl, ParseError> {
        self.step();

        let name = match &self.current_token {
            Token::Ident(n) => n.clone(),
            _ => return Err(self.err("Expected constant name")),
        };

        self.step();

        if self.current_token != Token::OpAssign {
            return Err(self.err("Expected '=' after constant name"));
        }

        self.step();

        let value = self.parse_expr()?;

        if let Ok(v) = self.eval_parser_const_expr(&value) {
            self.constants.insert(name.clone(), v);
        }

        if self.current_token == Token::Semicolon {
            self.step();
        }

        Ok(ConstDecl { name, value })
    }

    fn parse_array_size(&mut self) -> Result<usize, ParseError> {
        let expr = self.parse_expr()?;

        match self.eval_parser_const_expr(&expr) {
            Ok(value) => Ok(value as usize),
            Err(_) => Err(self.err("array size must be a compile-time constant number")),
        }
    }

    fn eval_parser_const_expr(&self, expr: &Expr) -> Result<u64, String> {
        self.eval_parser_const_expr_depth(expr, 0)
    }

    fn eval_parser_const_expr_depth(&self, expr: &Expr, depth: usize) -> Result<u64, String> {
        if depth > 64 {
            return Err("constant evaluation depth too high".to_string());
        }

        match expr {
            Expr::Number(n) => Ok(*n),
            Expr::SignedNumber(n) => Ok(*n as u64),
            Expr::Null => Ok(0),

            Expr::Variable(name) => self
                .constants
                .get(name)
                .cloned()
                .ok_or_else(|| format!("unknown constant '{}'", name)),

            Expr::Binary { left, op, right } => {
                if op == "OpCastF64" || op == "OpCastInt" || op == "OpCast" {
                    return self.eval_parser_const_expr_depth(left, depth + 1);
                }

                if op == "OpBitNot" {
                    let value = self.eval_parser_const_expr_depth(left, depth + 1)?;
                    return Ok(!value);
                }

                let a = self.eval_parser_const_expr_depth(left, depth + 1)?;
                let b = self.eval_parser_const_expr_depth(right, depth + 1)?;

                match op.as_str() {
                    "OpAdd" => Ok(a.wrapping_add(b)),
                    "OpSub" => Ok(a.wrapping_sub(b)),
                    "OpMul" => Ok(a.wrapping_mul(b)),

                    "OpDiv" => {
                        if b == 0 {
                            return Err("division by zero in constant expression".to_string());
                        }

                        Ok(a / b)
                    }

                    "OpMod" => {
                        if b == 0 {
                            return Err("modulo by zero in constant expression".to_string());
                        }

                        Ok(a % b)
                    }

                    "OpBitAnd" => Ok(a & b),
                    "OpBitOr" => Ok(a | b),
                    "OpBitXor" => Ok(a ^ b),
                    "OpShl" => Ok(a.wrapping_shl(b as u32)),
                    "OpShr" => Ok(a.wrapping_shr(b as u32)),

                    "OpEq" | "OpEqEq" | "==" => Ok(if a == b { 1 } else { 0 }),
                    "OpNotEq" | "OpNe" | "!=" => Ok(if a != b { 1 } else { 0 }),
                    "OpLt" | "Lt" | "<" => Ok(if a < b { 1 } else { 0 }),
                    "OpLtEq" | "OpLe" | "<=" => Ok(if a <= b { 1 } else { 0 }),
                    "OpGt" | "Gt" | ">" => Ok(if a > b { 1 } else { 0 }),
                    "OpGtEq" | "OpGe" | ">=" => Ok(if a >= b { 1 } else { 0 }),
                    "OpAnd" | "&&" => Ok(if a != 0 && b != 0 { 1 } else { 0 }),
                    "OpOr" | "||" => Ok(if a != 0 || b != 0 { 1 } else { 0 }),

                    _ => Err(format!("unsupported constant operator '{}'", op)),
                }
            }

            _ => Err("unsupported constant expression".to_string()),
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
            Token::Ident(_) => match &self.peek_token {
                Token::Ident(_)
                | Token::PtrInputModifier(_)
                | Token::PtrOutputModifier(_)
                | Token::PtrInputOutputModifier(_) => true,
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
            enums: Vec::new(),
            constants: Vec::new(),
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
                    self.step();
                    let mut import_path = String::new();
                    while self.current_token != Token::Semicolon && self.current_token != Token::EOF
                    {
                        match &self.current_token {
                            Token::Ident(name) => import_path.push_str(name),
                            Token::StringLiteral(s) => import_path.push_str(&format!("\"{}\"", s)),
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
                    let is_system = import_path.starts_with('<') && import_path.ends_with('>');
                    let is_local = import_path.starts_with('"') && import_path.ends_with('"');
                    if !is_system && !is_local {
                        return Err(self.err(
                        "import must use <module> for system modules or \"module\" for local modules",
                    ));
                    }
                    let inner = &import_path[1..import_path.len() - 1];
                    if inner.is_empty() {
                        return Err(self.err("Import path cannot be empty"));
                    }
                    if is_system {
                        if inner.contains('.') || inner.contains('/') {
                            return Err(self.err(
                            "system import must be a logical module name without path separators or extensions",
                        ));
                        }
                    } else {
                        let lower = inner.to_ascii_lowercase();
                        if lower.ends_with(".w")
                            || lower.ends_with(".wh")
                            || lower.ends_with(".wlib")
                            || lower.ends_with(".wexp")
                            || lower.ends_with(".h")
                        {
                            return Err(self.err(
                            "Import path must not contain file extensions (such as .h, .w, .wh, .wlib or .wexp). WandC expects logical module names.",
                        ));
                        }
                    }
                    program.imports.push(import_path);
                    if self.current_token == Token::Semicolon {
                        self.step();
                    }
                }
                Token::Const => {
                    program.constants.push(self.parse_const_decl()?);
                }
                Token::Union => {
                    self.step();
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
                            modifier: PtrAccess::Normal,
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
                        is_union: true,
                        is_packed: false,
                        alignment: 0,
                    });
                }
                Token::Enum => {
                    self.step();
                    let name = match &self.current_token {
                        Token::Ident(n) => n.clone(),
                        _ => return Err(self.err("Expected enum name")),
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
                        return Err(self.err("Expected '{' after enum name"));
                    }
                    self.step();
                    let mut values = Vec::new();
                    let mut next_value: u64 = 0;
                    while self.current_token != Token::RBrace && self.current_token != Token::EOF {
                        let value_name = match &self.current_token {
                            Token::Ident(n) => n.clone(),
                            _ => return Err(self.err("Expected enum value identifier")),
                        };
                        self.step();
                        let mut value = next_value;
                        if self.current_token == Token::OpAssign {
                            self.step();
                            if let Token::Number(n) = self.current_token {
                                value = n;
                                self.step();
                            } else {
                                return Err(self.err("Enum value must be an integer literal"));
                            }
                        }
                        let mut version_added = 1;
                        let version_removed = 0xFFFFFFFF;
                        if self.current_token == Token::Version {
                            self.step();
                            if let Token::Number(v) = self.current_token {
                                version_added = v as u32;
                                self.step();
                            }
                        }
                        values.push(EnumValueDecl {
                            name: value_name,
                            value,
                            version_added,
                            version_removed,
                        });
                        next_value = value.wrapping_add(1);
                        if self.current_token == Token::Semicolon
                            || self.current_token == Token::Comma
                        {
                            self.step();
                        }
                    }
                    self.step();
                    program.enums.push(EnumDecl {
                        name,
                        version,
                        values,
                    });
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

                Token::Align | Token::Ro | Token::Noinit => {
                    let mut sect_alignment = 0u32;
                    let mut sect_ro = false;
                    let mut sect_noinit = false;
                    loop {
                        if self.current_token == Token::Align {
                            self.step();
                            if self.current_token != Token::LParen {
                                return Err(self.err("Expected '(' after align"));
                            }
                            self.step();
                            let alignment = match self.current_token {
                                Token::Number(n) => n as u32,
                                _ => return Err(self.err("Expected alignment number")),
                            };
                            self.step();
                            if alignment == 0 || (alignment & (alignment - 1)) != 0 {
                                return Err(self.err("align value must be a power of two"));
                            }
                            if self.current_token != Token::RParen {
                                return Err(self.err("Expected ')' after alignment"));
                            }
                            self.step();
                            sect_alignment = alignment;
                        } else if self.current_token == Token::Ro {
                            self.step();
                            sect_ro = true;
                        } else if self.current_token == Token::Noinit {
                            self.step();
                            sect_noinit = true;
                        } else {
                            break;
                        }
                    }
                    if self.current_token == Token::Packed {
                        self.step();
                        if self.current_token != Token::Struct {
                            return Err(self.err("Expected 'struct' after 'packed'"));
                        }
                        let mut s_decl = self.parse_struct_decl()?;
                        s_decl.is_packed = true;
                        s_decl.alignment = sect_alignment;
                        program.structs.push(s_decl);
                    } else if self.current_token == Token::Struct {
                        let mut s_decl = self.parse_struct_decl()?;
                        s_decl.alignment = sect_alignment;
                        program.structs.push(s_decl);
                    } else if self.current_token == Token::Sect {
                        let mut sect_decl = self.parse_section_decl()?;
                        sect_decl.alignment = sect_alignment;
                        sect_decl.is_ro = sect_ro;
                        sect_decl.is_noinit = sect_noinit;
                        program.sections.push(sect_decl);
                    } else {
                        return Err(self.err("align/ro/noinit requires struct or sect"));
                    }
                }

                Token::Packed => {
                    self.step();
                    if self.current_token == Token::Struct {
                        let mut s_decl = self.parse_struct_decl()?;
                        s_decl.is_packed = true;
                        program.structs.push(s_decl);
                    } else {
                        return Err(self.err("Expected 'struct' after 'packed'"));
                    }
                }
                Token::Struct => {
                    let s_decl = self.parse_struct_decl()?;
                    program.structs.push(s_decl);
                }
                Token::Sect => {
                    let sect_decl = self.parse_section_decl()?;
                    program.sections.push(sect_decl);
                }
                Token::Extern => {
                    self.step();
                    if self.current_token != Token::Fn {
                        return Err(self.err("Expected 'fn' after 'extern'"));
                    }
                    let mut func_decl = self.parse_function_signature()?;
                    func_decl.is_extern = true;
                    program.functions.push(func_decl);
                }
                Token::Export => {
                    self.step();
                    if self.current_token != Token::Fn {
                        return Err(self.err("Expected 'fn' after 'export'"));
                    }
                    let mut func_decl = self.parse_function_signature()?;
                    func_decl.is_export = true;
                    program.functions.push(func_decl);
                }
                Token::Irq => {
                    self.step();
                    if self.current_token != Token::Fn {
                        return Err(self.err("Expected 'fn' after 'irq'"));
                    }
                    let mut func_decl = self.parse_function_signature()?;
                    func_decl.is_irq = true;
                    program.functions.push(func_decl);
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
            if self.current_token == Token::Const {
                self.parse_const_decl()?;
                continue;
            }

            if self.current_token == Token::Fn {
                self.step();

                if let Token::Ident(n) = &self.current_token {
                    if n == name {
                        self.step();

                        while self.current_token != Token::LBrace
                            && self.current_token != Token::Semicolon
                            && self.current_token != Token::EOF
                        {
                            self.step();
                        }

                        if self.current_token == Token::LBrace {
                            return Ok(());
                        }

                        return Err(self.err(&format!("Function {} has no body", name)));
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
            Token::Jmpto => {
                if self.peek_token == Token::LParen {
                    self.parse_expr_statement()
                } else {
                    self.parse_jmpto_statement()
                }
            }
            Token::Match => self.parse_match_statement(),
            Token::Critical => {
                self.step();
                if self.current_token != Token::LBrace {
                    return Err(self.err("Expected '{' after critical"));
                }
                self.step();

                let mut body = Vec::new();
                while self.current_token != Token::RBrace && self.current_token != Token::EOF {
                    body.push(self.parse_statement()?);
                }

                if self.current_token == Token::RBrace {
                    self.step();
                }

                Ok(Stmt::Critical(body))
            }
            Token::Volatile | Token::Atomic => {
                let forced_modifier = if self.current_token == Token::Volatile {
                    PtrAccess::Volatile
                } else {
                    PtrAccess::Atomic
                };

                self.step();

                let dt = self.parse_data_type()?;
                let mut var_decl = self.parse_var_decl_tail(dt)?;

                if var_decl.modifier != PtrAccess::Normal {
                    return Err(self.err("volatile/atomic cannot be combined with *i or *o"));
                }

                var_decl.modifier = forced_modifier;

                if self.current_token == Token::Semicolon {
                    self.step();
                }

                Ok(Stmt::VarDefinition(var_decl))
            }

            Token::Align => {
                self.step();
                if self.current_token != Token::LParen {
                    return Err(self.err("Expected '(' after align"));
                }
                self.step();
                let alignment = match self.current_token {
                    Token::Number(n) => n as u32,
                    _ => return Err(self.err("Expected alignment number")),
                };
                self.step();
                if alignment == 0 || (alignment & (alignment - 1)) != 0 {
                    return Err(self.err("align value must be a power of two"));
                }
                if self.current_token != Token::RParen {
                    return Err(self.err("Expected ')' after alignment"));
                }
                self.step();
                let dt = self.parse_data_type()?;
                let mut var_decl = self.parse_var_decl_tail(dt)?;
                var_decl.alignment = alignment;
                if self.current_token == Token::Semicolon {
                    self.step();
                }
                Ok(Stmt::VarDefinition(var_decl))
            }
            Token::TypeU8
            | Token::TypeU16
            | Token::TypeU32
            | Token::TypeU64
            | Token::TypeI8
            | Token::TypeI16
            | Token::TypeI32
            | Token::TypeI64
            | Token::TypeF64
            | Token::TypeVoid => {
                let dt = self.parse_data_type()?;
                let mut var_decl = self.parse_var_decl_tail(dt)?;
                var_decl.alignment = 0;
                if self.current_token == Token::Semicolon {
                    self.step();
                }
                Ok(Stmt::VarDefinition(var_decl))
            }

            Token::Continue => {
                self.step();
                if self.current_token == Token::Semicolon {
                    self.step();
                }
                Ok(Stmt::Continue)
            }
            Token::Break => {
                self.step();
                if self.current_token == Token::Semicolon {
                    self.step();
                }
                Ok(Stmt::Break)
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
                    | Token::PtrInputOutputModifier(_)
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
                    if self.current_token == Token::Semicolon {
                        self.step();
                    }

                    return Ok(Stmt::Return(vec![(DataType::U64, Expr::Number(0))]));
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
        self.step();

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

    fn parse_match_statement(&mut self) -> Result<Stmt, ParseError> {
        self.step();
        if self.current_token != Token::LParen {
            return Err(self.err("Expected '(' after 'match'"));
        }
        self.step();

        let expr = self.parse_expr()?;

        if self.current_token != Token::RParen {
            return Err(self.err("Expected ')' after match expression"));
        }
        self.step();

        if self.current_token != Token::LBrace {
            return Err(self.err("Expected '{' after match expression"));
        }
        self.step();

        let mut cases = Vec::new();
        let mut default = None;

        while self.current_token != Token::RBrace && self.current_token != Token::EOF {
            if self.current_token == Token::Case {
                self.step();
                let case_expr = self.parse_expr()?;

                if self.current_token != Token::LBrace {
                    return Err(self.err("Expected '{' after case value"));
                }
                self.step();

                let mut body = Vec::new();
                while self.current_token != Token::RBrace && self.current_token != Token::EOF {
                    body.push(self.parse_statement()?);
                }

                if self.current_token == Token::RBrace {
                    self.step();
                }

                cases.push((case_expr, body));
            } else if self.current_token == Token::Default {
                self.step();

                if self.current_token != Token::LBrace {
                    return Err(self.err("Expected '{' after default"));
                }
                self.step();

                let mut body = Vec::new();
                while self.current_token != Token::RBrace && self.current_token != Token::EOF {
                    body.push(self.parse_statement()?);
                }

                if self.current_token == Token::RBrace {
                    self.step();
                }

                default = Some(body);
            } else {
                return Err(self.err("Expected 'case' or 'default' inside match"));
            }
        }

        if self.current_token == Token::RBrace {
            self.step();
        }

        Ok(Stmt::Match {
            expr,
            cases,
            default,
        })
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
            if self.current_token == Token::Semicolon {
                self.step();
            }
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

        let compound_op = match &self.current_token {
            Token::OpAddAssign => Some("OpAdd"),
            Token::OpSubAssign => Some("OpSub"),
            Token::OpMulAssign => Some("OpMul"),
            Token::OpDivAssign => Some("OpDiv"),
            Token::OpModAssign => Some("OpMod"),
            Token::OpBitAndAssign => Some("OpBitAnd"),
            Token::OpBitOrAssign => Some("OpBitOr"),
            Token::OpBitXorAssign => Some("OpBitXor"),
            Token::OpShlAssign => Some("OpShl"),
            Token::OpShrAssign => Some("OpShr"),
            _ => None,
        };
        if let Some(op) = compound_op {
            self.step();
            let right = self.parse_expr()?;
            if self.current_token == Token::Semicolon {
                self.step();
            }
            return Ok(Stmt::Assignment {
                targets: vec![left.clone()],
                value: Expr::Binary {
                    left: Box::new(left),
                    op: op.to_string(),
                    right: Box::new(right),
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
        self.step(); // consume 'struct'
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

        let mut is_packed = false;
        if self.current_token == Token::Packed {
            is_packed = true;
            self.step();
        }

        if self.current_token != Token::LBrace {
            return Err(self.err("Expected '{' after struct declaration"));
        }
        self.step();

        let mut fields = Vec::new();
        while self.current_token != Token::RBrace && self.current_token != Token::EOF {
            let mut field_modifier = PtrAccess::Normal;

            if self.current_token == Token::Volatile {
                field_modifier = PtrAccess::Volatile;
                self.step();
            } else if self.current_token == Token::Atomic {
                field_modifier = PtrAccess::Atomic;
                self.step();
            }

            let mut field_type = self.parse_data_type()?;
            let field_name = match &self.current_token {
                Token::Ident(n) => {
                    if n == "_" {
                        return Err(self.err("'_' cannot be used as a field name"));
                    }
                    n.clone()
                }
                _ => return Err(self.err("Expected field name")),
            };
            self.step();

            if self.current_token == Token::LBracket {
                self.step();

                let count = self.parse_array_size()?;

                if self.current_token != Token::RBracket {
                    return Err(self.err("Expected ']' after array size"));
                }

                self.step();

                field_type = DataType::Array(Box::new(field_type), count);
            }

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
                modifier: field_modifier,
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
            is_packed,
            alignment: 0,
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
            let mut forced_modifier = PtrAccess::Normal;

            if self.current_token == Token::Volatile || self.current_token == Token::Atomic {
                forced_modifier = if self.current_token == Token::Volatile {
                    PtrAccess::Volatile
                } else {
                    PtrAccess::Atomic
                };
                self.step();
            }

            let var_type = self.parse_data_type()?;
            let mut var_decl = self.parse_var_decl_tail(var_type)?;

            if forced_modifier != PtrAccess::Normal {
                if var_decl.modifier != PtrAccess::Normal {
                    return Err(self.err("volatile/atomic cannot be combined with *i or *o"));
                }
                var_decl.modifier = forced_modifier;
            }

            variables.push(var_decl);

            if self.current_token == Token::Semicolon {
                self.step();
            }
        }
        self.step();

        Ok(SectionDecl {
            name,
            variables,
            alignment: 0,
            is_ro: false,
            is_noinit: false,
        })
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
                Token::Ident(n) => {
                    if n == "_" {
                        return Err(self.err("'_' cannot be used as a parameter name"));
                    }
                    (n.clone(), PtrAccess::Normal)
                }
                Token::PtrInputModifier(n) => (n.clone(), PtrAccess::Input),
                Token::PtrOutputModifier(n) => (n.clone(), PtrAccess::Output),
                Token::PtrInputOutputModifier(n) => (n.clone(), PtrAccess::InputOutput),
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
                is_extern: false,
                is_export: false,
                is_irq: false,
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
            is_extern: false,
            is_export: false,
            is_irq: false,
        })
    }

    fn parse_var_decl_tail(&mut self, mut base_type: DataType) -> Result<VarDecl, ParseError> {
        let (name, modifier) = match &self.current_token {
            Token::Ident(n) => {
                if n == "_" {
                    return Err(self.err("'_' cannot be used as a variable name"));
                }
                (n.clone(), PtrAccess::Normal)
            }
            Token::PtrInputModifier(n) => (n.clone(), PtrAccess::Input),
            Token::PtrOutputModifier(n) => (n.clone(), PtrAccess::Output),
            Token::PtrInputOutputModifier(n) => (n.clone(), PtrAccess::InputOutput),
            _ => return Err(self.err("Expected variable name")),
        };
        self.step();
        if self.current_token == Token::LBracket {
            self.step();

            let count = self.parse_array_size()?;

            if self.current_token != Token::RBracket {
                return Err(self.err("Expected ']' after array size"));
            }

            self.step();

            base_type = DataType::Array(Box::new(base_type), count);
        }

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
            alignment: 0,
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
                let count = self.parse_array_size()?;
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
            self.step();

            let count = self.parse_array_size()?;

            if self.current_token != Token::RBracket {
                return Err(self.err("Expected ']' after array size"));
            }

            self.step();

            dt = DataType::Array(Box::new(dt), count);
        }

        if self.current_token == Token::OpMul {
            self.step();
            dt = DataType::Pointer(Box::new(dt));

            if self.current_token == Token::OpMul {
                return Err(self.err(
                    "multi-level pointers are not allowed in WandC; use a single pointer and pass array addresses with *adr",
                ));
            }
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
            Token::OpSub => {
                self.step();
                let expr = self.parse_primary_expr()?;
                match expr {
                    Expr::Number(n) => Ok(Expr::SignedNumber(-(n as i64))),
                    Expr::FloatLit(s) => {
                        if s.starts_with('-') {
                            Ok(Expr::FloatLit(s[1..].to_string()))
                        } else {
                            Ok(Expr::FloatLit(format!("-{}", s)))
                        }
                    }
                    _ => Ok(Expr::Binary {
                        left: Box::new(Expr::Number(0)),
                        op: "OpSub".to_string(),
                        right: Box::new(expr),
                    }),
                }
            }
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
                self.step();
                if self.is_current_token_type() {
                    let cast_type = self.parse_data_type()?;
                    if self.current_token != Token::RParen {
                        return Err(self.err("Expected ')' after cast type"));
                    }
                    self.step();
                    let inner_expr = self.parse_primary_expr()?;

                    let op_str = match cast_type {
                        DataType::F64 => "OpCastF64".to_string(),
                        _ => "OpCastInt".to_string(),
                    };

                    return Ok(Expr::Binary {
                        left: Box::new(inner_expr),
                        op: op_str,
                        right: Box::new(Expr::Number(0)),
                    });
                }

                let expr = self.parse_expr()?;

                if self.current_token != Token::RParen {
                    return Err(self.err("Expected ')' after parenthesized expression"));
                }

                self.step();

                if self.current_token == Token::OpAddrOf {
                    self.step();
                    return Ok(Expr::AddrOfExpr(Box::new(expr)));
                }

                Ok(expr)
            }
            Token::Ident(name) => {
                let name_val = name.clone();
                self.step();

                let mut current_expr = if self.current_token == Token::Colon {
                    self.step();
                    match &self.current_token {
                        Token::Ident(var) => {
                            let var_name = var.clone();
                            self.step();
                            Expr::SectionAccess {
                                section: name_val.clone(),
                                variable: var_name,
                            }
                        }
                        Token::AddrOf(var) => {
                            let var_name = var.clone();
                            self.step();
                            Expr::AddrOf(format!("{}:{}", name_val, var_name))
                        }
                        _ => return Err(self.err("Expected section variable name")),
                    }
                } else if self.current_token == Token::LParen {
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
                        name: name_val.clone(),
                        args,
                    });
                } else {
                    Expr::Variable(name_val.clone())
                };

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
                        match &self.current_token {
                            Token::Ident(member) => {
                                current_expr = Expr::MemberAccess {
                                    expr: Box::new(current_expr),
                                    member: member.clone(),
                                    is_arrow,
                                };
                                self.step();
                            }
                            Token::AddrOf(member) => {
                                current_expr = Expr::MemberAccess {
                                    expr: Box::new(current_expr),
                                    member: member.clone(),
                                    is_arrow,
                                };
                                self.step();
                                current_expr = Expr::AddrOfExpr(Box::new(current_expr));
                            }
                            _ => return Err(self.err("Expected member identifier")),
                        }
                    }
                }

                if self.current_token == Token::OpAddrOf {
                    self.step();
                    current_expr = Expr::AddrOfExpr(Box::new(current_expr));
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
            Token::TypeU8 => {
                self.step();
                Ok(Expr::Variable("u8".to_string()))
            }
            Token::TypeU16 => {
                self.step();
                Ok(Expr::Variable("u16".to_string()))
            }
            Token::TypeU32 => {
                self.step();
                Ok(Expr::Variable("u32".to_string()))
            }
            Token::TypeU64 => {
                self.step();
                Ok(Expr::Variable("u64".to_string()))
            }
            Token::TypeI8 => {
                self.step();
                Ok(Expr::Variable("i8".to_string()))
            }
            Token::TypeI16 => {
                self.step();
                Ok(Expr::Variable("i16".to_string()))
            }
            Token::TypeI32 => {
                self.step();
                Ok(Expr::Variable("i32".to_string()))
            }
            Token::TypeI64 => {
                self.step();
                Ok(Expr::Variable("i64".to_string()))
            }
            Token::TypeF64 => {
                self.step();
                Ok(Expr::Variable("f64".to_string()))
            }
            Token::TypeVoid => {
                self.step();
                Ok(Expr::Variable("void".to_string()))
            }
            Token::LBrace => {
                self.step();
                let mut elements = Vec::new();
                while self.current_token != Token::RBrace && self.current_token != Token::EOF {
                    elements.push(self.parse_expr()?);
                    if self.current_token == Token::Comma {
                        self.step();
                    }
                }
                if self.current_token != Token::RBrace {
                    return Err(self.err("Expected '}' after array initializer"));
                }
                self.step();
                Ok(Expr::ArrayInit(elements))
            }
            Token::Jmpto => {
                self.step();
                if self.current_token != Token::LParen {
                    return Err(self.err("Expected '(' after jmpto"));
                }
                self.step();
                let path_expr = self.parse_expr()?;
                if self.current_token != Token::RParen {
                    return Err(self.err("Expected ')' after jmpto path"));
                }
                self.step();
                Ok(Expr::Call {
                    name: "jmpto".to_string(),
                    args: vec![path_expr],
                })
            }
            _ => Err(self.err("Unexpected primary expression")),
        }
    }

    fn get_tok_precedence(&self) -> Option<u8> {
        match self.current_token {
            Token::OpMul | Token::OpDiv | Token::OpMod => Some(10),
            Token::OpAdd | Token::OpSub => Some(9),
            Token::OpShl | Token::OpShr => Some(8),
            Token::OpLt | Token::OpLtEq | Token::OpGt | Token::OpGtEq => Some(7),
            Token::OpEq | Token::OpNotEq => Some(6),
            Token::OpBitAnd => Some(5),
            Token::OpBitXor => Some(4),
            Token::OpBitOr => Some(3),
            Token::OpAnd => Some(2),
            Token::OpOr => Some(1),
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
