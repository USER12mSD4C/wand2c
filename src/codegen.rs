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
    float_constants: HashMap<String, u32>,
    global_data_start_offset: usize,

    address_patches: Vec<(usize, String)>,

    function_signatures: HashMap<String, Vec<DataType>>,

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
            float_constants: HashMap::new(),
            global_data_start_offset: 0,
            address_patches: Vec::new(),
            function_signatures: HashMap::new(),
            struct_fields: HashMap::new(),
        }
    }

    /// Рекурсивное вычисление типа любого сложного выражения
    fn resolve_expr_type(&self, expr: &Expr) -> Option<DataType> {
        match expr {
            Expr::Variable(name) => self.local_types.get(name).cloned(),
            Expr::MemberAccess {
                expr: base_expr,
                member,
                ..
            } => {
                let base_type = self.resolve_expr_type(base_expr)?;
                let struct_name = match base_type {
                    DataType::Struct(n) => n,
                    DataType::Pointer(boxed) => match *boxed {
                        DataType::Struct(n) => n,
                        _ => return None,
                    },
                    _ => return None,
                };
                self.struct_fields.get(&struct_name)?.get(member).cloned()
            }
            Expr::Index {
                expr: base_expr, ..
            } => {
                let base_type = self.resolve_expr_type(base_expr)?;
                match base_type {
                    DataType::Array(elem, _) => Some(*elem),
                    DataType::Pointer(elem) => Some(*elem),
                    DataType::Typedef(name, _) => {
                        if let Some(DataType::Array(elem, _)) = self.typedefs_map.get(&name) {
                            Some(*elem.clone())
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn can_resolve_type(&self, dt: &DataType) -> bool {
        match dt {
            DataType::Struct(name) => {
                self.struct_layouts.contains_key(name) || self.typedefs_map.contains_key(name)
            }
            DataType::Array(elem, _) => self.can_resolve_type(elem),
            DataType::Pointer(_) => true,
            DataType::Typedef(_, underlying) => self.can_resolve_type(underlying),
            _ => true,
        }
    }

    fn calculate_and_insert_layout(&mut self, s: &StructDecl) {
        let mut fields_offsets = HashMap::new();
        let mut current_offset = 0;
        let mut max_alignment = 1;

        if s.is_union {
            let mut max_size = 0;
            for field in &s.fields {
                fields_offsets.insert(field.name.clone(), 0);
                let size = self.get_type_size_internal(&field.data_type);
                if size > max_size {
                    max_size = size;
                }
            }
            self.struct_layouts
                .insert(s.name.clone(), (max_size, fields_offsets));
            return;
        }

        for field in &s.fields {
            let size = self.get_type_size_internal(&field.data_type);
            // Если структура упакована, выравнивание равно 1
            let mut alignment = if s.is_packed { 1 } else { size }; // <--- Изменено!
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

    pub fn compile_program(&mut self, program: &Program) -> Vec<u8> {
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
        self.float_constants.clear();
        self.address_patches.clear();
        self.struct_layouts.clear();
        self.typedefs_map.clear();

        for (name, dt) in &program.typedefs {
            self.typedefs_map.insert(name.clone(), dt.clone());
        }

        self.function_signatures.clear();
        for func in &program.functions {
            let param_types = func.params.iter().map(|(dt, _, _)| dt.clone()).collect();
            self.function_signatures
                .insert(func.name.clone(), param_types);
        }

        self.struct_fields.clear();
        for s in &program.structs {
            let mut fields_types = HashMap::new();
            for field in &s.fields {
                fields_types.insert(field.name.clone(), field.data_type.clone());
            }
            self.struct_fields.insert(s.name.clone(), fields_types);
        }

        let mut resolved_any = true;
        while resolved_any {
            resolved_any = false;
            for s in &program.structs {
                if self.struct_layouts.contains_key(&s.name) {
                    continue;
                }

                let mut can_resolve = true;
                for field in &s.fields {
                    if !self.can_resolve_type(&field.data_type) {
                        can_resolve = false;
                        break;
                    }
                }

                if can_resolve {
                    self.calculate_and_insert_layout(s);
                    resolved_any = true;
                }
            }
        }

        for s in &program.structs {
            if !self.struct_layouts.contains_key(&s.name) {
                self.calculate_and_insert_layout(s);
            }
        }

        self.collect_string_constants_from_program(program);

        let mut global_data_bytes = Vec::new();
        for sect in &program.sections {
            for var in &sect.variables {
                let key = format!("{}:{}", sect.name, var.name);
                self.global_offsets
                    .insert(key, global_data_bytes.len() as u32);

                let var_size = self.get_type_size_internal(&var.data_type);

                let init_val = match &var.initial_value {
                    Some(expr) => match &**expr {
                        Expr::Number(n) => *n,
                        _ => 0,
                    },
                    None => 0,
                };

                let init_bytes = init_val.to_le_bytes();
                for i in 0..(var_size as usize) {
                    if i < init_bytes.len() {
                        global_data_bytes.push(init_bytes[i]);
                    } else {
                        global_data_bytes.push(0);
                    }
                }
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

        self.collect_float_constants_from_program(program);
        let mut float_offsets = HashMap::new();
        for (f_const, _) in &self.float_constants {
            let off = global_data_bytes.len() as u32;
            let val = f_const.parse::<f64>().unwrap_or(0.0);
            global_data_bytes.extend_from_slice(&val.to_bits().to_le_bytes());
            float_offsets.insert(f_const.clone(), off);
        }
        for (f_const, off) in &float_offsets {
            self.global_offsets
                .insert(format!("float:{}", f_const), *off);
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

    fn is_float_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::FloatLit(_) => true,
            Expr::Variable(name) => {
                if let Some(dt) = self.local_types.get(name) {
                    matches!(dt, DataType::F64)
                } else {
                    false
                }
            }
            Expr::Binary { left, .. } => self.is_float_expr(left),
            Expr::Call { name, .. } => {
                name == "sin" || name == "cos" || name == "tan" || name == "sqrt"
            }
            _ => false,
        }
    }

    fn collect_float_constants_from_program(&mut self, program: &Program) {
        for func in &program.functions {
            if let Some(body) = &func.body {
                for stmt in body {
                    self.collect_float_constants_from_stmt(stmt);
                }
            }
        }
    }

    fn collect_float_constants_from_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDefinition(decl) => {
                if let Some(ref init) = decl.initial_value {
                    self.collect_float_constants_from_expr(init);
                }
            }
            Stmt::Assignment { targets, value } => {
                for target in targets {
                    self.collect_float_constants_from_expr(target);
                }
                self.collect_float_constants_from_expr(value);
            }
            Stmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.collect_float_constants_from_expr(cond);
                for s in then_branch {
                    self.collect_float_constants_from_stmt(s);
                }
                if let Some(else_stmts) = else_branch {
                    for s in else_stmts {
                        self.collect_float_constants_from_stmt(s);
                    }
                }
            }
            Stmt::While { cond, body } => {
                self.collect_float_constants_from_expr(cond);
                for s in body {
                    self.collect_float_constants_from_stmt(s);
                }
            }
            Stmt::For {
                init,
                cond,
                post,
                body,
            } => {
                if let Some(i) = init {
                    self.collect_float_constants_from_stmt(i);
                }
                self.collect_float_constants_from_expr(cond);
                if let Some(p) = post {
                    self.collect_float_constants_from_stmt(p);
                }
                for s in body {
                    self.collect_float_constants_from_stmt(s);
                }
            }
            Stmt::Return(values) => {
                for (_, expr) in values {
                    self.collect_float_constants_from_expr(expr);
                }
            }
            Stmt::Expr(expr) => {
                self.collect_float_constants_from_expr(expr);
            }
            _ => {}
        }
    }

    fn collect_float_constants_from_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::FloatLit(s) => {
                self.float_constants.insert(s.clone(), 0);
            }
            Expr::Binary { left, right, .. } => {
                self.collect_float_constants_from_expr(left);
                self.collect_float_constants_from_expr(right);
            }
            Expr::Call { args, .. } => {
                for arg in args {
                    self.collect_float_constants_from_expr(arg);
                }
            }
            Expr::Index { expr: base, index } => {
                self.collect_float_constants_from_expr(base);
                self.collect_float_constants_from_expr(index);
            }
            Expr::MemberAccess { expr: base, .. } => {
                self.collect_float_constants_from_expr(base);
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
            DataType::F64 => 8,
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
        if let Some(dt) = self.resolve_expr_type(expr) {
            self.get_type_size_internal(&dt)
        } else {
            8
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
                self.code.extend_from_slice(&[0x48, 0x0F, 0xB6, modrm]);
            }
            2 => {
                self.code.extend_from_slice(&[0x48, 0x0F, 0xB7, modrm]);
            }
            4 => {
                self.code.extend_from_slice(&[0x8B, modrm]);
            }
            _ => {
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
                self.code.extend_from_slice(&[0x88, modrm]);
            }
            2 => {
                self.code.extend_from_slice(&[0x66, 0x89, modrm]);
            }
            4 => {
                self.code.extend_from_slice(&[0x89, modrm]);
            }
            _ => {
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

        self.code.push(0x55);
        self.code.extend_from_slice(&[0x48, 0x89, 0xE5]);

        let sub_rsp_offset = self.code.len();
        self.code
            .extend_from_slice(&[0x48, 0x81, 0xEC, 0x00, 0x00, 0x00, 0x00]);

        for (idx, (dt, name, access)) in func.params.iter().enumerate() {
            let is_ptr_modifier = *access == PtrAccess::Input || *access == PtrAccess::Output;
            let var_size = if is_ptr_modifier {
                8
            } else {
                self.get_type_size_internal(dt)
            };

            // Выравнивание смещения параметра на стеке
            let align_mask = if var_size >= 8 {
                7
            } else if var_size >= 4 {
                3
            } else if var_size >= 2 {
                1
            } else {
                0
            };
            self.next_offset = (self.next_offset + align_mask) & !align_mask;

            self.next_offset += var_size;
            let offset = self.next_offset;
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
                self.emit_mem_store(reg_code, offset, var_size);
            }
        }

        if let Some(body) = &func.body {
            for stmt in body {
                self.compile_stmt(stmt);
            }
        }

        let final_stack_size = (self.next_offset + 15) & !15;
        let bytes = final_stack_size.to_le_bytes();
        self.code[sub_rsp_offset + 3] = bytes[0];
        self.code[sub_rsp_offset + 4] = bytes[1];
        self.code[sub_rsp_offset + 5] = bytes[2];
        self.code[sub_rsp_offset + 6] = bytes[3];

        self.code.extend_from_slice(&[0x48, 0x89, 0xEC]);
        self.code.push(0x5D);
        self.code.push(0xC3);
    }

    fn compile_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDefinition(decl) => {
                let is_ptr_modifier =
                    decl.modifier == PtrAccess::Input || decl.modifier == PtrAccess::Output;
                let var_size = if is_ptr_modifier {
                    8
                } else {
                    self.get_type_size_internal(&decl.data_type)
                };

                // Выравнивание смещения локальной переменной на стеке
                let align_mask = if var_size >= 8 {
                    7
                } else if var_size >= 4 {
                    3
                } else if var_size >= 2 {
                    1
                } else {
                    0
                };
                self.next_offset = (self.next_offset + align_mask) & !align_mask;

                self.next_offset += var_size;
                let offset = self.next_offset;
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
                    self.compile_expr(value, 0, true);
                    self.store_assignment_target_from_rax(target_expr);

                    if targets.len() > 1 {
                        if let Some(target_expr2) = targets.get(1) {
                            self.code.extend_from_slice(&[0x48, 0x89, 0xD0]);
                            self.store_assignment_target_from_rax(target_expr2);
                        }
                    }

                    if targets.len() > 2 {
                        if let Some(target_expr3) = targets.get(2) {
                            self.code.extend_from_slice(&[0x48, 0x89, 0xC8]);
                            self.store_assignment_target_from_rax(target_expr3);
                        }
                    }

                    if targets.len() > 3 {
                        if let Some(target_expr4) = targets.get(3) {
                            self.code.extend_from_slice(&[0x4C, 0x89, 0xC0]);
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

                // Смещение рассчитывается ДО добавления опкода 0xE9
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

                // Смещение рассчитывается ДО добавления опкода 0xE9
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
                        if parsed_program.functions.iter().any(|f| f.name == "main") {
                            let local_lexer = crate::lexer::Lexer::new(&code);
                            let mut local_parser = crate::parser::Parser::new(local_lexer);
                            if local_parser.seek_to_function("main").is_ok() {
                                if let Ok(body) = local_parser.parse_function_body() {
                                    for stmt in body {
                                        if let Stmt::Return(values) = stmt {
                                            if let Some((dt, Expr::Variable(ref var_name))) =
                                                values.first()
                                            {
                                                if !self.local_offsets.contains_key(var_name) {
                                                    let mut is_global = false;
                                                    for key in self.global_offsets.keys() {
                                                        if key.ends_with(&format!(":{}", var_name))
                                                        {
                                                            is_global = true;
                                                            break;
                                                        }
                                                    }
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
                    self.compile_expr(expr, 0, true);
                }
                if let Some((_, ref expr)) = values.get(1) {
                    self.compile_expr(expr, 0, true);
                    self.code.extend_from_slice(&[0x48, 0x89, 0xC2]);
                }
                if let Some((_, ref expr)) = values.get(2) {
                    self.compile_expr(expr, 0, true);
                    self.code.extend_from_slice(&[0x48, 0x89, 0xC1]);
                }
                if let Some((_, ref expr)) = values.get(3) {
                    self.compile_expr(expr, 0, true);
                    self.code.extend_from_slice(&[0x49, 0x89, 0xC0]);
                }
                self.code.extend_from_slice(&[0x48, 0x89, 0xEC]);
                self.code.push(0x5D);
                self.code.push(0xC3);
            }
            Stmt::Nasm(asm_code) => {
                for line in asm_code.lines() {
                    let mut trimmed = line.trim();

                    if let Some(idx) = trimmed.find("//") {
                        trimmed = trimmed[..idx].trim();
                    }
                    if let Some(idx) = trimmed.find(';') {
                        trimmed = trimmed[..idx].trim();
                    }

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
            Expr::FloatLit(s) => {
                let opcode = 0xB8 + reg;
                self.code.extend_from_slice(&[0x48, opcode]);
                let patch_pos = self.code.len();
                self.code.extend_from_slice(&[0; 8]);
                self.address_patches
                    .push((patch_pos, format!("float:{}", s)));

                if reg == 0 {
                    self.code.extend_from_slice(&[0x48, 0x8B, 0x00]);
                } else {
                    self.code.extend_from_slice(&[0x48, 0x8B, 0x1B]);
                }
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
                    let modifier = self
                        .local_access
                        .get(name)
                        .cloned()
                        .unwrap_or(PtrAccess::Normal);

                    let is_ptr_modifier =
                        modifier == PtrAccess::Input || modifier == PtrAccess::Output;

                    let var_size = if is_ptr_modifier {
                        8
                    } else {
                        self.get_type_size_internal(
                            self.local_types.get(name).unwrap_or(&DataType::U64),
                        )
                    };

                    self.emit_mem_load(reg, offset, var_size);

                    if modifier == PtrAccess::Input && _deref_ptr {
                        let is_byte = if let Some(dt) = self.local_types.get(name) {
                            match dt {
                                DataType::Pointer(inner) => match &**inner {
                                    DataType::U8 | DataType::I8 => true,
                                    _ => false,
                                },
                                DataType::U8 | DataType::I8 => true,
                                _ => false,
                            }
                        } else {
                            false
                        };

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
                        self.compile_address(&Expr::SectionAccess { section, variable }, reg);
                    }
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
            Expr::Null => {
                let opcode = 0xB8 + reg;
                self.code.extend_from_slice(&[0x48, opcode]);
                self.code.extend_from_slice(&0u64.to_le_bytes());
            }
            Expr::SectionAccess { section, variable } => {
                let key = format!("{}:{}", section, variable);
                let opcode = if reg == 0 { 0xB8 } else { 0xBB };
                self.code.extend_from_slice(&[0x48, opcode]);
                let patch_pos = self.code.len();
                self.code.extend_from_slice(&[0; 8]);
                self.address_patches.push((patch_pos, key));

                if reg == 0 {
                    self.code.extend_from_slice(&[0x48, 0x8B, 0x00]);
                } else {
                    self.code.extend_from_slice(&[0x48, 0x8B, 0x1B]);
                }
            }
            Expr::Binary { left, op, right } => {
                if op == "OpCastF64" {
                    self.compile_expr(left, reg, _deref_ptr);
                    if !self.is_float_expr(left) {
                        self.code.extend_from_slice(&[
                            0xF2, 0x48, 0x0F, 0x2A, 0xC0, // cvtsi2sd xmm0, rax
                            0x66, 0x48, 0x0F, 0x7E, 0xC0, // movq rax, xmm0
                        ]);
                    }
                    return;
                }

                if op == "OpCastInt" {
                    self.compile_expr(left, reg, _deref_ptr);
                    if self.is_float_expr(left) {
                        self.code.extend_from_slice(&[
                            0x66, 0x48, 0x0F, 0x6E, 0xC0, // movq xmm0, rax
                            0xF3, 0x48, 0x0F, 0x2C, 0xC0, // cvttsd2si rax, xmm0
                        ]);
                    }
                    return;
                }

                if op == "OpCast" {
                    self.compile_expr(left, reg, _deref_ptr);
                    return;
                }

                if op == "OpBitNot" {
                    self.compile_expr(left, 0, false);
                    self.code.extend_from_slice(&[0x48, 0xF7, 0xD0]); // not rax
                    return;
                }

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

                let is_float_op = self.is_float_expr(left) || self.is_float_expr(right);

                match op.as_str() {
                    "OpAdd" => {
                        if is_float_op {
                            self.code.extend_from_slice(&[
                                0x66, 0x48, 0x0F, 0x6E, 0xC0, 0x66, 0x48, 0x0F, 0x6E, 0xCB, 0xF2,
                                0x0F, 0x58, 0xC1, 0x66, 0x48, 0x0F, 0x7E, 0xC0,
                            ]);
                        } else {
                            self.code.extend_from_slice(&[0x48, 0x01, 0xD8]);
                        }
                    }
                    "OpSub" => {
                        if is_float_op {
                            self.code.extend_from_slice(&[
                                0x66, 0x48, 0x0F, 0x6E, 0xC0, 0x66, 0x48, 0x0F, 0x6E, 0xCB, 0xF2,
                                0x0F, 0x5C, 0xC1, 0x66, 0x48, 0x0F, 0x7E, 0xC0,
                            ]);
                        } else {
                            self.code.extend_from_slice(&[0x48, 0x29, 0xD8]);
                        }
                    }
                    "OpMul" => {
                        if is_float_op {
                            self.code.extend_from_slice(&[
                                0x66, 0x48, 0x0F, 0x6E, 0xC0, 0x66, 0x48, 0x0F, 0x6E, 0xCB, 0xF2,
                                0x0F, 0x59, 0xC1, 0x66, 0x48, 0x0F, 0x7E, 0xC0,
                            ]);
                        } else {
                            self.code.extend_from_slice(&[0x48, 0x0F, 0xAF, 0xC3]);
                        }
                    }
                    "OpDiv" => {
                        if is_float_op {
                            self.code.extend_from_slice(&[
                                0x66, 0x48, 0x0F, 0x6E, 0xC0, 0x66, 0x48, 0x0F, 0x6E, 0xCB, 0xF2,
                                0x0F, 0x5E, 0xC1, 0x66, 0x48, 0x0F, 0x7E, 0xC0,
                            ]);
                        } else {
                            self.code
                                .extend_from_slice(&[0x48, 0x31, 0xD2, 0x48, 0xF7, 0xF3]);
                        }
                    }
                    "OpMod" => {
                        self.code.extend_from_slice(&[
                            0x48, 0x31, 0xD2, 0x48, 0xF7, 0xF3, 0x48, 0x89, 0xD0,
                        ]);
                    }
                    "OpBitAnd" => {
                        self.code.extend_from_slice(&[0x48, 0x21, 0xD8]);
                    }
                    "OpBitOr" => {
                        self.code.extend_from_slice(&[0x48, 0x09, 0xD8]);
                    }
                    "OpBitXor" => {
                        self.code.extend_from_slice(&[0x48, 0x31, 0xD8]);
                    }
                    "OpShl" => {
                        self.code
                            .extend_from_slice(&[0x48, 0x89, 0xD9, 0x48, 0xD3, 0xE0]);
                    }
                    "OpShr" => {
                        self.code
                            .extend_from_slice(&[0x48, 0x89, 0xD9, 0x48, 0xD3, 0xE8]);
                    }
                    _ => {}
                }
            }
            Expr::Call { name, args } => {
                if name == "mloc" {
                    let size_expr_opt = if args.len() >= 2 {
                        args.get(1)
                    } else {
                        args.get(0)
                    };

                    if let Some(size_expr) = size_expr_opt {
                        self.compile_expr(size_expr, 0, true);
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
                    }
                    if let Some(buf_expr) = args.get(1) {
                        self.compile_expr(buf_expr, 6, true);
                    }
                    if let Some(size_expr) = args.get(2) {
                        self.compile_expr(size_expr, 2, true);
                    }
                    self.code
                        .extend_from_slice(&[0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, 0x0F, 0x05]);
                } else if name == "sys_read" {
                    if let Some(fd_expr) = args.first() {
                        self.compile_expr(fd_expr, 7, true);
                    }
                    if let Some(buf_expr) = args.get(1) {
                        self.compile_expr(buf_expr, 6, true);
                    }
                    if let Some(size_expr) = args.get(2) {
                        self.compile_expr(size_expr, 2, true);
                    }
                    self.code.extend_from_slice(&[0x48, 0x31, 0xC0, 0x0F, 0x05]);
                } else if name == "sys_open" {
                    if let Some(path_expr) = args.first() {
                        self.compile_expr(path_expr, 7, true);
                    }
                    if let Some(flags_expr) = args.get(1) {
                        self.compile_expr(flags_expr, 6, true);
                    }
                    if let Some(mode_expr) = args.get(2) {
                        self.compile_expr(mode_expr, 2, true);
                    }
                    self.code
                        .extend_from_slice(&[0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00, 0x0F, 0x05]);
                } else if name == "sys_close" {
                    if let Some(fd_expr) = args.first() {
                        self.compile_expr(fd_expr, 7, true);
                    }
                    self.code
                        .extend_from_slice(&[0x48, 0xC7, 0xC0, 0x03, 0x00, 0x00, 0x00, 0x0F, 0x05]);
                } else if name == "sys_unlink" {
                    if let Some(path_expr) = args.first() {
                        self.compile_expr(path_expr, 7, true);
                    }
                    self.code
                        .extend_from_slice(&[0x48, 0xC7, 0xC0, 0x57, 0x00, 0x00, 0x00, 0x0F, 0x05]);
                } else if name == "sys_ioctl" {
                    if let Some(fd_expr) = args.first() {
                        self.compile_expr(fd_expr, 7, true);
                    }
                    if let Some(req_expr) = args.get(1) {
                        self.compile_expr(req_expr, 6, true);
                    }
                    if let Some(arg_expr) = args.get(2) {
                        self.compile_expr(arg_expr, 2, true);
                    }
                    self.code
                        .extend_from_slice(&[0x48, 0xC7, 0xC0, 0x10, 0x00, 0x00, 0x00, 0x0F, 0x05]);
                } else if name == "sys_exit" {
                    if let Some(code_expr) = args.first() {
                        self.compile_expr(code_expr, 7, true);
                    }
                    self.code
                        .extend_from_slice(&[0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, 0x0F, 0x05]);
                } else if name == "sys_fork" {
                    self.code
                        .extend_from_slice(&[0x48, 0xC7, 0xC0, 0x39, 0x00, 0x00, 0x00, 0x0F, 0x05]);
                } else if name == "sys_execve" {
                    if let Some(path_expr) = args.first() {
                        self.compile_expr(path_expr, 7, true);
                    }
                    if let Some(argv_expr) = args.get(1) {
                        self.compile_expr(argv_expr, 6, true);
                    }
                    if let Some(envp_expr) = args.get(2) {
                        self.compile_expr(envp_expr, 2, true);
                    }
                    self.code
                        .extend_from_slice(&[0x48, 0xC7, 0xC0, 0x3B, 0x00, 0x00, 0x00, 0x0F, 0x05]);
                } else if name == "sys_wait4" {
                    if let Some(pid_expr) = args.first() {
                        self.compile_expr(pid_expr, 7, true);
                    }
                    if let Some(status_expr) = args.get(1) {
                        self.compile_expr(status_expr, 6, true);
                    }
                    if let Some(options_expr) = args.get(2) {
                        self.compile_expr(options_expr, 2, true);
                    }
                    if let Some(ru_expr) = args.get(3) {
                        self.compile_expr(ru_expr, 1, true);
                    }
                    self.code
                        .extend_from_slice(&[0x48, 0xC7, 0xC0, 0x3D, 0x00, 0x00, 0x00, 0x0F, 0x05]);
                } else if name == "inb" {
                    if let Some(port_expr) = args.first() {
                        self.compile_expr(port_expr, 1, true);
                    }
                    self.code
                        .extend_from_slice(&[0x66, 0x89, 0xCA, 0xEC, 0x48, 0x0F, 0xB6, 0xC0]);
                } else if name == "outb" {
                    if let Some(port_expr) = args.first() {
                        self.compile_expr(port_expr, 1, true);
                    }
                    if let Some(val_expr) = args.get(1) {
                        self.compile_expr(val_expr, 0, true);
                    }
                    self.code.extend_from_slice(&[0x66, 0x89, 0xCA, 0xEE]);
                } else if name == "inw" {
                    if let Some(port_expr) = args.first() {
                        self.compile_expr(port_expr, 1, true);
                    }
                    self.code
                        .extend_from_slice(&[0x66, 0x89, 0xCA, 0x66, 0xED, 0x48, 0x0F, 0xB7, 0xC0]);
                } else if name == "outw" {
                    if let Some(port_expr) = args.first() {
                        self.compile_expr(port_expr, 1, true);
                    }
                    if let Some(val_expr) = args.get(1) {
                        self.compile_expr(val_expr, 0, true);
                    }
                    self.code.extend_from_slice(&[0x66, 0x89, 0xCA, 0x66, 0xEF]);
                } else if name == "inl" {
                    if let Some(port_expr) = args.first() {
                        self.compile_expr(port_expr, 1, true);
                    }
                    self.code.extend_from_slice(&[0x66, 0x89, 0xCA, 0xED]);
                } else if name == "outl" {
                    if let Some(port_expr) = args.first() {
                        self.compile_expr(port_expr, 1, true);
                    }
                    if let Some(val_expr) = args.get(1) {
                        self.compile_expr(val_expr, 0, true);
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
        }

        if reg != 0 {
            match expr {
                Expr::Variable(_)
                | Expr::Number(_)
                | Expr::StringLit(_)
                | Expr::AddrOf(_)
                | Expr::FloatLit(_) => {}
                Expr::MemberAccess { .. } | Expr::Index { .. } | Expr::SectionAccess { .. } => {
                    if reg != 3 {
                        let modrm = 0xC0 | (3 << 3) | reg;
                        self.code.extend_from_slice(&[0x48, 0x89, modrm]);
                    }
                }
                Expr::Binary { .. } | Expr::Call { .. } => {
                    let modrm = 0xC0 | (0 << 3) | reg;
                    self.code.extend_from_slice(&[0x48, 0x89, modrm]);
                }
                _ => {}
            }
        }
    }

    fn compile_address(&mut self, expr: &Expr, reg: u8) {
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
                        self.emit_mem_op(0x8B, reg_opcode, offset);
                    } else {
                        self.emit_mem_op(0x8D, reg_opcode, offset);
                    }
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

                self.compile_address(base_expr, 3);
                self.code.push(0x53);

                self.compile_expr(index, 0, true);

                if elem_size == 8 {
                    self.code.extend_from_slice(&[0x48, 0xC1, 0xE0, 0x03]);
                } else if elem_size == 4 {
                    self.code.extend_from_slice(&[0x48, 0xC1, 0xE0, 0x02]);
                } else if elem_size == 2 {
                    self.code.extend_from_slice(&[0x48, 0xC1, 0xE0, 0x01]);
                }

                self.code.push(0x5B);
                self.code.extend_from_slice(&[0x48, 0x01, 0xC3]);

                if internal_reg == 0 {
                    self.code.extend_from_slice(&[0x48, 0x89, 0xD8]);
                }
            }
            Expr::MemberAccess {
                expr: base_expr,
                member,
                is_arrow,
            } => {
                self.compile_address(base_expr, internal_reg);

                if *is_arrow {
                    let mut is_base_pointer = false;
                    if let Some(base_type) = self.resolve_expr_type(base_expr) {
                        if let DataType::Pointer(_) = base_type {
                            is_base_pointer = true;
                        }
                    }

                    if !is_base_pointer {
                        let deref_op = if internal_reg == 0 {
                            &[0x48, 0x8B, 0x00][..]
                        } else {
                            &[0x48, 0x8B, 0x1B][..]
                        };
                        self.code.extend_from_slice(deref_op);
                    }
                }

                let mut struct_name = String::new();
                if let Some(base_type) = self.resolve_expr_type(base_expr) {
                    struct_name = match base_type {
                        DataType::Struct(n) => n,
                        DataType::Pointer(boxed) => match *boxed {
                            DataType::Struct(n) => n,
                            _ => String::new(),
                        },
                        _ => String::new(),
                    };
                }

                if let Some((_, fields)) = self.struct_layouts.get(&struct_name) {
                    if let Some(&field_offset) = fields.get(member) {
                        let add_opcode = if internal_reg == 0 { 0xC0 } else { 0xC3 };
                        self.code.extend_from_slice(&[0x48, 0x81, add_opcode]);
                        self.code.extend_from_slice(&field_offset.to_le_bytes());
                    }
                }
            }
            Expr::SectionAccess { section, variable } => {
                let key = format!("{}:{}", section, variable);
                let reg_opcode = if internal_reg == 0 { 0xB8 } else { 0xBB };
                self.code.extend_from_slice(&[0x48, reg_opcode]);
                let patch_pos = self.code.len();
                self.code.extend_from_slice(&[0; 8]);
                self.address_patches.push((patch_pos, key));
            }
            _ => {}
        }

        if reg != 0 && reg != 3 {
            let modrm = 0xC0 | (3 << 3) | reg;
            self.code.extend_from_slice(&[0x48, 0x89, modrm]);
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

                    let elem_size = if let Some(dt) = self.local_types.get(name) {
                        match dt {
                            DataType::Pointer(inner) => self.get_type_size_internal(inner),
                            _ => 8,
                        }
                    } else {
                        8
                    };

                    match elem_size {
                        1 => self.code.extend_from_slice(&[0x88, 0x03]),
                        2 => self.code.extend_from_slice(&[0x66, 0x89, 0x03]),
                        4 => self.code.extend_from_slice(&[0x89, 0x03]),
                        _ => self.code.extend_from_slice(&[0x48, 0x89, 0x03]),
                    }
                } else if let Some(&offset) = self.local_offsets.get(name) {
                    let var_size = if modifier == PtrAccess::Input {
                        8
                    } else {
                        self.get_type_size_internal(
                            self.local_types.get(name).unwrap_or(&DataType::U64),
                        )
                    };
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
            Expr::AddrOf(name) => {
                if let Some(&offset) = self.local_offsets.get(name) {
                    self.emit_mem_store(0, offset, 8);
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
                self.code.extend_from_slice(&[0x48, 0xBB]);
                let patch_pos = self.code.len();
                self.code.extend_from_slice(&[0; 8]);
                self.address_patches.push((patch_pos, key));

                self.code.extend_from_slice(&[0x48, 0x89, 0x03]);
            }
            Expr::MemberAccess { .. } | Expr::Index { .. } => {
                self.code.push(0x50);
                self.compile_address(target, 1);
                self.code.push(0x58);

                let target_size = self.get_expr_type_size(target);
                match target_size {
                    1 => {
                        self.code.extend_from_slice(&[0x88, 0x03]);
                    }
                    2 => {
                        self.code.extend_from_slice(&[0x66, 0x89, 0x03]);
                    }
                    4 => {
                        self.code.extend_from_slice(&[0x89, 0x03]);
                    }
                    _ => {
                        self.code.extend_from_slice(&[0x48, 0x89, 0x03]);
                    }
                }
            }
            _ => {}
        }
    }
}
