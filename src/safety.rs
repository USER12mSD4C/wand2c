#![allow(dead_code)]

use crate::ast::*;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
enum VarState {
    Uninitialized {
        declared_line: usize,
        type_name: String,
    },
    Safe,
    Allocated {
        allocated_line: usize,
        checked_not_null: bool,
        type_name: String,
    },
    Freed {
        freed_line: usize,
    },
}

pub struct MemorySafetyAnalyzer {
    states: HashMap<String, VarState>,
    errors: Vec<String>,
    source_lines: Vec<String>,
    base_line: usize,
}

impl MemorySafetyAnalyzer {
pub fn new() -> Self {
    Self {
        states: HashMap::new(),
        errors: Vec::new(),
        source_lines: Vec::new(),
        base_line: 0,
    }
}

    fn format_data_type(dt: &DataType) -> String {
        match dt {
            DataType::U8 => "u8".to_string(),
            DataType::U16 => "u16".to_string(),
            DataType::U32 => "u32".to_string(),
            DataType::U64 => "u64".to_string(),
            DataType::I8 => "i8".to_string(),
            DataType::I16 => "i16".to_string(),
            DataType::I32 => "i32".to_string(),
            DataType::I64 => "i64".to_string(),
            DataType::F64 => "f64".to_string(),
            DataType::Void => "void".to_string(),
            DataType::Pointer(inner) => format!("{}*", Self::format_data_type(inner)),
            DataType::Struct(name) => name.clone(),
            DataType::Array(inner, size) => format!("{}[{}]", Self::format_data_type(inner), size),
            DataType::Typedef(name, _) => name.clone(),
        }
    }

