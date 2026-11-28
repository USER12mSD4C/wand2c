#![allow(dead_code)]

use crate::ast::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueSign {
    Signed,
    Unsigned,
    Unknown,
}

#[derive(Clone, Debug)]
struct OptimizeEnv {
    signs: HashMap<String, ValueSign>,
    copy_sources: HashSet<String>,
    single_assignment: HashSet<String>,
    /* Variables whose value must be re-read at every access. */
    volatile: HashSet<String>,
    /* Array/struct roots whose elements or fields may alias. */
    array_roots: HashSet<String>,
    union_roots: HashSet<String>,
}

impl OptimizeEnv {
    fn from_function(
        func: &FuncDecl,
        structs: &HashMap<String, StructDecl>,
        typedefs: &HashMap<String, DataType>,
    ) -> Self {
        let mut signs = HashMap::new();
        let mut assigned: HashMap<String, usize> = HashMap::new();
        let mut volatile = HashSet::new();
        let mut array_roots = HashSet::new();
        let mut union_roots = HashSet::new();

        for (ty, name, access) in &func.params {
            signs.insert(name.clone(), data_type_sign(ty));
            if matches!(access, PtrAccess::Volatile | PtrAccess::Atomic) {
                volatile.insert(name.clone());
            }
            if type_is_array(ty, typedefs) {
                array_roots.insert(name.clone());
            }
            if type_is_union(ty, structs, typedefs) {
                union_roots.insert(name.clone());
            }
            let count = assigned.entry(name.clone()).or_insert(0);
            if matches!(
                access,
                PtrAccess::Output
                    | PtrAccess::InputOutput
                    | PtrAccess::Volatile
                    | PtrAccess::Atomic
            ) {
                *count += 1;
            }
        }

        if let Some(ref body) = func.body {
            collect_signs(body, &mut signs);
            collect_assignment_counts(body, &mut assigned);
            collect_decl_aliases(
                body,
                structs,
                typedefs,
                &mut volatile,
                &mut array_roots,
                &mut union_roots,
            );

            let mut reads = HashMap::new();
            collect_reads(body, &mut reads);
            for name in reads.keys() {
                assigned.entry(name.clone()).or_insert(0);
            }
        }

        let mut escaped = HashSet::new();
        if let Some(ref body) = func.body {
            collect_escaped(body, &mut escaped);
        }

        let copy_sources = assigned
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(name, _)| name.clone())
            .filter(|name| !escaped.contains(name))
            .collect();

        let single_assignment = assigned
            .iter()
            .filter(|(_, count)| **count == 1)
            .map(|(name, _)| name.clone())
            .filter(|name| !escaped.contains(name))
            .collect();

        Self {
            signs,
            copy_sources,
            single_assignment,
            volatile,
            array_roots,
            union_roots,
        }
    }

    fn sign(&self, name: &str) -> ValueSign {
        self.signs.get(name).copied().unwrap_or(ValueSign::Unknown)
    }

    fn can_copy_from(&self, name: &str) -> bool {
        self.copy_sources.contains(name)
    }
}

fn data_type_sign(ty: &DataType) -> ValueSign {
    match ty {
        DataType::U8 | DataType::U16 | DataType::U32 | DataType::U64 => ValueSign::Unsigned,
        DataType::I8 | DataType::I16 | DataType::I32 | DataType::I64 => ValueSign::Signed,
        DataType::Pointer(_) | DataType::Array(_, _) => ValueSign::Unsigned,
        DataType::Typedef(_, inner) => data_type_sign(inner),
        _ => ValueSign::Unknown,
    }
}

fn type_is_array(ty: &DataType, typedefs: &HashMap<String, DataType>) -> bool {
    match ty {
        DataType::Array(_, _) => true,
        DataType::Typedef(_, inner) => type_is_array(inner, typedefs),
        /* Named typedefs are packed as Struct(name) in the optimizer AST. */
        DataType::Struct(name) => typedefs
            .get(name)
            .map(|inner| type_is_array(inner, typedefs))
            .unwrap_or(false),
        _ => false,
    }
}

fn type_is_union(
    ty: &DataType,
    structs: &HashMap<String, StructDecl>,
    typedefs: &HashMap<String, DataType>,
) -> bool {
    match ty {
        DataType::Struct(name) => {
            if let Some(inner) = typedefs.get(name) {
                return type_is_union(inner, structs, typedefs);
            }
            structs.get(name).map(|s| s.is_union).unwrap_or(false)
        }
        DataType::Typedef(_, inner) | DataType::Pointer(inner) | DataType::Array(inner, _) => {
            type_is_union(inner, structs, typedefs)
        }
        _ => false,
    }
}

fn collect_decl_aliases(
    body: &[Stmt],
    structs: &HashMap<String, StructDecl>,
    typedefs: &HashMap<String, DataType>,
    volatile: &mut HashSet<String>,
    array_roots: &mut HashSet<String>,
    union_roots: &mut HashSet<String>,
) {
    for stmt in body {
        match stmt {
            Stmt::VarDefinition(decl) => {
                if matches!(decl.modifier, PtrAccess::Volatile | PtrAccess::Atomic) {
                    volatile.insert(decl.name.clone());
                }
                if type_is_array(&decl.data_type, typedefs) {
                    array_roots.insert(decl.name.clone());
                }
                if type_is_union(&decl.data_type, structs, typedefs) {
                    union_roots.insert(decl.name.clone());
                }
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_decl_aliases(then_branch, structs, typedefs, volatile, array_roots, union_roots);
                if let Some(branch) = else_branch {
                    collect_decl_aliases(branch, structs, typedefs, volatile, array_roots, union_roots);
                }
            }
            Stmt::While { body, .. } | Stmt::Critical(body) => {
                collect_decl_aliases(body, structs, typedefs, volatile, array_roots, union_roots);
            }
            Stmt::For { init, post, body, .. } => {
                for nested in [init, post] {
                    if let Some(stmt) = nested {
                        collect_decl_aliases(
                            std::slice::from_ref(stmt.as_ref()),
                            structs,
                            typedefs,
                            volatile,
                            array_roots,
                            union_roots,
                        );
                    }
                }
                collect_decl_aliases(body, structs, typedefs, volatile, array_roots, union_roots);
            }
            Stmt::Jmpto { args, .. } => {
                collect_decl_aliases(args, structs, typedefs, volatile, array_roots, union_roots);
            }
            Stmt::Match { cases, default, .. } => {
                for (_, body) in cases {
                    collect_decl_aliases(body, structs, typedefs, volatile, array_roots, union_roots);
                }
                if let Some(body) = default {
                    collect_decl_aliases(body, structs, typedefs, volatile, array_roots, union_roots);
                }
            }
            _ => {}
        }
    }
}

fn expr_sign(expr: &Expr, env: &OptimizeEnv) -> ValueSign {
    match expr {
        Expr::Number(_) | Expr::AddrOf(_) => ValueSign::Unsigned,
        Expr::SignedNumber(_) => ValueSign::Signed,
        Expr::Variable(name) => env.sign(name),
        Expr::Binary {
            left, op, right, ..
        } => match op.as_str() {
            "OpAdd" | "OpSub" | "OpMul" | "OpBitAnd" | "OpBitOr" | "OpBitXor" | "OpShl"
            | "OpShr" => {
                let l = expr_sign(left, env);
                let r = expr_sign(right, env);
                if l == ValueSign::Unsigned && r == ValueSign::Unsigned {
                    ValueSign::Unsigned
                } else if l == ValueSign::Signed && r == ValueSign::Signed {
                    ValueSign::Signed
                } else {
                    ValueSign::Unknown
                }
            }
            "OpEq" | "OpEqEq" | "==" | "OpNotEq" | "OpNe" | "!=" | "OpLt" | "Lt" | "<"
            | "OpLtEq" | "OpLe" | "<=" | "OpGt" | "Gt" | ">" | "OpGtEq" | "OpGe" | ">="
            | "OpAnd" | "&&" | "OpOr" | "||" => ValueSign::Unsigned,
            _ => ValueSign::Unknown,
        },
        _ => ValueSign::Unknown,
    }
}

/* A volatile/atomic variable is a fresh read on every access, so `v - v`
 * and `v & v` may not be simplified and operands containing it may not be
 * discarded. */
fn is_pure_expr(expr: &Expr, env: &OptimizeEnv) -> bool {
    match expr {
        Expr::Variable(name) => !env.volatile.contains(name),
        Expr::Number(_)
        | Expr::SignedNumber(_)
        | Expr::FloatLit(_)
        | Expr::StringLit(_)
        | Expr::AddrOf(_)
        | Expr::Null => true,
        Expr::Binary { left, right, .. } => {
            is_pure_expr(left, env) && is_pure_expr(right, env)
        }
        Expr::AddrOfExpr(inner) => is_pure_expr(inner, env),
        _ => false,
    }
}

fn collect_signs(body: &[Stmt], signs: &mut HashMap<String, ValueSign>) {
    for stmt in body {
        collect_signs_stmt(stmt, signs);
    }
}

