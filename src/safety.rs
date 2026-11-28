#![allow(dead_code)]

use crate::ast::*;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
enum VarState {
    Uninitialized {
        declared_line: usize,
        type_name: String,
    },
    Safe,
    /* A pointer was explicitly set to null (or a non-pointer scalar). It is
     * not an allocation, so it must not create a leak warning, but dereferencing
     * it still requires an explicit null check. */
    NullPointer {
        type_name: String,
    },
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
    source_lines: Rc<Vec<String>>,
    base_line: usize,
    typedefs: HashMap<String, DataType>,
    decl_types: HashMap<String, DataType>,
}

impl MemorySafetyAnalyzer {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            errors: Vec::new(),
            source_lines: Rc::new(Vec::new()),
            base_line: 0,
            typedefs: HashMap::new(),
            decl_types: HashMap::new(),
        }
    }

    pub fn set_typedefs(&mut self, typedefs: &[(String, DataType)]) {
        self.typedefs.clear();
        for (name, dt) in typedefs {
            self.typedefs.insert(name.clone(), dt.clone());
        }
    }

    /* Typedef uses may arrive as `Typedef(name, inner)` or as a named struct
     * reference that is really a typedef alias. Resolve both forms before
     * deciding whether a declaration is a pointer. */
    fn resolve_decl_type(&self, dt: &DataType) -> DataType {
        match dt {
            DataType::Typedef(_, inner) => self.resolve_decl_type(inner),
            DataType::Struct(name) => match self.typedefs.get(name) {
                Some(inner) => self.resolve_decl_type(inner),
                None => dt.clone(),
            },
            _ => dt.clone(),
        }
    }

    fn is_pointer_type(&self, dt: &DataType) -> bool {
        matches!(self.resolve_decl_type(dt), DataType::Pointer(_))
    }

    fn is_pointer_path(&self, path: &str) -> bool {
        if let Some(state) = self.states.get(path) {
            let type_name = match state {
                VarState::Uninitialized { type_name, .. }
                | VarState::NullPointer { type_name }
                | VarState::Allocated { type_name, .. } => Some(type_name.as_str()),
                VarState::Freed { .. } | VarState::Safe => None,
            };
            if let Some(type_name) = type_name {
                return type_name.ends_with('*');
            }
        }
        self.decl_types
            .get(path)
            .map_or(false, |dt| self.is_pointer_type(dt))
    }

    fn is_array_path(&self, path: &str) -> bool {
        if let Some(dt) = self.decl_types.get(path) {
            if matches!(self.resolve_decl_type(dt), DataType::Array(_, _)) {
                return true;
            }
        }
        self.states
            .get(path)
            .map(|state| match state {
                VarState::Uninitialized { type_name, .. } => type_name.contains('['),
                _ => false,
            })
            .unwrap_or(false)
    }

    /* A dynamic index collapses to one path. Recover the strongest safe state
     * shared by all constant elements so a checked pointer array is not called
     * uninitialized, while any null or unchecked element is still reported. */
    fn collapsed_indexed_state(&self, src: &str, suffix: &str) -> Option<VarState> {
        let elem_prefix = format!("{}[", src);
        let mut states = Vec::new();
        for (key, state) in &self.states {
            if let Some(rest) = key.strip_prefix(&elem_prefix) {
                if let Some(pos) = rest.find(']') {
                    if suffix.is_empty() {
                        if &rest[pos + 1..] == "" {
                            states.push(state.clone());
                        }
                    } else if rest[pos..].starts_with("].") && &rest[pos + 2..] == suffix {
                        states.push(state.clone());
                    }
                }
            }
        }
        if states.is_empty() {
            return None;
        }
        if states.iter().any(|s| matches!(s, VarState::Uninitialized { .. })) {
            return Some(states[0].clone());
        }
        if let Some(state) = states.iter().find(|s| matches!(s, VarState::NullPointer { .. })) {
            return Some(state.clone());
        }
        if let Some(state) = states.iter().find(|s| {
            matches!(s, VarState::Allocated { checked_not_null: false, .. })
        }) {
            return Some(state.clone());
        }
        if states
            .iter()
            .all(|s| matches!(s, VarState::Allocated { checked_not_null: true, .. }))
        {
            return Some(states[0].clone());
        }
        Some(VarState::Safe)
    }

    fn effective_state(&self, path: &str) -> Option<VarState> {
        let state = self.states.get(path).cloned()?;
        if !matches!(state, VarState::Uninitialized { .. }) {
            return Some(state);
        }
        let candidates = match path.find('.') {
            Some(dot) => vec![(&path[..dot], &path[dot + 1..]), (path, "")],
            None => vec![(path, "")],
        };
        for (base, suffix) in candidates {
            if let Some(effective) = self.collapsed_indexed_state(base, suffix) {
                return Some(effective);
            }
        }
        Some(state)
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
            DataType::Typedef(_, inner) => Self::format_data_type(inner),
        }
    }

    fn const_index_text(index: &Expr) -> Option<String> {
        match index {
            Expr::Number(n) => Some(n.to_string()),
            Expr::SignedNumber(n) => Some(n.to_string()),
            _ => None,
        }
    }

    /* A dynamic index collapses to the base object's field paths. Treat the
     * collapsed field as initialized when every constant element with that
     * field is initialized, so reading `a[i]` after initializing all elements
     * does not report a false positive. */
    fn all_indexed_fields_safe(&self, src: &str, suffix: &str) -> bool {
        let elem_prefix = format!("{}[", src);
        let mut found = false;
        for (key, state) in &self.states {
            if let Some(rest) = key.strip_prefix(&elem_prefix) {
                if let Some(pos) = rest.find(']') {
                    if rest[pos..].starts_with("].") && &rest[pos + 2..] == suffix {
                        found = true;
                        if !matches!(state, VarState::Safe) {
                            return false;
                        }
                    }
                }
            }
        }
        found
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
            Expr::Index { expr: base, index } => {
                let base_path = self.get_expr_path(base)?;
                match Self::const_index_text(index) {
                    /* Constant array elements are distinct objects. Dynamic
                     * indices keep the collapsed base path so writes and
                     * reads through the same variable still agree. */
                    Some(idx) => Some(format!("{}[{}]", base_path, idx)),
                    None => Some(base_path),
                }
            }
            Expr::AddrOf(name) => Some(name.clone()),
            Expr::AddrOfExpr(inner) => self.get_expr_path(inner),
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

    fn push_uninitialized_error(&mut self, path: &str, type_name: &str, line: usize) {
        if path.contains('.') {
            let parts: Vec<&str> = path.split('.').collect();
            self.push_error(
                "error",
                &format!(
                    "use of uninitialized field '{}' of struct '{}'",
                    parts[1], parts[0]
                ),
                line,
                &format!(
                    "initialize the field before use: {}.{} = ...;",
                    parts[0], parts[1]
                ),
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

    fn is_allocation_call(&self, expr: &Expr) -> bool {
        if let Expr::Call { name, .. } = expr {
            name == "mloc"
        } else {
            false
        }
    }

    fn is_null_or_scalar(expr: &Expr) -> bool {
        matches!(
            expr,
            Expr::Null | Expr::Number(_) | Expr::SignedNumber(_) | Expr::FloatLit(_)
        )
    }

    fn is_direct_pointer_target(expr: &Expr) -> bool {
        match expr {
            Expr::Variable(_) => true,
            Expr::MemberAccess { is_arrow: false, .. } => true,
            /* Every index in the path must be constant. A dynamic index
             * collapses to the base and can otherwise make a scalar store to
             * `p[i][0]` poison the state of `p[0]`. */
            Expr::Index { expr: base, index } => {
                Self::const_index_text(index).is_some() && Self::is_direct_pointer_target(base)
            }
            _ => false,
        }
    }

    fn report_pointer_state(&mut self, path: &str, line: usize) {
        match self.effective_state(path) {
            /* An uninitialized pointer field is dereferenced when used as an
             * index base. Uninitialized arrays are storage, not pointers, so
             * their `type_name` does not end in `*`. */
            Some(VarState::Uninitialized { type_name, .. }) if type_name.ends_with('*') => {
                self.push_uninitialized_error(path, &type_name, line);
            }
            Some(VarState::Freed { freed_line }) => {
                self.push_error(
                    "error",
                    &format!("use-after-free violation on pointer '{}'", path),
                    line,
                    &format!(
                        "pointer '{}' was freed on line {}, remove this access or re-allocate before use",
                        path, freed_line
                    ),
                );
            }
            Some(VarState::NullPointer { .. })
            | Some(VarState::Allocated {
                checked_not_null: false,
                ..
            }) => {
                self.push_error(
                    "error",
                    &format!("potential null pointer dereference of '{}'", path),
                    line,
                    &format!("wrap in null check: if ({} != null) {{ ... }}", path),
                );
            }
            _ => {}
        }
    }

    /* Walk the dereference chain of an indexed base without treating field
     * addresses as reads. `p->arr[0]`, `h.p[0]`, and `p[0]` all dereference a
     * pointer before the final element access. */
    fn check_pointer_chain_safety(&mut self, expr: &Expr, line: usize) {
        match expr {
            Expr::Variable(_) => {
                let path = self.get_expr_path(expr);
                if !path.as_deref().map_or(false, |p| self.is_array_path(p)) {
                    self.check_expression_safety(expr, line);
                }
                if let Some(path) = path {
                    if let Some(VarState::NullPointer { .. })
                    | Some(VarState::Allocated {
                        checked_not_null: false,
                        ..
                    }) = self.states.get(&path)
                    {
                        self.push_error(
                            "error",
                            &format!("potential null pointer dereference of '{}'", path),
                            line,
                            &format!("wrap in null check: if ({} != null) {{ ... }}", path),
                        );
                    }
                }
            }
            Expr::MemberAccess { expr: base, .. } => {
                self.check_pointer_chain_safety(base, line);
                if let Some(path) = self.get_expr_path(expr) {
                    self.report_pointer_state(&path, line);
                }
            }
            Expr::Index { expr: base, index } => {
                self.check_pointer_chain_safety(base, line);
                self.check_expression_safety(index, line);
                if let Some(path) = self.get_expr_path(base) {
                    self.report_pointer_state(&path, line);
                }
            }
            _ => self.check_expression_safety(expr, line),
        }
    }

    /* An indexed access dereferences its base pointer even when the indexed
     * path itself has separate field state. Check the base for freed, null,
     * or unchecked-allocation state so `p[0]` behaves like `p->field`. */
    fn check_index_base_safety(&mut self, base: &Expr, line: usize) {
        if Self::is_null_or_scalar(base) {
            self.push_error(
                "error",
                "potential null pointer dereference in indexed access",
                line,
                "check the pointer before indexing it",
            );
            return;
        }

        self.check_pointer_chain_safety(base, line);
    }

    pub fn analyze_function(
        &mut self,
        func: &FuncDecl,
        structs: &HashMap<String, StructDecl>,
        source: Option<&str>,
        base_line: usize,
    ) -> Result<(), Vec<String>> {
        self.states.clear();
        self.decl_types.clear();
        self.errors.clear();
        self.source_lines = Rc::new(
            source
                .map(|s| s.lines().map(|l| l.to_string()).collect())
                .unwrap_or_default(),
        );
        self.base_line = base_line;

        for (dt, name, _) in &func.params {
            self.states.insert(name.clone(), VarState::Safe);
            self.decl_types.insert(name.clone(), dt.clone());
            self.register_struct_fields(name, dt, structs, VarState::Safe, 0);
        }

        if let Some(body) = &func.body {
            self.analyze_statements(body, structs);
        }

        let mut leak_warnings: Vec<(String, usize)> = Vec::new();

        for (path, state) in &self.states {
            if let VarState::Allocated { allocated_line, .. } = state {
                if !path.contains('.') {
                    leak_warnings.push((path.clone(), *allocated_line));
                }
            }
        }

        for (path, allocated_line) in leak_warnings {
            self.push_error(
                "warning",
                &format!(
                    "potential memory leak in function '{}': pointer '{}' was never freed via 'mfree()'",
                    func.name, path
                ),
                allocated_line,
                "call mfree(ptr) before the function returns, or document that ownership is transferred",
            );
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }

    /* Only literal brace/string initializers are known to fill every leaf of
     * a struct. A copy from a variable or a call may carry indeterminate or
     * partially materialized fields, so those must stay uninitialized. */
    fn initializer_covers_struct_fields(expr: &Expr) -> bool {
        matches!(expr, Expr::ArrayInit(_) | Expr::StringLit(_))
    }

    fn register_struct_fields(
        &mut self,
        prefix: &str,
        dt: &DataType,
        structs: &HashMap<String, StructDecl>,
        initial_state: VarState,
        depth: usize,
    ) {
        /* Constant array elements get their own field paths; a collapsed base
         * path is kept as a conservative fallback for dynamic indices. The
         * parser keeps a typedef use as `Struct(alias)`, so resolve aliases
         * through the program's typedef table before giving up on tracking. */
        if depth > 8 {
            return;
        }
        let struct_name = match dt {
            DataType::Struct(name) => {
                if structs.contains_key(name) {
                    name.clone()
                } else if let Some(inner) = self.typedefs.get(name).cloned() {
                    return self.register_struct_fields(
                        prefix,
                        &inner,
                        structs,
                        initial_state,
                        depth + 1,
                    );
                } else {
                    return;
                }
            }
            DataType::Typedef(_, inner) => {
                return self.register_struct_fields(
                    prefix,
                    inner,
                    structs,
                    initial_state,
                    depth + 1,
                );
            }
            DataType::Array(inner, count) => {
                /* Pointer arrays have no struct fields to register, but each
                 * element still has pointer state. Register both constant
                 * elements and the collapsed dynamic-index path. */
                if self.is_pointer_type(inner) {
                    let element_state = match &initial_state {
                        VarState::Uninitialized { declared_line, .. } => {
                            VarState::Uninitialized {
                                declared_line: *declared_line,
                                type_name: Self::format_data_type(
                                    &self.resolve_decl_type(inner),
                                ),
                            }
                        }
                        _ => initial_state.clone(),
                    };
                    self.states.insert(prefix.to_string(), initial_state.clone());
                    for i in 0..*count {
                        let element_prefix = format!("{}[{}]", prefix, i);
                        self.states.insert(element_prefix, element_state.clone());
                    }
                }
                self.register_struct_fields(
                    prefix,
                    inner,
                    structs,
                    initial_state.clone(),
                    depth + 1,
                );
                for i in 0..*count {
                    let element_prefix = format!("{}[{}]", prefix, i);
                    self.register_struct_fields(
                        &element_prefix,
                        inner,
                        structs,
                        initial_state.clone(),
                        depth + 1,
                    );
                }
                return;
            }
            _ => return,
        };

        if let Some(s_decl) = structs.get(&struct_name) {
            for field in &s_decl.fields {
                let field_path = format!("{}.{}", prefix, field.name);
                let resolved_field = self.resolve_decl_type(&field.data_type);
                let type_str = Self::format_data_type(&resolved_field);

                let field_state = match &initial_state {
                    VarState::Uninitialized { declared_line, .. } => VarState::Uninitialized {
                        declared_line: *declared_line,
                        type_name: type_str.clone(),
                    },
                    _ => initial_state.clone(),
                };

                self.states.insert(field_path.clone(), field_state.clone());

                self.register_struct_fields(
                    &field_path,
                    &field.data_type,
                    structs,
                    field_state,
                    depth + 1,
                );
            }
        }
    }

    /* A literal initializer can explicitly store null into a pointer leaf.
     * Those leaves must be NullPointer rather than generically Safe so a later
     * dereference is still diagnosed. */
    fn apply_literal_field_states(
        &mut self,
        prefix: &str,
        dt: &DataType,
        init: &Expr,
        structs: &HashMap<String, StructDecl>,
        depth: usize,
    ) {
        if depth > 8 {
            return;
        }

        let resolved = self.resolve_decl_type(dt);
        match resolved {
            DataType::Struct(name) => {
                let decl = match structs.get(&name).cloned() {
                    Some(decl) => decl,
                    None => return,
                };
                if let Expr::ArrayInit(items) = init {
                    for (idx, field) in decl.fields.iter().enumerate() {
                        if let Some(item) = items.get(idx) {
                            let path = format!("{}.{}", prefix, field.name);
                            if matches!(item, Expr::Null) && self.is_pointer_type(&field.data_type) {
                                self.states.insert(
                                    path,
                                    VarState::NullPointer {
                                        type_name: Self::format_data_type(
                                            &self.resolve_decl_type(&field.data_type),
                                        ),
                                    },
                                );
                            } else {
                                self.apply_literal_field_states(
                                    &path,
                                    &field.data_type,
                                    item,
                                    structs,
                                    depth + 1,
                                );
                            }
                        }
                    }
                }
            }
            DataType::Array(inner, count) => {
                if let Expr::ArrayInit(items) = init {
                    for idx in 0..count.min(items.len()) {
                        let path = format!("{}[{}]", prefix, idx);
                        if matches!(&items[idx], Expr::Null) && self.is_pointer_type(&inner) {
                            self.states.insert(
                                path,
                                VarState::NullPointer {
                                    type_name: Self::format_data_type(&self.resolve_decl_type(&inner)),
                                },
                            );
                        } else {
                            self.apply_literal_field_states(
                                &path,
                                &inner,
                                &items[idx],
                                structs,
                                depth + 1,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /* Whole-object assignment behaves like initialization for field states:
     * literal values initialize every field, aggregate values copy the source
     * field states, and anything else leaves indeterminate fields. */
    fn update_assigned_fields(
        &mut self,
        target_path: &str,
        value: &Expr,
        current_line: usize,
    ) {
        /* Null and scalar stores do not copy aggregate state. Leaving the old
         * field facts alone avoids path-insensitive false positives when a
         * pointer is reset to null and re-checked before a later use. */
        if Self::is_null_or_scalar(value) {
            return;
        }

        /* A whole-object assignment must cover both collapsed struct-array
         * paths (`arr.n`) and constant element paths (`arr[0].n`). */
        let target_prefixes = [format!("{}.", target_path), format!("{}[", target_path)];
        let old_fields: Vec<String> = self
            .states
            .keys()
            .filter(|key| target_prefixes.iter().any(|p| key.starts_with(p)))
            .cloned()
            .collect();

        let source_path = match value {
            Expr::Variable(_)
            | Expr::MemberAccess { .. }
            | Expr::Index { .. }
            | Expr::AddrOf(_)
            | Expr::AddrOfExpr(_) => self.get_expr_path(value),
            _ => None,
        };
        let copied_fields: Vec<(String, VarState)> = match source_path {
            Some(ref src) => {
                let src_dot = format!("{}.", src);
                let src_index = format!("{}[", src);
                self.states
                    .iter()
                    .filter_map(|(key, state)| {
                        if let Some(rest) = key.strip_prefix(&src_dot) {
                            let effective = if matches!(state, VarState::Uninitialized { .. })
                                && self.all_indexed_fields_safe(src, rest)
                            {
                                VarState::Safe
                            } else {
                                state.clone()
                            };
                            Some((format!("{}.{}", target_path, rest), effective))
                        } else if let Some(rest) = key.strip_prefix(&src_index) {
                            Some((format!("{}[{}", target_path, rest), state.clone()))
                        } else {
                            None
                        }
                    })
                    .collect()
            }
            None => Vec::new(),
        };

        for key in &old_fields {
            self.states.remove(key);
        }
        if Self::initializer_covers_struct_fields(value) {
            for key in old_fields {
                self.states.insert(key, VarState::Safe);
            }
        } else if !copied_fields.is_empty() {
            for (key, state) in copied_fields {
                self.states.insert(key, state);
            }
        } else {
            for key in old_fields {
                self.states.insert(
                    key,
                    VarState::Uninitialized {
                        declared_line: current_line,
                        type_name: String::new(),
                    },
                );
            }
        }
    }

    fn check_lhs_safety(&mut self, expr: &Expr, line: usize) {
        match expr {
            Expr::Variable(_) => {}
            Expr::MemberAccess { expr: base, .. } => {
                self.check_expression_safety(base, line);
            }
            Expr::Index { expr: base, index } => {
                /* Writing `obj.field[0]` initializes that element; the base
                 * field is an address, not a read of its old value. The pointer
                 * chain leading to that address still needs null/UAF checks. */
                self.check_index_base_safety(base, line);
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
                    let resolved_type = self.resolve_decl_type(&decl.data_type);
                    let type_str = Self::format_data_type(&resolved_type);
                    let is_pointer = matches!(resolved_type, DataType::Pointer(_));
                    self.decl_types
                        .insert(decl.name.clone(), decl.data_type.clone());

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
                        } else if is_pointer && Self::is_null_or_scalar(init_expr) {
                            self.states.insert(
                                decl.name.clone(),
                                VarState::NullPointer {
                                    type_name: type_str.clone(),
                                },
                            );
                        } else {
                            self.states.insert(decl.name.clone(), VarState::Safe);

                            if !is_pointer {
                                let field_state = if Self::initializer_covers_struct_fields(
                                    init_expr,
                                ) {
                                    VarState::Safe
                                } else {
                                    VarState::Uninitialized {
                                        declared_line: current_line,
                                        type_name: type_str.clone(),
                                    }
                                };
                                self.register_struct_fields(
                                    &decl.name,
                                    &decl.data_type,
                                    structs,
                                    field_state,
                                    0,
                                );
                                if Self::initializer_covers_struct_fields(init_expr) {
                                    self.apply_literal_field_states(
                                        &decl.name,
                                        &decl.data_type,
                                        init_expr,
                                        structs,
                                        0,
                                    );
                                }
                            }
                        }
                    } else {
                        if is_pointer {
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
                                0,
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
                            } else if Self::is_null_or_scalar(value)
                                && Self::is_direct_pointer_target(target)
                                && self.is_pointer_path(&path)
                            {
                                /* `p = null` makes `p` unchecked; `p[i] = 7`
                                 * only writes an element and must not replace
                                 * the base pointer's state. */
                                self.states.insert(
                                    path.clone(),
                                    VarState::NullPointer {
                                        type_name: "void*".to_string(),
                                    },
                                );
                            } else {
                                self.states.insert(path.clone(), VarState::Safe);
                            }
                            self.update_assigned_fields(&path, value, current_line);
                            if Self::initializer_covers_struct_fields(value) {
                                if let Some(dt) = self.decl_types.get(&path).cloned() {
                                    self.apply_literal_field_states(
                                        &path, &dt, value, structs, 0,
                                    );
                                }
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
                        let path = if **right == Expr::Null {
                            self.get_expr_path(left)
                        } else if **left == Expr::Null {
                            self.get_expr_path(right)
                        } else {
                            None
                        };
                        if let Some(path) = path {
                            checked_var = Some(path);
                            if op == "OpEq" || op == "==" {
                                check_is_not_null = false;
                            }
                        }
                    }

                    let mut then_states = self.states.clone();
                    if let Some(ref path) = checked_var {
                        if check_is_not_null {
                            match then_states.get(path).cloned() {
                                Some(VarState::Allocated {
                                    allocated_line,
                                    type_name,
                                    ..
                                }) => {
                                    then_states.insert(
                                        path.clone(),
                                        VarState::Allocated {
                                            allocated_line,
                                            checked_not_null: true,
                                            type_name,
                                        },
                                    );
                                }
                                Some(VarState::NullPointer { .. }) => {
                                    then_states.insert(path.clone(), VarState::Safe);
                                }
                                _ => {}
                            }
                        }
                    }

                    let mut then_analyzer = MemorySafetyAnalyzer {
                        states: then_states.clone(),
                        errors: Vec::new(),
                        source_lines: self.source_lines.clone(),
                        base_line: self.base_line,
                        typedefs: self.typedefs.clone(),
                        decl_types: self.decl_types.clone(),
                    };
                    then_analyzer.analyze_statements(then_branch, structs);
                    self.errors.extend(then_analyzer.errors);

                    let then_terminates = Self::branch_terminates(then_branch);

                    let mut else_states = self.states.clone();
                    if let Some(else_stmts) = else_branch {
                        if let Some(ref path) = checked_var {
                            if !check_is_not_null {
                                match else_states.get(path).cloned() {
                                    Some(VarState::Allocated {
                                        allocated_line,
                                        type_name,
                                        ..
                                    }) => {
                                        else_states.insert(
                                            path.clone(),
                                            VarState::Allocated {
                                                allocated_line,
                                                checked_not_null: true,
                                                type_name,
                                            },
                                        );
                                    }
                                    Some(VarState::NullPointer { .. }) => {
                                        else_states.insert(path.clone(), VarState::Safe);
                                    }
                                    _ => {}
                                }
                            }
                        }

                        let mut else_analyzer = MemorySafetyAnalyzer {
                            states: else_states.clone(),
                            errors: Vec::new(),
                            source_lines: self.source_lines.clone(),
                            base_line: self.base_line,
                            typedefs: self.typedefs.clone(),
                        decl_types: self.decl_types.clone(),
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
                        if then_terminates {
                            /* The implicit else path is the one that continues.
                             * Refine the checked variable for that path before
                             * adopting its state. */
                            let mut fallthrough_states = self.states.clone();
                            if let Some(ref path) = checked_var {
                                if !check_is_not_null {
                                    /* `if (p == null) return;` leaves a non-null
                                     * pointer on the fallthrough path. */
                                    match fallthrough_states.get(path).cloned() {
                                        Some(VarState::NullPointer { .. }) => {
                                            fallthrough_states.insert(path.clone(), VarState::Safe);
                                        }
                                        Some(VarState::Allocated {
                                            allocated_line,
                                            type_name,
                                            ..
                                        }) => {
                                            fallthrough_states.insert(
                                                path.clone(),
                                                VarState::Allocated {
                                                    allocated_line,
                                                    checked_not_null: true,
                                                    type_name,
                                                },
                                            );
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            self.states = fallthrough_states;
                        } else {
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

        if let Expr::AddrOfExpr(inner) = expr {
            if let Some(path) = self.get_expr_path(inner) {
                self.states.insert(path.clone(), VarState::Safe);

                let prefix = format!("{}.", path);

                let keys: Vec<String> = self
                    .states
                    .keys()
                    .filter(|k| k.starts_with(&prefix))
                    .cloned()
                    .collect();

                for key in keys {
                    self.states.insert(key, VarState::Safe);
                }
            }

            self.check_expression_safety(inner, line);
            return;
        }

        if let Expr::Index { expr: base, .. } = expr {
            self.check_index_base_safety(base, line);
        }

        /* A dot field access can sit on top of an arrow or index dereference,
         * e.g. `p->in.n` or `p[0].n`. Check the base expression so those
         * dereferences are not hidden by the outer field access. */
        if let Expr::MemberAccess { expr: base, .. } = expr {
            self.check_expression_safety(base, line);
        }

        match expr {
            Expr::Variable(_) | Expr::MemberAccess { .. } | Expr::Index { .. } => {
                if let Some(path) = self.get_expr_path(expr) {
                    if let Some(state) = self.effective_state(&path) {
                        match &state {
                            /* Arrays are storage. Their collapsed base state
                             * is uninitialized until elements are written, but
                             * indexing that storage is not a pointer use. */
                            VarState::Uninitialized {
                                declared_line: _,
                                type_name,
                            } if !type_name.contains('[') => {
                                self.push_uninitialized_error(&path, type_name, line);
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
            Expr::ArrayInit(items) => {
                for item in items {
                    self.check_expression_safety(item, line);
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
            if Self::is_null_or_scalar(base) {
                self.push_error(
                    "error",
                    &format!(
                        "potential null pointer dereference when accessing field '{}'",
                        member
                    ),
                    line,
                    "check the pointer before dereferencing it",
                );
                return;
            }

            if let Some(path) = self.get_expr_path(base) {
                let unchecked = match self.states.get(&path) {
                    Some(VarState::Allocated {
                        checked_not_null: false,
                        ..
                    })
                    | Some(VarState::NullPointer { .. }) => true,
                    _ => false,
                };
                if unchecked {
                    self.push_error(
                        "error",
                        &format!(
                            "potential null pointer dereference of '{}' when accessing field '{}'",
                            path, member
                        ),
                        line,
                        &format!("wrap in null check: if ({} != null) {{ ... }}", path),
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
                (Some(state), None) | (None, Some(state)) => match state {
                    VarState::Freed { .. } => {
                        self.states.insert(name, state.clone());
                    }
                    _ => {
                        self.states.insert(name, VarState::Safe);
                    }
                },
                (None, None) => unreachable!(),
            }
        }
    }
}
