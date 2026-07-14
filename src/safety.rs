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
    // Состояния путей: "q" -> Safe, "q.doorbell_ptr" -> Uninitialized, и т.д.
    states: HashMap<String, VarState>,
    errors: Vec<String>,
}

impl MemorySafetyAnalyzer {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            errors: Vec::new(),
        }
    }

    // Форматирование типов данных в строку WandC для красивых подсказок
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

    // Преобразует сложные AST-выражения доступа (a.b, a->b) в плоский строковый путь "a.b"
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
                // Для массивов отслеживаем базовый массив
                self.get_expr_path(base)
            }
            _ => None,
        }
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
    ) -> Result<(), Vec<String>> {
        self.states.clear();
        self.errors.clear();

        // Параметры функции по умолчанию считаются инициализированными и безопасными
        for (dt, name, _) in &func.params {
            self.states.insert(name.clone(), VarState::Safe);
            self.register_struct_fields(name, dt, structs, VarState::Safe);
        }

        if let Some(body) = &func.body {
            self.analyze_statements(body, structs);
        }

        // Поиск утечек памяти в конце функции
        for (path, state) in &self.states {
            if let VarState::Allocated { allocated_line, .. } = state {
                // Если путь не содержит точек (это корень), рапортуем об утечке
                if !path.contains('.') {
                    self.errors.push(format!(
                        "\x1b[33;1mwarning\x1b[0m: potential memory leak in function '{}'\n\
                         \x1b[34;1m  -->\x1b[0m Pointer '{}' allocated on line {} was never freed via 'mfree()'.",
                        func.name, path, allocated_line
                    ));
                }
            }
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }

    // Регистрация полей структуры для отслеживания неинициализированного состояния
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

                // Рекурсивно регистрируем вложенные структуры
                self.register_struct_fields(&field_path, &field.data_type, structs, field_state);
            }
        }
    }

    fn check_lhs_safety(&mut self, expr: &Expr, line: usize) {
        match expr {
            Expr::Variable(_) => {
                // Перезапись локальной переменной всегда безопасна
            }
            Expr::MemberAccess { expr: base, .. } => {
                // Проверяем безопасность базовой структуры/указателя, но не само перезаписываемое поле
                self.check_expression_safety(base, line);
            }
            Expr::Index { expr: base, index } => {
                // Проверяем безопасность базового массива и используемого индекса
                self.check_expression_safety(base, line);
                self.check_expression_safety(index, line);
            }
            _ => {
                self.check_expression_safety(expr, line);
            }
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
                        // Если это указатель, помечаем его как неинициализированный
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
                            // Если это структура на стеке, регистрируем её поля как неинициализированные
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
                                let type_str = "void*".to_string(); // Временное имя типа для аллокаций
                                self.states.insert(
                                    path.clone(),
                                    VarState::Allocated {
                                        allocated_line: current_line,
                                        checked_not_null: false,
                                        type_name: type_str,
                                    },
                                );
                            } else {
                                // Запись в путь делает его безопасным (инициализированным)
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

                    // Анализ ветки THEN
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
                    };
                    then_analyzer.analyze_statements(then_branch, structs);
                    self.errors.extend(then_analyzer.errors);

                    // Анализ ветки ELSE
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
                        };
                        else_analyzer.analyze_statements(else_stmts, structs);
                        self.errors.extend(else_analyzer.errors);

                        // Сливаем состояния веток
                        self.merge_states(then_analyzer.states, else_analyzer.states);
                    } else {
                        // Если else нет, берем консервативные состояния из then
                        self.states = then_analyzer.states;
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
                _ => {}
            }
        }
    }

    fn check_expression_safety(&mut self, expr: &Expr, line: usize) {
        // Правило передачи адреса (Escape Analysis):
        // Если адрес переменной передается в функцию (q*adr), мы предполагаем,
        // что функция проведет инициализацию, и помечаем этот путь и все его поля как безопасные.
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
                                declared_line,
                                type_name,
                            } => {
                                let is_field = path.contains('.');
                                let err_msg = if is_field {
                                    let parts: Vec<&str> = path.split('.').collect();
                                    format!(
                                        "\x1b[31;1merror\x1b[0m: use of uninitialized field '{}' of struct '{}'\n\
                                         \x1b[34;1m  -->\x1b[0m line {}\n\
                                         \x1b[37;1m  help\x1b[0m: Field '{}' was declared on line {} but never initialized.\n\
                                         \x1b[32;1m  suggestion\x1b[0m: initialize the field before use:\n\
                                         \x1b[32;1m             {}.{} = ...;",
                                        parts[1], parts[0], line, parts[1], declared_line, parts[0], parts[1]
                                    )
                                } else {
                                    format!(
                                        "\x1b[31;1merror\x1b[0m: use of potentially uninitialized variable '{}'\n\
                                         \x1b[34;1m  -->\x1b[0m line {}\n\
                                         \x1b[37;1m  help\x1b[0m: Variable '{}' of type '{}' was declared on line {} but never initialized.\n\
                                         \x1b[32;1m  suggestion\x1b[0m: initialize it: '{} {} = null;'",
                                        path, line, path, type_name, declared_line, type_name, path
                                    )
                                };
                                self.errors.push(err_msg);
                            }
                            VarState::Freed { freed_line } => {
                                self.errors.push(format!(
                                    "\x1b[31;1merror\x1b[0m: use-after-free violation on pointer '{}'\n\
                                     \x1b[34;1m  -->\x1b[0m line {}\n\
                                     \x1b[37;1m  help\x1b[0m: Memory pointed to by '{}' was already freed on line {}.\n\
                                     \x1b[32;1m  suggestion\x1b[0m: remove this access or re-allocate the pointer before use.",
                                    path, line, path, freed_line
                                ));
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
                                self.errors.push(format!(
                                    "\x1b[33;1mwarning\x1b[0m: freeing potentially null pointer '{}'\n\
                                     \x1b[34;1m  -->\x1b[0m line {}\n\
                                     \x1b[37;1m  help\x1b[0m: Pointer '{}' was allocated on line {} but never checked for 'null'.",
                                    path, line, path, allocated_line
                                ));
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        // Проверка потенциального разыменования NULL через оператор -> (для полей)
        if let Expr::MemberAccess {
            expr: base,
            is_arrow: true,
            member,
        } = expr
        {
            if let Some(path) = self.get_expr_path(base) {
                if let Some(VarState::Allocated {
                    allocated_line,
                    checked_not_null: false,
                    ..
                }) = self.states.get(&path)
                {
                    self.errors.push(format!(
                        "\x1b[31;1merror\x1b[0m: potential null pointer dereference of '{}' when accessing field '{}'\n\
                         \x1b[34;1m  -->\x1b[0m line {}\n\
                         \x1b[37;1m  help\x1b[0m: Pointer '{}' was allocated on line {} but never checked for 'null'.\n\
                         \x1b[32;1m  suggestion\x1b[0m: wrap this block in a null-check:\n\
                         \x1b[32;1m            if ({} != null) {{\n\
                         \x1b[32;1m                // access {}.{} safely\n\
                         \x1b[32;1m            }} \x1b[0m",
                        path, member, line, path, allocated_line, path, path, member
                    ));
                }
            }
        }
    }

    // Слияние веток
    fn merge_states(
        &mut self,
        branch_a: HashMap<String, VarState>,
        branch_b: HashMap<String, VarState>,
    ) {
        for (name, state_a) in branch_a {
            if let Some(state_b) = branch_b.get(&name) {
                if state_a == *state_b {
                    self.states.insert(name, state_a);
                } else {
                    match (&state_a, state_b) {
                        (VarState::Freed { freed_line }, _) => {
                            self.states.insert(
                                name,
                                VarState::Freed {
                                    freed_line: *freed_line,
                                },
                            );
                        }
                        (_, VarState::Freed { freed_line }) => {
                            self.states.insert(
                                name,
                                VarState::Freed {
                                    freed_line: *freed_line,
                                },
                            );
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
                        ) => {
                            // Консервативно: если хоть в одной ветке не проверено — считаем непроверенным
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
        }
    }
}
