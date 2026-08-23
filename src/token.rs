#![allow(dead_code)]

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    ScTrue,
    ScFalse,

    Fn,
    Struct,
    Version,
    Sect,
    Eos,
    If,
    Else,
    While,
    For,
    Import,
    Return,
    Null,
    Typedef,
    Jmpto,
    Packed,
    Extern,
    Export,
    Const,
    Match,
    Case,
    Default,
    Volatile,
    Atomic,
    Critical,
    Irq,
    Align,
    Ro,
    Noinit,

    TypeU8,
    TypeU16,
    TypeU32,
    TypeU64,
    TypeI8,
    TypeI16,
    TypeI32,
    TypeI64,
    TypeVoid,

    Ident(String),
    Number(u64),
    StringLiteral(String),

    PtrInputModifier(String),
    PtrOutputModifier(String),
    PtrInputOutputModifier(String),
    AddrOf(String),

    OpAssign, // =
    OpAdd,    // +
    OpSub,    // -
    OpMul,    // *
    OpDiv,    // /
    OpMod,    // %
    OpEq,     // ==
    OpNotEq,  // !=
    OpLt,     // <
    OpLtEq,   // <=
    OpGt,     // >
    OpGtEq,   // >=
    OpAnd,    // &&
    OpOr,     // ||
    OpNot,    // !
    Arrow,    // ->
    Dot,      // .
    Colon,    // :
    OpInc,    // ++
    OpDec,    // --

    LBrace,    // {
    RBrace,    // }
    LBracket,  // [
    RBracket,  // ]
    LParen,    // (
    RParen,    // )
    Semicolon, // ;
    Comma,     // ,

    OpAddrOf,

    Enum,
    Union,
    OpBitAnd, // &
    OpBitOr,  // |
    OpBitXor, // ^
    OpBitNot, // ~
    OpShl,    // <<
    OpShr,    // >>
    TypeF64,
    FloatLiteral(String),

    NasmBlock(String),

    EOF,
}
