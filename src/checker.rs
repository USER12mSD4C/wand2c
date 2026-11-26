#![allow(dead_code)]

use crate::ast::*;
use std::collections::HashMap;

pub struct TypeChecker {
    structs: HashMap<String, StructDecl>,
    sections: HashMap<String, SectionDecl>,
    functions: HashMap<String, FuncDecl>,
    typedefs: HashMap<String, DataType>,
    use_os: bool,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            structs: HashMap::new(),
            sections: HashMap::new(),
            functions: HashMap::new(),
            typedefs: HashMap::new(),
            use_os: false,
        }
    }

    pub fn populate_symbols(&mut self, program: &Program) {
        self.use_os = program.use_os;
        for s in &program.structs {
            self.structs.insert(s.name.clone(), s.clone());
        }
        for sect in &program.sections {
            self.sections.insert(sect.name.clone(), sect.clone());
        }
        for func in &program.functions {
            self.functions.insert(func.name.clone(), func.clone());
        }
        for (name, dt) in &program.typedefs {
            self.typedefs.insert(name.clone(), dt.clone());
        }
    }

    pub fn calculate_struct_layout(&self, struct_name: &str) -> Result<(u32, Vec<u32>), String> {
        let mut visiting = Vec::new();
        self.calculate_struct_layout_inner(struct_name, &mut visiting)
    }

    fn calculate_struct_layout_inner(
        &self,
        struct_name: &str,
        visiting: &mut Vec<String>,
    ) -> Result<(u32, Vec<u32>), String> {
        if visiting.iter().any(|n| n == struct_name) {
            return Err(format!("recursive struct type '{}'", struct_name));
        }
        visiting.push(struct_name.to_string());
        let result = self.calculate_struct_layout_body(struct_name, visiting);
        visiting.pop();
        result
    }

    fn calculate_struct_layout_body(
        &self,
        struct_name: &str,
        visiting: &mut Vec<String>,
    ) -> Result<(u32, Vec<u32>), String> {
        let s = self
            .structs
            .get(struct_name)
            .ok_or_else(|| format!("Struct {} not defined", struct_name))?;

        let mut offsets = Vec::new();
        let declared_alignment = if s.alignment > 0 { s.alignment } else { 1 };

        if s.is_union {
            let mut max_size = 0u32;
            let mut max_alignment = declared_alignment;

            for field in &s.fields {
                offsets.push(0);
                let size = self.get_type_size_inner(&field.data_type, visiting)?;
                if size > max_size {
                    max_size = size;
                }

                let mut alignment = if s.is_packed { 1 } else { size };
                if alignment > 8 {
                    alignment = 8;
                }
                if alignment > max_alignment {
                    max_alignment = alignment;
                }
            }

            if max_size % max_alignment != 0 {
                max_size += max_alignment - (max_size % max_alignment);
            }

            return Ok((max_size, offsets));
        }

        let mut current_offset = 0u32;
        let mut max_alignment = declared_alignment;

        for field in &s.fields {
            let size = self.get_type_size_inner(&field.data_type, visiting)?;
            let mut alignment = if s.is_packed { 1 } else { size };
            if alignment > 8 {
                alignment = 8;
            }
            if alignment > max_alignment {
                max_alignment = alignment;
            }

            if current_offset % alignment != 0 {
                current_offset += alignment - (current_offset % alignment);
            }

            offsets.push(current_offset);
            current_offset += size;
        }

        if current_offset % max_alignment != 0 {
            current_offset += max_alignment - (current_offset % max_alignment);
        }

        Ok((current_offset, offsets))
    }

    fn get_type_size(&self, dt: &DataType) -> Result<u32, String> {
        let mut visiting = Vec::new();
        self.get_type_size_inner(dt, &mut visiting)
    }

    fn get_type_size_inner(
        &self,
        dt: &DataType,
        visiting: &mut Vec<String>,
    ) -> Result<u32, String> {
        match dt {
            DataType::U8 | DataType::I8 => Ok(1),
            DataType::U16 | DataType::I16 => Ok(2),
            DataType::U32 | DataType::I32 => Ok(4),
            DataType::U64 | DataType::I64 => Ok(8),
            DataType::F64 => Ok(8),
            DataType::Void => Ok(0),
            DataType::Pointer(_) => Ok(8),
            DataType::Array(elem, count) => {
                let size = self.get_type_size_inner(elem, visiting)?;
                Ok(size * (*count as u32))
            }
            DataType::Typedef(_, underlying) => self.get_type_size_inner(underlying, visiting),
            DataType::Struct(name) => {
                if let Some(alias) = self.typedefs.get(name) {
                    self.get_type_size_inner(alias, visiting)
                } else {
                    let (size, _) = self.calculate_struct_layout_inner(name, visiting)?;
                    Ok(size)
                }
            }
        }
    }
    pub fn verify_calls(&self, program: &Program) -> Result<(), String> {
        for func in &program.functions {
            if func.is_irq && self.use_os {
                return Err("irq requires sc.false".to_string());
            }
            if let Some(body) = &func.body {
                self.verify_stmts_calls(body)?;
            }
        }
        self.verify_arities(program)
    }

    /* Return/call arity is part of the ABI: the backend places exactly the
     * values present in `return(...)`, so a missing value leaves a stale
     * register. `jmpto` blocks are skipped because their `return(var)`
     * entries name continuation results, not function returns. */
    pub fn verify_arities(&self, program: &Program) -> Result<(), String> {
        for func in &program.functions {
            if let Some(body) = &func.body {
                self.verify_arity_stmts(body, func)?;
            }
        }
        Ok(())
    }

    fn verify_arity_stmts(&self, stmts: &[Stmt], func: &FuncDecl) -> Result<(), String> {
        for stmt in stmts {
            match stmt {
                Stmt::VarDefinition(decl) => {
                    if let Some(init) = &decl.initial_value {
                        self.verify_arity_expr(init)?;
                    }
                }
                Stmt::Assignment { targets, value } => {
                    for target in targets {
                        self.verify_arity_expr(target)?;
                    }
                    self.verify_arity_expr(value)?;
                }
                Stmt::If {
                    cond,
                    then_branch,
                    else_branch,
                } => {
                    self.verify_arity_expr(cond)?;
                    self.verify_arity_stmts(then_branch, func)?;
                    if let Some(else_b) = else_branch {
                        self.verify_arity_stmts(else_b, func)?;
                    }
                }
                Stmt::While { cond, body } => {
                    self.verify_arity_expr(cond)?;
                    self.verify_arity_stmts(body, func)?;
                }
                Stmt::For {
                    init,
                    cond,
                    post,
                    body,
                } => {
                    if let Some(i) = init {
                        self.verify_arity_stmts(std::slice::from_ref(i.as_ref()), func)?;
                    }
                    self.verify_arity_expr(cond)?;
                    if let Some(p) = post {
                        self.verify_arity_stmts(std::slice::from_ref(p), func)?;
                    }
                    self.verify_arity_stmts(body, func)?;
                }
                Stmt::Return(values) => {
                    if !func.return_types.is_empty() && values.len() != func.return_types.len() {
                        return Err(format!(
                            "function '{}' declares {} return value(s) but returns {}",
                            func.name,
                            func.return_types.len(),
                            values.len()
                        ));
                    }
                }
                Stmt::Critical(body) => self.verify_arity_stmts(body, func)?,
                Stmt::Match {
                    expr,
                    cases,
                    default,
                } => {
                    self.verify_arity_expr(expr)?;
                    for (ce, body) in cases {
                        self.verify_arity_expr(ce)?;
                        self.verify_arity_stmts(body, func)?;
                    }
                    if let Some(d) = default {
                        self.verify_arity_stmts(d, func)?;
                    }
                }
                Stmt::Expr(expr) => self.verify_arity_expr(expr)?,
                _ => {}
            }
        }
        Ok(())
    }

    fn verify_arity_expr(&self, expr: &Expr) -> Result<(), String> {
        match expr {
            Expr::Call { name, args } => {
                /* The standard-library formatters are declared with a fixed
                 * set of argument slots but are called with a varying number
                 * of arguments throughout the language's programs. */
                let variadic = name == "printf" || name == "snprintf";
                if let Some(callee) = self.functions.get(name) {
                    if !variadic && args.len() != callee.params.len() {
                        return Err(format!(
                            "call to '{}' passes {} argument(s), expected {}",
                            name,
                            args.len(),
                            callee.params.len()
                        ));
                    }
                }
                for arg in args {
                    self.verify_arity_expr(arg)?;
                }
            }
            Expr::Binary { left, right, .. } => {
                self.verify_arity_expr(left)?;
                self.verify_arity_expr(right)?;
            }
            Expr::Index { expr: base, index } => {
                self.verify_arity_expr(base)?;
                self.verify_arity_expr(index)?;
            }
            Expr::MemberAccess { expr: base, .. } => self.verify_arity_expr(base)?,
            Expr::AddrOfExpr(inner) => self.verify_arity_expr(inner)?,
            Expr::ArrayInit(items) => {
                for item in items {
                    self.verify_arity_expr(item)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn verify_stmts_calls(&self, stmts: &[Stmt]) -> Result<(), String> {
        for stmt in stmts {
            match stmt {
                Stmt::VarDefinition(decl) => {
                    if let Some(ref init) = decl.initial_value {
                        self.verify_expr_calls(init)?;
                    }
                }
                Stmt::Assignment { targets, value } => {
                    for target in targets {
                        self.verify_expr_calls(target)?;
                    }
                    self.verify_expr_calls(value)?;
                }
                Stmt::If {
                    cond,
                    then_branch,
                    else_branch,
                } => {
                    self.verify_expr_calls(cond)?;
                    self.verify_stmts_calls(then_branch)?;
                    if let Some(else_b) = else_branch {
                        self.verify_stmts_calls(else_b)?;
                    }
                }
                Stmt::While { cond, body } => {
                    self.verify_expr_calls(cond)?;
                    self.verify_stmts_calls(body)?;
                }
                Stmt::For {
                    init,
                    cond,
                    post,
                    body,
                } => {
                    if let Some(i) = init {
                        self.verify_stmts_calls(&[*i.clone()])?;
                    }
                    self.verify_expr_calls(cond)?;
                    if let Some(p) = post {
                        self.verify_stmts_calls(&[*p.clone()])?;
                    }
                    self.verify_stmts_calls(body)?;
                }
                Stmt::Return(values) => {
                    for (_, expr) in values {
                        self.verify_expr_calls(expr)?;
                    }
                }
                Stmt::Jmpto { args, .. } => {
                    self.verify_stmts_calls(args)?;
                }
                Stmt::Critical(body) => {
                    if self.use_os {
                        return Err("critical requires sc.false".to_string());
                    }
                    self.verify_stmts_calls(body)?;
                }
                Stmt::Expr(expr) => {
                    self.verify_expr_calls(expr)?;
                }
                Stmt::Match {
                    expr,
                    cases,
                    default,
                } => {
                    self.verify_expr_calls(expr)?;
                    for (ce, body) in cases {
                        self.verify_expr_calls(ce)?;
                        self.verify_stmts_calls(body)?;
                    }
                    if let Some(d) = default {
                        self.verify_stmts_calls(d)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn verify_expr_calls(&self, expr: &Expr) -> Result<(), String> {
        match expr {
            Expr::Call { name, args } => {
                let builtins = [
                    "syscall0",
                    "syscall1",
                    "syscall2",
                    "syscall3",
                    "syscall4",
                    "syscall5",
                    "syscall6",
                    "inb",
                    "outb",
                    "inw",
                    "outw",
                    "inl",
                    "outl",
                    "sizeof",
                    "alignof",
                    "offsetof",
                    "versionof",
                    "fieldsof",
                    "nameof",
                    "atomic_load",
                    "atomic_store",
                    "atomic_add",
                    "atomic_sub",
                    "atomic_inc",
                    "atomic_dec",
                    "atomic_swap",
                    "atomic_cas",
                    "memory_barrier",
                    "compiler_barrier",
                    "mem_load_u8",
                    "mem_load_u16",
                    "mem_load_u32",
                    "mem_load_u64",
                    "mem_load_i8",
                    "mem_load_i16",
                    "mem_load_i32",
                    "mem_load_i64",
                    "mem_store_u8",
                    "mem_store_u16",
                    "mem_store_u32",
                    "mem_store_u64",
                    "mem_store_i8",
                    "mem_store_i16",
                    "mem_store_i32",
                    "mem_store_i64",
                    "volatile_load_u8",
                    "volatile_load_u16",
                    "volatile_load_u32",
                    "volatile_load_u64",
                    "volatile_load_i8",
                    "volatile_load_i16",
                    "volatile_load_i32",
                    "volatile_load_i64",
                    "volatile_store_u8",
                    "volatile_store_u16",
                    "volatile_store_u32",
                    "volatile_store_u64",
                    "volatile_store_i8",
                    "volatile_store_i16",
                    "volatile_store_i32",
                    "volatile_store_i64",
                    "jmpto",
                ];

                let hosted_only = [
                    "syscall0", "syscall1", "syscall2", "syscall3", "syscall4", "syscall5",
                    "syscall6",
                ];

                if hosted_only.contains(&name.as_str()) && !self.use_os {
                    return Err(format!("'{}' requires sc.true", name));
                }

                if !builtins.contains(&name.as_str()) && !self.functions.contains_key(name) {
                    return Err(format!("call to undeclared function '{}'", name));
                }
                for arg in args {
                    self.verify_expr_calls(arg)?;
                }
            }
            Expr::Binary { left, right, .. } => {
                self.verify_expr_calls(left)?;
                self.verify_expr_calls(right)?;
            }
            Expr::Index { expr: base, index } => {
                self.verify_expr_calls(base)?;
                self.verify_expr_calls(index)?;
            }
            Expr::MemberAccess { expr: base, .. } => {
                self.verify_expr_calls(base)?;
            }
            Expr::AddrOfExpr(inner) => {
                self.verify_expr_calls(inner)?;
            }
            Expr::ArrayInit(items) => {
                for item in items {
                    self.verify_expr_calls(item)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
