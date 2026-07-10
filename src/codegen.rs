use crate::ast::*;
use std::collections::HashMap;

pub struct NativeGenerator {
    code: Vec<u8>,
    local_offsets: HashMap<String, u32>,
    local_access: HashMap<String, PtrAccess>,
    local_types: HashMap<String, DataType>,
    next_offset: u32,

    pub function_offsets: HashMap<String, usize>,
    pub call_patches: Vec<(usize, String)>,

    global_offsets: HashMap<String, u32>,
    global_data_size: u32,

    struct_layouts: HashMap<String, (u32, HashMap<String, u32>)>,
    typedefs_map: HashMap<String, DataType>,

    string_constants: HashMap<String, u32>,
    global_data_start_offset: usize,

    // Таблица бэкпатчинга абсолютных адресов памяти
    address_patches: Vec<(usize, String)>,

    // Таблица сигнатур функций для контекстного разыменования аргументов
    function_signatures: HashMap<String, Vec<DataType>>,

    // Таблица типов полей структур для точного определения размера при записи по точке/стрелочке
    struct_fields: HashMap<String, HashMap<String, DataType>>,
}

impl NativeGenerator {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            local_offsets: HashMap::new(),
            local_access: HashMap::new(),
            local_types: HashMap::new(),
            next_offset: 8,
            function_offsets: HashMap::new(),
            call_patches: Vec::new(),
            global_offsets: HashMap::new(),
            global_data_size: 0,
            struct_layouts: HashMap::new(),
            typedefs_map: HashMap::new(),
            string_constants: HashMap::new(),
            global_data_start_offset: 0,
            address_patches: Vec::new(),
            function_signatures: HashMap::new(),
            struct_fields: HashMap::new(),
        }
    }

    pub fn compile_program(&mut self, program: &Program) -> Vec<u8> {
        // Очистка состояния генератора для предотвращения накопления мусора между сборками
        self.code.clear();
        self.local_offsets.clear();
        self.local_access.clear();
        self.local_types.clear();
        self.next_offset = 8;
        self.function_offsets.clear();
        self.call_patches.clear();
        self.global_offsets.clear();
        self.global_data_size = 0;
        self.string_constants.clear();
        self.address_patches.clear();
        self.struct_layouts.clear();
        self.typedefs_map.clear();

        // Наполнение карты typedefs
        for (name, dt) in &program.typedefs {
            self.typedefs_map.insert(name.clone(), dt.clone());
        }

        // Сбор сигнатур всех функций (как локальных, так и импортированных из хедеров)
        self.function_signatures.clear();
        for func in &program.functions {
            let param_types = func.params.iter().map(|(dt, _, _)| dt.clone()).collect();
            self.function_signatures
                .insert(func.name.clone(), param_types);
        }

        // Сбор типов полей всех структур для точного определения размеров
        self.struct_fields.clear();
        for s in &program.structs {
            let mut fields_types = HashMap::new();
            for field in &s.fields {
                fields_types.insert(field.name.clone(), field.data_type.clone());
            }
            self.struct_fields.insert(s.name.clone(), fields_types);
        }

        // Расчет смещения полей для всех структур
        for s in &program.structs {
            let mut fields_offsets = HashMap::new();
            let mut current_offset = 0;
            let mut max_alignment = 1;
            for field in &s.fields {
                let size = self.get_type_size_internal(&field.data_type);
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
                fields_offsets.insert(field.name.clone(), current_offset);
                current_offset += size;
            }
            if current_offset % max_alignment != 0 {
                current_offset += max_alignment - (current_offset % max_alignment);
            }
            self.struct_layouts
                .insert(s.name.clone(), (current_offset, fields_offsets));
        }

        // Предварительный сбор всех строковых констант и jmpto-констант
        self.collect_string_constants_from_program(program);

        let mut global_data_bytes = Vec::new();
        for sect in &program.sections {
            for var in &sect.variables {
                let key = format!("{}:{}", sect.name, var.name);
                self.global_offsets
                    .insert(key, global_data_bytes.len() as u32);

                let init_val = match &var.initial_value {
                    Some(expr) => match &**expr {
                        Expr::Number(n) => *n,
                        _ => 0,
                    },
                    None => 0,
                };
                global_data_bytes.extend_from_slice(&init_val.to_le_bytes());
            }
        }

        let mut string_offsets = HashMap::new();
        for (str_const, _) in &self.string_constants {
            let off = global_data_bytes.len() as u32;

            let unescaped = str_const
                .replace("\\n", "\n")
                .replace("\\t", "\t")
                .replace("\\r", "\r")
                .replace("\\\"", "\"");

            global_data_bytes.extend_from_slice(unescaped.as_bytes());
            global_data_bytes.push(0); // null terminator
            string_offsets.insert(str_const.clone(), off);
        }

        for (str_const, off) in &string_offsets {
            self.global_offsets
                .insert(format!("str:{}", str_const), *off);
        }

        self.global_data_size = global_data_bytes.len() as u32;

        self.code.extend_from_slice(&[0xE8, 0x0C, 0x00, 0x00, 0x00]);
        self.code.extend_from_slice(&[0x48, 0x89, 0xC7]);
        self.code
            .extend_from_slice(&[0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00]);
        self.code.extend_from_slice(&[0x0F, 0x05]);

        for func in &program.functions {
            let offset = self.code.len();
            self.function_offsets.insert(func.name.clone(), offset);
            self.compile_function(func);
        }

        let functions_end_offset = self.code.len();
        self.global_data_start_offset = (functions_end_offset + 7) & !7;

        while self.code.len() < self.global_data_start_offset {
            self.code.push(0);
        }
        self.code.extend_from_slice(&global_data_bytes);

        if let Some(&main_offset) = self.function_offsets.get("main") {
            let relative_offset = (main_offset as i32) - 5;
            let bytes = relative_offset.to_le_bytes();
            self.code[1] = bytes[0];
            self.code[2] = bytes[1];
            self.code[3] = bytes[2];
            self.code[4] = bytes[3];
        }

        let patches = std::mem::take(&mut self.call_patches);
        for (patch_pos, target_name) in patches {
            if let Some(&target_offset) = self.function_offsets.get(&target_name) {
                let relative_offset = (target_offset as i32) - ((patch_pos + 4) as i32);
                let bytes = relative_offset.to_le_bytes();
                self.code[patch_pos] = bytes[0];
                self.code[patch_pos + 1] = bytes[1];
                self.code[patch_pos + 2] = bytes[2];
                self.code[patch_pos + 3] = bytes[3];
            } else {
                self.call_patches.push((patch_pos, target_name));
            }
        }
        let addr_patches = std::mem::take(&mut self.address_patches);
        for (patch_pos, key) in addr_patches {
            let local_offset = self.global_offsets.get(&key).cloned().unwrap_or(0);
            let abs_addr =
                0x400078u64 + (self.global_data_start_offset as u64) + (local_offset as u64);
            let bytes = abs_addr.to_le_bytes();
            self.code[patch_pos] = bytes[0];
            self.code[patch_pos + 1] = bytes[1];
            self.code[patch_pos + 2] = bytes[2];
            self.code[patch_pos + 3] = bytes[3];
            self.code[patch_pos + 4] = bytes[4];
            self.code[patch_pos + 5] = bytes[5];
            self.code[patch_pos + 6] = bytes[6];
            self.code[patch_pos + 7] = bytes[7];
        }

        self.code.clone()
    }

    fn collect_string_constants_from_program(&mut self, program: &Program) {
        for func in &program.functions {
            if let Some(body) = &func.body {
                for stmt in body {
                    self.collect_string_constants_from_stmt(stmt);
                }
            }
        }
    }

    fn collect_string_constants_from_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDefinition(decl) => {
                if let Some(ref init) = decl.initial_value {
                    self.collect_string_constants_from_expr(init);
                }
            }
            Stmt::Assignment { targets, value } => {
                for target in targets {
                    self.collect_string_constants_from_expr(target);
                }
                self.collect_string_constants_from_expr(value);
            }
            Stmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.collect_string_constants_from_expr(cond);
                for s in then_branch {
                    self.collect_string_constants_from_stmt(s);
                }
                if let Some(else_stmts) = else_branch {
                    for s in else_stmts {
                        self.collect_string_constants_from_stmt(s);
                    }
                }
            }
            Stmt::While { cond, body } => {
                self.collect_string_constants_from_expr(cond);
                for s in body {
                    self.collect_string_constants_from_stmt(s);
                }
            }
            Stmt::For {
                init,
                cond,
                post,
                body,
            } => {
                if let Some(i) = init {
                    self.collect_string_constants_from_stmt(i);
                }
                self.collect_string_constants_from_expr(cond);
                if let Some(p) = post {
                    self.collect_string_constants_from_stmt(p);
                }
                for s in body {
                    self.collect_string_constants_from_stmt(s);
                }
            }
            Stmt::Jmpto { module_name, args } => {
                self.string_constants.insert(module_name.clone(), 0);
                for arg in args {
                    self.collect_string_constants_from_stmt(arg);
                }
            }
            Stmt::Return(values) => {
                for (_, expr) in values {
                    self.collect_string_constants_from_expr(expr);
                }
            }
            Stmt::Expr(expr) => {
                self.collect_string_constants_from_expr(expr);
            }
            _ => {}
        }
    }

    fn collect_string_constants_from_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::StringLit(s) => {
                self.string_constants.insert(s.clone(), 0);
            }
            Expr::Binary { left, right, .. } => {
                self.collect_string_constants_from_expr(left);
                self.collect_string_constants_from_expr(right);
            }
            Expr::Call { args, .. } => {
                for arg in args {
                    self.collect_string_constants_from_expr(arg);
                }
            }
            Expr::Index { expr: base, index } => {
                self.collect_string_constants_from_expr(base);
                self.collect_string_constants_from_expr(index);
            }
            Expr::MemberAccess { expr: base, .. } => {
                self.collect_string_constants_from_expr(base);
            }
            _ => {}
        }
    }

    fn get_type_size_internal(&self, dt: &DataType) -> u32 {
        match dt {
            DataType::U8 | DataType::I8 => 1,
            DataType::U16 | DataType::I16 => 2,
            DataType::U32 | DataType::I32 => 4,
            DataType::U64 | DataType::I64 => 8,
            DataType::Pointer(_) => 8,
            DataType::Array(elem, count) => self.get_type_size_internal(elem) * (*count as u32),
            DataType::Typedef(_, underlying) => self.get_type_size_internal(underlying),
            DataType::Struct(name) => {
                if let Some(alias) = self.typedefs_map.get(name) {
                    self.get_type_size_internal(alias)
                } else if let Some((size, _)) = self.struct_layouts.get(name) {
                    *size
                } else {
                    16
                }
            }
            DataType::Void => 0,
        }
    }

    fn get_expr_type_size(&self, expr: &Expr) -> u32 {
        match expr {
            Expr::Variable(name) => {
                if let Some(dt) = self.local_types.get(name) {
                    self.get_type_size_internal(dt)
                } else {
                    8
                }
            }
            Expr::Index {
                expr: base_expr, ..
            } => {
                if let Expr::Variable(ref name) = &**base_expr {
                    if let Some(dt) = self.local_types.get(name) {
                        match dt {
                            DataType::Array(elem, _) => {
                                return self.get_type_size_internal(elem);
                            }
                            DataType::Pointer(elem) => {
                                return self.get_type_size_internal(elem);
                            }
                            DataType::Struct(struct_name) => {
                                if let Some(DataType::Array(elem, _)) =
                                    self.typedefs_map.get(struct_name)
                                {
                                    return self.get_type_size_internal(elem);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                8
            }
            Expr::MemberAccess {
                expr: base_expr,
                member,
                ..
            } => {
                let mut struct_name = String::new();
                if let Expr::Variable(base_var) = &**base_expr {
                    if let Some(dt) = self.local_types.get(base_var) {
                        struct_name = match dt {
                            DataType::Struct(n) => n.clone(),
                            DataType::Pointer(boxed) => match &**boxed {
                                DataType::Struct(n) => n.clone(),
                                _ => String::new(),
                            },
                            _ => String::new(),
                        };
                    }
                }

                if !struct_name.is_empty() {
                    if let Some(fields) = self.struct_fields.get(&struct_name) {
                        if let Some(dt) = fields.get(member) {
                            return self.get_type_size_internal(dt);
                        }
                    }
                }
                8
            }
            _ => 8,
        }
    }

    fn emit_mem_op(&mut self, opcode: u8, reg_code: u8, offset: u32) {
        if offset <= 127 {
            let modrm = 0x40 | (reg_code << 3) | 5;
            self.code.extend_from_slice(&[0x48, opcode, modrm]);
            let disp = (-(offset as i8)) as u8;
            self.code.push(disp);
        } else {
            let modrm = 0x80 | (reg_code << 3) | 5;
            self.code.extend_from_slice(&[0x48, opcode, modrm]);
            let disp = -(offset as i32);
            self.code.extend_from_slice(&disp.to_le_bytes());
        }
    }

    fn emit_mem_load(&mut self, reg_code: u8, offset: u32, size: u32) {
        let is_large_disp = offset > 127;
        let modrm = if is_large_disp {
            0x80 | (reg_code << 3) | 5
        } else {
            0x40 | (reg_code << 3) | 5
        };

        match size {
            1 => {
                // movzx r64, byte ptr [rbp - offset] -> 48 0F B6 modrm disp
                self.code.extend_from_slice(&[0x48, 0x0F, 0xB6, modrm]);
            }
            2 => {
                // movzx r64, word ptr [rbp - offset] -> 48 0F B7 modrm disp
                self.code.extend_from_slice(&[0x48, 0x0F, 0xB7, modrm]);
            }
            4 => {
                // mov r32, dword ptr [rbp - offset] -> 8B modrm disp (автоматическое zero-extend в 64-бит)
                self.code.extend_from_slice(&[0x8B, modrm]);
            }
            _ => {
                // mov r64, qword ptr [rbp - offset] -> 48 8B modrm disp
                self.code.extend_from_slice(&[0x48, 0x8B, modrm]);
            }
        }

        if is_large_disp {
            let disp = -(offset as i32);
            self.code.extend_from_slice(&disp.to_le_bytes());
        } else {
            let disp = (-(offset as i8)) as u8;
            self.code.push(disp);
        }
    }

    fn emit_mem_store(&mut self, reg_code: u8, offset: u32, size: u32) {
        let is_large_disp = offset > 127;
        let modrm = if is_large_disp {
            0x80 | (reg_code << 3) | 5
        } else {
            0x40 | (reg_code << 3) | 5
        };

        match size {
            1 => {
                if reg_code >= 4 {
                    self.code.push(0x40);
                }
                // mov byte ptr [rbp - offset], reg_8 -> 88 modrm disp
                self.code.extend_from_slice(&[0x88, modrm]);
            }
            2 => {
                // mov word ptr [rbp - offset], reg_16 -> 66 89 modrm disp
                self.code.extend_from_slice(&[0x66, 0x89, modrm]);
            }
            4 => {
                // mov dword ptr [rbp - offset], reg_32 -> 89 modrm disp
                self.code.extend_from_slice(&[0x89, modrm]);
            }
            _ => {
                // mov qword ptr [rbp - offset], reg_64 -> 48 89 modrm disp
                self.code.extend_from_slice(&[0x48, 0x89, modrm]);
            }
        }

        if is_large_disp {
            let disp = -(offset as i32);
            self.code.extend_from_slice(&disp.to_le_bytes());
        } else {
            let disp = (-(offset as i8)) as u8;
            self.code.push(disp);
        }
    }

    fn compile_function(&mut self, func: &FuncDecl) {
        self.local_offsets.clear();
        self.local_access.clear();
        self.local_types.clear();
        self.next_offset = 8;

        // Пролог кадра стека
        self.code.push(0x55);
        self.code.extend_from_slice(&[0x48, 0x89, 0xE5]);

        // sub rsp, 512 (Задаем безопасный размер кадра стека для массивов)
        self.code
            .extend_from_slice(&[0x48, 0x81, 0xEC, 0x00, 0x02, 0x00, 0x00]);

        for (idx, (dt, name, access)) in func.params.iter().enumerate() {
            let var_size = self.get_type_size_internal(dt);
            self.next_offset += var_size; // Сначала увеличиваем смещение
            let offset = self.next_offset; // Затем берем базовый адрес
            self.local_offsets.insert(name.clone(), offset);
            self.local_access.insert(name.clone(), *access);
            self.local_types.insert(name.clone(), dt.clone());

            if idx < 4 {
                let reg_code = match idx {
                    0 => 7, // RDI
                    1 => 6, // RSI
                    2 => 2, // RDX
                    _ => 1, // RCX
                };
                self.emit_mem_store(reg_code, offset, var_size); // mov [rbp - offset], reg
            }
        }

        if let Some(body) = &func.body {
            for stmt in body {
                self.compile_stmt(stmt);
            }
        }

        // Эпилог
        self.code.extend_from_slice(&[0x48, 0x89, 0xEC]);
        self.code.push(0x5D);
        self.code.push(0xC3);
    }

    fn compile_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDefinition(decl) => {
                let var_size = self.get_type_size_internal(&decl.data_type);
                self.next_offset += var_size; // Сначала увеличиваем смещение
                let offset = self.next_offset; // Затем берем базовый адрес
                self.local_offsets.insert(decl.name.clone(), offset);
                self.local_access.insert(decl.name.clone(), decl.modifier);
                self.local_types
                    .insert(decl.name.clone(), decl.data_type.clone());

                if let Some(ref init_expr) = decl.initial_value {
                    self.compile_expr(init_expr, 0, true);
                    self.emit_mem_store(0, offset, var_size);
                }
            }
            Stmt::Assignment { targets, value } => {
                if let Some(target_expr) = targets.first() {
                    // Вычисляем правое значение (оно возвращается в rax)
                    self.compile_expr(value, 0, true);
                    // Записываем его в первый приемник
                    self.store_assignment_target_from_rax(target_expr);

                    // Для 2-го значения (в rdx)
                    if targets.len() > 1 {
                        if let Some(target_expr2) = targets.get(1) {
                            self.code.extend_from_slice(&[0x48, 0x89, 0xD0]); // mov rax, rdx
                            self.store_assignment_target_from_rax(target_expr2);
                        }
                    }

                    // Для 3-го значения (в rcx)
                    if targets.len() > 2 {
                        if let Some(target_expr3) = targets.get(2) {
                            self.code.extend_from_slice(&[0x48, 0x89, 0xC8]); // mov rax, rcx
                            self.store_assignment_target_from_rax(target_expr3);
                        }
                    }

                    // Для 4-го значения (в r8)
                    if targets.len() > 3 {
                        if let Some(target_expr4) = targets.get(3) {
                            self.code.extend_from_slice(&[0x4C, 0x89, 0xC0]); // mov rax, r8 (4C 89 C0)
                            self.store_assignment_target_from_rax(target_expr4);
                        }
                    }
                }
            }
            Stmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let jump_op = self.compile_condition_helper(cond);
                self.code.push(0x0F);
                self.code.push(jump_op);
                let jump_false_patch_pos = self.code.len();
                self.code.extend_from_slice(&[0, 0, 0, 0]);

                for then_stmt in then_branch {
                    self.compile_stmt(then_stmt);
                }

                if let Some(else_stmts) = else_branch {
                    self.code.push(0xE9);
                    let jump_end_patch_pos = self.code.len();
                    self.code.extend_from_slice(&[0, 0, 0, 0]);

                    let offset_false = (self.code.len() - (jump_false_patch_pos + 4)) as i32;
                    self.patch_address(jump_false_patch_pos, offset_false);

                    for else_stmt in else_stmts {
                        self.compile_stmt(else_stmt);
                    }

                    let offset_end = (self.code.len() - (jump_end_patch_pos + 4)) as i32;
                    self.patch_address(jump_end_patch_pos, offset_end);
                } else {
                    let offset_false = (self.code.len() - (jump_false_patch_pos + 4)) as i32;
                    self.patch_address(jump_false_patch_pos, offset_false);
                }
            }
            Stmt::While { cond, body } => {
                let loop_start_pos = self.code.len();
                let jump_op = self.compile_condition_helper(cond);

                self.code.push(0x0F);
                self.code.push(jump_op);
                let exit_patch_pos = self.code.len();
                self.code.extend_from_slice(&[0, 0, 0, 0]);

                for body_stmt in body {
                    self.compile_stmt(body_stmt);
                }

                let jmp_offset = (loop_start_pos as i32) - ((self.code.len() + 5) as i32);
                self.code.push(0xE9);
                self.code.extend_from_slice(&jmp_offset.to_le_bytes());

                let exit_offset = (self.code.len() - (exit_patch_pos + 4)) as i32;
                self.patch_address(exit_patch_pos, exit_offset);
            }
            Stmt::For {
                init,
                cond,
                post,
                body,
            } => {
                if let Some(init_stmt) = init {
                    self.compile_stmt(init_stmt);
                }

                let loop_start_pos = self.code.len();
                let jump_op = self.compile_condition_helper(cond);

                self.code.push(0x0F);
                self.code.push(jump_op);
                let exit_patch_pos = self.code.len();
                self.code.extend_from_slice(&[0; 4]);

                for body_stmt in body {
                    self.compile_stmt(body_stmt);
                }

                if let Some(post_stmt) = post {
                    self.compile_stmt(post_stmt);
                }

                let jmp_offset = (loop_start_pos as i32) - ((self.code.len() + 5) as i32);
                self.code.push(0xE9);
                self.code.extend_from_slice(&jmp_offset.to_le_bytes());

                let exit_offset = (self.code.len() - (exit_patch_pos + 4)) as i32;
                self.patch_address(exit_patch_pos, exit_offset);
            }
            Stmt::Jmpto { module_name, args } => {
                for arg in args {
                    self.compile_stmt(arg);
                }

                // mov rdi, abs_addr (0x48, 0xBF, 8 байт пустого адреса)
                self.code.extend_from_slice(&[0x48, 0xBF]);
                let patch_pos = self.code.len();
                self.code.extend_from_slice(&[0; 8]); // заглушка
                self.address_patches
                    .push((patch_pos, format!("str:{}", module_name)));

                // call sld_jmpto
                self.code.push(0xE8);
                let patch_pos_call = self.code.len();
                self.code.extend_from_slice(&[0, 0, 0, 0]);
                self.call_patches
                    .push((patch_pos_call, "sld_jmpto".to_string()));

                // АВТОМАТИЧЕСКАЯ РЕГИСТРАЦИЯ И ЗАПИСЬ ВОЗВРАЩАЕМОЙ ПЕРЕМЕННОЙ ИЗ ИСХОДНИКА!
                // Пытаемся прочитать исходный код по имени модуля напрямую (.wexp) или по его копии (.w)
                let mut source_code = None;
                if let Ok(code) = std::fs::read_to_string(&module_name) {
                    source_code = Some(code);
                } else {
                    let source_filename = module_name.replace(".wexp", ".w");
                    if let Ok(code) = std::fs::read_to_string(&source_filename) {
                        source_code = Some(code);
                    }
                }

                let mut compiled_inline = false;
                if let Some(code) = source_code {
                    let lexer = crate::lexer::Lexer::new(&code);
                    let mut parser = crate::parser::Parser::new(lexer);
                    if let Ok(parsed_program) = parser.parse_program() {
                        if let Some(_) = parsed_program.functions.iter().find(|f| f.name == "main")
                        {
                            // Ищем и парсим тело функции main, как это делает main.rs!
                            let local_lexer = crate::lexer::Lexer::new(&code);
                            let mut local_parser = crate::parser::Parser::new(local_lexer);
                            if local_parser.seek_to_function("main").is_ok() {
                                if let Ok(body) = local_parser.parse_function_body() {
                                    // Компилируем стейтменты тела main модуля ИНЛАЙН!
                                    for stmt in body {
                                        if let Stmt::Return(values) = stmt {
                                            if let Some((dt, Expr::Variable(ref var_name))) =
                                                values.first()
                                            {
                                                // Если переменная еще не задекларирована локально на стеке
                                                if !self.local_offsets.contains_key(var_name) {
                                                    // Проверяем, есть ли разделяемая переменная в секциях (например, args:z)
                                                    let mut is_global = false;
                                                    for key in self.global_offsets.keys() {
                                                        if key.ends_with(&format!(":{}", var_name))
                                                        {
                                                            is_global = true;
                                                            break;
                                                        }
                                                    }
                                                    // Если это чисто локальная переменная, выделяем под нее место
                                                    if !is_global {
                                                        let var_size =
                                                            self.get_type_size_internal(dt);
                                                        self.next_offset += var_size;
                                                        let offset = self.next_offset;
                                                        self.local_offsets
                                                            .insert(var_name.clone(), offset);
                                                        self.local_access.insert(
                                                            var_name.clone(),
                                                            PtrAccess::Normal,
                                                        );
                                                        self.local_types
                                                            .insert(var_name.clone(), dt.clone());
                                                    }
                                                }
                                                // Сохраняем RAX в эту переменную на стеке текущей функции
                                                if let Some(&offset) =
                                                    self.local_offsets.get(var_name)
                                                {
                                                    let var_size = self.get_type_size_internal(dt);
                                                    self.emit_mem_store(0, offset, var_size);
                                                }
                                            }
                                        } else {
                                            self.compile_stmt(&stmt);
                                        }
                                    }
                                    compiled_inline = true;
                                }
                            }
                        }
                    }
                }

                if !compiled_inline {
                    // Если инлайн не удался (файла нет), оставляем классический вызов динамического sld
                    self.code.extend_from_slice(&[0x48, 0xBF]);
                    let patch_pos = self.code.len();
                    self.code.extend_from_slice(&[0; 8]);
                    self.address_patches
                        .push((patch_pos, format!("str:{}", module_name)));

                    self.code.push(0xE8);
                    let patch_pos_call = self.code.len();
                    self.code.extend_from_slice(&[0, 0, 0, 0]);
                    self.call_patches
                        .push((patch_pos_call, "sld_jmpto".to_string()));
                }
            }
            Stmt::Return(values) => {
                if let Some((_, ref expr)) = values.first() {
                    self.compile_expr(expr, 0, true); // RAX (1-й результат)
                }
                if let Some((_, ref expr)) = values.get(1) {
                    self.compile_expr(expr, 0, true); // Вычисляем в RAX
                    self.code.extend_from_slice(&[0x48, 0x89, 0xC2]); // mov rdx, rax (2-й результат)
                }
                if let Some((_, ref expr)) = values.get(2) {
                    self.compile_expr(expr, 0, true); // Вычисляем в RAX
                    self.code.extend_from_slice(&[0x48, 0x89, 0xC1]); // mov rcx, rax (3-й результат)
                }
                if let Some((_, ref expr)) = values.get(3) {
                    self.compile_expr(expr, 0, true); // Вычисляем в RAX
                    self.code.extend_from_slice(&[0x49, 0x89, 0xC0]); // mov r8, rax (4-й результат)
                }
                self.code.extend_from_slice(&[0x48, 0x89, 0xEC]);
                self.code.push(0x5D);
                self.code.push(0xC3);
            }
            Stmt::Nasm(asm_code) => {
                for line in asm_code.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    if trimmed == "syscall" {
                        self.code.extend_from_slice(&[0x0F, 0x05]);
                    } else if trimmed == "ret" {
                        self.code.push(0xC3);
                    } else if trimmed == "push rbp" {
                        self.code.push(0x55);
                    } else if trimmed == "pop rbp" {
                        self.code.push(0x5D);
                    } else if trimmed.starts_with("mov ") {
                        let parts: Vec<&str> = trimmed["mov ".len()..]
                            .split(',')
                            .map(|s| s.trim())
                            .collect();
                        if parts.len() == 2 {
                            let dest = parts[0];
                            let src = parts[1];

                            let dest_reg_code = match dest {
                                "rax" => Some(0),
                                "rcx" => Some(1),
                                "rdx" => Some(2),
                                "rbx" => Some(3),
                                "rsi" => Some(6),
                                "rdi" => Some(7),
                                _ => None,
                            };

                            // Добавлена поддержка сохранения регистра в локальную переменную (mov [var], reg)
                            if dest.starts_with('[') && dest.ends_with(']') {
                                let var_name = dest[1..dest.len() - 1].trim();
                                if let Some(&offset) = self.local_offsets.get(var_name) {
                                    let src_reg_code = match src {
                                        "rax" => Some(0),
                                        "rcx" => Some(1),
                                        "rdx" => Some(2),
                                        "rbx" => Some(3),
                                        "rsi" => Some(6),
                                        "rdi" => Some(7),
                                        _ => None,
                                    };
                                    if let Some(src_code) = src_reg_code {
                                        self.emit_mem_store(src_code, offset, 8);
                                    }
                                }
                            } else if let Some(dest_code) = dest_reg_code {
                                if src.starts_with('[') && src.ends_with(']') {
                                    let var_name = src[1..src.len() - 1].trim();
                                    if let Some(&offset) = self.local_offsets.get(var_name) {
                                        self.emit_mem_load(dest_code, offset, 8);
                                    }
                                } else if let Ok(num) = src.parse::<i32>() {
                                    // mov reg, imm32 -> 48 C7 C(dest_code) imm32
                                    let modrm = 0xC0 | dest_code;
                                    self.code.extend_from_slice(&[0x48, 0xC7, modrm]);
                                    self.code.extend_from_slice(&num.to_le_bytes());
                                } else {
                                    let src_reg_code = match src {
                                        "rax" => Some(0),
                                        "rcx" => Some(1),
                                        "rdx" => Some(2),
                                        "rbx" => Some(3),
                                        "rsi" => Some(6),
                                        "rdi" => Some(7),
                                        _ => None,
                                    };
                                    if let Some(src_code) = src_reg_code {
                                        // mov dest, src -> 48 89 modrm
                                        let modrm = 0xC0 | (src_code << 3) | dest_code;
                                        self.code.extend_from_slice(&[0x48, 0x89, modrm]);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Stmt::Expr(ref expr) => {
                self.compile_expr(expr, 0, true);
            }
        }
    }

    fn compile_expr(&mut self, expr: &Expr, reg: u8, _deref_ptr: bool) {
        match expr {
            Expr::Number(n) => {
                let val = *n;
                let opcode = 0xB8 + reg;
                self.code.extend_from_slice(&[0x48, opcode]);
                self.code.extend_from_slice(&val.to_le_bytes());
            }
            Expr::StringLit(s) => {
                let opcode = 0xB8 + reg;
                self.code.extend_from_slice(&[0x48, opcode]);
                let patch_pos = self.code.len();
                self.code.extend_from_slice(&[0; 8]);
                self.address_patches.push((patch_pos, format!("str:{}", s)));
            }
            Expr::Variable(name) => {
                if let Some(&offset) = self.local_offsets.get(name) {
                    let var_size = self.get_type_size_internal(
                        self.local_types.get(name).unwrap_or(&DataType::U64),
                    );
                    self.emit_mem_load(reg, offset, var_size);

                    let modifier = self
                        .local_access
                        .get(name)
                        .cloned()
                        .unwrap_or(PtrAccess::Normal);
                    if modifier == PtrAccess::Input && _deref_ptr {
                        let mut is_byte = false;
                        if let Some(DataType::Pointer(ref boxed)) = self.local_types.get(name) {
                            if **boxed == DataType::U8 || **boxed == DataType::I8 {
                                is_byte = true;
                            }
                        }

                        let deref_op = if reg == 0 {
                            if is_byte {
                                &[0x48, 0x0F, 0xB6, 0x00][..]
                            } else {
                                &[0x48, 0x8B, 0x00][..]
                            }
                        } else {
                            if is_byte {
                                &[0x48, 0x0F, 0xB6, 0x1B][..]
                            } else {
                                &[0x48, 0x8B, 0x1B][..]
                            }
                        };
                        self.code.extend_from_slice(deref_op);
                    }
                } else {
                    // ФОЛБЕК НА ГЛOБАЛЬНЫЕ ПЕРЕМЕННЫЕ СЕКЦИЙ (args:x_val и т.д.)
                    let mut found_key = None;
                    for key in self.global_offsets.keys() {
                        if key.ends_with(&format!(":{}", name)) || key == name {
                            found_key = Some(key.clone());
                            break;
                        }
                    }
                    if let Some(key) = found_key {
                        let parts: Vec<&str> = key.split(':').collect();
                        let section = parts[0].to_string();
                        let variable = parts[1].to_string();
                        self.compile_expr(
                            &Expr::SectionAccess { section, variable },
                            reg,
                            _deref_ptr,
                        );
                    }
                }
            }
            Expr::AddrOf(name) => {
                if let Some(&offset) = self.local_offsets.get(name) {
                    self.emit_mem_op(0x8D, reg, offset);
                }
            }
            Expr::MemberAccess { .. } | Expr::Index { .. } => {
                self.compile_address(expr, 1);
                let size = self.get_expr_type_size(expr);
                if reg == 0 {
                    match size {
                        1 => self.code.extend_from_slice(&[0x48, 0x0F, 0xB6, 0x03]),
                        2 => self.code.extend_from_slice(&[0x48, 0x0F, 0xB7, 0x03]),
                        4 => self.code.extend_from_slice(&[0x8B, 0x03]),
                        _ => self.code.extend_from_slice(&[0x48, 0x8B, 0x03]),
                    }
                } else {
                    match size {
                        1 => self.code.extend_from_slice(&[0x48, 0x0F, 0xB6, 0x1B]),
                        2 => self.code.extend_from_slice(&[0x48, 0x0F, 0xB7, 0x1B]),
                        4 => self.code.extend_from_slice(&[0x8B, 0x1B]),
                        _ => self.code.extend_from_slice(&[0x48, 0x8B, 0x1B]),
                    }
                }
            }
            Expr::SectionAccess { section, variable } => {
                let key = format!("{}:{}", section, variable);
                let opcode = if reg == 0 { 0xB8 } else { 0xBB };
                self.code.extend_from_slice(&[0x48, opcode]); // mov reg, imm64
                let patch_pos = self.code.len();
                self.code.extend_from_slice(&[0; 8]); // 8-байтовая заглушка
                self.address_patches.push((patch_pos, key));

                if reg == 0 {
                    self.code.extend_from_slice(&[0x48, 0x8B, 0x00]); // mov rax, [rax]
                } else {
                    self.code.extend_from_slice(&[0x48, 0x8B, 0x1B]); // mov rbx, [rbx]
                }
            }
            Expr::Binary { left, op, right } => {
                // ПРОВЕРКА НА *adr (Взятие адреса элемента или переменной)
                if op == "OpMul" {
                    if let Expr::Variable(ref name) = &**right {
                        if name == "adr" {
                            self.compile_address(left, reg);
                            return;
                        }
                    }
                }

                self.compile_expr(left, 0, false);
                self.code.push(0x50);
                self.compile_expr(right, 0, false);
                self.code.extend_from_slice(&[0x48, 0x89, 0xC3]);
                self.code.push(0x58);

                match op.as_str() {
                    "OpAdd" => {
                        self.code.extend_from_slice(&[0x48, 0x01, 0xD8]);
                    }
                    "OpSub" => {
                        self.code.extend_from_slice(&[0x48, 0x29, 0xD8]);
                    }
                    "OpMul" => {
                        self.code.extend_from_slice(&[0x48, 0x0F, 0xAF, 0xC3]);
                    }
                    "OpDiv" => {
                        // xor rdx, rdx (48 31 D2)
                        // div rbx (48 F7 F3) -> частное в rax
                        self.code
                            .extend_from_slice(&[0x48, 0x31, 0xD2, 0x48, 0xF7, 0xF3]);
                    }
                    "OpMod" => {
                        // xor rdx, rdx (48 31 D2)
                        // div rbx (48 F7 F3) -> остаток в rdx
                        // mov rax, rdx (48 89 D0) -> переносим остаток в rax
                        self.code.extend_from_slice(&[
                            0x48, 0x31, 0xD2, 0x48, 0xF7, 0xF3, 0x48, 0x89, 0xD0,
                        ]);
                    }
                    _ => {}
                }
            }
            Expr::Call { name, args } => {
                if name == "mloc" {
                    // Поддержка как mloc(owner, size), так и mloc(size)
                    let size_expr_opt = if args.len() >= 2 {
                        args.get(1)
                    } else {
                        args.get(0)
                    };

                    if let Some(size_expr) = size_expr_opt {
                        self.compile_expr(size_expr, 0, true); // ...
                        self.code.extend_from_slice(&[0x48, 0x83, 0xC0, 0x08]);
                        self.code.extend_from_slice(&[0x48, 0x89, 0xC6]);
                    } else {
                        self.code
                            .extend_from_slice(&[0x48, 0xC7, 0xC6, 0x08, 0x10, 0x00, 0x00]);
                    }

                    self.code.push(0x56);

                    self.code.extend_from_slice(&[
                        0x48, 0x31, 0xFF, 0x48, 0xC7, 0xC2, 0x03, 0x00, 0x00, 0x00, 0x49, 0xC7,
                        0xC2, 0x22, 0x00, 0x00, 0x00, 0x49, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF,
                        0x4D, 0x31, 0xC9, 0x48, 0xC7, 0xC0, 0x09, 0x00, 0x00, 0x00, 0x0F, 0x05,
                    ]);

                    self.code.push(0x59);
                    self.code.extend_from_slice(&[0x48, 0x89, 0x08]);
                    self.code.extend_from_slice(&[0x48, 0x83, 0xC0, 0x08]);
                } else if name == "bmloc" {
                    if let Some(addr_expr) = args.first() {
                        self.compile_expr(addr_expr, 0, true);
                    } else {
                        self.code.extend_from_slice(&[0x48, 0x31, 0xC0]);
                    }
                } else if name == "mfree" {
                    if let Some(ptr_expr) = args.first() {
                        self.compile_expr(ptr_expr, 0, true);
                    }
                    self.code.extend_from_slice(&[
                        0x48, 0x83, 0xE8, 0x08, 0x48, 0x8B, 0x30, 0x48, 0x89, 0xC7, 0x48, 0xC7,
                        0xC0, 0x0B, 0x00, 0x00, 0x00, 0x0F, 0x05,
                    ]);
                } else if name == "sys_write" {
                    if let Some(fd_expr) = args.first() {
                        self.compile_expr(fd_expr, 7, true);
                    } // rdi (7)
                    if let Some(buf_expr) = args.get(1) {
                        self.compile_expr(buf_expr, 6, true);
                    } // rsi (6)
                    if let Some(size_expr) = args.get(2) {
                        self.compile_expr(size_expr, 2, true);
                    } // rdx (2)
                    self.code.extend_from_slice(&[
                        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1 (sys_write)
                        0x0F, 0x05, // syscall
                    ]);
                } else if name == "sys_read" {
                    if let Some(fd_expr) = args.first() {
                        self.compile_expr(fd_expr, 7, true);
                    } // rdi (7)
                    if let Some(buf_expr) = args.get(1) {
                        self.compile_expr(buf_expr, 6, true);
                    } // rsi (6)
                    if let Some(size_expr) = args.get(2) {
                        self.compile_expr(size_expr, 2, true);
                    } // rdx (2)
                    self.code.extend_from_slice(&[
                        0x48, 0x31, 0xC0, // xor rax, rax (sys_read)
                        0x0F, 0x05, // syscall
                    ]);
                } else if name == "sys_open" {
                    if let Some(path_expr) = args.first() {
                        self.compile_expr(path_expr, 7, true);
                    } // rdi (7)
                    if let Some(flags_expr) = args.get(1) {
                        self.compile_expr(flags_expr, 6, true);
                    } // rsi (6)
                    if let Some(mode_expr) = args.get(2) {
                        self.compile_expr(mode_expr, 2, true);
                    } // rdx (2)
                    self.code.extend_from_slice(&[
                        0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00, // mov rax, 2 (sys_open)
                        0x0F, 0x05, // syscall
                    ]);
                } else if name == "sys_close" {
                    if let Some(fd_expr) = args.first() {
                        self.compile_expr(fd_expr, 7, true);
                    } // rdi (7)
                    self.code.extend_from_slice(&[
                        0x48, 0xC7, 0xC0, 0x03, 0x00, 0x00, 0x00, // mov rax, 3 (sys_close)
                        0x0F, 0x05, // syscall
                    ]);
                } else if name == "sys_unlink" {
                    if let Some(path_expr) = args.first() {
                        self.compile_expr(path_expr, 7, true);
                    } // rdi (7)
                    self.code.extend_from_slice(&[
                        0x48, 0xC7, 0xC0, 0x57, 0x00, 0x00, 0x00, // mov rax, 87 (sys_unlink)
                        0x0F, 0x05, // syscall
                    ]);
                } else if name == "sys_ioctl" {
                    if let Some(fd_expr) = args.first() {
                        self.compile_expr(fd_expr, 7, true);
                    } // rdi (7)
                    if let Some(req_expr) = args.get(1) {
                        self.compile_expr(req_expr, 6, true);
                    } // rsi (6)
                    if let Some(arg_expr) = args.get(2) {
                        self.compile_expr(arg_expr, 2, true);
                    } // rdx (2)
                    self.code.extend_from_slice(&[
                        0x48, 0xC7, 0xC0, 0x10, 0x00, 0x00, 0x00, // mov rax, 16 (sys_ioctl)
                        0x0F, 0x05, // syscall
                    ]);
                } else if name == "sys_exit" {
                    if let Some(code_expr) = args.first() {
                        self.compile_expr(code_expr, 7, true);
                    } // rdi (7)
                    self.code.extend_from_slice(&[
                        0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, // mov rax, 60 (sys_exit)
                        0x0F, 0x05, // syscall
                    ]);
                } else if name == "inb" {
                    if let Some(port_expr) = args.first() {
                        self.compile_expr(port_expr, 1, true); // RCX
                    }
                    self.code
                        .extend_from_slice(&[0x66, 0x89, 0xCA, 0xEC, 0x48, 0x0F, 0xB6, 0xC0]);
                } else if name == "outb" {
                    if let Some(port_expr) = args.first() {
                        self.compile_expr(port_expr, 1, true); // RCX
                    }
                    if let Some(val_expr) = args.get(1) {
                        self.compile_expr(val_expr, 0, true); // RAX
                    }
                    self.code.extend_from_slice(&[0x66, 0x89, 0xCA, 0xEE]);
                } else if name == "inw" {
                    if let Some(port_expr) = args.first() {
                        self.compile_expr(port_expr, 1, true); // RCX
                    }
                    self.code
                        .extend_from_slice(&[0x66, 0x89, 0xCA, 0x66, 0xED, 0x48, 0x0F, 0xB7, 0xC0]);
                } else if name == "outw" {
                    if let Some(port_expr) = args.first() {
                        self.compile_expr(port_expr, 1, true); // RCX
                    }
                    if let Some(val_expr) = args.get(1) {
                        self.compile_expr(val_expr, 0, true); // RAX
                    }
                    self.code.extend_from_slice(&[0x66, 0x89, 0xCA, 0x66, 0xEF]);
                } else if name == "inl" {
                    if let Some(port_expr) = args.first() {
                        self.compile_expr(port_expr, 1, true); // RCX
                    }
                    self.code.extend_from_slice(&[0x66, 0x89, 0xCA, 0xED]);
                } else if name == "outl" {
                    if let Some(port_expr) = args.first() {
                        self.compile_expr(port_expr, 1, true); // RCX
                    }
                    if let Some(val_expr) = args.get(1) {
                        self.compile_expr(val_expr, 0, true); // RAX
                    }
                    self.code.extend_from_slice(&[0x66, 0x89, 0xCA, 0xEF]);
                } else {
                    let arg_registers_out = [
                        &[0x48, 0x89, 0xC7][..],
                        &[0x48, 0x89, 0xC6][..],
                        &[0x48, 0x89, 0xC2][..],
                        &[0x48, 0x89, 0xC1][..],
                    ];

                    let param_types = self.function_signatures.get(name).cloned();

                    for (idx, arg_expr) in args.iter().enumerate() {
                        let mut deref_ptr_arg = true;
                        if let Some(ref types) = param_types {
                            if let Some(param_type) = types.get(idx) {
                                if let DataType::Pointer(_) = param_type {
                                    deref_ptr_arg = false;
                                }
                            }
                        }

                        self.compile_expr(arg_expr, 0, deref_ptr_arg);
                        if idx < arg_registers_out.len() {
                            self.code.extend_from_slice(arg_registers_out[idx]);
                        }
                    }

                    self.code.push(0xE8);
                    let patch_pos = self.code.len();
                    self.code.extend_from_slice(&[0, 0, 0, 0]);
                    self.call_patches.push((patch_pos, name.clone()));
                }
            }
            _ => {}
        }

        // Пост-обработка: гарантируем, что результат находится в требуемом `reg`
        if reg != 0 {
            match expr {
                Expr::Variable(_) | Expr::Number(_) | Expr::StringLit(_) | Expr::AddrOf(_) => {
                    // Эти выражения уже генерируют код напрямую в `reg`, ничего делать не нужно
                }
                Expr::MemberAccess { .. } | Expr::Index { .. } | Expr::SectionAccess { .. } => {
                    // Они вычисляются в RBX (3). Переносим в reg, если reg != 3
                    if reg != 3 {
                        let modrm = 0xC0 | (3 << 3) | reg;
                        self.code.extend_from_slice(&[0x48, 0x89, modrm]); // mov reg, rbx
                    }
                }
                Expr::Binary { .. } | Expr::Call { .. } => {
                    // Они вычисляются в RAX (0). Переносим в reg
                    let modrm = 0xC0 | (0 << 3) | reg;
                    self.code.extend_from_slice(&[0x48, 0x89, modrm]); // mov reg, rax
                }
                _ => {} // Catch-all для всех остальных выражений, не генерирующих код (например, Null)
            }
        }
    }

    fn compile_address(&mut self, expr: &Expr, reg: u8) {
        // Промежуточные шаги компиляции адреса принудительно направляем на RAX (0) или RBX (3)
        let internal_reg = if reg == 0 { 0 } else { 3 };

        match expr {
            Expr::Variable(name) => {
                if let Some(&offset) = self.local_offsets.get(name) {
                    let reg_opcode = internal_reg;

                    let modifier = self
                        .local_access
                        .get(name)
                        .cloned()
                        .unwrap_or(PtrAccess::Normal);

                    // Фикс: Если тип переменной является указателем (Pointer),
                    // то ее базовым адресом является значение, хранящееся в ней (mov),
                    // а не адрес ячейки на стеке (lea).
                    let mut is_pointer = false;
                    if let Some(dt) = self.local_types.get(name) {
                        match dt {
                            DataType::Pointer(_) => {
                                is_pointer = true;
                            }
                            _ => {}
                        }
                    }

                    if modifier == PtrAccess::Input || modifier == PtrAccess::Output || is_pointer {
                        self.emit_mem_op(0x8B, reg_opcode, offset); // mov (0x8B)
                    } else {
                        self.emit_mem_op(0x8D, reg_opcode, offset); // lea (0x8D)
                    }
                } else {
                    // ФОЛБЕК НА ГЛOБАЛЬНЫЕ ПЕРЕМЕННЫЕ СЕКЦИЙ (args:x_val и т.д.)
                    let mut found_key = None;
                    for key in self.global_offsets.keys() {
                        if key.ends_with(&format!(":{}", name)) || key == name {
                            found_key = Some(key.clone());
                            break;
                        }
                    }
                    if let Some(key) = found_key {
                        let parts: Vec<&str> = key.split(':').collect();
                        let section = parts[0].to_string();
                        let variable = parts[1].to_string();
                        self.compile_address(
                            &Expr::SectionAccess { section, variable },
                            internal_reg,
                        );
                    }
                }
            }
            Expr::Index {
                expr: base_expr,
                index,
            } => {
                let mut elem_size = 8u32;
                if let Expr::Variable(ref name) = &**base_expr {
                    if let Some(dt) = self.local_types.get(name) {
                        match dt {
                            DataType::Array(elem, _) => {
                                elem_size = self.get_type_size_internal(elem);
                            }
                            DataType::Pointer(elem) => {
                                elem_size = self.get_type_size_internal(elem);
                            }
                            DataType::Struct(struct_name) => {
                                if let Some(DataType::Array(elem, _)) =
                                    self.typedefs_map.get(struct_name)
                                {
                                    elem_size = self.get_type_size_internal(elem);
                                }
                            }
                            _ => {}
                        }
                    }
                }

                self.compile_address(base_expr, 3); // Принудительно вычисляем базу в RBX
                self.code.push(0x53); // push rbx

                self.compile_expr(index, 0, true);

                if elem_size == 8 {
                    self.code.extend_from_slice(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3 (умножение на 8)
                } else if elem_size == 4 {
                    self.code.extend_from_slice(&[0x48, 0xC1, 0xE0, 0x02]); // shl rax, 2 (умножение на 4)
                } else if elem_size == 2 {
                    self.code.extend_from_slice(&[0x48, 0xC1, 0xE0, 0x01]); // shl rax, 1 (умножение на 2)
                }

                self.code.push(0x5B); // pop rbx
                self.code.extend_from_slice(&[0x48, 0x01, 0xC3]);

                if internal_reg == 0 {
                    self.code.extend_from_slice(&[0x48, 0x89, 0xD8]); // mov rax, rbx
                }
            }
            Expr::MemberAccess {
                expr: base_expr,
                member,
                is_arrow,
            } => {
                self.compile_address(base_expr, internal_reg);

                if *is_arrow {
                    let deref_op = if internal_reg == 0 {
                        &[0x48, 0x8B, 0x00][..]
                    } else {
                        &[0x48, 0x8B, 0x1B][..]
                    };
                    self.code.extend_from_slice(deref_op);
                }

                let mut struct_name = String::new();
                if let Expr::Variable(base_var) = &**base_expr {
                    if let Some(dt) = self.local_types.get(base_var) {
                        struct_name = match dt {
                            DataType::Struct(n) => n.clone(),
                            DataType::Pointer(boxed) => match &**boxed {
                                DataType::Struct(n) => n.clone(),
                                _ => String::new(),
                            },
                            _ => String::new(),
                        };
                    }
                }

                if let Some((_, fields)) = self.struct_layouts.get(&struct_name) {
                    if let Some(&field_offset) = fields.get(member) {
                        let add_opcode = if internal_reg == 0 { 0xC0 } else { 0xC3 };
                        self.code.extend_from_slice(&[0x48, 0x81, add_opcode]);
                        self.code.extend_from_slice(&field_offset.to_le_bytes());
                    }
                }
            }
            _ => {}
        }

        // Копируем итоговый вычисленный адрес в запрашиваемый целевой регистр
        if reg != 0 && reg != 3 {
            let modrm = 0xC0 | (3 << 3) | reg;
            self.code.extend_from_slice(&[0x48, 0x89, modrm]); // mov reg, rbx
        }
    }

    fn compile_condition_helper(&mut self, cond: &Expr) -> u8 {
        if let Expr::Binary { left, op, right } = cond {
            self.compile_expr(left, 0, true);
            self.code.push(0x50);
            self.compile_expr(right, 0, true);
            self.code.extend_from_slice(&[0x48, 0x89, 0xC3]);
            self.code.push(0x58);
            self.code.extend_from_slice(&[0x48, 0x39, 0xD8]);

            match op.as_str() {
                "OpEq" | "OpEqEq" | "==" => 0x85,
                "OpNotEq" | "OpNe" | "!=" => 0x84,
                "OpLt" | "Lt" | "<" => 0x8D,
                "OpLtEq" | "OpLe" | "OpLessEq" | "<=" => 0x8F,
                "OpGt" | "Gt" | ">" => 0x8E,
                "OpGtEq" | "OpGe" | "OpGreaterEq" | ">=" => 0x8C,
                _ => 0x85,
            }
        } else {
            self.compile_expr(cond, 0, true);
            self.code.extend_from_slice(&[0x48, 0x85, 0xC0]);
            0x84
        }
    }

    fn patch_address(&mut self, pos: usize, offset: i32) {
        let bytes = offset.to_le_bytes();
        self.code[pos] = bytes[0];
        self.code[pos + 1] = bytes[1];
        self.code[pos + 2] = bytes[2];
        self.code[pos + 3] = bytes[3];
    }

    fn store_assignment_target_from_rax(&mut self, target: &Expr) {
        match target {
            Expr::Variable(name) => {
                let modifier = self
                    .local_access
                    .get(name)
                    .cloned()
                    .unwrap_or(PtrAccess::Normal);

                if modifier == PtrAccess::Output {
                    if let Some(&offset) = self.local_offsets.get(name) {
                        self.emit_mem_load(3, offset, 8);
                    }

                    // Вычисляем размер базового типа указателя для предотвращения перезаписи памяти
                    let mut elem_size = 8;
                    if let Some(DataType::Pointer(ref boxed)) = self.local_types.get(name) {
                        elem_size = self.get_type_size_internal(boxed);
                    }

                    match elem_size {
                        1 => self.code.extend_from_slice(&[0x88, 0x03]), // mov [rbx], al
                        2 => self.code.extend_from_slice(&[0x66, 0x89, 0x03]), // mov [rbx], ax
                        4 => self.code.extend_from_slice(&[0x89, 0x03]), // mov [rbx], eax
                        _ => self.code.extend_from_slice(&[0x48, 0x89, 0x03]), // mov [rbx], rax
                    }
                } else if let Some(&offset) = self.local_offsets.get(name) {
                    let var_size = self.get_type_size_internal(
                        self.local_types.get(name).unwrap_or(&DataType::U64),
                    );
                    self.emit_mem_store(0, offset, var_size);
                } else {
                    let mut found_key = None;
                    for key in self.global_offsets.keys() {
                        if key.ends_with(&format!(":{}", name)) || key == name {
                            found_key = Some(key.clone());
                            break;
                        }
                    }
                    if let Some(key) = found_key {
                        let parts: Vec<&str> = key.split(':').collect();
                        let section = parts[0].to_string();
                        let variable = parts[1].to_string();
                        self.store_assignment_target_from_rax(&Expr::SectionAccess {
                            section,
                            variable,
                        });
                    }
                }
            }
            Expr::SectionAccess { section, variable } => {
                let key = format!("{}:{}", section, variable);
                self.code.extend_from_slice(&[0x48, 0xBB]); // mov rbx, imm64
                let patch_pos = self.code.len();
                self.code.extend_from_slice(&[0; 8]);
                self.address_patches.push((patch_pos, key));

                self.code.extend_from_slice(&[0x48, 0x89, 0x03]); // mov [rbx], rax
            }
            Expr::MemberAccess { .. } | Expr::Index { .. } => {
                self.code.push(0x50); // push rax
                self.compile_address(target, 1); // вычисляем адрес в RBX
                self.code.push(0x58); // pop rax

                let target_size = self.get_expr_type_size(target);
                match target_size {
                    1 => {
                        self.code.extend_from_slice(&[0x88, 0x03]); // mov [rbx], al
                    }
                    2 => {
                        self.code.extend_from_slice(&[0x66, 0x89, 0x03]); // mov [rbx], ax
                    }
                    4 => {
                        self.code.extend_from_slice(&[0x89, 0x03]); // mov [rbx], eax
                    }
                    _ => {
                        self.code.extend_from_slice(&[0x48, 0x89, 0x03]); // mov [rbx], rax
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn generate_elf64_binary(
    payload_bytes: &[u8],
    program: &Program,
    gen: &NativeGenerator,
) -> Vec<u8> {
    let mut p46_strtab = Vec::new();
    p46_strtab.push(0);
    let mut str_offsets = HashMap::new();

    let mut add_str = |s: &str| -> u32 {
        if let Some(&off) = str_offsets.get(s) {
            return off;
        }
        let off = p46_strtab.len() as u32;
        p46_strtab.extend_from_slice(s.as_bytes());
        p46_strtab.push(0);
        str_offsets.insert(s.to_string(), off);
        off
    };

    let mut p46_types = Vec::new();

    for s in &program.structs {
        let name_off = add_str(&s.name);

        let mut fields_data = Vec::new();
        for field in &s.fields {
            let f_name_off = add_str(&field.name);
            fields_data.extend_from_slice(&f_name_off.to_le_bytes());

            let type_id = match &field.data_type {
                DataType::U64 => 4u32,
                DataType::U32 => 3u32,
                DataType::Array(..) => 6u32,
                DataType::Typedef(..) => 9u32,
                _ => 11u32,
            };
            fields_data.extend_from_slice(&type_id.to_le_bytes());
            fields_data.extend_from_slice(&0u32.to_le_bytes());
            fields_data.extend_from_slice(&field.version_added.to_le_bytes());
            fields_data.extend_from_slice(&field.version_removed.to_le_bytes());
        }

        let mut val = Vec::new();
        val.extend_from_slice(&name_off.to_le_bytes());
        val.extend_from_slice(&s.version.to_le_bytes());
        val.extend_from_slice(&16u32.to_le_bytes());
        val.extend_from_slice(&(s.fields.len() as u32).to_le_bytes());
        val.extend(fields_data);

        p46_types.extend_from_slice(&1u16.to_le_bytes());
        p46_types.extend_from_slice(&(val.len() as u32).to_le_bytes());
        p46_types.extend(val);
    }

    for (name, dt) in &program.typedefs {
        let alias_off = add_str(name);

        let underlying_id = match dt {
            DataType::Array(..) => 6u32,
            DataType::U64 => 4u32,
            _ => 11u32,
        };

        let mut val = Vec::new();
        val.extend_from_slice(&alias_off.to_le_bytes());
        val.extend_from_slice(&underlying_id.to_le_bytes());

        p46_types.extend_from_slice(&9u16.to_le_bytes());
        p46_types.extend_from_slice(&(val.len() as u32).to_le_bytes());
        p46_types.extend(val);
    }

    let mut p46_exports = Vec::new();
    p46_exports.extend_from_slice(&(program.functions.len() as u32).to_le_bytes());
    for (idx, func) in program.functions.iter().enumerate() {
        let name_off = add_str(&func.name);
        let mod_off = add_str("main_module");
        p46_exports.extend_from_slice(&name_off.to_le_bytes());
        p46_exports.extend_from_slice(&mod_off.to_le_bytes());
        p46_exports.extend_from_slice(&1u32.to_le_bytes());
        p46_exports.push(1);

        let local_offset = gen.function_offsets.get(&func.name).cloned().unwrap_or(0);
        let abs_addr = 0x400078u64 + (local_offset as u64);
        p46_exports.extend_from_slice(&abs_addr.to_le_bytes());
        p46_exports.extend_from_slice(&(idx as u32).to_le_bytes());

        p46_exports.extend_from_slice(&(func.params.len() as u32).to_le_bytes());
        p46_exports.extend_from_slice(&4u32.to_le_bytes());
        for _ in &func.params {
            p46_exports.extend_from_slice(&4u32.to_le_bytes());
        }
    }

    let mut p46_imports = Vec::new();
    p46_imports.extend_from_slice(&0u32.to_le_bytes());

    let mut p46_deps = Vec::new();
    p46_deps.extend_from_slice(&(program.imports.len() as u32).to_le_bytes());
    for imp in &program.imports {
        let name_off = add_str(imp);
        p46_deps.extend_from_slice(&name_off.to_le_bytes());
        p46_deps.extend_from_slice(&1u32.to_le_bytes());
        p46_deps.extend_from_slice(&0u32.to_le_bytes());
        p46_deps.extend_from_slice(&0u32.to_le_bytes());
        p46_deps.extend_from_slice(&0u32.to_le_bytes());
    }

    let mut p46_reflect = Vec::new();
    p46_reflect.extend_from_slice(&0u32.to_le_bytes());

    let text_offset = 120usize;
    let text_size = payload_bytes.len();

    let p46_hdr_offset = text_offset + text_size;
    let p46_hdr_size = 24usize;

    let p46_types_offset = p46_hdr_offset + p46_hdr_size; // Фикс: p46_hdr_size вместо p46_types_size
    let p46_types_size = p46_types.len();

    let p46_exp_offset = p46_types_offset + p46_types_size;
    let p46_exp_size = p46_exports.len();

    let p46_imp_offset = p46_exp_offset + p46_exp_size;
    let p46_imp_size = p46_imports.len();

    let p46_deps_offset = p46_imp_offset + p46_imp_size;
    let p46_deps_size = p46_deps.len();

    let p46_refl_offset = p46_deps_offset + p46_deps_size;
    let p46_refl_size = p46_reflect.len();

    let p46_strtab_offset = p46_refl_offset + p46_refl_size;
    let p46_strtab_size = p46_strtab.len();

    let mut shstrtab = Vec::new();
    shstrtab.push(0);
    let n_text = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".text\0");
    let n_p46_hdr = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".p46_header\0");
    let n_p46_typ = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".p46_types\0");
    let n_p46_exp = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".p46_exports\0");
    let n_p46_imp = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".p46_imports\0");
    let n_p46_dep = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".p46_deps\0");
    let n_p46_ref = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".p46_reflect\0");
    let n_shstr = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".shstrtab\0");
    let n_p46_str = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".p46_strtab\0");

    let shstrtab_offset = p46_strtab_offset + p46_strtab_size;
    let shstrtab_size = shstrtab.len();

    let sht_offset = shstrtab_offset + shstrtab_size;

    let mut p46_header = Vec::new();
    p46_header.extend_from_slice(&[0x50, 0x34, 0x36, 0x00]);
    p46_header.push(1);
    p46_header.push(5);
    p46_header.push(0);
    p46_header.push(1);
    p46_header.push(8);
    p46_header.extend_from_slice(&[0, 0, 0]);
    p46_header.extend_from_slice(&5u32.to_le_bytes());
    p46_header.extend_from_slice(&(p46_strtab_offset as u32).to_le_bytes());
    p46_header.extend_from_slice(&(p46_strtab_size as u32).to_le_bytes());

    let mut elf = Vec::new();

    elf.extend_from_slice(&[0x7F, b'E', b'L', b'F']);
    elf.push(2);
    elf.push(1);
    elf.push(1);
    elf.push(0);
    elf.extend_from_slice(&[0; 8]);

    elf.extend_from_slice(&2u16.to_le_bytes());
    elf.extend_from_slice(&62u16.to_le_bytes());
    elf.extend_from_slice(&1u32.to_le_bytes());
    elf.extend_from_slice(&0x400078u64.to_le_bytes());
    elf.extend_from_slice(&64u64.to_le_bytes());
    elf.extend_from_slice(&(sht_offset as u64).to_le_bytes());
    elf.extend_from_slice(&0u32.to_le_bytes());
    elf.extend_from_slice(&64u16.to_le_bytes());
    elf.extend_from_slice(&56u16.to_le_bytes());
    elf.extend_from_slice(&1u16.to_le_bytes());
    elf.extend_from_slice(&64u16.to_le_bytes());
    elf.extend_from_slice(&10u16.to_le_bytes());
    elf.extend_from_slice(&8u16.to_le_bytes());

    let total_file_size = (sht_offset + 10 * 64) as u64;
    elf.extend_from_slice(&1u32.to_le_bytes());
    elf.extend_from_slice(&7u32.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0x400000u64.to_le_bytes());
    elf.extend_from_slice(&0x400000u64.to_le_bytes());
    elf.extend_from_slice(&total_file_size.to_le_bytes());
    elf.extend_from_slice(&total_file_size.to_le_bytes());
    elf.extend_from_slice(&0x1000u64.to_le_bytes());

    elf.extend_from_slice(payload_bytes);
    elf.extend_from_slice(&p46_header);
    elf.extend_from_slice(&p46_types);
    elf.extend_from_slice(&p46_exports);
    elf.extend_from_slice(&p46_imports);
    elf.extend_from_slice(&p46_deps);
    elf.extend_from_slice(&p46_reflect);
    elf.extend_from_slice(&p46_strtab);
    elf.extend_from_slice(&shstrtab);

    let build_shdr =
        |name: u32, ty: u32, flags: u64, addr: u64, offset: u64, size: u64| -> Vec<u8> {
            let mut shdr = Vec::new();
            shdr.extend_from_slice(&name.to_le_bytes());
            shdr.extend_from_slice(&type_id_helper(ty).to_le_bytes());
            shdr.extend_from_slice(&flags.to_le_bytes());
            shdr.extend_from_slice(&addr.to_le_bytes());
            shdr.extend_from_slice(&offset.to_le_bytes());
            shdr.extend_from_slice(&size.to_le_bytes());
            shdr.extend_from_slice(&0u32.to_le_bytes());
            shdr.extend_from_slice(&0u32.to_le_bytes());
            shdr.extend_from_slice(&8u64.to_le_bytes());
            shdr.extend_from_slice(&0u64.to_le_bytes());
            shdr
        };

    fn type_id_helper(ty: u32) -> u32 {
        ty
    }

    elf.extend(build_shdr(0, 0, 0, 0, 0, 0));
    elf.extend(build_shdr(
        n_text,
        1,
        7,
        0x400078,
        text_offset as u64,
        text_size as u64,
    ));
    elf.extend(build_shdr(
        n_p46_hdr,
        1,
        2,
        0,
        p46_hdr_offset as u64,
        p46_hdr_size as u64,
    ));
    elf.extend(build_shdr(
        n_p46_typ,
        1,
        2,
        0,
        p46_types_offset as u64,
        p46_types_size as u64,
    ));
    elf.extend(build_shdr(
        n_p46_exp,
        1,
        2,
        0,
        p46_exp_offset as u64,
        p46_exp_size as u64,
    ));
    elf.extend(build_shdr(
        n_p46_imp,
        1,
        2,
        0,
        p46_imp_offset as u64,
        p46_imp_size as u64,
    ));
    elf.extend(build_shdr(
        n_p46_dep,
        1,
        2,
        0,
        p46_deps_offset as u64,
        p46_deps_size as u64,
    ));
    elf.extend(build_shdr(
        n_p46_ref,
        1,
        2,
        0,
        p46_refl_offset as u64,
        p46_refl_size as u64,
    ));
    elf.extend(build_shdr(
        n_shstr,
        3,
        0,
        0,
        shstrtab_offset as u64,
        shstrtab_size as u64,
    ));
    elf.extend(build_shdr(
        n_p46_str,
        3,
        0,
        0,
        p46_strtab_offset as u64,
        p46_strtab_size as u64,
    ));

    elf
}
