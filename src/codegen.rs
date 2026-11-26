//! Frontend side of the x86-64 backend.
//!
//! The backend itself lives in `src/backend/x86/codegen.c`.  This module only
//! serializes the already-checked AST into a compact, length-prefixed wire
//! format and copies the result (plus relocation metadata) back into Rust.
//! Keeping the AST borrowed while generating code avoids the old generator's
//! habit of cloning whole expression trees and doubling/tripling its heap.

use crate::ast::{DataType, Expr, FuncDecl, PtrAccess, Program, Stmt, VarDecl};
use crate::lexer::Lexer;
use crate::parser::Parser;
use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::{c_char, c_uchar};
use std::path::Path;

const WIRE_MAGIC: u32 = 0x5741_4E44; // "WAND"
const WIRE_VERSION: u16 = 1;
const MAX_SERIALIZE_DEPTH: usize = 128;
const MAX_JMPTO_DEPTH: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum OutputFormat {
    Program = 0,
    Object = 1,
    Raw = 2,
    Kernel = 3,
    Wexp = 4,
}

#[repr(C)]
struct CSymbol {
    name: *mut c_char,
    offset: u64,
}

#[repr(C)]
struct CCallPatch {
    position: u64,
    target: *mut c_char,
}

#[repr(C)]
struct CAddrPatch {
    position: u64,
    key: *mut c_char,
    is_rip: u8,
}

#[repr(C)]
struct CCompileResult {
    data: *mut c_uchar,
    size: usize,
    functions: *mut CSymbol,
    function_count: usize,
    call_patches: *mut CCallPatch,
    call_patch_count: usize,
    addr_patches: *mut CAddrPatch,
    addr_patch_count: usize,
    error: u32,
}

extern "C" {
    fn wand_x86_compile_program(
        program_buf: *const c_uchar,
        program_len: usize,
        format: u32,
        entry_name: *const c_uchar,
        entry_len: usize,
    ) -> CCompileResult;
    fn wand_compile_result_free(result: *mut CCompileResult);
}

/// Machine code (or relocatable object) plus the metadata needed by the ELF
/// packer in `abi.rs`.
pub struct CodegenOutput {
    pub code: Vec<u8>,
    pub function_offsets: HashMap<String, usize>,
    pub call_patches: Vec<(usize, String)>,
    pub addr_patches: Vec<(usize, String, bool)>,
}

pub fn compile_program(
    program: &Program,
    format: OutputFormat,
    entry_name: Option<&str>,
) -> Result<CodegenOutput, String> {
    let mut wire = Wire::new();
    wire.write_program(program);
    if wire.failed {
        return Err("AST nesting limit exceeded while serializing".to_string());
    }

    let entry_bytes = entry_name.unwrap_or("").as_bytes();
    let mut result = unsafe {
        wand_x86_compile_program(
            wire.buf.as_ptr(),
            wire.buf.len(),
            format as u32,
            entry_bytes.as_ptr(),
            entry_bytes.len(),
        )
    };

    let error = result.error;
    if error != 0 {
        unsafe { wand_compile_result_free(&mut result) };
        return Err(match error {
            2 => "x86 backend ran out of memory".to_string(),
            3 => "x86 backend hit its recursion limit".to_string(),
            _ => "x86 backend rejected the serialized program".to_string(),
        });
    }

    let code = if result.size == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(result.data, result.size) }.to_vec()
    };

    let mut function_offsets = HashMap::with_capacity(result.function_count);
    if !result.functions.is_null() {
        let symbols = unsafe { std::slice::from_raw_parts(result.functions, result.function_count) };
        for symbol in symbols {
            let name = unsafe { cstr_owned(symbol.name) };
            function_offsets.insert(name, symbol.offset as usize);
        }
    }

    let mut call_patches = Vec::with_capacity(result.call_patch_count);
    if !result.call_patches.is_null() {
        let patches =
            unsafe { std::slice::from_raw_parts(result.call_patches, result.call_patch_count) };
        for patch in patches {
            call_patches.push((patch.position as usize, unsafe { cstr_owned(patch.target) }));
        }
    }

    let mut addr_patches = Vec::with_capacity(result.addr_patch_count);
    if !result.addr_patches.is_null() {
        let patches =
            unsafe { std::slice::from_raw_parts(result.addr_patches, result.addr_patch_count) };
        for patch in patches {
            addr_patches.push((
                patch.position as usize,
                unsafe { cstr_owned(patch.key) },
                patch.is_rip != 0,
            ));
        }
    }

    unsafe { wand_compile_result_free(&mut result) };

    Ok(CodegenOutput {
        code,
        function_offsets,
        call_patches,
        addr_patches,
    })
}

