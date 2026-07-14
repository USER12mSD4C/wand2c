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
    Input,  // *i (Read-Only)
    Output, // *o (Write-Only)
    Normal,
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    pub data_type: DataType,
    pub version_added: u32,
    pub version_removed: u32,
}

#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: String,
    pub version: u32,
    pub fields: Vec<FieldDecl>,
    pub is_union: bool,
    pub is_packed: bool,
}

#[derive(Debug, Clone)]
pub struct VarDecl {
    pub name: String,
    pub data_type: DataType,
    pub modifier: PtrAccess,
    pub initial_value: Option<Box<Expr>>,
}

#[derive(Debug, Clone)]
pub struct SectionDecl {
    pub name: String,
    pub variables: Vec<VarDecl>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(u64),
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
    Nasm(String),
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct FuncDecl {
    pub name: String,
    pub params: Vec<(DataType, String, PtrAccess)>,
    pub return_types: Vec<DataType>,
    pub body: Option<Vec<Stmt>>,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub use_os: bool,
    pub imports: Vec<String>,
    pub typedefs: Vec<(String, DataType)>,
    pub structs: Vec<StructDecl>,
    pub sections: Vec<SectionDecl>,
    pub functions: Vec<FuncDecl>,
}