fn collect_signs_stmt(stmt: &Stmt, signs: &mut HashMap<String, ValueSign>) {
    match stmt {
        Stmt::VarDefinition(decl) => {
            signs.insert(decl.name.clone(), data_type_sign(&decl.data_type));
        }
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_signs(then_branch, signs);
            if let Some(branch) = else_branch {
                collect_signs(branch, signs);
            }
        }
        Stmt::While { body, .. } => collect_signs(body, signs),
        Stmt::For {
            init, post, body, ..
        } => {
            if let Some(init) = init {
                collect_signs_stmt(init, signs);
            }
            if let Some(post) = post {
                collect_signs_stmt(post, signs);
            }
            collect_signs(body, signs);
        }
        Stmt::Jmpto { args, .. } => collect_signs(args, signs),
        Stmt::Critical(body) => collect_signs(body, signs),
        Stmt::Match { cases, default, .. } => {
            for (_, body) in cases {
                collect_signs(body, signs);
            }
            if let Some(body) = default {
                collect_signs(body, signs);
            }
        }
        _ => {}
    }
}

fn collect_assignment_counts(body: &[Stmt], counts: &mut HashMap<String, usize>) {
    for stmt in body {
        collect_assignment_counts_stmt(stmt, counts);
    }
}

fn count_assignment(counts: &mut HashMap<String, usize>, name: &str) {
    *counts.entry(name.to_string()).or_insert(0) += 1;
}

fn collect_assignment_counts_stmt(stmt: &Stmt, counts: &mut HashMap<String, usize>) {
    match stmt {
        Stmt::VarDefinition(decl) => count_assignment(counts, &decl.name),
        Stmt::Assignment { targets, .. } => {
            for target in targets {
                if let Some(path) = get_expr_path(target) {
                    count_assignment(counts, expr_path_root(&path));
                }
            }
        }
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_assignment_counts(then_branch, counts);
            if let Some(branch) = else_branch {
                collect_assignment_counts(branch, counts);
            }
        }
        Stmt::While { body, .. } => collect_assignment_counts(body, counts),
        Stmt::For {
            init, post, body, ..
        } => {
            if let Some(init) = init {
                collect_assignment_counts_stmt(init, counts);
            }
            if let Some(post) = post {
                collect_assignment_counts_stmt(post, counts);
            }
            collect_assignment_counts(body, counts);
        }
        Stmt::Jmpto { args, .. } => collect_assignment_counts(args, counts),
        Stmt::Critical(body) => collect_assignment_counts(body, counts),
        Stmt::Match { cases, default, .. } => {
            for (_, body) in cases {
                collect_assignment_counts(body, counts);
            }
            if let Some(body) = default {
                collect_assignment_counts(body, counts);
            }
        }
        _ => {}
    }
}

pub struct Optimizer;

impl Optimizer {
    pub fn optimize_program(program: &mut Program) -> usize {
        let structs: HashMap<String, StructDecl> = program
            .structs
            .iter()
            .map(|s| (s.name.clone(), s.clone()))
            .collect();
        let typedefs: HashMap<String, DataType> = program
            .typedefs
            .iter()
            .map(|(name, ty)| (name.clone(), ty.clone()))
            .collect();
        let mut total_optimized = 0;
        for func in &mut program.functions {
            let env = OptimizeEnv::from_function(&*func, &structs, &typedefs);
            if let Some(ref mut body) = func.body {
                let mut iteration = 0;
                loop {
                    let mut count = 0;
                    let mut escaped = HashSet::new();
                    collect_escaped(body, &mut escaped);
                    collect_call_array_escaped(body, &env.array_roots, &mut escaped);

                    let mut reads = HashMap::new();
                    collect_reads(body, &mut reads);

                    let mut output_ptrs = HashSet::new();
                    for (_, name, access) in &func.params {
                        if *access == PtrAccess::Output
                            || *access == PtrAccess::InputOutput
                            || *access == PtrAccess::Volatile
                            || *access == PtrAccess::Atomic
                        {
                            output_ptrs.insert(name.clone());
                        }
                    }

                    let mut consts = HashMap::new();
                    let old_body = std::mem::take(body);
                    let new_body = optimize_statements(
                        old_body,
                        &mut consts,
                        &escaped,
                        &reads,
                        &mut output_ptrs,
                        &mut count,
                        &env,
                    );
                    *body = new_body;

                    total_optimized += count;
                    iteration += 1;
                    if count == 0 || iteration >= 10 {
                        break;
                    }
                }
            }
        }
        total_optimized
    }
}

fn collect_escaped(body: &[Stmt], escaped: &mut HashSet<String>) {
    for stmt in body {
        collect_escaped_stmt(stmt, escaped);
    }
}

fn collect_escaped_stmt(stmt: &Stmt, escaped: &mut HashSet<String>) {
    match stmt {
        Stmt::VarDefinition(decl) => {
            if let Some(ref init) = decl.initial_value {
                collect_escaped_expr(init, escaped);
            }
        }
        Stmt::Assignment { targets, value } => {
            for target in targets {
                collect_escaped_expr(target, escaped);
            }
            collect_escaped_expr(value, escaped);
        }
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_escaped_expr(cond, escaped);
            collect_escaped(then_branch, escaped);
            if let Some(else_stmts) = else_branch {
                collect_escaped(else_stmts, escaped);
            }
        }
        Stmt::While { cond, body } => {
            collect_escaped_expr(cond, escaped);
            collect_escaped(body, escaped);
        }
        Stmt::For {
            init,
            cond,
            post,
            body,
        } => {
            if let Some(ref i) = init {
                collect_escaped_stmt(i, escaped);
            }
            collect_escaped_expr(cond, escaped);
            if let Some(ref p) = post {
                collect_escaped_stmt(p, escaped);
            }
            collect_escaped(body, escaped);
        }
        Stmt::Return(values) => {
            for (_, expr) in values {
                collect_escaped_expr(expr, escaped);
            }
        }
        Stmt::Jmpto { args, .. } => {
            for arg in args {
                /* `return(var)` inside a jmpto block names the variable that
                 * receives a continuation result. The backend requires the
                 * marker to remain a plain variable, and the loader may write
                 * that variable, so it must never be constant-propagated. */
                if let Stmt::Return(values) = arg {
                    for (_, expr) in values {
                        if let Expr::Variable(name) = expr {
                            escaped.insert(name.clone());
                        }
                    }
                }
                collect_escaped_stmt(arg, escaped);
                /* The inlined module body can write to locals declared in the
                 * block, and the dynamic loader can write result variables.
                 * Keep those definitions alive and unpropagated. */
                collect_assigned_stmt(arg, escaped);
            }
        }
        Stmt::Expr(expr) => {
            collect_escaped_expr(expr, escaped);
        }
        Stmt::Match {
            expr,
            cases,
            default,
        } => {
            collect_escaped_expr(expr, escaped);
            for (ce, body) in cases {
                collect_escaped_expr(ce, escaped);
                collect_escaped(body, escaped);
            }
            if let Some(d) = default {
                collect_escaped(d, escaped);
            }
        }
        Stmt::Critical(body) => {
            collect_escaped(body, escaped);
        }
        Stmt::Nasm(code) => {
            // Locals named as `[name]` inside an asm block are read/written by
            // the backend, not by AST expressions. Keep their definitions and
            // prevent constant propagation into them.
            let bytes = code.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'[' {
                    let start = i + 1;
                    let mut end = start;
                    while end < bytes.len() && bytes[end] != b']' {
                        end += 1;
                    }
                    let name = code[start..end].trim();
                    if !name.is_empty()
                        && name
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':')
                    {
                        let root = name.rsplit(':').next().unwrap_or(name);
                        escaped.insert(root.to_string());
                    }
                    i = end;
                }
                i += 1;
            }
        }
        _ => {}
    }
}

fn collect_escaped_expr(expr: &Expr, escaped: &mut HashSet<String>) {
    match expr {
        Expr::AddrOf(name) => {
            let root = if let Some(idx) = name.find(':') {
                &name[idx + 1..]
            } else {
                name.as_str()
            };
            escaped.insert(root.to_string());
        }
        Expr::Binary { left, right, .. } => {
            collect_escaped_expr(left, escaped);
            collect_escaped_expr(right, escaped);
        }
        Expr::Call { args, .. } => {
            for arg in args {
                collect_escaped_expr(arg, escaped);
            }
        }
        Expr::Index { expr: base, index } => {
            collect_escaped_expr(base, escaped);
            collect_escaped_expr(index, escaped);
        }
        Expr::MemberAccess { expr: base, .. } => {
            collect_escaped_expr(base, escaped);
        }
        Expr::AddrOfExpr(inner) => {
            collect_addr_target_roots(inner, escaped);
            collect_escaped_expr(inner, escaped);
        }
        Expr::ArrayInit(items) => {
            for item in items {
                collect_escaped_expr(item, escaped);
            }
        }
        _ => {}
    }
}

/* An array may be passed to a pointer parameter without an explicit `*adr`,
 * so the callee can write through it. Mark those roots escaped or cached
 * element constants survive the call. */