unsafe fn cstr_owned(ptr: *mut c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    CStr::from_ptr(ptr).to_string_lossy().into_owned()
}

struct Wire {
    buf: Vec<u8>,
    failed: bool,
    depth: usize,
    jmpto_stack: Vec<String>,
    jmpto_cache: HashMap<String, Option<Vec<Stmt>>>,
}

impl Wire {
    fn new() -> Self {
        Self {
            buf: Vec::with_capacity(4096),
            failed: false,
            depth: 0,
            jmpto_stack: Vec::new(),
            jmpto_cache: HashMap::new(),
        }
    }

    fn enter(&mut self) -> bool {
        if self.failed {
            return false;
        }
        self.depth += 1;
        if self.depth > MAX_SERIALIZE_DEPTH {
            self.failed = true;
            return false;
        }
        true
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn u8(&mut self, value: u8) {
        if !self.failed {
            self.buf.push(value);
        }
    }

    fn bool(&mut self, value: bool) {
        self.u8(if value { 1 } else { 0 });
    }

    fn u16(&mut self, value: u16) {
        if self.failed {
            return;
        }
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        if self.failed {
            return;
        }
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        if self.failed {
            return;
        }
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.u64(value as u64);
    }

    fn str(&mut self, value: &str) {
        if self.failed {
            return;
        }
        let bytes = value.as_bytes();
        self.u32(bytes.len() as u32);
        self.buf.extend_from_slice(bytes);
    }

    fn write_program(&mut self, program: &Program) {
        self.u32(WIRE_MAGIC);
        self.u16(WIRE_VERSION);
        self.bool(program.use_os);

        self.u32(program.typedefs.len() as u32);
        for (name, dt) in &program.typedefs {
            self.str(name);
            self.datatype(dt);
        }

        self.u32(program.structs.len() as u32);
        for s in &program.structs {
            self.str(&s.name);
            self.u32(s.version);
            self.u32(s.fields.len() as u32);
            for field in &s.fields {
                self.str(&field.name);
                self.datatype(&field.data_type);
                self.u8(access_tag(field.modifier));
                self.u32(field.version_added);
                self.u32(field.version_removed);
            }
            self.bool(s.is_union);
            self.bool(s.is_packed);
            self.u32(s.alignment);
        }

        self.u32(program.enums.len() as u32);
        for e in &program.enums {
            self.str(&e.name);
            self.u32(e.values.len() as u32);
            for value in &e.values {
                self.str(&value.name);
                self.u64(value.value);
            }
        }

        self.u32(program.constants.len() as u32);
        for c in &program.constants {
            self.str(&c.name);
            self.expr(&c.value);
        }

        self.u32(program.sections.len() as u32);
        for section in &program.sections {
            self.str(&section.name);
            self.u32(section.variables.len() as u32);
            for var in &section.variables {
                self.var_decl(var);
            }
            self.u32(section.alignment);
            self.bool(section.is_ro);
            self.bool(section.is_noinit);
        }

        self.u32(program.functions.len() as u32);
        for func in &program.functions {
            self.function(func);
        }
    }

    fn datatype(&mut self, dt: &DataType) {
        if !self.enter() {
            return;
        }
        match dt {
            DataType::U8 => self.u8(0),
            DataType::U16 => self.u8(1),
            DataType::U32 => self.u8(2),
            DataType::U64 => self.u8(3),
            DataType::I8 => self.u8(4),
            DataType::I16 => self.u8(5),
            DataType::I32 => self.u8(6),
            DataType::I64 => self.u8(7),
            DataType::F64 => self.u8(8),
            DataType::Void => self.u8(9),
            DataType::Pointer(inner) => {
                self.u8(10);
                self.datatype(inner);
            }
            DataType::Struct(name) => {
                self.u8(11);
                self.str(name);
            }
            DataType::Array(inner, count) => {
                self.u8(12);
                self.datatype(inner);
                self.u64(*count as u64);
            }
            DataType::Typedef(name, inner) => {
                self.u8(13);
                self.str(name);
                self.datatype(inner);
            }
        }
        self.leave();
    }

    fn expr(&mut self, expr: &Expr) {
        if !self.enter() {
            return;
        }
        match expr {
            Expr::Number(n) => {
                self.u8(0);
                self.u64(*n);
            }
            Expr::SignedNumber(n) => {
                self.u8(1);
                self.i64(*n);
            }
            Expr::StringLit(s) => {
                self.u8(2);
                self.str(s);
            }
            Expr::Variable(name) => {
                self.u8(3);
                self.str(name);
            }
            Expr::AddrOf(name) => {
                self.u8(4);
                self.str(name);
            }
            Expr::AddrOfExpr(inner) => {
                self.u8(5);
                self.expr(inner);
            }
            Expr::MemberAccess {
                expr,
                member,
                is_arrow,
            } => {
                self.u8(6);
                self.expr(expr);
                self.str(member);
                self.bool(*is_arrow);
            }
            Expr::SectionAccess { section, variable } => {
                self.u8(7);
                self.str(section);
                self.str(variable);
            }
            Expr::Binary { left, op, right } => {
                self.u8(8);
                self.expr(left);
                self.str(op);
                self.expr(right);
            }
            Expr::Call { name, args } => {
                self.u8(9);
                self.str(name);
                self.u32(args.len() as u32);
                for arg in args {
                    self.expr(arg);
                }
            }
            Expr::Index { expr, index } => {
                self.u8(10);
                self.expr(expr);
                self.expr(index);
            }
            Expr::FloatLit(s) => {
                self.u8(11);
                self.str(s);
            }
            Expr::ArrayInit(elements) => {
                self.u8(12);
                self.u32(elements.len() as u32);
                for element in elements {
                    self.expr(element);
                }
            }
            Expr::Null => self.u8(13),
        }
        self.leave();
    }

    fn var_decl(&mut self, decl: &VarDecl) {
        self.str(&decl.name);
        self.datatype(&decl.data_type);
        self.u8(access_tag(decl.modifier));
        self.bool(decl.initial_value.is_some());
        if let Some(init) = &decl.initial_value {
            self.expr(init);
        }
        self.u32(decl.alignment);
    }

    fn stmts(&mut self, stmts: &[Stmt]) {
        self.u32(stmts.len() as u32);
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &Stmt) {
        if !self.enter() {
            return;
        }
        match stmt {
            Stmt::VarDefinition(decl) => {
                self.u8(0);
                self.var_decl(decl);
            }
            Stmt::Assignment { targets, value } => {
                self.u8(1);
                self.u32(targets.len() as u32);
                for target in targets {
                    self.expr(target);
                }
                self.expr(value);
            }
            Stmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.u8(2);
                self.expr(cond);
                self.stmts(then_branch);
                self.bool(else_branch.is_some());
                if let Some(else_stmts) = else_branch {
                    self.stmts(else_stmts);
                }
            }
            Stmt::While { cond, body } => {
                self.u8(3);
                self.expr(cond);
                self.stmts(body);
            }
            Stmt::For {
                init,
                cond,
                post,
                body,
            } => {
                self.u8(4);
                self.bool(init.is_some());
                if let Some(init) = init {
                    self.stmt(init);
                }
                self.expr(cond);
                self.bool(post.is_some());
                if let Some(post) = post {
                    self.stmt(post);
                }
                self.stmts(body);
            }
            Stmt::Jmpto { module_name, args } => {
                self.u8(5);
                self.str(module_name);
                self.u32(args.len() as u32);
                for arg in args {
                    self.stmt(arg);
                }
                let inline = self.jmpto_inline_body(module_name);
                self.bool(inline.is_some());
                if let Some(body) = inline {
                    self.jmpto_stack.push(module_name.clone());
                    self.stmts(&body);
                    self.jmpto_stack.pop();
                }
            }
            Stmt::Return(values) => {
                self.u8(6);
                self.u32(values.len() as u32);
                for (dt, expr) in values {
                    self.datatype(dt);
                    self.expr(expr);
                }
            }
            Stmt::Critical(body) => {
                self.u8(7);
                self.stmts(body);
            }
            Stmt::Nasm(code) => {
                self.u8(8);
                self.str(code);
            }
            Stmt::Match {
                expr,
                cases,
                default,
            } => {
                self.u8(9);
                self.expr(expr);
                self.u32(cases.len() as u32);
                for (case_expr, body) in cases {
                    self.expr(case_expr);
                    self.stmts(body);
                }
                self.bool(default.is_some());
                if let Some(default) = default {
                    self.stmts(default);
                }
            }
            Stmt::Expr(expr) => {
                self.u8(10);
                self.expr(expr);
            }
            Stmt::Continue => self.u8(11),
            Stmt::Break => self.u8(12),
        }
        self.leave();
    }

