use crate::ast::Span;
use crate::token::Token;

pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn current(&self) -> char {
        if self.pos >= self.input.len() {
            '\0'
        } else {
            self.input[self.pos]
        }
    }

    fn peek(&self, offset: usize) -> char {
        if self.pos + offset >= self.input.len() {
            '\0'
        } else {
            self.input[self.pos + offset]
        }
    }

    fn step(&mut self) {
        if self.pos < self.input.len() {
            if self.input[self.pos] == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
            self.pos += 1;
        }
    }

    fn step_by(&mut self, n: usize) {
        for _ in 0..n {
            self.step();
        }
    }

    pub fn next_token_with_span(&mut self) -> (Token, Span) {
        self.skip_whitespace_and_comments();

        let start_pos = self.pos;
        let start_line = self.line;
        let start_col = self.col;

        if self.pos >= self.input.len() {
            let span = Span {
                line: start_line,
                col: start_col,
                start: start_pos,
                end: start_pos,
            };
            return (Token::EOF, span);
        }

        let ch = self.current();

        if ch == 's' && self.peek(1) == 'c' && self.peek(2) == '.' {
            if self.peek_str(3, "true") && !self.peek(7).is_alphanumeric() {
                self.step_by(7);
                let span = Span {
                    line: start_line,
                    col: start_col,
                    start: start_pos,
                    end: self.pos,
                };
                return (Token::ScTrue, span);
            } else if self.peek_str(3, "false") && !self.peek(8).is_alphanumeric() {
                self.step_by(8);
                let span = Span {
                    line: start_line,
                    col: start_col,
                    start: start_pos,
                    end: self.pos,
                };
                return (Token::ScFalse, span);
            }
        }

        if ch == ':' && self.peek(1) == ':' && self.peek_str(2, "nasm::") {
            self.step_by(8);
            if self.current() == '{' {
                self.step();
                let nasm_code = self.read_until_brace();
                let span = Span {
                    line: start_line,
                    col: start_col,
                    start: start_pos,
                    end: self.pos,
                };
                return (Token::NasmBlock(nasm_code), span);
            }
        }

        if ch.is_ascii_digit() {
            let tok = self.read_number_or_float();
            let span = Span {
                line: start_line,
                col: start_col,
                start: start_pos,
                end: self.pos,
            };
            return (tok, span);
        }

        if ch == '#' {
            self.step();
            let mut name = String::new();
            while self.pos < self.input.len() && self.input[self.pos].is_alphabetic() {
                name.push(self.input[self.pos]);
                self.step();
            }
            let span = Span {
                line: start_line,
                col: start_col,
                start: start_pos,
                end: self.pos,
            };
            if name == "import" {
                return (Token::Import, span);
            } else {
                return (Token::Ident(format!("#{}", name)), span);
            }
        }

        if ch == '*' && self.peek(1) == 'a' && self.peek(2) == 'd' && self.peek(3) == 'r' {
            if !self.peek(4).is_alphanumeric() {
                self.step_by(4);
                let span = Span {
                    line: start_line,
                    col: start_col,
                    start: start_pos,
                    end: self.pos,
                };
                return (Token::OpAddrOf, span);
            }
        }

        if ch.is_alphabetic() || ch == '_' {
            let tok = self.read_identifier_or_modifier();
            let span = Span {
                line: start_line,
                col: start_col,
                start: start_pos,
                end: self.pos,
            };
            return (tok, span);
        }

        if ch == '"' {
            let tok = self.read_string_literal();
            let span = Span {
                line: start_line,
                col: start_col,
                start: start_pos,
                end: self.pos,
            };
            return (tok, span);
        }

        self.step();
        let tok = match ch {
            ';' => Token::Semicolon,
            ',' => Token::Comma,
            '{' => Token::LBrace,
            '}' => Token::RBrace,
            '[' => Token::LBracket,
            ']' => Token::RBracket,
            '(' => Token::LParen,
            ')' => Token::RParen,
            ':' => Token::Colon,
            '.' => Token::Dot,
            '~' => Token::OpBitNot,
            '^' => Token::OpBitXor,
            '!' => {
                if self.current() == '=' {
                    self.step();
                    Token::OpNotEq
                } else {
                    Token::OpNot
                }
            }
            '=' => {
                if self.current() == '=' {
                    self.step();
                    Token::OpEq
                } else {
                    Token::OpAssign
                }
            }
            '<' => {
                if self.current() == '=' {
                    self.step();
                    Token::OpLtEq
                } else if self.current() == '<' {
                    self.step();
                    Token::OpShl
                } else {
                    Token::OpLt
                }
            }
            '>' => {
                if self.current() == '=' {
                    self.step();
                    Token::OpGtEq
                } else if self.current() == '>' {
                    self.step();
                    Token::OpShr
                } else {
                    Token::OpGt
                }
            }
            '+' => {
                if self.current() == '+' {
                    self.step();
                    Token::OpInc
                } else {
                    Token::OpAdd
                }
            }
            '-' => {
                if self.current() == '-' {
                    self.step();
                    Token::OpDec
                } else if self.current() == '>' {
                    self.step();
                    Token::Arrow
                } else {
                    Token::OpSub
                }
            }
            '*' => Token::OpMul,
            '/' => Token::OpDiv,
            '%' => Token::OpMod,
            '&' => {
                if self.current() == '&' {
                    self.step();
                    Token::OpAnd
                } else {
                    Token::OpBitAnd
                }
            }
            '|' => {
                if self.current() == '|' {
                    self.step();
                    Token::OpOr
                } else {
                    Token::OpBitOr
                }
            }
            _ => Token::Ident(ch.to_string()),
        };

        let span = Span {
            line: start_line,
            col: start_col,
            start: start_pos,
            end: self.pos,
        };
        (tok, span)
    }

    fn peek_str(&self, start_offset: usize, s: &str) -> bool {
        let chars: Vec<char> = s.chars().collect();
        for (i, c) in chars.iter().enumerate() {
            if self.peek(start_offset + i) != *c {
                return false;
            }
        }
        true
    }

    fn skip_whitespace_and_comments(&mut self) {
        while self.pos < self.input.len() {
            let ch = self.current();
            if ch.is_whitespace() {
                self.step();
            } else if ch == '/' && self.peek(1) == '/' {
                while self.pos < self.input.len() && self.current() != '\n' {
                    self.step();
                }
            } else {
                break;
            }
        }
    }

    fn read_until_brace(&mut self) -> String {
        let mut depth = 1;
        let mut result = String::new();
        while self.pos < self.input.len() {
            let ch = self.current();
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    self.step();
                    break;
                }
            }
            result.push(ch);
            self.step();
        }
        result
    }

    fn read_number_or_float(&mut self) -> Token {
        let mut num_str = String::new();
        let mut is_float = false;

        if self.current() == '0' && (self.peek(1) == 'x' || self.peek(1) == 'X') {
            num_str.push(self.current()); // '0'
            self.step();
            num_str.push(self.current()); // 'x' or 'X'
            self.step();
            while self.pos < self.input.len() && self.current().is_ascii_hexdigit() {
                num_str.push(self.current());
                self.step();
            }
            let val = u64::from_str_radix(&num_str[2..], 16).unwrap_or(0);
            return Token::Number(val);
        }

        while self.pos < self.input.len()
            && (self.current().is_ascii_digit()
                || (self.current() == '.' && self.peek(1).is_ascii_digit()))
        {
            if self.current() == '.' {
                is_float = true;
            }
            num_str.push(self.current());
            self.step();
        }

        if is_float {
            Token::FloatLiteral(num_str)
        } else {
            let val = num_str.parse::<u64>().unwrap_or(0);
            Token::Number(val)
        }
    }

    fn read_string_literal(&mut self) -> Token {
        self.step();
        let mut s = String::new();
        while self.pos < self.input.len() && self.current() != '"' {
            s.push(self.current());
            self.step();
        }
        self.step();
        Token::StringLiteral(s)
    }

    fn read_identifier_or_modifier(&mut self) -> Token {
        let mut name = String::new();
        while self.pos < self.input.len()
            && (self.current().is_alphanumeric() || self.current() == '_')
        {
            name.push(self.current());
            self.step();
        }

        if self.current() == '*' {
            if self.peek_str(1, "adr") && !self.peek(4).is_alphanumeric() {
                self.pos += 4; // * + a + d + r
                return Token::AddrOf(name);
            }
            if self.peek(1) == 'i' && !self.peek(2).is_alphanumeric() {
                self.pos += 2;
                return Token::PtrInputModifier(name);
            }
            if self.peek(1) == 'o' && !self.peek(2).is_alphanumeric() {
                self.pos += 2;
                return Token::PtrOutputModifier(name);
            }
        }

        match name.as_str() {
            "sc.true" => Token::ScTrue,
            "sc.false" => Token::ScFalse,
            "fn" => Token::Fn,
            "struct" => Token::Struct,
            "union" => Token::Union,
            "enum" => Token::Enum,
            "version" => Token::Version,
            "sect" => Token::Sect,
            "EOS" => Token::Eos,
            "if" => Token::If,
            "else" => Token::Else,
            "while" => Token::While,
            "for" => Token::For,
            "return" => Token::Return,
            "null" => Token::Null,
            "typedef" => Token::Typedef,
            "jmpto" => Token::Jmpto,
            "packed" => Token::Packed,
            "u8" => Token::TypeU8,
            "u16" => Token::TypeU16,
            "u32" => Token::TypeU32,
            "u64" => Token::TypeU64,
            "i8" => Token::TypeI8,
            "i16" => Token::TypeI16,
            "i32" => Token::TypeI32,
            "i64" => Token::TypeI64,
            "f64" => Token::TypeF64,
            "void" => Token::TypeVoid,
            "adr" => {
                if !self.current().is_alphanumeric() && self.current() != '_' {
                    Token::OpAddrOf
                } else {
                    Token::Ident(name)
                }
            }
            _ => Token::Ident(name),
        }
    }
}