fn collect_call_array_escaped(
    body: &[Stmt],
    array_roots: &HashSet<String>,
    escaped: &mut HashSet<String>,
) {
    fn walk_expr(expr: &Expr, array_roots: &HashSet<String>, escaped: &mut HashSet<String>) {
        match expr {
            Expr::Call { args, .. } => {
                for arg in args {
                    if let Expr::Variable(name) = arg {
                        if array_roots.contains(name) {
                            escaped.insert(name.clone());
                        }
                    }
                    walk_expr(arg, array_roots, escaped);
                }
            }
            Expr::Binary { left, right, .. } => {
                walk_expr(left, array_roots, escaped);
                walk_expr(right, array_roots, escaped);
            }
            Expr::Index { expr: base, index } => {
                walk_expr(base, array_roots, escaped);
                walk_expr(index, array_roots, escaped);
            }
            Expr::MemberAccess { expr: base, .. } | Expr::AddrOfExpr(base) => {
                walk_expr(base, array_roots, escaped);
            }
            Expr::ArrayInit(items) => {
                for item in items {
                    walk_expr(item, array_roots, escaped);
                }
            }
            _ => {}
        }
    }
    fn walk_stmt(stmt: &Stmt, array_roots: &HashSet<String>, escaped: &mut HashSet<String>) {
        match stmt {
            Stmt::VarDefinition(decl) => {
                if let Some(init) = &decl.initial_value {
                    walk_expr(init, array_roots, escaped);
                }
            }
            Stmt::Assignment { targets, value } => {
                for target in targets {
                    walk_expr(target, array_roots, escaped);
                }
                walk_expr(value, array_roots, escaped);
            }
            Stmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                walk_expr(cond, array_roots, escaped);
                walk_stmts(then_branch, array_roots, escaped);
                if let Some(b) = else_branch {
                    walk_stmts(b, array_roots, escaped);
                }
            }
            Stmt::While { cond, body } => {
                walk_expr(cond, array_roots, escaped);
                walk_stmts(body, array_roots, escaped);
            }
            Stmt::For {
                init,
                cond,
                post,
                body,
            } => {
                for nested in [init, post] {
                    if let Some(stmt) = nested {
                        walk_stmt(stmt, array_roots, escaped);
                    }
                }
                walk_expr(cond, array_roots, escaped);
                walk_stmts(body, array_roots, escaped);
            }
            Stmt::Return(values) => {
                for (_, expr) in values {
                    walk_expr(expr, array_roots, escaped);
                }
            }
            Stmt::Jmpto { args, .. } | Stmt::Critical(args) => {
                walk_stmts(args, array_roots, escaped);
            }
            Stmt::Expr(expr) => walk_expr(expr, array_roots, escaped),
            Stmt::Match {
                expr,
                cases,
                default,
            } => {
                walk_expr(expr, array_roots, escaped);
                for (ce, body) in cases {
                    walk_expr(ce, array_roots, escaped);
                    walk_stmts(body, array_roots, escaped);
                }
                if let Some(body) = default {
                    walk_stmts(body, array_roots, escaped);
                }
            }
            _ => {}
        }
    }
    fn walk_stmts(body: &[Stmt], array_roots: &HashSet<String>, escaped: &mut HashSet<String>) {
        for stmt in body {
            walk_stmt(stmt, array_roots, escaped);
        }
    }
    walk_stmts(body, array_roots, escaped);
}

fn collect_addr_target_roots(expr: &Expr, escaped: &mut HashSet<String>) {
    match expr {
        Expr::Variable(name) => {
            escaped.insert(name.clone());
        }

        Expr::MemberAccess { expr: base, .. } => {
            collect_addr_target_roots(base, escaped);
        }

        Expr::Index { expr: base, index } => {
            collect_addr_target_roots(base, escaped);
            collect_addr_target_roots(index, escaped);
        }

        Expr::Binary { left, right, .. } => {
            collect_addr_target_roots(left, escaped);
            collect_addr_target_roots(right, escaped);
        }

        Expr::Call { args, .. } => {
            for arg in args {
                collect_addr_target_roots(arg, escaped);
            }
        }

        _ => {}
    }
}

fn collect_reads(body: &[Stmt], reads: &mut HashMap<String, usize>) {
    for stmt in body {
        collect_reads_stmt(stmt, reads);
    }
}

fn collect_reads_stmt(stmt: &Stmt, reads: &mut HashMap<String, usize>) {
    match stmt {
        Stmt::VarDefinition(decl) => {
            if let Some(ref init) = decl.initial_value {
                collect_reads_expr(init, reads);
            }
        }
        Stmt::Assignment { targets, value } => {
            for target in targets {
                collect_reads_lhs(target, reads);
            }
            collect_reads_expr(value, reads);
        }
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_reads_expr(cond, reads);
            collect_reads(then_branch, reads);
            if let Some(else_stmts) = else_branch {
                collect_reads(else_stmts, reads);
            }
        }
        Stmt::While { cond, body } => {
            collect_reads_expr(cond, reads);
            collect_reads(body, reads);
        }
        Stmt::For {
            init,
            cond,
            post,
            body,
        } => {
            if let Some(ref i) = init {
                collect_reads_stmt(i, reads);
            }
            collect_reads_expr(cond, reads);
            if let Some(ref p) = post {
                collect_reads_stmt(p, reads);
            }
            collect_reads(body, reads);
        }
        Stmt::Return(values) => {
            for (_, expr) in values {
                collect_reads_expr(expr, reads);
            }
        }
        Stmt::Jmpto { args, .. } => {
            for arg in args {
                collect_reads_stmt(arg, reads);
            }
        }
        Stmt::Expr(expr) => {
            collect_reads_expr(expr, reads);
        }
        Stmt::Match {
            expr,
            cases,
            default,
        } => {
            collect_reads_expr(expr, reads);
            for (ce, body) in cases {
                collect_reads_expr(ce, reads);
                collect_reads(body, reads);
            }
            if let Some(d) = default {
                collect_reads(d, reads);
            }
        }
        Stmt::Critical(body) => {
            collect_reads(body, reads);
        }
        _ => {}
    }
}

fn collect_reads_lhs(expr: &Expr, reads: &mut HashMap<String, usize>) {
    match expr {
        Expr::Variable(_) => {}
        Expr::MemberAccess { expr: base, .. } => {
            collect_reads_expr(base, reads);
        }
        Expr::Index { expr: base, index } => {
            collect_reads_expr(base, reads);
            collect_reads_expr(index, reads);
        }
        _ => {
            collect_reads_expr(expr, reads);
        }
    }
}

fn collect_reads_expr(expr: &Expr, reads: &mut HashMap<String, usize>) {
    match expr {
        Expr::Variable(name) => {
            *reads.entry(name.clone()).or_insert(0) += 1;
        }
        Expr::Binary { left, right, .. } => {
            collect_reads_expr(left, reads);
            collect_reads_expr(right, reads);
        }
        Expr::Call { args, .. } => {
            for arg in args {
                collect_reads_expr(arg, reads);
            }
        }
        Expr::Index { expr: base, index } => {
            collect_reads_expr(base, reads);
            collect_reads_expr(index, reads);
        }
        Expr::MemberAccess { expr: base, .. } => {
            collect_reads_expr(base, reads);
        }
        Expr::AddrOfExpr(inner) => {
            collect_reads_expr(inner, reads);
        }
        Expr::ArrayInit(items) => {
            for item in items {
                collect_reads_expr(item, reads);
            }
        }
        _ => {}
    }
}

/* The root of a path is its leftmost identifier. Member/index paths can mix
 * `.` and `[` (`p[0].x`), so taking either delimiter alone picks the wrong
 * root and lets cached values survive writes that alias the array. Section
 * variables are keyed by the part after `:`. */
fn expr_path_root(path: &str) -> &str {
    let base = if let Some(idx) = path.find(':') {
        &path[idx + 1..]
    } else {
        path
    };
    let dot = base.find('.');
    let bracket = base.find('[');
    let idx = match (dot, bracket) {
        (Some(d), Some(b)) => Some(d.min(b)),
        (Some(d), None) => Some(d),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    idx.map(|i| &base[..i]).unwrap_or(base)
}

fn get_expr_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Variable(name) => Some(name.clone()),
        Expr::MemberAccess {
            expr: base, member, ..
        } => {
            let base_path = get_expr_path(base)?;
            Some(format!("{}.{}", base_path, member))
        }
        Expr::Index { expr: base, index } => {
            let base_path = get_expr_path(base)?;
            if let Expr::Number(n) = &**index {
                Some(format!("{}[{}]", base_path, n))
            } else {
                None
            }
        }
        Expr::AddrOfExpr(inner) => get_expr_path(inner),
        _ => None,
    }
}

fn expr_root_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Variable(name) => Some(name),
        Expr::AddrOf(name) => Some(name.rsplit(':').next().unwrap_or(name)),
        Expr::MemberAccess { expr: base, .. } | Expr::AddrOfExpr(base) => expr_root_name(base),
        Expr::Index { expr: base, .. } => expr_root_name(base),
        _ => None,
    }
}

fn invalidate_root_elements(consts: &mut HashMap<String, Expr>, root: &str) {
    let prefix = format!("{}[", root);
    consts.retain(|k, _| !k.starts_with(&prefix));
}

fn is_constant_expr(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Number(_)
            | Expr::SignedNumber(_)
            | Expr::FloatLit(_)
            | Expr::StringLit(_)
            | Expr::Null
    )
}

/* A call can mutate bare globals, and string literals are aggregate storage
 * rather than scalar constants. Neither may be cached across later reads. */
fn expr_contains_call(expr: &Expr) -> bool {
    match expr {
        Expr::Call { .. } => true,
        Expr::Binary { left, right, .. } => {
            expr_contains_call(left) || expr_contains_call(right)
        }
        Expr::Index { expr, index } => expr_contains_call(expr) || expr_contains_call(index),
        Expr::MemberAccess { expr, .. } | Expr::AddrOfExpr(expr) => expr_contains_call(expr),
        Expr::ArrayInit(items) => items.iter().any(expr_contains_call),
        _ => false,
    }
}