    fn function(&mut self, func: &FuncDecl) {
        self.str(&func.name);
        self.u32(func.version);
        self.u32(func.params.len() as u32);
        for (dt, name, access) in &func.params {
            self.datatype(dt);
            self.str(name);
            self.u8(access_tag(*access));
        }
        self.u32(func.return_types.len() as u32);
        for dt in &func.return_types {
            self.datatype(dt);
        }
        self.bool(func.body.is_some());
        if let Some(body) = &func.body {
            self.stmts(body);
        }
        self.bool(func.is_extern);
        self.bool(func.is_export);
        self.bool(func.is_irq);
    }

    /// The old Rust backend re-parsed `jmpto` modules during code generation.
    /// Do that here instead so the C backend never needs a frontend parser.
    fn jmpto_inline_body(&mut self, module_name: &str) -> Option<Vec<Stmt>> {
        if let Some(cached) = self.jmpto_cache.get(module_name) {
            return cached.clone();
        }
        if self.jmpto_stack.len() >= MAX_JMPTO_DEPTH
            || self.jmpto_stack.iter().any(|m| m == module_name)
        {
            return None;
        }

        let candidates = [module_name.to_string(), module_name.replace(".wexp", ".w")];
        let mut body = None;
        'outer: for candidate in &candidates {
            if candidate.is_empty() || !Path::new(candidate).is_file() {
                continue;
            }
            let source = match std::fs::read_to_string(candidate) {
                Ok(source) => source,
                Err(_) => continue,
            };
            let parsed = {
                let lexer = Lexer::new(&source);
                let mut parser = Parser::new(lexer);
                parser.parse_program()
            };
            let parsed = match parsed {
                Ok(program) => program,
                Err(_) => continue,
            };
            if !parsed.functions.iter().any(|f| f.name == "main") {
                continue;
            }
            let lexer = Lexer::new(&source);
            let mut parser = Parser::new(lexer);
            if parser.seek_to_function("main").is_err() {
                continue;
            }
            if let Ok(parsed_body) = parser.parse_function_body() {
                body = Some(parsed_body);
                break 'outer;
            }
        }

        self.jmpto_cache
            .insert(module_name.to_string(), body.clone());
        body
    }
}

fn access_tag(access: PtrAccess) -> u8 {
    match access {
        PtrAccess::Input => 0,
        PtrAccess::Output => 1,
        PtrAccess::InputOutput => 2,
        PtrAccess::Normal => 3,
        PtrAccess::Volatile => 4,
        PtrAccess::Atomic => 5,
    }
}