    fn get_expr_path(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Variable(name) => Some(name.clone()),
            Expr::MemberAccess {
                expr: base, member, ..
            } => {
                let base_path = self.get_expr_path(base)?;
                Some(format!("{}.{}", base_path, member))
            }
            Expr::Index { expr: base, .. } => {
                self.get_expr_path(base)
            }
            _ => None,
        }
    }

    fn push_error(&mut self, severity: &str, message: &str, relative_line: usize, note: &str) {
        let abs_line = if self.base_line > 0 {
            self.base_line + relative_line
        } else {
            relative_line
        };
        let mut err = String::new();
        if severity == "error" {
            err.push_str(&format!("\x1b[31;1merror\x1b[0m: {}\n", message));
        } else {
            err.push_str(&format!("\x1b[33;1mwarning\x1b[0m: {}\n", message));
        }
        err.push_str(&format!("  \x1b[34;1m-->\x1b[0m line {}\n", abs_line));
        if !self.source_lines.is_empty() && abs_line > 0 && abs_line <= self.source_lines.len() {
            let start = if abs_line > 2 { abs_line - 2 } else { 1 };
            let end = if abs_line + 1 <= self.source_lines.len() {
                abs_line + 1
            } else {
                self.source_lines.len()
            };
            err.push_str("   |\n");
            for i in start..=end {
                let marker = if i == abs_line { ">" } else { " " };
                err.push_str(&format!(
                    "{} {:3} | {}\n",
                    marker,
                    i,
                    self.source_lines[i - 1]
                ));
            }
            err.push_str("   |\n");
        }
        err.push_str(&format!("  \x1b[32;1mnote\x1b[0m: {}\n", note));
        self.errors.push(err);
    }

    fn is_allocation_call(&self, expr: &Expr) -> bool {
        if let Expr::Call { name, .. } = expr {
            name == "mloc" || name == "bmloc"
        } else {
            false
        }
    }

    pub fn analyze_function(
        &mut self,
        func: &FuncDecl,
        structs: &HashMap<String, StructDecl>,
        source: Option<&str>,
        base_line: usize,
    ) -> Result<(), Vec<String>> {
        self.states.clear();
        self.errors.clear();
        self.source_lines = source
            .map(|s| s.lines().map(|l| l.to_string()).collect())
            .unwrap_or_default();
        self.base_line = base_line;

        for (dt, name, _) in &func.params {
            self.states.insert(name.clone(), VarState::Safe);
            self.register_struct_fields(name, dt, structs, VarState::Safe);
        }

        if let Some(body) = &func.body {
            self.analyze_statements(body, structs);
        }

        let states_snapshot = self.states.clone();
        for (path, state) in &states_snapshot {
            if let VarState::Allocated { allocated_line, .. } = state {
                if !path.contains('.') {
                    self.push_error(
                        "warning",
                        &format!(
                            "potential memory leak in function '{}': pointer '{}' was never freed via 'mfree()'",
                            func.name, path
                        ),
                        *allocated_line,
                        "call mfree(ptr) before the function returns, or document that ownership is transferred",
                    );
                }
            }
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }

    fn register_struct_fields(
        &mut self,
        prefix: &str,
        dt: &DataType,
        structs: &HashMap<String, StructDecl>,
        initial_state: VarState,
    ) {
        let struct_name = match dt {
            DataType::Struct(name) => name.clone(),
            DataType::Pointer(inner) => {
                if let DataType::Struct(name) = &**inner {
                    name.clone()
                } else {
                    return;
                }
            }
            _ => return,
        };

        if let Some(s_decl) = structs.get(&struct_name) {
            for field in &s_decl.fields {
                let field_path = format!("{}.{}", prefix, field.name);
                let type_str = Self::format_data_type(&field.data_type);

                let field_state = match &initial_state {
                    VarState::Uninitialized { declared_line, .. } => VarState::Uninitialized {
                        declared_line: *declared_line,
                        type_name: type_str.clone(),
                    },
                    _ => initial_state.clone(),
                };

                self.states.insert(field_path.clone(), field_state.clone());

                self.register_struct_fields(&field_path, &field.data_type, structs, field_state);
            }
        }
    }

    fn check_lhs_safety(&mut self, expr: &Expr, line: usize) {
        match expr {
            Expr::Variable(_) => {
            }
            Expr::MemberAccess { expr: base, .. } => {
                self.check_expression_safety(base, line);
            }
            Expr::Index { expr: base, index } => {
                self.check_expression_safety(base, line);
                self.check_expression_safety(index, line);
            }
            _ => {
                self.check_expression_safety(expr, line);
            }
        }
    }

    fn mark_expr_as_freed(&mut self, expr: &Expr, line: usize) {
        match expr {
            Expr::Variable(name) => {
                self.states
                    .insert(name.clone(), VarState::Freed { freed_line: line });
            }
            Expr::Binary { left, right, .. } => {
                self.mark_expr_as_freed(left, line);
                self.mark_expr_as_freed(right, line);
            }
            Expr::Index { expr: base, .. } => {
                self.mark_expr_as_freed(base, line);
            }
            Expr::MemberAccess { expr: base, .. } => {
                self.mark_expr_as_freed(base, line);
            }
            Expr::Call { args, .. } => {
                for arg in args {
                    self.mark_expr_as_freed(arg, line);
                }
            }
            _ => {}
        }
    }

    fn branch_terminates(stmts: &[Stmt]) -> bool {
        if stmts.is_empty() {
            return false;
        }
        match stmts.last().unwrap() {
            Stmt::Return(_) => true,
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                Self::branch_terminates(then_branch)
                    && else_branch
                        .as_ref()
                        .map_or(false, |e| Self::branch_terminates(e))
            }
            _ => false,
        }
    }

    fn analyze_statements(&mut self, stmts: &[Stmt], structs: &HashMap<String, StructDecl>) {
        for (idx, stmt) in stmts.iter().enumerate() {
            let current_line = idx + 1;

            match stmt {
                Stmt::VarDefinition(decl) => {
                    let type_str = Self::format_data_type(&decl.data_type);

                    if let Some(init_expr) = &decl.initial_value {
                        self.check_expression_safety(init_expr, current_line);

                        if self.is_allocation_call(init_expr) {
                            self.states.insert(
                                decl.name.clone(),
                                VarState::Allocated {
                                    allocated_line: current_line,
                                    checked_not_null: false,
                                    type_name: type_str.clone(),
                                },
                            );
                        } else {
                            self.states.insert(decl.name.clone(), VarState::Safe);
                            self.register_struct_fields(
                                &decl.name,
                                &decl.data_type,
                                structs,
                                VarState::Uninitialized {
                                    declared_line: current_line,
                                    type_name: type_str.clone(),
                                },
                            );
                        }
                    } else {
                        if matches!(decl.data_type, DataType::Pointer(_)) {
                            self.states.insert(
                                decl.name.clone(),
                                VarState::Uninitialized {
                                    declared_line: current_line,
                                    type_name: type_str.clone(),
                                },
                            );
                        } else {
                            self.states.insert(decl.name.clone(), VarState::Safe);
                            self.register_struct_fields(
                                &decl.name,
                                &decl.data_type,
                                structs,
                                VarState::Uninitialized {
                                    declared_line: current_line,
                                    type_name: type_str.clone(),
                                },
                            );
                        }
                    }
                }
                Stmt::Assignment { targets, value } => {
                    self.check_expression_safety(value, current_line);
                    let is_alloc = self.is_allocation_call(value);

                    for target in targets {
                        self.check_lhs_safety(target, current_line);

                        if let Some(path) = self.get_expr_path(target) {
                            if is_alloc {
                                let type_str = "void*".to_string();
                                self.states.insert(
                                    path.clone(),
                                    VarState::Allocated {
                                        allocated_line: current_line,
                                        checked_not_null: false,
                                        type_name: type_str,
                                    },
                                );
                            } else {
                                self.states.insert(path.clone(), VarState::Safe);
                            }
                        }
                    }
                }
                Stmt::If {
                    cond,
                    then_branch,
                    else_branch,
                } => {
                    self.check_expression_safety(cond, current_line);

                    let mut checked_var: Option<String> = None;
                    let mut check_is_not_null = true;

                    if let Expr::Binary { left, op, right } = cond {
                        if let Some(path) = self.get_expr_path(left) {
                            if **right == Expr::Null {
                                checked_var = Some(path);
                                if op == "OpEq" || op == "==" {
                                    check_is_not_null = false;
                                }
                            }
                        }
                    }

                    let mut then_states = self.states.clone();
                    if let Some(ref path) = checked_var {
                        if check_is_not_null {
                            if let Some(VarState::Allocated {
                                allocated_line,
                                type_name,
                                ..
                            }) = then_states.get(path)
                            {
                                then_states.insert(
                                    path.clone(),
                                    VarState::Allocated {
                                        allocated_line: *allocated_line,
                                        checked_not_null: true,
                                        type_name: type_name.clone(),
                                    },
                                );
                            }
                        }
                    }

                    let mut then_analyzer = MemorySafetyAnalyzer {
                        states: then_states.clone(),
                        errors: Vec::new(),
                        source_lines: self.source_lines.clone(),
                        base_line: self.base_line,
                    };
                    then_analyzer.analyze_statements(then_branch, structs);
                    self.errors.extend(then_analyzer.errors);

                    let then_terminates = Self::branch_terminates(then_branch);

                    let mut else_states = self.states.clone();
                    if let Some(else_stmts) = else_branch {
                        if let Some(ref path) = checked_var {
                            if !check_is_not_null {
                                if let Some(VarState::Allocated {
                                    allocated_line,
                                    type_name,
                                    ..
                                }) = else_states.get(path)
                                {
                                    else_states.insert(
                                        path.clone(),
                                        VarState::Allocated {
                                            allocated_line: *allocated_line,
                                            checked_not_null: true,
                                            type_name: type_name.clone(),
                                        },
                                    );
                                }
                            }
                        }

                        let mut else_analyzer = MemorySafetyAnalyzer {
                            states: else_states.clone(),
                            errors: Vec::new(),
                            source_lines: self.source_lines.clone(),
                            base_line: self.base_line,
                        };
                        else_analyzer.analyze_statements(else_stmts, structs);
                        self.errors.extend(else_analyzer.errors);

                        let else_terminates = Self::branch_terminates(else_stmts);

                        if then_terminates && else_terminates {
                        } else if then_terminates {
                            self.states = else_analyzer.states;
                        } else if else_terminates {
                            self.states = then_analyzer.states;
                        } else {
                            self.merge_states(then_analyzer.states, else_analyzer.states);
                        }
                    } else {
                        if !then_terminates {
                            self.states = then_analyzer.states;
                        }
                    }
                }
                Stmt::While { cond, body } => {
                    self.check_expression_safety(cond, current_line);
                    self.analyze_statements(body, structs);
                }
                Stmt::For {
                    init,
                    cond,
                    post,
                    body,
                } => {
                    if let Some(i) = init {
                        self.analyze_statements(&[*i.clone()], structs);
                    }
                    self.check_expression_safety(cond, current_line);
                    if let Some(p) = post {
                        self.analyze_statements(&[*p.clone()], structs);
                    }
                    self.analyze_statements(body, structs);
                }
                Stmt::Expr(expr) => {
                    self.check_expression_safety(expr, current_line);

                    if let Expr::Call { name, args } = expr {
                        if name == "mfree" {
                            if let Some(arg) = args.first() {
                                if let Some(path) = self.get_expr_path(arg) {
                                    self.states.insert(
                                        path.clone(),
                                        VarState::Freed {
                                            freed_line: current_line,
                                        },
                                    );
                                } else {
                                    self.mark_expr_as_freed(arg, current_line);
                                }
                            }
                        }
                    }
                }
                Stmt::Return(values) => {
                    for (_, expr) in values {
                        self.check_expression_safety(expr, current_line);
                        if let Some(path) = self.get_expr_path(expr) {
                            self.states.remove(&path);
                        }
                    }
                }
                Stmt::Critical(body) => {
                    self.analyze_statements(body, structs);
                }
                Stmt::Match {
                    expr,
                    cases,
                    default,
                } => {
                    self.check_expression_safety(expr, current_line);
                    for (ce, body) in cases {
                        self.check_expression_safety(ce, current_line);
                        self.analyze_statements(body, structs);
                    }
                    if let Some(d) = default {
                        self.analyze_statements(d, structs);
                    }
                }
                _ => {}
            }
        }
    }

    fn check_expression_safety(&mut self, expr: &Expr, line: usize) {
        if let Expr::AddrOf(name) = expr {
            self.states.insert(name.clone(), VarState::Safe);
            let prefix = format!("{}.", name);
            let mut keys_to_update = Vec::new();
            for key in self.states.keys() {
                if key.starts_with(&prefix) {
                    keys_to_update.push(key.clone());
                }
            }
            for key in keys_to_update {
                self.states.insert(key, VarState::Safe);
            }
            return;
        }

        match expr {
            Expr::Variable(_) | Expr::MemberAccess { .. } | Expr::Index { .. } => {
                if let Some(path) = self.get_expr_path(expr) {
                    if let Some(state) = self.states.get(&path) {
                        match state {
                            VarState::Uninitialized {
                                declared_line: _,
                                type_name,
                            } => {
                                let is_field = path.contains('.');
                                if is_field {
                                    let parts: Vec<&str> = path.split('.').collect();
                                    self.push_error(
                                        "error",
                                        &format!(
                                            "use of uninitialized field '{}' of struct '{}'",
                                            parts[1], parts[0]
                                        ),
                                        line,
                                        &format!("initialize the field before use: {}.{} = ...;", parts[0], parts[1]),
                                    );
                                } else {
                                    self.push_error(
                                        "error",
                                        &format!("use of potentially uninitialized variable '{}'", path),
                                        line,
                                        &format!("initialize it: '{} {} = null;'", type_name, path),
                                    );
                                }
                            }
                            VarState::Freed { freed_line } => {
                                self.push_error(
                                    "error",
                                    &format!(
                                        "use-after-free violation on pointer '{}'",
                                        path
                                    ),
                                    line,
                                    &format!(
                                        "pointer '{}' was freed on line {}, remove this access or re-allocate before use",
                                        path, freed_line
                                    ),
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
            Expr::Binary { left, right, op: _ } => {
                self.check_expression_safety(left, line);
                self.check_expression_safety(right, line);
            }
            Expr::Call { name, args } => {
                for arg in args {
                    self.check_expression_safety(arg, line);
                }

                if name == "mfree" {
                    if let Some(arg) = args.first() {
                        if let Some(path) = self.get_expr_path(arg) {
                            if let Some(VarState::Allocated {
                                allocated_line,
                                checked_not_null: false,
                                ..
                            }) = self.states.get(&path)
                            {
                                self.push_error(
                                    "warning",
                                    &format!("freeing potentially null pointer '{}'", path),
                                    line,
                                    &format!(
                                        "pointer '{}' was allocated on line {} but never checked for null, add if ({} != null) before mfree",
                                        path, allocated_line, path
                                    ),
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        if let Expr::MemberAccess {
            expr: base,
            is_arrow: true,
            member,
        } = expr
        {
            if let Some(path) = self.get_expr_path(base) {
                if let Some(VarState::Allocated {
                    allocated_line: _,
                    checked_not_null: false,
                    ..
                }) = self.states.get(&path)
                {
                    self.push_error(
                        "error",
                        &format!(
                            "potential null pointer dereference of '{}' when accessing field '{}'",
                            path, member
                        ),
                        line,
                        &format!(
                            "wrap in null check: if ({} != null) {{ ... }}",
                            path
                        ),
                    );
                }
            }
        }
    }

    fn merge_states(
        &mut self,
        branch_a: HashMap<String, VarState>,
        branch_b: HashMap<String, VarState>,
    ) {
        let all_keys: std::collections::HashSet<String> =
            branch_a.keys().chain(branch_b.keys()).cloned().collect();

        for name in all_keys {
            match (branch_a.get(&name), branch_b.get(&name)) {
                (Some(state_a), Some(state_b)) => {
                    if state_a == state_b {
                        self.states.insert(name, state_a.clone());
                    } else {
                        match (state_a, state_b) {
                            (VarState::Freed { .. }, _) | (_, VarState::Freed { .. }) => {
                                let freed_line = match state_a {
                                    VarState::Freed { freed_line } => *freed_line,
                                    _ => match state_b {
                                        VarState::Freed { freed_line } => *freed_line,
                                        _ => 0,
                                    },
                                };
                                self.states.insert(name, VarState::Freed { freed_line });
                            }
                            (
                                VarState::Allocated {
                                    allocated_line,
                                    checked_not_null: true,
                                    type_name,
                                },
                                VarState::Allocated {
                                    checked_not_null: false,
                                    ..
                                },
                            )
                            | (
                                VarState::Allocated {
                                    checked_not_null: false,
                                    ..
                                },
                                VarState::Allocated {
                                    allocated_line,
                                    checked_not_null: true,
                                    type_name,
                                },
                            ) => {
                                self.states.insert(
                                    name,
                                    VarState::Allocated {
                                        allocated_line: *allocated_line,
                                        checked_not_null: false,
                                        type_name: type_name.clone(),
                                    },
                                );
                            }
                            _ => {
                                self.states.insert(name, VarState::Safe);
                            }
                        }
                    }
                }
                (Some(state), None) | (None, Some(state)) => {
                    match state {
                        VarState::Freed { .. } => {
                            self.states.insert(name, state.clone());
                        }
                        _ => {
                            self.states.insert(name, VarState::Safe);
                        }
                    }
                }
                (None, None) => unreachable!(),
            }
        }
    }
}