fn stmt_contains_call(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::VarDefinition(decl) => decl
            .initial_value
            .as_deref()
            .map_or(false, expr_contains_call),
        Stmt::Assignment { targets, value } => {
            expr_contains_call(value) || targets.iter().any(expr_contains_call)
        }
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            expr_contains_call(cond)
                || then_branch.iter().any(stmt_contains_call)
                || else_branch
                    .as_ref()
                    .map_or(false, |body| body.iter().any(stmt_contains_call))
        }
        Stmt::While { cond, body } => {
            expr_contains_call(cond) || body.iter().any(stmt_contains_call)
        }
        Stmt::For {
            init,
            cond,
            post,
            body,
        } => {
            init.as_deref().map_or(false, stmt_contains_call)
                || expr_contains_call(cond)
                || post.as_deref().map_or(false, stmt_contains_call)
                || body.iter().any(stmt_contains_call)
        }
        Stmt::Jmpto { args, .. } => args.iter().any(stmt_contains_call),
        Stmt::Return(values) => values.iter().any(|(_, expr)| expr_contains_call(expr)),
        Stmt::Critical(body) => body.iter().any(stmt_contains_call),
        Stmt::Match {
            expr,
            cases,
            default,
        } => {
            expr_contains_call(expr)
                || cases.iter().any(|(case, body)| {
                    expr_contains_call(case) || body.iter().any(stmt_contains_call)
                })
                || default
                    .as_ref()
                    .map_or(false, |body| body.iter().any(stmt_contains_call))
        }
        Stmt::Expr(expr) => expr_contains_call(expr),
        _ => false,
    }
}

fn has_side_effects(expr: &Expr) -> bool {
    match expr {
        Expr::Call { .. }
        | Expr::Index { .. }
        | Expr::MemberAccess { .. }
        | Expr::SectionAccess { .. } => true,
        Expr::Binary { left, right, .. } => has_side_effects(left) || has_side_effects(right),
        Expr::AddrOfExpr(inner) => has_side_effects(inner),
        Expr::ArrayInit(elements) => elements.iter().any(has_side_effects),
        _ => false,
    }
}

fn is_power_of_two(val: u64) -> bool {
    val > 0 && (val & (val - 1)) == 0
}

fn is_same_expr(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Number(n1), Expr::Number(n2)) => n1 == n2,
        (Expr::SignedNumber(n1), Expr::SignedNumber(n2)) => n1 == n2,
        (Expr::FloatLit(s1), Expr::FloatLit(s2)) => s1 == s2,
        (Expr::StringLit(s1), Expr::StringLit(s2)) => s1 == s2,
        (Expr::Variable(v1), Expr::Variable(v2)) => v1 == v2,
        (Expr::AddrOf(v1), Expr::AddrOf(v2)) => v1 == v2,
        (
            Expr::SectionAccess {
                section: s1,
                variable: v1,
            },
            Expr::SectionAccess {
                section: s2,
                variable: v2,
            },
        ) => s1 == s2 && v1 == v2,
        (
            Expr::MemberAccess {
                expr: e1,
                member: m1,
                is_arrow: a1,
            },
            Expr::MemberAccess {
                expr: e2,
                member: m2,
                is_arrow: a2,
            },
        ) => is_same_expr(e1, e2) && m1 == m2 && a1 == a2,
        (
            Expr::Index {
                expr: e1,
                index: i1,
            },
            Expr::Index {
                expr: e2,
                index: i2,
            },
        ) => is_same_expr(e1, e2) && is_same_expr(i1, i2),
        (
            Expr::Binary {
                left: l1,
                op: op1,
                right: r1,
            },
            Expr::Binary {
                left: l2,
                op: op2,
                right: r2,
            },
        ) => op1 == op2 && is_same_expr(l1, l2) && is_same_expr(r1, r2),
        (
            Expr::Call {
                name: n1,
                args: args1,
            },
            Expr::Call {
                name: n2,
                args: args2,
            },
        ) => {
            if n1 != n2 || args1.len() != args2.len() {
                return false;
            }
            args1
                .iter()
                .zip(args2.iter())
                .all(|(x, y)| is_same_expr(x, y))
        }
        (Expr::AddrOfExpr(a), Expr::AddrOfExpr(b)) => is_same_expr(a, b),
        _ => false,
    }
}

fn collect_commutative_chain(expr: &Expr, target_op: &str, operands: &mut Vec<Expr>) {
    if let Expr::Binary { left, op, right } = expr {
        if op == target_op {
            collect_commutative_chain(left, target_op, operands);
            collect_commutative_chain(right, target_op, operands);
            return;
        }
    }
    operands.push(expr.clone());
}

fn fold_commutative_chain(expr: &mut Expr) -> usize {
    if let Expr::Binary { op, .. } = expr {
        let op_str = op.clone();
        if op_str == "OpAdd"
            || op_str == "OpMul"
            || op_str == "OpBitAnd"
            || op_str == "OpBitOr"
            || op_str == "OpBitXor"
        {
            let mut operands = Vec::new();
            collect_commutative_chain(expr, &op_str, &mut operands);

            let mut constants = Vec::new();
            let mut non_constants = Vec::new();

            for op_expr in operands {
                if let Expr::Number(n) = op_expr {
                    constants.push(n);
                } else {
                    non_constants.push(op_expr);
                }
            }

            if constants.len() > 1 {
                let mut folded = constants[0];
                for &val in &constants[1..] {
                    folded = match op_str.as_str() {
                        "OpAdd" => folded.wrapping_add(val),
                        "OpMul" => folded.wrapping_mul(val),
                        "OpBitAnd" => folded & val,
                        "OpBitOr" => folded | val,
                        "OpBitXor" => folded ^ val,
                        _ => folded,
                    };
                }

                if non_constants.is_empty() {
                    *expr = Expr::Number(folded);
                } else {
                    let mut current = non_constants[0].clone();
                    for next_expr in &non_constants[1..] {
                        current = Expr::Binary {
                            left: Box::new(current),
                            op: op_str.clone(),
                            right: Box::new(next_expr.clone()),
                        };
                    }
                    *expr = Expr::Binary {
                        left: Box::new(current),
                        op: op_str.clone(),
                        right: Box::new(Expr::Number(folded)),
                    };
                }
                return 1;
            }
        }
    }
    0
}

