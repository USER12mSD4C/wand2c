#![allow(dead_code)]

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Системный заголовок
    ScTrue,
    ScFalse,

    // Ключевые слова
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

    // Примитивные типы данных
    TypeU8,
    TypeU16,
    TypeU32,
    TypeU64,
    TypeI8,
    TypeI16,
    TypeI32,
    TypeI64,
    TypeVoid,

    // Идентификаторы и литералы
    Ident(String),
    Number(u64),
    StringLiteral(String),

    // Модификаторы указателей
    PtrInputModifier(String),
    PtrOutputModifier(String),
    AddrOf(String),

    // Операторы
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

    // Разделители
    LBrace,    // {
    RBrace,    // }
    LBracket,  // [
    RBracket,  // ]
    LParen,    // (
    RParen,    // )
    Semicolon, // ;
    Comma,     // ,

    OpAddrOf, // унарный оператор adr (для *adr)

    //euo токены
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

    // Ассемблерные блоки
    NasmBlock(String),

    EOF,
}
