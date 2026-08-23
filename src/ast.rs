#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F64,
    Void,
    Pointer(Box<DataType>),
    Struct(String),
    Array(Box<DataType>, usize),
    Typedef(String, Box<DataType>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PtrAccess {
    Input,
    Output,
    InputOutput,
    Normal,
    Volatile,
    Atomic,
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    pub data_type: DataType,
    pub version_added: u32,
    pub version_removed: u32,
    pub modifier: PtrAccess,
}

#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: String,
    pub version: u32,
    pub fields: Vec<FieldDecl>,
    pub is_union: bool,
    pub is_packed: bool,
    pub alignment: u32,
}

#[derive(Debug, Clone)]
pub struct VarDecl {
    pub name: String,
    pub data_type: DataType,
    pub modifier: PtrAccess,
    pub initial_value: Option<Box<Expr>>,
    pub alignment: u32,
}

#[derive(Debug, Clone)]
pub struct SectionDecl {
    pub name: String,
    pub variables: Vec<VarDecl>,
    pub alignment: u32,
    pub is_ro: bool,
    pub is_noinit: bool,
}

#[derive(Debug, Clone)]
pub struct ConstDecl {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct EnumValueDecl {
    pub name: String,
    pub value: u64,
    pub version_added: u32,
    pub version_removed: u32,
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub version: u32,
    pub values: Vec<EnumValueDecl>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(u64),
    SignedNumber(i64),
    StringLit(String),
    Variable(String),
    AddrOf(String),
    MemberAccess {
        expr: Box<Expr>,
        member: String,
        is_arrow: bool,
    },
    SectionAccess {
        section: String,
        variable: String,
    },
    Binary {
        left: Box<Expr>,
        op: String,
        right: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
    Index {
        expr: Box<Expr>,
        index: Box<Expr>,
    },
    FloatLit(String),
    Null,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    VarDefinition(VarDecl),
    Assignment {
        targets: Vec<Expr>,
        value: Expr,
    },
    If {
        cond: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    For {
        init: Option<Box<Stmt>>,
        cond: Expr,
        post: Option<Box<Stmt>>,
        body: Vec<Stmt>,
    },
    Jmpto {
        module_name: String,
        args: Vec<Stmt>,
    },
    Return(Vec<(DataType, Expr)>),
    Critical(Vec<Stmt>),
    Nasm(String),
    Match {
        expr: Expr,
        cases: Vec<(Expr, Vec<Stmt>)>,
        default: Option<Vec<Stmt>>,
    },
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct FuncDecl {
    pub name: String,
    pub params: Vec<(DataType, String, PtrAccess)>,
    pub return_types: Vec<DataType>,
    pub body: Option<Vec<Stmt>>,
    pub is_extern: bool,
    pub is_export: bool,
    pub is_irq: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PtrModifier {
    Volatile,
    Atomic,
}

#[derive(Debug, Clone)]
pub struct AttributeDecl {
    pub name: String,
    pub value: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub use_os: bool,
    pub imports: Vec<String>,
    pub typedefs: Vec<(String, DataType)>,
    pub structs: Vec<StructDecl>,
    pub enums: Vec<EnumDecl>,
    pub constants: Vec<ConstDecl>,
    pub sections: Vec<SectionDecl>,
    pub functions: Vec<FuncDecl>,
}
