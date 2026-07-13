#![allow(dead_code)]

use crate::ast::*;
use std::collections::HashMap;

pub struct TypeChecker {
    structs: HashMap<String, StructDecl>,
    sections: HashMap<String, SectionDecl>,
    functions: HashMap<String, FuncDecl>,
    typedefs: HashMap<String, DataType>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            structs: HashMap::new(),
            sections: HashMap::new(),
            functions: HashMap::new(),
            typedefs: HashMap::new(),
        }
    }

    pub fn populate_symbols(&mut self, program: &Program) {
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
        let s = self
            .structs
            .get(struct_name)
            .ok_ok_or_else(|| format!("Struct {} not defined", struct_name))?;

        let mut offsets = Vec::new();

        // Расчет для объединений (Unions)
        if s.is_union {
            let mut max_size = 0;
            for field in &s.fields {
                offsets.push(0);
                let size = self.get_type_size(&field.data_type)?;
                if size > max_size {
                    max_size = size;
                }
            }
            return Ok((max_size, offsets));
        }

        let mut current_offset = 0;
        let mut max_alignment = 1;

        for field in &s.fields {
            let size = self.get_type_size(&field.data_type)?;
            let mut alignment = size;
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
        match dt {
            DataType::U8 | DataType::I8 => Ok(1),
            DataType::U16 | DataType::I16 => Ok(2),
            DataType::U32 | DataType::I32 => Ok(4),
            DataType::U64 | DataType::I64 => Ok(8),
            DataType::F64 => Ok(8),
            DataType::Void => Ok(0),
            DataType::Pointer(_) => Ok(8),
            DataType::Array(elem, count) => {
                let size = self.get_type_size(elem)?;
                Ok(size * (*count as u32))
            }
            DataType::Typedef(_, underlying) => self.get_type_size(underlying),
            DataType::Struct(name) => {
                if let Some(alias) = self.typedefs.get(name) {
                    self.get_type_size(alias)
                } else {
                    let (size, _) = self.calculate_struct_layout(name)?;
                    Ok(size)
                }
            }
        }
    }
    pub fn verify_calls(&self, program: &Program) -> Result<(), String> {
        for func in &program.functions {
            if let Some(body) = &func.body {
                self.verify_stmts_calls(body)?;
            }
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
                Stmt::Expr(expr) => {
                    self.verify_expr_calls(expr)?;
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
                    "mloc",
                    "bmloc",
                    "mfree",
                    "sys_read",
                    "sys_write",
                    "sys_open",
                    "sys_close",
                    "sys_unlink",
                    "sys_ioctl",
                    "sys_exit",
                    "sys_fork",
                    "sys_execve",
                    "sys_wait4",
                    "inb",
                    "outb",
                    "inw",
                    "outw",
                    "inl",
                    "outl",
                ];
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
            _ => {}
        }
        Ok(())
    }
}

trait OkOr {
    type Value;
    fn ok_ok_or_else<F, E>(self, f: F) -> Result<Self::Value, E>
    where
        F: FnOnce() -> E;
}

impl<T> OkOr for Option<T> {
    type Value = T;
    fn ok_ok_or_else<F, E>(self, f: F) -> Result<Self::Value, E>
    where
        F: FnOnce() -> E,
    {
        self.ok_or_else(f)
    }
}