fn optimize_expr_recursive(
    expr: &mut Expr,
    consts: &HashMap<String, Expr>,
    escaped: &HashSet<String>,
    env: &OptimizeEnv,
) -> usize {
    let mut count = 0;

    match expr {
        Expr::Binary { left, right, .. } => {
            /* Preserve the variable in a null comparison. Substituting a
             * cached null here removes the information the safety analyzer
             * needs to refine the checked branch. */
            let is_null_comparison =
                matches!(&**left, Expr::Null) || matches!(&**right, Expr::Null);
            if !is_null_comparison {
                count += optimize_expr_recursive(left, consts, escaped, env);
                count += optimize_expr_recursive(right, consts, escaped, env);
            }
        }
        Expr::MemberAccess { expr: base, .. } => {
            /* Keep a cached null/scalar value out of the pointer base. The
             * safety analyzer needs the original variable so an explicit null
             * check can refine it and an unchecked dereference can be
             * diagnosed. */
            let cached_constant_base = get_expr_path(base)
                .and_then(|path| consts.get(&path))
                .map(|value| is_constant_expr(value))
                .unwrap_or(false);
            if !cached_constant_base {
                count += optimize_expr_recursive(base, consts, escaped, env);
            }
        }
        Expr::Index { expr: base, index } => {
            count += optimize_expr_recursive(base, consts, escaped, env);
            count += optimize_expr_recursive(index, consts, escaped, env);
        }
        Expr::Call { args, .. } => {
            for arg in args {
                count += optimize_expr_recursive(arg, consts, escaped, env);
            }
        }
        Expr::AddrOfExpr(inner) => {
            count += optimize_expr_recursive(inner, consts, escaped, env);
        }
        _ => {}
    }

    if let Some(path) = get_expr_path(expr) {
        let root = expr_path_root(&path);

        if !escaped.contains(root) && !path.contains(':') {
            if let Some(const_val) = consts.get(&path) {
                *expr = const_val.clone();
                return count + 1;
            }
        }
    }

    count += fold_commutative_chain(expr);

    if let Expr::Binary { left, op, right } = expr {
        let op_str = op.clone();
        let a_opt = match &**left {
            Expr::SignedNumber(n) => Some(*n),
            Expr::Number(n) => Some(*n as i64),
            _ => None,
        };
        let b_opt = match &**right {
            Expr::SignedNumber(n) => Some(*n),
            Expr::Number(n) => Some(*n as i64),
            _ => None,
        };
        let is_left_signed = matches!(&**left, Expr::SignedNumber(_));
        let is_right_signed = matches!(&**right, Expr::SignedNumber(_));

        if (a_opt.is_some() && b_opt.is_some()) && (is_left_signed || is_right_signed) {
            let val_a = a_opt.unwrap();
            let val_b = b_opt.unwrap();
            let folded = match op_str.as_str() {
                "OpAdd" => Some(val_a.wrapping_add(val_b)),
                "OpSub" => Some(val_a.wrapping_sub(val_b)),
                "OpMul" => Some(val_a.wrapping_mul(val_b)),
                /* checked_div/checked_mod also reject i64::MIN / -1, which
                 * overflows in Rust (and traps on x86 idiv). */
                "OpDiv" => val_a.checked_div(val_b),
                "OpMod" => val_a.checked_rem(val_b),
                "OpLt" | "Lt" | "<" => Some(if val_a < val_b { 1 } else { 0 }),
                "OpLtEq" | "OpLe" | "<=" => Some(if val_a <= val_b { 1 } else { 0 }),
                "OpGt" | "Gt" | ">" => Some(if val_a > val_b { 1 } else { 0 }),
                "OpGtEq" | "OpGe" | ">=" => Some(if val_a >= val_b { 1 } else { 0 }),
                "OpEq" | "OpEqEq" | "==" => Some(if val_a == val_b { 1 } else { 0 }),
                "OpNotEq" | "OpNe" | "!=" => Some(if val_a != val_b { 1 } else { 0 }),
                _ => None,
            };
            if let Some(f_val) = folded {
                *expr = Expr::SignedNumber(f_val);
                return count + 1;
            }
        }
    }

    if let Expr::Binary { left, op, right } = expr {
        let op_str = op.clone();
        if let (Expr::Number(a), Expr::Number(b)) = (&**left, &**right) {
            let val_a = *a;
            let val_b = *b;
            let folded = match op_str.as_str() {
                "OpAdd" => Some(val_a.wrapping_add(val_b)),
                "OpSub" => Some(val_a.wrapping_sub(val_b)),
                "OpMul" => Some(val_a.wrapping_mul(val_b)),
                /* checked_div/checked_mod also reject i64::MIN / -1, which
                 * overflows in Rust (and traps on x86 idiv). */
                "OpDiv" => val_a.checked_div(val_b),
                "OpMod" => val_a.checked_rem(val_b),
                "OpBitAnd" => Some(val_a & val_b),
                "OpBitOr" => Some(val_a | val_b),
                "OpBitXor" => Some(val_a ^ val_b),
                "OpShl" => Some(val_a.checked_shl(val_b as u32).unwrap_or(0)),
                "OpShr" => Some(val_a.checked_shr(val_b as u32).unwrap_or(0)),
                "OpEq" | "OpEqEq" | "==" => Some(if val_a == val_b { 1 } else { 0 }),
                "OpNotEq" | "OpNe" | "!=" => Some(if val_a != val_b { 1 } else { 0 }),
                "OpLt" | "Lt" | "<" => Some(if val_a < val_b { 1 } else { 0 }),
                "OpLtEq" | "OpLe" | "<=" => Some(if val_a <= val_b { 1 } else { 0 }),
                "OpGt" | "Gt" | ">" => Some(if val_a > val_b { 1 } else { 0 }),
                "OpGtEq" | "OpGe" | ">=" => Some(if val_a >= val_b { 1 } else { 0 }),
                "OpAnd" | "&&" => Some(if val_a != 0 && val_b != 0 { 1 } else { 0 }),
                "OpOr" | "||" => Some(if val_a != 0 || val_b != 0 { 1 } else { 0 }),
                _ => None,
            };
            if let Some(f_val) = folded {
                *expr = Expr::Number(f_val);
                return count + 1;
            }
        }

        let is_left_zero = matches!(&**left, Expr::Number(0));
        let is_right_zero = matches!(&**right, Expr::Number(0));
        let is_left_one = matches!(&**left, Expr::Number(1));
        let is_right_one = matches!(&**right, Expr::Number(1));

        if op_str == "OpAdd" {
            if is_left_zero {
                *expr = *right.clone();
                return count + 1;
            }
            if is_right_zero {
                *expr = *left.clone();
                return count + 1;
            }
        }
        if op_str == "OpSub" {
            if is_right_zero {
                *expr = *left.clone();
                return count + 1;
            }
            /* `x - x` is NaN for IEEE-754 floats. Fold only values whose
             * integer sign is known here; f64 expressions report Unknown. */
            if expr_sign(left, env) != ValueSign::Unknown &&
                is_pure_expr(left, env) && is_same_expr(left, right) {
                *expr = Expr::Number(0);
                return count + 1;
            }
        }
        if op_str == "OpMul" {
            if is_left_one {
                *expr = *right.clone();
                return count + 1;
            }
            if is_right_one {
                *expr = *left.clone();
                return count + 1;
            }
            /* IEEE-754 `inf * 0` is NaN, so only integer operands with a known
             * sign may be folded to zero. */
            if is_left_zero && expr_sign(right, env) != ValueSign::Unknown &&
                is_pure_expr(right, env) {
                *expr = Expr::Number(0);
                return count + 1;
            }
            if is_right_zero && expr_sign(left, env) != ValueSign::Unknown &&
                is_pure_expr(left, env) {
                *expr = Expr::Number(0);
                return count + 1;
            }

            /* Shifting is integer-only. A float literal such as `3.0` has an
             * Unknown sign here, so it will not be mistaken for a power of
             * two times an integer. */
            if let Expr::Number(val) = &**right {
                if *val > 1 && is_power_of_two(*val) &&
                    expr_sign(left, env) != ValueSign::Unknown {
                    let shift = val.trailing_zeros() as u64;
                    *expr = Expr::Binary {
                        left: left.clone(),
                        op: "OpShl".to_string(),
                        right: Box::new(Expr::Number(shift)),
                    };
                    return count + 1;
                }
            }
            if let Expr::Number(val) = &**left {
                if *val > 1 && is_power_of_two(*val) &&
                    expr_sign(right, env) != ValueSign::Unknown {
                    let shift = val.trailing_zeros() as u64;
                    *expr = Expr::Binary {
                        left: right.clone(),
                        op: "OpShl".to_string(),
                        right: Box::new(Expr::Number(shift)),
                    };
                    return count + 1;
                }
            }
        }
        if op_str == "OpDiv" {
            if is_right_one {
                *expr = *left.clone();
                return count + 1;
            }

            if let Expr::Number(val) = &**right {
                if *val > 1 && is_power_of_two(*val) && expr_sign(left, env) == ValueSign::Unsigned
                {
                    let shift = val.trailing_zeros() as u64;
                    *expr = Expr::Binary {
                        left: left.clone(),
                        op: "OpShr".to_string(),
                        right: Box::new(Expr::Number(shift)),
                    };
                    return count + 1;
                }
            }
        }
        if op_str == "OpMod" {
            if let Expr::Number(val) = &**right {
                if *val > 1 && is_power_of_two(*val) && expr_sign(left, env) == ValueSign::Unsigned
                {
                    *expr = Expr::Binary {
                        left: left.clone(),
                        op: "OpBitAnd".to_string(),
                        right: Box::new(Expr::Number(val - 1)),
                    };
                    return count + 1;
                }
            }
        }
        if op_str == "OpBitAnd" {
            if is_left_zero && is_pure_expr(right, env) {
                *expr = Expr::Number(0);
                return count + 1;
            }
            if is_right_zero && is_pure_expr(left, env) {
                *expr = Expr::Number(0);
                return count + 1;
            }
            if let Expr::Number(val) = &**right {
                if *val == u64::MAX {
                    *expr = *left.clone();
                    return count + 1;
                }
            }
            if is_pure_expr(left, env) && is_same_expr(left, right) {
                *expr = *left.clone();
                return count + 1;
            }
        }
        if op_str == "OpBitOr" {
            if is_left_zero {
                *expr = *right.clone();
                return count + 1;
            }
            if is_right_zero {
                *expr = *left.clone();
                return count + 1;
            }
            if is_pure_expr(left, env) && is_same_expr(left, right) {
                *expr = *left.clone();
                return count + 1;
            }
        }
        if op_str == "OpBitXor" {
            if is_left_zero {
                *expr = *right.clone();
                return count + 1;
            }
            if is_right_zero {
                *expr = *left.clone();
                return count + 1;
            }
            if is_pure_expr(left, env) && is_same_expr(left, right) {
                *expr = Expr::Number(0);
                return count + 1;
            }
        }
        if op_str == "OpShl" || op_str == "OpShr" {
            if is_right_zero {
                *expr = *left.clone();
                return count + 1;
            }

            let folded_shift = if let Expr::Number(outer) = &**right {
                if let Expr::Binary {
                    left: inner_left,
                    op: inner_op,
                    right: inner_right,
                } = &**left
                {
                    if inner_op == &op_str {
                        if let Expr::Number(inner) = &**inner_right {
                            /* x86 masks each shift count independently:
                             * (x << 63) << 1 is zero, not x << 0. Only
                             * fold when neither count wraps and their sum
                             * stays below the machine width. */
                            if *inner < 64 && *outer < 64 && inner + *outer < 64 {
                                Some((inner_left.clone(), inner + *outer))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            if let Some((inner_left, total)) = folded_shift {
                *expr = Expr::Binary {
                    left: inner_left,
                    op: op_str.clone(),
                    right: Box::new(Expr::Number(total)),
                };
                return count + 1;
            }
        }
        if op_str == "OpAnd" || op_str == "&&" {
            if is_left_zero && is_pure_expr(right, env) {
                *expr = Expr::Number(0);
                return count + 1;
            }
            if is_left_one {
                *expr = *right.clone();
                return count + 1;
            }
            if is_right_one {
                *expr = *left.clone();
                return count + 1;
            }
            if is_right_zero && is_pure_expr(left, env) {
                *expr = Expr::Number(0);
                return count + 1;
            }
        }
        if op_str == "OpOr" || op_str == "||" {
            if is_left_one && is_pure_expr(right, env) {
                *expr = Expr::Number(1);
                return count + 1;
            }
            if is_left_zero {
                *expr = *right.clone();
                return count + 1;
            }
            if is_right_zero {
                *expr = *left.clone();
                return count + 1;
            }
            if is_right_one && is_pure_expr(left, env) {
                *expr = Expr::Number(1);
                return count + 1;
            }
        }

        if is_pure_expr(left, env) && is_same_expr(left, right) {
            match op_str.as_str() {
                "OpEq" | "OpEqEq" | "==" | "OpLtEq" | "OpLe" | "<=" | "OpGtEq" | "OpGe" | ">=" => {
                    *expr = Expr::Number(1);
                    return count + 1;
                }
                "OpNotEq" | "OpNe" | "!=" | "OpLt" | "Lt" | "<" | "OpGt" | "Gt" | ">" => {
                    *expr = Expr::Number(0);
                    return count + 1;
                }
                _ => {}
            }
        }
    }

    if let Expr::Binary { left, op, .. } = expr {
        if op == "OpBitNot" {
            if let Expr::Binary {
                left: inner_left,
                op: inner_op,
                ..
            } = &**left
            {
                if inner_op == "OpBitNot" {
                    *expr = *inner_left.clone();
                    return count + 1;
                }
            }
        }
    }

    count
}

fn collect_assigned(body: &[Stmt], assigned: &mut HashSet<String>) {
    for stmt in body {
        collect_assigned_stmt(stmt, assigned);
    }
}

fn collect_assigned_stmt(stmt: &Stmt, assigned: &mut HashSet<String>) {
    match stmt {
        Stmt::VarDefinition(decl) => {
            assigned.insert(decl.name.clone());
            if let Some(ref init) = decl.initial_value {
                collect_assigned_expr(init, assigned);
            }
        }
        Stmt::Assignment { targets, value } => {
            for target in targets {
                if let Some(path) = get_expr_path(target) {
                    assigned.insert(path.clone());
                    assigned.insert(expr_path_root(&path).to_string());
                }
                /* A write through a computed index can alias any element,
                 * so treat the whole array as assigned. */
                if let Expr::Index { expr: base, .. } = target {
                    if let Some(root) = expr_root_name(base) {
                        assigned.insert(root.to_string());
                    }
                }
                collect_assigned_expr(target, assigned);
            }
            collect_assigned_expr(value, assigned);
        }
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_assigned_expr(cond, assigned);
            collect_assigned(then_branch, assigned);
            if let Some(else_stmts) = else_branch {
                collect_assigned(else_stmts, assigned);
            }
        }
        Stmt::While { cond, body } => {
            collect_assigned_expr(cond, assigned);
            collect_assigned(body, assigned);
        }
        Stmt::For {
            init,
            cond,
            post,
            body,
        } => {
            if let Some(ref i) = init {
                collect_assigned_stmt(i, assigned);
            }
            collect_assigned_expr(cond, assigned);
            if let Some(ref p) = post {
                collect_assigned_stmt(p, assigned);
            }
            collect_assigned(body, assigned);
        }
        Stmt::Return(values) => {
            for (_, expr) in values {
                collect_assigned_expr(expr, assigned);
            }
        }
        Stmt::Jmpto { args, .. } => {
            for arg in args {
                collect_assigned_stmt(arg, assigned);
            }
        }
        Stmt::Expr(expr) => {
            collect_assigned_expr(expr, assigned);
        }
        Stmt::Match {
            expr,
            cases,
            default,
        } => {
            collect_assigned_expr(expr, assigned);
            for (ce, body) in cases {
                collect_assigned_expr(ce, assigned);
                collect_assigned(body, assigned);
            }
            if let Some(d) = default {
                collect_assigned(d, assigned);
            }
        }
        Stmt::Critical(body) => {
            collect_assigned(body, assigned);
        }
        _ => {}
    }
}

fn collect_assigned_expr(expr: &Expr, assigned: &mut HashSet<String>) {
    match expr {
        Expr::Binary { left, right, .. } => {
            collect_assigned_expr(left, assigned);
            collect_assigned_expr(right, assigned);
        }
        Expr::Call { args, .. } => {
            for arg in args {
                collect_assigned_expr(arg, assigned);
            }
        }
        Expr::Index { expr: base, index } => {
            collect_assigned_expr(base, assigned);
            collect_assigned_expr(index, assigned);
        }
        Expr::MemberAccess { expr: base, .. } => {
            collect_assigned_expr(base, assigned);
        }
        Expr::ArrayInit(items) => {
            for item in items {
                collect_assigned_expr(item, assigned);
            }
        }
        _ => {}
    }
}

fn optimize_lhs_expr_recursive(
    expr: &mut Expr,
    consts: &HashMap<String, Expr>,
    escaped: &HashSet<String>,
    env: &OptimizeEnv,
) -> usize {
    let mut count = 0;
    match expr {
        Expr::Variable(_) => {}
        Expr::MemberAccess { expr: base, .. } => {
            count += optimize_expr_recursive(base, consts, escaped, env);
        }
        Expr::Index { expr: base, index } => {
            count += optimize_expr_recursive(base, consts, escaped, env);
            count += optimize_expr_recursive(index, consts, escaped, env);
        }
        _ => {
            count += optimize_expr_recursive(expr, consts, escaped, env);
        }
    }
    count
}

fn is_pointer_like(ty: &DataType) -> bool {
    match ty {
        DataType::Pointer(_) | DataType::Array(_, _) => true,
        DataType::Typedef(_, inner) => is_pointer_like(inner),
        _ => false,
    }
}

fn optimize_statements(
    stmts: Vec<Stmt>,
    consts: &mut HashMap<String, Expr>,
    escaped: &HashSet<String>,
    reads: &HashMap<String, usize>,
    output_ptrs: &mut HashSet<String>,
    count: &mut usize,
    env: &OptimizeEnv,
) -> Vec<Stmt> {
    let mut new_stmts = Vec::new();
    for stmt in stmts {
        /* A callee can write a bare global, so no fact from before the call
         * may be used after it. This is conservative for locals, which a call
         * normally cannot reach, but keeps shared state correct. */
        if stmt_contains_call(&stmt) {
            consts.clear();
        }
        match stmt {
            Stmt::VarDefinition(mut decl) => {
                if decl.modifier == PtrAccess::Output
                    || decl.modifier == PtrAccess::InputOutput
                    || decl.modifier == PtrAccess::Volatile
                    || decl.modifier == PtrAccess::Atomic
                {
                    output_ptrs.insert(decl.name.clone());
                }
                if let Some(ref mut init) = decl.initial_value {
                    *count += optimize_expr_recursive(init, consts, escaped, env);
                }
                if !escaped.contains(&decl.name)
                    && !output_ptrs.contains(&decl.name)
                    && reads.get(&decl.name).cloned().unwrap_or(0) == 0
                {
                    *count += 1;
                    if let Some(init) = decl.initial_value {
                        if has_side_effects(&init) {
                            new_stmts.push(Stmt::Expr(*init));
                        }
                    }
                    continue;
                }

                if let Some(ref init) = decl.initial_value {
                    let is_mmio =
                        decl.modifier == PtrAccess::Volatile || decl.modifier == PtrAccess::Atomic;
                    let propagatable = !escaped.contains(&decl.name)
                        && !output_ptrs.contains(&decl.name)
                        && !is_mmio;
                    /* A pointer local initialized from a differently typed
                     * expression (for example `u8* p = ptr;` where ptr is a
                     * u64) carries element-type information that the source
                     * expression does not. Propagating it would turn p[i]
                     * into ptr[i], making the backend scale by 8 and load a
                     * full qword instead of one byte. */
                    if propagatable && !is_pointer_like(&decl.data_type)
                        && !env.array_roots.contains(&decl.name)
                        && !matches!(&**init, Expr::StringLit(_))
                    {
                        if is_constant_expr(init) {
                            consts.insert(decl.name.clone(), *init.clone());
                        } else if env.single_assignment.contains(&decl.name) {
                            if let Expr::Variable(source) = &**init {
                                if env.can_copy_from(source) && !output_ptrs.contains(source) {
                                    consts.insert(decl.name.clone(), *init.clone());
                                }
                            }
                        }
                    }
                }
                new_stmts.push(Stmt::VarDefinition(decl));
            }
            Stmt::Assignment {
                mut targets,
                mut value,
            } => {
                *count += optimize_expr_recursive(&mut value, consts, escaped, env);
                for target in &mut targets {
                    *count += optimize_lhs_expr_recursive(target, consts, escaped, env);
                }

                if targets.len() == 1 {
                    if let Expr::Variable(name) = &targets[0] {
                        if !escaped.contains(name)
                            && !output_ptrs.contains(name)
                            && reads.get(name).cloned().unwrap_or(0) == 0
                        {
                            *count += 1;
                            if has_side_effects(&value) {
                                new_stmts.push(Stmt::Expr(value));
                            }
                            continue;
                        }
                    }
                }

                if targets.len() == 1 {
                    let target = &targets[0];
                    if let Some(target_path) = get_expr_path(target) {
                        let root = expr_path_root(&target_path);

                        if !escaped.contains(root)
                            && !target_path.contains(':')
                            && !output_ptrs.contains(&target_path)
                        {
                            /* Union fields share storage: writing one field
                             * invalidates every cached field of that union. */
                            if env.union_roots.contains(root) && target_path.contains('.') {
                                let prefix = format!("{}.", root);
                                consts.retain(|k, _| !k.starts_with(&prefix));
                            }
                            if env.array_roots.contains(root) {
                                /* A whole-array assignment is a memcpy, not a
                                 * value binding. Propagating the string/array
                                 * expression would replace the index base with
                                 * the literal and leave the real array dead. */
                                consts.remove(&target_path);
                                invalidate_root_elements(consts, root);
                            } else if is_constant_expr(&value)
                                && !matches!(&value, Expr::StringLit(_))
                            {
                                consts.insert(target_path, value.clone());
                            } else {
                                consts.remove(&target_path);
                                let prefix = format!("{}.", target_path);
                                let sub_keys: Vec<String> = consts
                                    .keys()
                                    .filter(|k| k.starts_with(&prefix))
                                    .cloned()
                                    .collect();
                                for k in sub_keys {
                                    consts.remove(&k);
                                }
                            }
                        }
                    } else if let Expr::Index { expr: base, .. } = target {
                        /* Computed index: the write can hit any element. */
                        if let Some(root) = expr_root_name(base) {
                            if !escaped.contains(root) {
                                invalidate_root_elements(consts, root);
                                consts.remove(root);
                            }
                        }
                    }
                } else {
                    for target in &targets {
                        if let Some(target_path) = get_expr_path(target) {
                            consts.remove(&target_path);
                            let root = target_path
                                .split(|c| c == '.' || c == '[')
                                .next()
                                .unwrap_or(&target_path);
                            if env.union_roots.contains(root) {
                                let prefix = format!("{}.", root);
                                consts.retain(|k, _| !k.starts_with(&prefix));
                            }
                        } else if let Expr::Index { expr: base, .. } = target {
                            if let Some(root) = expr_root_name(base) {
                                invalidate_root_elements(consts, root);
                                consts.remove(root);
                            }
                        }
                    }
                }

                new_stmts.push(Stmt::Assignment { targets, value });
            }
            Stmt::If {
                mut cond,
                then_branch,
                else_branch,
            } => {
                *count += optimize_expr_recursive(&mut cond, consts, escaped, env);
                if let Expr::Number(n) = cond {
                    *count += 1;
                    if n != 0 {
                        let opt_then = optimize_statements(
                            then_branch,
                            consts,
                            escaped,
                            reads,
                            output_ptrs,
                            count,
                            env,
                        );
                        new_stmts.extend(opt_then);
                    } else if let Some(else_b) = else_branch {
                        let opt_else = optimize_statements(
                            else_b,
                            consts,
                            escaped,
                            reads,
                            output_ptrs,
                            count,
                            env,
                        );
                        new_stmts.extend(opt_else);
                    }
                } else {
                    let mut then_consts = consts.clone();
                    let mut then_output = output_ptrs.clone();
                    let opt_then = optimize_statements(
                        then_branch,
                        &mut then_consts,
                        escaped,
                        reads,
                        &mut then_output,
                        count,
                        env,
                    );

                    let mut else_consts = consts.clone();
                    let mut else_output = output_ptrs.clone();
                    let opt_else = if let Some(else_b) = else_branch {
                        Some(optimize_statements(
                            else_b,
                            &mut else_consts,
                            escaped,
                            reads,
                            &mut else_output,
                            count,
                            env,
                        ))
                    } else {
                        None
                    };

                    consts.retain(|k, v| {
                        then_consts.get(k) == Some(v) && else_consts.get(k) == Some(v)
                    });

                    output_ptrs.extend(then_output);
                    output_ptrs.extend(else_output);

                    new_stmts.push(Stmt::If {
                        cond,
                        then_branch: opt_then,
                        else_branch: opt_else,
                    });
                }
            }
            Stmt::While { mut cond, body } => {
                let mut loop_assigned = HashSet::new();
                collect_assigned(&body, &mut loop_assigned);
                collect_assigned_expr(&cond, &mut loop_assigned);

                consts.retain(|k, _| !loop_assigned.contains(expr_path_root(k)));

                *count += optimize_expr_recursive(&mut cond, consts, escaped, env);
                if let Expr::Number(0) = cond {
                    *count += 1;
                    continue;
                }

                /* The body may run zero times. Optimize it with a copy of the
                 * entry constants and discard everything it learns, otherwise
                 * a constant assigned only inside the loop would appear to
                 * hold after it. */
                let mut loop_consts = consts.clone();
                let opt_body = optimize_statements(
                    body,
                    &mut loop_consts,
                    escaped,
                    reads,
                    output_ptrs,
                    count,
                    env,
                );
                new_stmts.push(Stmt::While {
                    cond,
                    body: opt_body,
                });
            }
            Stmt::For {
                init,
                mut cond,
                post,
                body,
            } => {
                let mut loop_assigned = HashSet::new();
                collect_assigned(&body, &mut loop_assigned);
                if let Some(ref p) = post {
                    collect_assigned_stmt(p, &mut loop_assigned);
                }
                collect_assigned_expr(&cond, &mut loop_assigned);

                let opt_init = if let Some(i) = init {
                    let init_wrapper = vec![*i];
                    let mut opt_init_vec = optimize_statements(
                        init_wrapper,
                        consts,
                        escaped,
                        reads,
                        output_ptrs,
                        count,
                        env,
                    );
                    if opt_init_vec.is_empty() {
                        None
                    } else {
                        Some(Box::new(opt_init_vec.remove(0)))
                    }
                } else {
                    None
                };

                consts.retain(|k, _| !loop_assigned.contains(expr_path_root(k)));

                *count += optimize_expr_recursive(&mut cond, consts, escaped, env);

                /* `post` runs after the body, while the source order below is
                 * init/cond/post/body. Give each a private copy of the entry
                 * constants so facts learned in one cannot leak into the
                 * other or past the loop. */
                let mut post_consts = consts.clone();
                let opt_post = if let Some(p) = post {
                    let post_wrapper = vec![*p];
                    let mut opt_post_vec = optimize_statements(
                        post_wrapper,
                        &mut post_consts,
                        escaped,
                        reads,
                        output_ptrs,
                        count,
                        env,
                    );
                    if opt_post_vec.is_empty() {
                        None
                    } else {
                        Some(Box::new(opt_post_vec.remove(0)))
                    }
                } else {
                    None
                };

                let mut body_consts = consts.clone();
                let opt_body = optimize_statements(
                    body,
                    &mut body_consts,
                    escaped,
                    reads,
                    output_ptrs,
                    count,
                    env,
                );

                if let Expr::Number(0) = cond {
                    *count += 1;
                    if let Some(init_stmt) = opt_init {
                        new_stmts.push(*init_stmt);
                    }
                } else {
                    new_stmts.push(Stmt::For {
                        init: opt_init,
                        cond,
                        post: opt_post,
                        body: opt_body,
                    });
                }
            }
            Stmt::Return(mut values) => {
                for (_, ref mut expr) in &mut values {
                    *count += optimize_expr_recursive(expr, consts, escaped, env);
                }
                new_stmts.push(Stmt::Return(values));
            }
            Stmt::Jmpto { module_name, args } => {
                /* Result markers must keep their plain `Variable` form: the
                 * backend only records result targets for those. Optimize the
                 * argument statements, but pass markers through untouched. */
                let mut opt_args = Vec::with_capacity(args.len());
                for arg in args {
                    if matches!(arg, Stmt::Return(_)) {
                        opt_args.push(arg);
                    } else {
                        opt_args.extend(optimize_statements(
                            vec![arg],
                            consts,
                            escaped,
                            reads,
                            output_ptrs,
                            count,
                            env,
                        ));
                    }
                }
                /* The worker body is emitted after optimization (or executed
                 * by the loader at runtime), so it can change any local the
                 * caller has cached as a constant. */
                consts.clear();
                new_stmts.push(Stmt::Jmpto {
                    module_name,
                    args: opt_args,
                });
            }
            Stmt::Expr(mut expr) => {
                *count += optimize_expr_recursive(&mut expr, consts, escaped, env);
                new_stmts.push(Stmt::Expr(expr));
            }
            Stmt::Critical(body) => {
                let opt_body =
                    optimize_statements(body, consts, escaped, reads, output_ptrs, count, env);
                new_stmts.push(Stmt::Critical(opt_body));
            }
            Stmt::Match {
                mut expr,
                cases,
                default,
            } => {
                *count += optimize_expr_recursive(&mut expr, consts, escaped, env);

                let mut new_cases = Vec::new();
                let mut branch_assigned = HashSet::new();
                for (case_expr, body) in cases {
                    let mut ce = case_expr;
                    *count += optimize_expr_recursive(&mut ce, consts, escaped, env);
                    collect_assigned(&body, &mut branch_assigned);
                    let mut case_consts = consts.clone();
                    let opt_body = optimize_statements(
                        body,
                        &mut case_consts,
                        escaped,
                        reads,
                        output_ptrs,
                        count,
                        env,
                    );
                    new_cases.push((ce, opt_body));
                }

                let new_default = default.map(|d| {
                    collect_assigned(&d, &mut branch_assigned);
                    let mut default_consts = consts.clone();
                    optimize_statements(
                        d,
                        &mut default_consts,
                        escaped,
                        reads,
                        output_ptrs,
                        count,
                        env,
                    )
                });

                /* Only one case executes. Facts learned while optimizing one
                 * case must not be visible in another or after the match. */
                consts.retain(|k, _| {
                    let root = k
                        .split(|c| c == '.' || c == '[')
                        .next()
                        .unwrap_or(k);
                    !branch_assigned.contains(root)
                });

                new_stmts.push(Stmt::Match {
                    expr,
                    cases: new_cases,
                    default: new_default,
                });
            }
            other => {
                new_stmts.push(other);
            }
        }

        if matches!(
            new_stmts.last(),
            Some(Stmt::Return(_)) | Some(Stmt::Break) | Some(Stmt::Continue)
        ) {
            break;
        }
    }
    new_stmts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_with_sign(name: &str, sign: ValueSign) -> OptimizeEnv {
        let mut signs = HashMap::new();
        signs.insert(name.to_string(), sign);
        OptimizeEnv {
            signs,
            copy_sources: HashSet::new(),
            single_assignment: HashSet::new(),
            volatile: HashSet::new(),
            array_roots: HashSet::new(),
            union_roots: HashSet::new(),
        }
    }

    fn num(value: u64) -> Expr {
        Expr::Number(value)
    }

    fn var(name: &str) -> Expr {
        Expr::Variable(name.to_string())
    }

    fn bin(left: Expr, op: &str, right: Expr) -> Expr {
        Expr::Binary {
            left: Box::new(left),
            op: op.to_string(),
            right: Box::new(right),
        }
    }

    fn fold(expr: &mut Expr, env: &OptimizeEnv) -> usize {
        let consts = HashMap::new();
        let escaped = HashSet::new();
        optimize_expr_recursive(expr, &consts, &escaped, env)
    }

    fn op_of(expr: &Expr) -> &str {
        match expr {
            Expr::Binary { op, .. } => op,
            _ => panic!("expected binary expression"),
        }
    }

    #[test]
    fn unsigned_division_by_power_of_two_uses_shift() {
        let env = env_with_sign("x", ValueSign::Unsigned);
        let mut expr = bin(var("x"), "OpDiv", num(8));
        assert!(fold(&mut expr, &env) > 0);
        assert_eq!(op_of(&expr), "OpShr");
    }

    #[test]
    fn signed_division_by_power_of_two_is_not_shifted() {
        let env = env_with_sign("x", ValueSign::Signed);
        let mut expr = bin(var("x"), "OpDiv", num(8));
        assert_eq!(fold(&mut expr, &env), 0);
        assert_eq!(op_of(&expr), "OpDiv");
    }

    #[test]
    fn unknown_division_by_power_of_two_is_not_shifted() {
        let env = env_with_sign("unused", ValueSign::Unknown);
        let mut expr = bin(var("x"), "OpDiv", num(8));
        assert_eq!(fold(&mut expr, &env), 0);
        assert_eq!(op_of(&expr), "OpDiv");
    }

    #[test]
    fn unsigned_modulo_by_power_of_two_uses_bit_and() {
        let env = env_with_sign("x", ValueSign::Unsigned);
        let mut expr = bin(var("x"), "OpMod", num(8));
        assert!(fold(&mut expr, &env) > 0);
        assert_eq!(op_of(&expr), "OpBitAnd");
        match expr {
            Expr::Binary { right, .. } => assert_eq!(*right, num(7)),
            _ => unreachable!(),
        }
    }

    #[test]
    fn signed_modulo_by_power_of_two_is_not_bit_and() {
        let env = env_with_sign("x", ValueSign::Signed);
        let mut expr = bin(var("x"), "OpMod", num(8));
        assert_eq!(fold(&mut expr, &env), 0);
        assert_eq!(op_of(&expr), "OpMod");
    }

    #[test]
    fn x_divided_by_x_is_not_folded() {
        let env = env_with_sign("x", ValueSign::Unsigned);
        let mut expr = bin(var("x"), "OpDiv", var("x"));
        assert_eq!(fold(&mut expr, &env), 0);
        assert_eq!(op_of(&expr), "OpDiv");
    }

    #[test]
    fn zero_multiply_does_not_drop_call_effects() {
        let env = env_with_sign("unused", ValueSign::Unknown);
        let mut expr = bin(
            num(0),
            "OpMul",
            Expr::Call {
                name: "side_effect".to_string(),
                args: Vec::new(),
            },
        );
        assert_eq!(fold(&mut expr, &env), 0);
        assert_eq!(op_of(&expr), "OpMul");
    }

    #[test]
    fn zero_and_does_not_drop_call_effects() {
        let env = env_with_sign("unused", ValueSign::Unknown);
        let mut expr = bin(
            num(0),
            "OpAnd",
            Expr::Call {
                name: "side_effect".to_string(),
                args: Vec::new(),
            },
        );
        assert_eq!(fold(&mut expr, &env), 0);
        assert_eq!(op_of(&expr), "OpAnd");
    }

    #[test]
    fn nested_shifts_are_combined() {
        let env = env_with_sign("x", ValueSign::Unsigned);
        let mut expr = bin(bin(var("x"), "OpShl", num(2)), "OpShl", num(4));
        assert!(fold(&mut expr, &env) > 0);
        assert_eq!(op_of(&expr), "OpShl");
        match expr {
            Expr::Binary { right, .. } => assert_eq!(*right, num(6)),
            _ => unreachable!(),
        }
    }

    fn empty_program_with(func: FuncDecl) -> Program {
        Program {
            use_os: true,
            imports: Vec::new(),
            typedefs: Vec::new(),
            structs: Vec::new(),
            enums: Vec::new(),
            constants: Vec::new(),
            sections: Vec::new(),
            functions: vec![func],
        }
    }

    fn test_func(body: Vec<Stmt>) -> FuncDecl {
        FuncDecl {
            name: "main".to_string(),
            version: 1,
            params: vec![(DataType::U64, "x".to_string(), PtrAccess::Input)],
            return_types: vec![DataType::U64],
            body: Some(body),
            is_extern: false,
            is_export: false,
            is_irq: false,
        }
    }

    #[test]
    fn shift_counts_do_not_wrap() {
        let env = env_with_sign("x", ValueSign::Unsigned);
        let mut expr = bin(bin(var("x"), "OpShl", num(u64::MAX)), "OpShl", num(1));
        /* x86 masks each shift count to 63 bits independently: (x << 63) << 1
         * is not x << 0. The nested form must survive folding. */
        assert_eq!(fold(&mut expr, &env), 0);
        assert_eq!(op_of(&expr), "OpShl");
        match expr {
            Expr::Binary { left, right, .. } => {
                assert_eq!(*right, num(1));
                assert_eq!(op_of(&left), "OpShl");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn nested_shifts_below_width_are_folded() {
        let env = env_with_sign("x", ValueSign::Unsigned);
        let mut expr = bin(bin(var("x"), "OpShl", num(3)), "OpShl", num(4));
        assert!(fold(&mut expr, &env) > 0);
        match expr {
            Expr::Binary { right, .. } => assert_eq!(*right, num(7)),
            _ => unreachable!(),
        }
    }

    #[test]
    fn signed_min_div_minus_one_is_not_folded() {
        let env = env_with_sign("x", ValueSign::Signed);
        let mut expr = bin(
            Expr::SignedNumber(i64::MIN),
            "OpDiv",
            Expr::SignedNumber(-1),
        );
        assert_eq!(fold(&mut expr, &env), 0);
        assert_eq!(op_of(&expr), "OpDiv");
    }

    #[test]
    fn volatile_self_subtraction_is_not_folded() {
        let mut env = env_with_sign("v", ValueSign::Unsigned);
        env.volatile.insert("v".to_string());
        let mut expr = bin(var("v"), "OpSub", var("v"));
        assert_eq!(fold(&mut expr, &env), 0);
        assert_eq!(op_of(&expr), "OpSub");
    }

    #[test]
    fn volatile_operand_is_not_discarded() {
        let mut env = env_with_sign("v", ValueSign::Unsigned);
        env.volatile.insert("v".to_string());
        let mut expr = bin(num(0), "OpMul", var("v"));
        assert_eq!(fold(&mut expr, &env), 0);
        assert_eq!(op_of(&expr), "OpMul");
    }

    #[test]
    fn immutable_copy_is_propagated() {
        let body = vec![
            Stmt::VarDefinition(VarDecl {
                name: "y".to_string(),
                data_type: DataType::U64,
                modifier: PtrAccess::Normal,
                initial_value: Some(Box::new(var("x"))),
                alignment: 8,
            }),
            Stmt::Return(vec![(DataType::U64, var("y"))]),
        ];
        let mut program = empty_program_with(test_func(body));
        assert!(Optimizer::optimize_program(&mut program) > 0);

        let body = program.functions[0].body.as_ref().unwrap();
        assert_eq!(body.len(), 1);
        match &body[0] {
            Stmt::Return(values) => assert_eq!(values[0].1, var("x")),
            _ => panic!("expected return statement"),
        }
    }

    #[test]
    fn unreachable_statements_are_removed() {
        let body = vec![
            Stmt::Return(vec![(DataType::U64, var("x"))]),
            Stmt::Expr(Expr::Call {
                name: "side_effect".to_string(),
                args: Vec::new(),
            }),
        ];
        let mut program = empty_program_with(test_func(body));
        Optimizer::optimize_program(&mut program);
        assert_eq!(program.functions[0].body.as_ref().unwrap().len(), 1);
    }
}
