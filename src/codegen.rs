use crate::ast::*;
use std::collections::HashMap;
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Program,
    Object,
    Raw,
    Kernel,
    Wexp,
}

pub struct NativeGenerator {
    code: Vec<u8>,
    local_offsets: HashMap<String, u32>,
    local_access: HashMap<String, PtrAccess>,
    local_types: HashMap<String, DataType>,
    next_offset: u32,

    pub function_offsets: HashMap<String, usize>,
    pub call_patches: Vec<(usize, String)>,

    global_offsets: HashMap<String, u32>,
    pub global_data_size: u32,
    section_volatile: std::collections::HashSet<String>,
    section_var_sizes: HashMap<String, u32>,

    struct_layouts: HashMap<String, (u32, HashMap<String, u32>)>,
    typedefs_map: HashMap<String, DataType>,

    string_constants: HashMap<String, u32>,
    float_constants: HashMap<String, u32>,
    global_data_start_offset: usize,

    address_patches: Vec<(usize, String, bool)>,

    function_signatures: HashMap<String, Vec<DataType>>,

    struct_fields: HashMap<String, HashMap<String, DataType>>,
    pub output_format: OutputFormat,
    pub entry_name: Option<String>,
    pub use_os: bool,
    current_is_irq: bool,
    constants: HashMap<String, Expr>,
    enums: HashMap<String, HashMap<String, u64>>,
    struct_alignments: HashMap<String, u32>,
    struct_versions: HashMap<String, u32>,
    struct_volatile_fields: HashMap<String, std::collections::HashSet<String>>,
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
            section_volatile: std::collections::HashSet::new(),
            section_var_sizes: HashMap::new(),
            struct_layouts: HashMap::new(),
            typedefs_map: HashMap::new(),
            string_constants: HashMap::new(),
            float_constants: HashMap::new(),
            global_data_start_offset: 0,
            address_patches: Vec::new(),
            function_signatures: HashMap::new(),
            struct_fields: HashMap::new(),
            output_format: OutputFormat::Program,
            entry_name: None,
            current_is_irq: false,
            use_os: false,
            constants: HashMap::new(),
            enums: HashMap::new(),
            struct_alignments: HashMap::new(),
            struct_versions: HashMap::new(),
            struct_volatile_fields: HashMap::new(),
        }
    }

    fn emit_rip_relative_lea(&mut self, reg_code: u8, patch_key: String) {
        let rex = if reg_code >= 8 { 0x4C } else { 0x48 };
        let modrm = 0x05 | ((reg_code & 7) << 3);
        self.code.extend_from_slice(&[rex, 0x8D, modrm]);
        let patch_pos = self.code.len();
        self.code.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        self.address_patches.push((patch_pos, patch_key, true));
    }

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

            Expr::AddrOfExpr(inner) => {
                let inner_type = self.resolve_expr_type(inner)?;
                Some(DataType::Pointer(Box::new(inner_type)))
            }

            _ => None,
        }
    }

    fn is_volatile_member_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::MemberAccess {
                expr: base, member, ..
            } => {
                if let Some(base_type) = self.resolve_expr_type(base) {
                    let struct_name = match base_type {
                        DataType::Struct(n) => Some(n),
                        DataType::Pointer(boxed) => match *boxed {
                            DataType::Struct(n) => Some(n),
                            _ => None,
                        },
                        _ => None,
                    };

                    if let Some(name) = struct_name {
                        if let Some(fields) = self.struct_volatile_fields.get(&name) {
                            return fields.contains(member);
                        }
                    }
                }

                false
            }

            Expr::Index { expr: base, .. } => self.is_volatile_member_expr(base),

            _ => false,
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
        let declared_alignment = if s.alignment > 0 { s.alignment } else { 1 };

        if s.is_union {
            let mut max_size = 0u32;
            let mut max_alignment = declared_alignment;

            for field in &s.fields {
                fields_offsets.insert(field.name.clone(), 0);
                let size = self.get_type_size_internal(&field.data_type);
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
                max_size += max_alignment - (max_alignment - (max_size % max_alignment));
            }

            self.struct_layouts
                .insert(s.name.clone(), (max_size, fields_offsets));
            self.struct_alignments.insert(s.name.clone(), max_alignment);
            return;
        }

        let mut current_offset = 0u32;
        let mut max_alignment = declared_alignment;

        for field in &s.fields {
            let size = self.get_type_size_internal(&field.data_type);
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

            fields_offsets.insert(field.name.clone(), current_offset);
            current_offset += size;
        }

        if current_offset % max_alignment != 0 {
            current_offset += max_alignment - (current_offset % max_alignment);
        }

        self.struct_layouts
            .insert(s.name.clone(), (current_offset, fields_offsets));
        self.struct_alignments.insert(s.name.clone(), max_alignment);
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
        self.constants.clear();
        self.enums.clear();
        self.struct_alignments.clear();
        self.struct_versions.clear();
        self.struct_volatile_fields.clear();
        for s in &program.structs {
            let mut fields_types = HashMap::new();
            let mut volatile_fields = std::collections::HashSet::new();

            for field in &s.fields {
                fields_types.insert(field.name.clone(), field.data_type.clone());

                if field.modifier == PtrAccess::Volatile || field.modifier == PtrAccess::Atomic {
                    volatile_fields.insert(field.name.clone());
                }
            }

            if !volatile_fields.is_empty() {
                self.struct_volatile_fields
                    .insert(s.name.clone(), volatile_fields);
            }

            self.struct_fields.insert(s.name.clone(), fields_types);
            self.struct_versions.insert(s.name.clone(), s.version);
        }

        for constant in &program.constants {
            self.constants
                .insert(constant.name.clone(), constant.value.clone());
        }

        for enum_decl in &program.enums {
            let mut values = HashMap::new();
            for value in &enum_decl.values {
                values.insert(value.name.clone(), value.value);
            }
            self.enums.insert(enum_decl.name.clone(), values);
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
                    .insert(key.clone(), global_data_bytes.len() as u32);
                let var_size = self.get_type_size_internal(&var.data_type);
                if var.modifier == PtrAccess::Volatile || var.modifier == PtrAccess::Atomic {
                    self.section_volatile.insert(key.clone());
                }
                self.section_var_sizes.insert(key.clone(), var_size);

                let init_val = match &var.initial_value {
                    Some(expr) => self.eval_const_expr(expr).unwrap_or(0),
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
            let unescaped = Self::unescape_wand_string(str_const);

            global_data_bytes.extend_from_slice(&unescaped);
            global_data_bytes.push(0);

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

        let entry_call_patch: Option<usize>;

        match self.output_format {
            OutputFormat::Program => {
                self.code.extend_from_slice(&[0x48, 0x8B, 0x3C, 0x24]);
                self.code.extend_from_slice(&[0x48, 0x8D, 0x74, 0x24, 0x08]);
                self.code.extend_from_slice(&[0x48, 0x89, 0xFA]);
                self.code.extend_from_slice(&[0x48, 0xFF, 0xC2]);
                self.code.extend_from_slice(&[0x48, 0xC1, 0xE2, 0x03]);
                self.code.extend_from_slice(&[0x48, 0x01, 0xF2]);
                self.code.push(0xE8);
                entry_call_patch = Some(self.code.len());
                self.code.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
                self.code.extend_from_slice(&[0x48, 0x89, 0xC7]);
                self.code
                    .extend_from_slice(&[0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00]);
                self.code.extend_from_slice(&[0x0F, 0x05]);
            }
            OutputFormat::Object => {
                entry_call_patch = None;
            }
            OutputFormat::Raw => {
                self.code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]);
                entry_call_patch = Some(1usize);
                self.code.extend_from_slice(&[0xEB, 0xFE]);
            }
            OutputFormat::Kernel => {
                self.code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]);
                entry_call_patch = Some(1usize);
                self.code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]);
            }
            OutputFormat::Wexp => {
                self.code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]);
                entry_call_patch = Some(1usize);
                self.code.push(0xC3);
            }
        }

        for func in &program.functions {
            if func.is_extern || func.body.is_none() {
                continue;
            }

            let offset = self.code.len();
            self.function_offsets.insert(func.name.clone(), offset);
            self.compile_function(func);
        }

        if self.output_format == OutputFormat::Object {
            return self.build_relocatable_elf(&global_data_bytes, program);
        }

        let functions_end_offset = self.code.len();
        self.global_data_start_offset = (functions_end_offset + 0xFFF) & !0xFFF;

        while self.code.len() < self.global_data_start_offset {
            self.code.push(0);
        }
        self.code.extend_from_slice(&global_data_bytes);

        let entry_target = match self.output_format {
            OutputFormat::Program | OutputFormat::Wexp => "main".to_string(),
            _ => self
                .entry_name
                .clone()
                .unwrap_or_else(|| "main".to_string()),
        };

        if let Some(patch_pos) = entry_call_patch {
            if let Some(&entry_offset) = self.function_offsets.get(&entry_target) {
                let relative_offset = (entry_offset as i32) - ((patch_pos + 4) as i32);
                let bytes = relative_offset.to_le_bytes();
                self.code[patch_pos] = bytes[0];
                self.code[patch_pos + 1] = bytes[1];
                self.code[patch_pos + 2] = bytes[2];
                self.code[patch_pos + 3] = bytes[3];
            }
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
        let call_patches_final = std::mem::take(&mut self.call_patches);
        for (patch_pos, target_name) in &call_patches_final {
            if let Some(&target_offset) = self.function_offsets.get(target_name) {
                let relative_offset = (target_offset as i32) - ((*patch_pos + 4) as i32);
                let bytes = relative_offset.to_le_bytes();
                self.code[*patch_pos] = bytes[0];
                self.code[*patch_pos + 1] = bytes[1];
                self.code[*patch_pos + 2] = bytes[2];
                self.code[*patch_pos + 3] = bytes[3];
            } else {
                self.call_patches.push((*patch_pos, target_name.clone()));
            }
        }
        let addr_patches = std::mem::take(&mut self.address_patches);
        for (patch_pos, key, is_rip) in addr_patches {
            let local_offset = self.global_offsets.get(&key).cloned().unwrap_or(0) as i64;
            let target_addr = (self.global_data_start_offset as i64) + local_offset;

            if is_rip {
                let next_ip = (patch_pos + 4) as i64;
                let disp32 = (target_addr - next_ip) as i32;
                let bytes = disp32.to_le_bytes();
                self.code[patch_pos] = bytes[0];
                self.code[patch_pos + 1] = bytes[1];
                self.code[patch_pos + 2] = bytes[2];
                self.code[patch_pos + 3] = bytes[3];
            } else {
                let abs_addr =
                    0x400078u64 + (self.global_data_start_offset as u64) + (local_offset as u64);
                let bytes = abs_addr.to_le_bytes();
                for i in 0..8 {
                    self.code[patch_pos + i] = bytes[i];
                }
            }
        }

        self.code.clone()
    }

    fn collect_string_constants_from_program(&mut self, program: &Program) {
        for constant in &program.constants {
            self.collect_string_constants_from_expr(&constant.value);
        }

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
            Stmt::Match {
                expr,
                cases,
                default,
            } => {
                self.collect_string_constants_from_expr(expr);
                for (ce, body) in cases {
                    self.collect_string_constants_from_expr(ce);
                    for s in body {
                        self.collect_string_constants_from_stmt(s);
                    }
                }
                if let Some(d) = default {
                    for s in d {
                        self.collect_string_constants_from_stmt(s);
                    }
                }
            }
            Stmt::Critical(body) => {
                for s in body {
                    self.collect_string_constants_from_stmt(s);
                }
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
            Expr::Call { name, args } => {
                if name == "nameof" {
                    if let Some(Expr::Variable(type_name)) = args.first() {
                        self.string_constants.insert(type_name.clone(), 0);
                    }
                }
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
            Expr::AddrOfExpr(inner) => {
                self.collect_string_constants_from_expr(inner);
            }
            _ => {}
        }
    }

    fn is_float_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::FloatLit(_) => true,
            Expr::SignedNumber(_) => false,
            Expr::Variable(name) => {
                if let Some(dt) = self.local_types.get(name) {
                    matches!(dt, DataType::F64)
                } else {
                    false
                }
            }
            Expr::Binary { left, right, .. } => {
                self.is_float_expr(left) || self.is_float_expr(right)
            }
            Expr::Call { name, .. } => {
                matches!(name.as_str(), "sin" | "cos" | "tan" | "sqrt")
            }
            _ => false,
        }
    }

    fn is_signed_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::SignedNumber(_) => true,
            Expr::Variable(name) => {
                if let Some(dt) = self.local_types.get(name) {
                    matches!(
                        dt,
                        DataType::I8 | DataType::I16 | DataType::I32 | DataType::I64
                    )
                } else {
                    false
                }
            }
            Expr::Number(_) => false,
            Expr::Binary { left, right, .. } => {
                self.is_signed_expr(left) || self.is_signed_expr(right)
            }
            Expr::Call { name, .. } => {
                if let Some(types) = self.function_signatures.get(name) {
                    !types.is_empty()
                        && matches!(
                            types[0],
                            DataType::I8 | DataType::I16 | DataType::I32 | DataType::I64
                        )
                } else {
                    false
                }
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
            Stmt::Match {
                expr,
                cases,
                default,
            } => {
                self.collect_float_constants_from_expr(expr);
                for (ce, body) in cases {
                    self.collect_float_constants_from_expr(ce);
                    for s in body {
                        self.collect_float_constants_from_stmt(s);
                    }
                }
                if let Some(d) = default {
                    for s in d {
                        self.collect_float_constants_from_stmt(s);
                    }
                }
            }
            Stmt::Critical(body) => {
                for s in body {
                    self.collect_float_constants_from_stmt(s);
                }
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
            Expr::AddrOfExpr(inner) => {
                self.collect_float_constants_from_expr(inner);
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

    fn emit_mem_op(&mut self, opcode: u8, reg: u8, offset: u32) {
        let low = reg & 7;
        let rex = if reg >= 8 { 0x4C } else { 0x48 };

        self.code.push(rex);
        self.code.push(opcode);

        let neg = -(offset as i32);

        if neg >= -128 && neg <= 127 {
            self.code.push(0x45 | (low << 3));
            self.code.push(neg as u8);
        } else {
            self.code.push(0x85 | (low << 3));
            self.code.extend_from_slice(&neg.to_le_bytes());
        }
    }

    fn emit_mem_load(&mut self, reg: u8, offset: u32, size: u32) {
        let low = reg & 7;
        let neg = -(offset as i32);

        let modrm = if neg >= -128 && neg <= 127 {
            0x45 | (low << 3)
        } else {
            0x85 | (low << 3)
        };

        match size {
            1 => {
                let rex = if reg >= 8 { 0x4C } else { 0x48 };
                self.code.push(rex);
                self.code.extend_from_slice(&[0x0F, 0xB6, modrm]);
            }
            2 => {
                let rex = if reg >= 8 { 0x4C } else { 0x48 };
                self.code.push(rex);
                self.code.extend_from_slice(&[0x0F, 0xB7, modrm]);
            }
            4 => {
                if reg >= 8 {
                    self.code.push(0x44);
                }
                self.code.push(0x8B);
                self.code.push(modrm);
            }
            _ => {
                let rex = if reg >= 8 { 0x4C } else { 0x48 };
                self.code.push(rex);
                self.code.push(0x8B);
                self.code.push(modrm);
            }
        }

        if neg >= -128 && neg <= 127 {
            self.code.push(neg as u8);
        } else {
            self.code.extend_from_slice(&neg.to_le_bytes());
        }
    }

    fn emit_mem_store(&mut self, reg: u8, offset: u32, size: u32) {
        let low = reg & 7;
        let neg = -(offset as i32);

        let modrm = if neg >= -128 && neg <= 127 {
            0x45 | (low << 3)
        } else {
            0x85 | (low << 3)
        };

        match size {
            1 => {
                if reg >= 8 {
                    self.code.push(0x44);
                } else if reg >= 4 {
                    self.code.push(0x40);
                }
                self.code.push(0x88);
                self.code.push(modrm);
            }
            2 => {
                self.code.push(0x66);
                if reg >= 8 {
                    self.code.push(0x44);
                }
                self.code.push(0x89);
                self.code.push(modrm);
            }
            4 => {
                if reg >= 8 {
                    self.code.push(0x44);
                }
                self.code.push(0x89);
                self.code.push(modrm);
            }
            _ => {
                let rex = if reg >= 8 { 0x4C } else { 0x48 };
                self.code.push(rex);
                self.code.push(0x89);
                self.code.push(modrm);
            }
        }

        if neg >= -128 && neg <= 127 {
            self.code.push(neg as u8);
        } else {
            self.code.extend_from_slice(&neg.to_le_bytes());
        }
    }

    fn has_return_statement(stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            match stmt {
                Stmt::Return(_) => return true,
                Stmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    if Self::has_return_statement(then_branch) {
                        if let Some(else_stmts) = else_branch {
                            if Self::has_return_statement(else_stmts) {
                                return true;
                            }
                        }
                    }
                }
                Stmt::Match { cases, default, .. } => {
                    let all_cases = cases
                        .iter()
                        .all(|(_, body)| Self::has_return_statement(body));
                    let def_ret = default
                        .as_ref()
                        .map_or(false, |d| Self::has_return_statement(d));
                    if all_cases && def_ret {
                        return true;
                    }
                }
                Stmt::While { body, .. } | Stmt::For { body, .. } => {
                    if Self::has_return_statement(body) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn compile_function(&mut self, func: &FuncDecl) {
        self.local_offsets.clear();
        self.local_access.clear();
        self.local_types.clear();
        self.next_offset = 16;

        self.current_is_irq = func.is_irq;

        if func.is_irq {
            self.code.push(0x50);
            self.code.push(0x51);
            self.code.push(0x52);
            self.code.push(0x56);
            self.code.push(0x57);
            self.code.extend_from_slice(&[0x41, 0x50]);
            self.code.extend_from_slice(&[0x41, 0x51]);
            self.code.extend_from_slice(&[0x41, 0x52]);
            self.code.extend_from_slice(&[0x41, 0x53]);
            self.code.extend_from_slice(&[0x41, 0x54]);
            self.code.extend_from_slice(&[0x41, 0x55]);
            self.code.extend_from_slice(&[0x41, 0x56]);
            self.code.extend_from_slice(&[0x41, 0x57]);
        }

        self.code.push(0x55);
        self.code.extend_from_slice(&[0x48, 0x89, 0xE5]);
        self.code.push(0x53);

        let sub_rsp_offset = self.code.len();
        self.code
            .extend_from_slice(&[0x48, 0x81, 0xEC, 0x00, 0x00, 0x00, 0x00]);

        for (idx, (dt, name, access)) in func.params.iter().enumerate() {
            let is_ptr_modifier = *access == PtrAccess::Input
                || *access == PtrAccess::Output
                || *access == PtrAccess::InputOutput;
            let var_size = if is_ptr_modifier {
                8
            } else {
                self.get_type_size_internal(dt)
            };

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

            let actual_type = if is_ptr_modifier {
                match dt {
                    DataType::Pointer(_) => dt.clone(),
                    _ => DataType::Pointer(Box::new(dt.clone())),
                }
            } else {
                dt.clone()
            };
            self.local_types.insert(name.clone(), actual_type);

            if idx < 4 {
                let reg_code = match idx {
                    0 => 7,
                    1 => 6,
                    2 => 2,
                    _ => 1,
                };
                self.emit_mem_store(reg_code, offset, var_size);
            }
        }

        let has_explicit_return = if let Some(body) = &func.body {
            for stmt in body {
                self.compile_stmt(stmt);
            }
            Self::has_return_statement(body)
        } else {
            false
        };

        let final_stack_size = (self.next_offset + 15) & !15;
        let bytes = final_stack_size.to_le_bytes();
        self.code[sub_rsp_offset + 3] = bytes[0];
        self.code[sub_rsp_offset + 4] = bytes[1];
        self.code[sub_rsp_offset + 5] = bytes[2];
        self.code[sub_rsp_offset + 6] = bytes[3];

        if !has_explicit_return {
            self.code.extend_from_slice(&[0x48, 0x81, 0xC4]);
            self.code.extend_from_slice(&final_stack_size.to_le_bytes());
            self.code.push(0x5B);
            self.code.push(0x5D);
            self.emit_function_epilogue();
        }
    }

    fn compile_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDefinition(decl) => {
                let var_size = self.get_type_size_internal(&decl.data_type).max(1);

                let align = if decl.alignment > 0 {
                    decl.alignment
                } else {
                    self.resolve_type_alignment_for_datatype(&decl.data_type)
                        .unwrap_or(1)
                };

                self.next_offset = ((self.next_offset + align - 1) / align) * align;
                self.next_offset += var_size;

                let offset = self.next_offset;

                self.local_offsets.insert(decl.name.clone(), offset);
                self.local_access
                    .insert(decl.name.clone(), decl.modifier.clone());
                self.local_types
                    .insert(decl.name.clone(), decl.data_type.clone());

                if let Some(init) = &decl.initial_value {
                    self.compile_expr(init, 0, true);
                    self.emit_mem_store(0, offset, var_size);
                }
            }
            Stmt::Assignment { targets, value } => {
                if let Some(target_expr) = targets.first() {
                    self.compile_expr(value, 0, true);
                    self.store_assignment_target_from_rax(target_expr);
                    if let Expr::Variable(name) = target_expr {
                        if let Some(modifier) = self.local_access.get(name) {
                            if *modifier == PtrAccess::Volatile || *modifier == PtrAccess::Atomic {
                                self.code.extend_from_slice(&[0x0F, 0xAE, 0xF0]);
                            }
                        }
                    }

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
                let mut compiled_inline = false;
                let mut source_code = None;
                if let Ok(code) = std::fs::read_to_string(&module_name) {
                    source_code = Some(code);
                } else {
                    let source_filename = module_name.replace(".wexp", ".w");
                    if let Ok(code) = std::fs::read_to_string(&source_filename) {
                        source_code = Some(code);
                    }
                }

                if let Some(code) = source_code {
                    let lexer = crate::lexer::Lexer::new(&code);
                    let mut parser = crate::parser::Parser::new(lexer);
                    if let Ok(parsed_program) = parser.parse_program() {
                        if parsed_program.functions.iter().any(|f| f.name == "main") {
                            let local_lexer = crate::lexer::Lexer::new(&code);
                            let mut local_parser = crate::parser::Parser::new(local_lexer);
                            if local_parser.seek_to_function("main").is_ok() {
                                if let Ok(body) = local_parser.parse_function_body() {
                                    for arg in args {
                                        self.compile_stmt(arg);
                                    }
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
                    for arg in args {
                        self.compile_stmt(arg);
                    }
                    self.emit_rip_relative_lea(7, format!("str:{}", module_name));
                    self.code.push(0xE8);
                    let patch_pos_call = self.code.len();
                    self.code.extend_from_slice(&[0, 0, 0, 0]);
                    self.call_patches
                        .push((patch_pos_call, "__wand_jmpto_loader".to_string()));
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
                self.code.extend_from_slice(&[0x48, 0x8D, 0x65, 0xF8]);
                self.code.push(0x5B);
                self.code.push(0x5D);
                self.emit_function_epilogue();
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
            Stmt::Critical(body) => {
                if self.use_os {
                    for body_stmt in body {
                        self.compile_stmt(body_stmt);
                    }
                } else {
                    self.code.push(0x9C);
                    self.code.push(0x58);
                    self.code.push(0x50);
                    self.code.push(0xFA);

                    for body_stmt in body {
                        self.compile_stmt(body_stmt);
                    }

                    self.code.push(0x58);
                    self.code.push(0x9D);
                }
            }
            Stmt::Match {
                expr,
                cases,
                default,
            } => {
                self.compile_expr(expr, 0, true);

                let mut je_patches = Vec::new();

                for (case_expr, _) in cases {
                    let val = self.eval_const_expr(case_expr).unwrap_or(0);
                    self.emit_cmp_rax_imm(val);
                    self.code.push(0x0F);
                    self.code.push(0x84);
                    let patch_pos = self.code.len();
                    self.code.extend_from_slice(&[0, 0, 0, 0]);
                    je_patches.push(patch_pos);
                }

                self.code.push(0xE9);
                let default_jmp = self.code.len();
                self.code.extend_from_slice(&[0, 0, 0, 0]);

                let mut end_jmps = Vec::new();

                for (i, (_, body)) in cases.iter().enumerate() {
                    let body_pos = self.code.len();
                    let rel = (body_pos as i32) - ((je_patches[i] + 4) as i32);
                    self.patch_address(je_patches[i], rel);

                    for s in body {
                        self.compile_stmt(s);
                    }

                    self.code.push(0xE9);
                    let p = self.code.len();
                    self.code.extend_from_slice(&[0, 0, 0, 0]);
                    end_jmps.push(p);
                }

                let default_pos = self.code.len();
                let rel_default = (default_pos as i32) - ((default_jmp + 4) as i32);
                self.patch_address(default_jmp, rel_default);

                if let Some(d) = default {
                    for s in d {
                        self.compile_stmt(s);
                    }
                }

                let end_pos = self.code.len();
                for p in end_jmps {
                    let rel = (end_pos as i32) - ((p + 4) as i32);
                    self.patch_address(p, rel);
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
                self.emit_mov_imm64(reg, *n);
            }
            Expr::SignedNumber(n) => {
                self.emit_mov_imm64(reg, *n as u64);
            }
            Expr::FloatLit(s) => {
                let key = format!("float:{}", s);
                self.emit_rip_relative_lea(0, key);
                self.code.extend_from_slice(&[0xF2, 0x0F, 0x10, 0x00]);
                self.code.extend_from_slice(&[0x66, 0x48, 0x0F, 0x7E, 0xC0]);
                self.move_rax_to_reg(reg);
            }
            Expr::StringLit(s) => {
                self.emit_rip_relative_lea(reg, format!("str:{}", s));
            }
            Expr::Variable(name) => {
                if let Some(&offset) = self.local_offsets.get(name) {
                    let modifier = self
                        .local_access
                        .get(name)
                        .cloned()
                        .unwrap_or(PtrAccess::Normal);

                    let var_type = self.local_types.get(name).cloned().unwrap_or(DataType::U64);

                    let var_size = self.get_type_size_internal(&var_type);

                    if matches!(var_type, DataType::Array(..)) {
                        self.emit_mem_op(0x8D, reg, offset);
                    } else if (modifier == PtrAccess::Input && _deref_ptr)
                        || modifier == PtrAccess::InputOutput
                    {
                        self.emit_mem_load(reg, offset, 8);

                        let elem_size = if let DataType::Pointer(inner) = &var_type {
                            self.get_type_size_internal(inner)
                        } else {
                            var_size
                        };

                        let low = reg & 7;
                        let use_sib = low == 4 || low == 5;

                        let deref_modrm = if use_sib {
                            if low == 5 {
                                0x40 | (low << 3) | 0x04
                            } else {
                                (low << 3) | 0x04
                            }
                        } else {
                            (low << 3) | low
                        };

                        let sib = if use_sib { Some(0x20 | low) } else { None };
                        let disp = use_sib && low == 5;

                        match elem_size {
                            1 => {
                                let rex = if reg >= 8 { 0x4D } else { 0x48 };
                                self.code.push(rex);
                                self.code.extend_from_slice(&[0x0F, 0xB6, deref_modrm]);

                                if let Some(s) = sib {
                                    self.code.push(s);
                                }

                                if disp {
                                    self.code.push(0);
                                }
                            }
                            2 => {
                                let rex = if reg >= 8 { 0x4D } else { 0x48 };
                                self.code.push(rex);
                                self.code.extend_from_slice(&[0x0F, 0xB7, deref_modrm]);

                                if let Some(s) = sib {
                                    self.code.push(s);
                                }

                                if disp {
                                    self.code.push(0);
                                }
                            }
                            4 => {
                                if reg >= 8 {
                                    self.code.push(0x45);
                                }

                                self.code.extend_from_slice(&[0x8B, deref_modrm]);

                                if let Some(s) = sib {
                                    self.code.push(s);
                                }

                                if disp {
                                    self.code.push(0);
                                }
                            }
                            _ => {
                                let rex = if reg >= 8 { 0x4D } else { 0x48 };
                                self.code.push(rex);
                                self.code.extend_from_slice(&[0x8B, deref_modrm]);

                                if let Some(s) = sib {
                                    self.code.push(s);
                                }

                                if disp {
                                    self.code.push(0);
                                }
                            }
                        }
                    } else {
                        self.emit_mem_load(reg, offset, var_size);
                    }
                } else if let Some(const_expr) = self.constants.get(name) {
                    let const_expr = const_expr.clone();

                    match &const_expr {
                        Expr::StringLit(_)
                        | Expr::Number(_)
                        | Expr::SignedNumber(_)
                        | Expr::Null => {
                            self.compile_expr(&const_expr, reg, _deref_ptr);
                            return;
                        }
                        _ => {
                            if let Ok(value) = self.eval_const_expr(&Expr::Variable(name.clone())) {
                                let opcode = 0xB8 + reg;
                                self.code.extend_from_slice(&[0x48, opcode]);
                                self.code.extend_from_slice(&value.to_le_bytes());
                                return;
                            }
                        }
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
                    } else if let Some(&off) = self.function_offsets.get(name) {
                        self.emit_mov_imm64(reg, 0x400078 + off as u64);
                    }
                }
            }

            Expr::AddrOfExpr(inner) => {
                self.compile_address(inner, reg);
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
                    } else if let Some(&off) = self.function_offsets.get(name) {
                        self.emit_mov_imm64(reg, 0x400078 + off as u64);
                    }
                }
            }
            Expr::MemberAccess { .. } | Expr::Index { .. } => {
                self.compile_address(expr, 1);

                if matches!(self.resolve_expr_type(expr), Some(DataType::Array(..))) {
                    if reg == 0 {
                        self.code.extend_from_slice(&[0x48, 0x89, 0xD8]);
                    }
                } else {
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
            }
            Expr::Null => {
                self.emit_mov_imm64(reg, 0);
            }
            Expr::SectionAccess { section, variable } => {
                if let Some(values) = self.enums.get(section) {
                    if let Some(value) = values.get(variable) {
                        let opcode = 0xB8 + reg;
                        self.code.extend_from_slice(&[0x48, opcode]);
                        self.code.extend_from_slice(&value.to_le_bytes());
                        return;
                    }
                }

                let key = format!("{}:{}", section, variable);
                self.emit_rip_relative_lea(0, key);
                self.code.extend_from_slice(&[0x48, 0x8B, 0x00]);

                self.move_rax_to_reg(reg);
            }
            Expr::Binary { left, op, right } => {
                match op.as_str() {
                    "OpAnd" | "&&" | "OpOr" | "||" | "OpEq" | "OpEqEq" | "==" | "OpNotEq"
                    | "OpNe" | "!=" | "OpLt" | "Lt" | "<" | "OpLtEq" | "OpLe" | "<=" | "OpGt"
                    | "Gt" | ">" | "OpGtEq" | "OpGe" | ">=" => {
                        self.compile_bool_expr(expr);
                        self.move_rax_to_reg(reg);
                        return;
                    }
                    _ => {}
                }
                if op == "OpCastF64" {
                    self.compile_expr(left, reg, _deref_ptr);
                    if !self.is_float_expr(left) {
                        self.code.extend_from_slice(&[
                            0xF2, 0x48, 0x0F, 0x2A, 0xC0, 0x66, 0x48, 0x0F, 0x7E, 0xC0,
                        ]);
                    }
                    return;
                }
                if op == "OpCastInt" {
                    self.compile_expr(left, reg, _deref_ptr);
                    if self.is_float_expr(left) {
                        self.code.extend_from_slice(&[
                            0x66, 0x48, 0x0F, 0x6E, 0xC0, 0xF3, 0x48, 0x0F, 0x2C, 0xC0,
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
                    self.code.extend_from_slice(&[0x48, 0xF7, 0xD0]);
                    return;
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
                        } else if self.is_signed_expr(left) || self.is_signed_expr(right) {
                            self.code.extend_from_slice(&[0x48, 0x99, 0x48, 0xF7, 0xFB]);
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
                        if self.is_signed_expr(left) {
                            self.code
                                .extend_from_slice(&[0x48, 0x89, 0xD9, 0x48, 0xD3, 0xF8]);
                        } else {
                            self.code
                                .extend_from_slice(&[0x48, 0x89, 0xD9, 0x48, 0xD3, 0xE8]);
                        }
                    }
                    _ => {}
                }
            }
            Expr::Call { name, args } => {
                if name == "nameof" {
                    if let Some(Expr::Variable(type_name)) = args.first() {
                        self.emit_rip_relative_lea(reg, format!("str:{}", type_name));
                    } else {
                        self.compile_expr(&Expr::Null, reg, _deref_ptr);
                    }
                    return;
                }

                if name == "sizeof"
                    || name == "alignof"
                    || name == "offsetof"
                    || name == "versionof"
                    || name == "fieldsof"
                {
                    let value = self.eval_const_expr(expr).unwrap_or(0);
                    self.compile_expr(&Expr::Number(value), reg, _deref_ptr);
                    return;
                }

                if name == "memory_barrier" {
                    self.code.extend_from_slice(&[0x0F, 0xAE, 0xF0]);
                    self.code.extend_from_slice(&[0x48, 0x31, 0xC0]);
                    self.move_rax_to_reg(reg);
                    return;
                }

                if name == "compiler_barrier" {
                    self.code.extend_from_slice(&[0x48, 0x31, 0xC0]);
                    self.move_rax_to_reg(reg);
                    return;
                }

                if name == "atomic_load" {
                    if args.first().is_none() {
                        self.compile_expr(&Expr::Null, reg, _deref_ptr);
                        return;
                    }

                    if let Some(ptr_expr) = args.first() {
                        self.compile_expr(ptr_expr, 0, true);
                        self.code.extend_from_slice(&[0x48, 0x8B, 0x00]);
                        self.move_rax_to_reg(reg);
                    }

                    return;
                }

                if name == "atomic_store" {
                    if args.len() < 2 {
                        self.compile_expr(&Expr::Null, reg, _deref_ptr);
                        return;
                    }

                    if let Some(ptr_expr) = args.first() {
                        self.compile_expr(ptr_expr, 0, true);
                        self.code.push(0x50);
                    }

                    if let Some(val_expr) = args.get(1) {
                        self.compile_expr(val_expr, 0, true);
                        self.code.push(0x5B);
                        self.code.extend_from_slice(&[0x48, 0x89, 0x03]);
                        self.code.extend_from_slice(&[0x0F, 0xAE, 0xF0]);
                        self.move_rax_to_reg(reg);
                    }

                    return;
                }

                if name == "atomic_add" {
                    if args.len() < 2 {
                        self.compile_expr(&Expr::Null, reg, _deref_ptr);
                        return;
                    }

                    if let Some(ptr_expr) = args.first() {
                        self.compile_expr(ptr_expr, 0, true);
                        self.code.push(0x50);
                    }

                    if let Some(val_expr) = args.get(1) {
                        self.compile_expr(val_expr, 0, true);
                    }

                    self.code.push(0x5B);
                    self.code.extend_from_slice(&[0xF0, 0x48, 0x0F, 0xC1, 0x03]);
                    self.move_rax_to_reg(reg);

                    return;
                }

                if name == "atomic_sub" {
                    if args.len() < 2 {
                        self.compile_expr(&Expr::Null, reg, _deref_ptr);
                        return;
                    }

                    if let Some(ptr_expr) = args.first() {
                        self.compile_expr(ptr_expr, 0, true);
                        self.code.push(0x50);
                    }

                    if let Some(val_expr) = args.get(1) {
                        self.compile_expr(val_expr, 0, true);
                    }

                    self.code.extend_from_slice(&[0x48, 0xF7, 0xD8]);
                    self.code.push(0x5B);
                    self.code.extend_from_slice(&[0xF0, 0x48, 0x0F, 0xC1, 0x03]);
                    self.move_rax_to_reg(reg);

                    return;
                }

                if name == "atomic_inc" {
                    if args.first().is_none() {
                        self.compile_expr(&Expr::Null, reg, _deref_ptr);
                        return;
                    }

                    if let Some(ptr_expr) = args.first() {
                        self.compile_expr(ptr_expr, 0, true);
                        self.code.push(0x50);
                    }

                    self.code
                        .extend_from_slice(&[0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00]);
                    self.code.push(0x5B);
                    self.code.extend_from_slice(&[0xF0, 0x48, 0x0F, 0xC1, 0x03]);
                    self.code.extend_from_slice(&[0x48, 0xFF, 0xC0]);
                    self.move_rax_to_reg(reg);

                    return;
                }

                if name == "atomic_dec" {
                    if args.first().is_none() {
                        self.compile_expr(&Expr::Null, reg, _deref_ptr);
                        return;
                    }

                    if let Some(ptr_expr) = args.first() {
                        self.compile_expr(ptr_expr, 0, true);
                        self.code.push(0x50);
                    }

                    self.code
                        .extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]);
                    self.code.push(0x5B);
                    self.code.extend_from_slice(&[0xF0, 0x48, 0x0F, 0xC1, 0x03]);
                    self.code.extend_from_slice(&[0x48, 0xFF, 0xC8]);
                    self.move_rax_to_reg(reg);

                    return;
                }

                if name == "atomic_swap" {
                    if args.len() < 2 {
                        self.compile_expr(&Expr::Null, reg, _deref_ptr);
                        return;
                    }

                    if let Some(ptr_expr) = args.first() {
                        self.compile_expr(ptr_expr, 0, true);
                        self.code.push(0x50);
                    }

                    if let Some(val_expr) = args.get(1) {
                        self.compile_expr(val_expr, 0, true);
                    }

                    self.code.push(0x5B);
                    self.code.extend_from_slice(&[0x48, 0x87, 0x03]);
                    self.move_rax_to_reg(reg);

                    return;
                }

                if name == "atomic_cas" {
                    if args.len() < 3 {
                        self.compile_expr(&Expr::Null, reg, _deref_ptr);
                        return;
                    }

                    if let Some(ptr_expr) = args.first() {
                        self.compile_expr(ptr_expr, 0, true);
                        self.code.push(0x50);
                    }

                    if let Some(expected_expr) = args.get(1) {
                        self.compile_expr(expected_expr, 0, true);
                        self.code.push(0x50);
                    }

                    if let Some(desired_expr) = args.get(2) {
                        self.compile_expr(desired_expr, 0, true);
                    }

                    self.code.extend_from_slice(&[0x48, 0x89, 0xC1]);
                    self.code.push(0x58);
                    self.code.push(0x5B);
                    self.code.extend_from_slice(&[0xF0, 0x48, 0x0F, 0xB1, 0x0B]);
                    self.move_rax_to_reg(reg);

                    return;
                }

                if name == "syscall0" {
                    if let Some(nr_expr) = args.first() {
                        self.compile_expr(nr_expr, 0, true);
                    }
                    self.code.extend_from_slice(&[0x0F, 0x05]);
                } else if name == "syscall1" {
                    if let Some(a1_expr) = args.get(1) {
                        self.compile_expr(a1_expr, 7, true);
                    }
                    if let Some(nr_expr) = args.first() {
                        self.compile_expr(nr_expr, 0, true);
                    }
                    self.code.extend_from_slice(&[0x0F, 0x05]);
                } else if name == "syscall2" {
                    if let Some(a1_expr) = args.get(1) {
                        self.compile_expr(a1_expr, 7, true);
                    }
                    if let Some(a2_expr) = args.get(2) {
                        self.compile_expr(a2_expr, 6, true);
                    }
                    if let Some(nr_expr) = args.first() {
                        self.compile_expr(nr_expr, 0, true);
                    }
                    self.code.extend_from_slice(&[0x0F, 0x05]);
                } else if name == "syscall3" {
                    if let Some(a1_expr) = args.get(1) {
                        self.compile_expr(a1_expr, 7, true);
                    }
                    if let Some(a2_expr) = args.get(2) {
                        self.compile_expr(a2_expr, 6, true);
                    }
                    if let Some(a3_expr) = args.get(3) {
                        self.compile_expr(a3_expr, 2, true);
                    }
                    if let Some(nr_expr) = args.first() {
                        self.compile_expr(nr_expr, 0, true);
                    }
                    self.code.extend_from_slice(&[0x0F, 0x05]);
                } else if name == "syscall4" {
                    if let Some(a1_expr) = args.get(1) {
                        self.compile_expr(a1_expr, 7, true);
                    }
                    if let Some(a2_expr) = args.get(2) {
                        self.compile_expr(a2_expr, 6, true);
                    }
                    if let Some(a3_expr) = args.get(3) {
                        self.compile_expr(a3_expr, 2, true);
                    }
                    if let Some(a4_expr) = args.get(4) {
                        self.compile_expr(a4_expr, 10, true);
                    }
                    if let Some(nr_expr) = args.first() {
                        self.compile_expr(nr_expr, 0, true);
                    }
                    self.code.extend_from_slice(&[0x0F, 0x05]);
                } else if name == "syscall5" {
                    if let Some(a1_expr) = args.get(1) {
                        self.compile_expr(a1_expr, 7, true);
                    }
                    if let Some(a2_expr) = args.get(2) {
                        self.compile_expr(a2_expr, 6, true);
                    }
                    if let Some(a3_expr) = args.get(3) {
                        self.compile_expr(a3_expr, 2, true);
                    }
                    if let Some(a4_expr) = args.get(4) {
                        self.compile_expr(a4_expr, 10, true);
                    }
                    if let Some(a5_expr) = args.get(5) {
                        self.compile_expr(a5_expr, 8, true);
                    }
                    if let Some(nr_expr) = args.first() {
                        self.compile_expr(nr_expr, 0, true);
                    }
                    self.code.extend_from_slice(&[0x0F, 0x05]);
                } else if name == "syscall6" {
                    if let Some(a1_expr) = args.get(1) {
                        self.compile_expr(a1_expr, 7, true);
                    }
                    if let Some(a2_expr) = args.get(2) {
                        self.compile_expr(a2_expr, 6, true);
                    }
                    if let Some(a3_expr) = args.get(3) {
                        self.compile_expr(a3_expr, 2, true);
                    }
                    if let Some(a4_expr) = args.get(4) {
                        self.compile_expr(a4_expr, 10, true);
                    }
                    if let Some(a5_expr) = args.get(5) {
                        self.compile_expr(a5_expr, 8, true);
                    }
                    if let Some(a6_expr) = args.get(6) {
                        self.compile_expr(a6_expr, 9, true);
                    }
                    if let Some(nr_expr) = args.first() {
                        self.compile_expr(nr_expr, 0, true);
                    }
                    self.code.extend_from_slice(&[0x0F, 0x05]);
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
                    self.code.extend_from_slice(&[0x89, 0xCA, 0xED]);
                } else if name == "outl" {
                    if let Some(port_expr) = args.first() {
                        self.compile_expr(port_expr, 1, true);
                    }
                    if let Some(val_expr) = args.get(1) {
                        self.compile_expr(val_expr, 0, true);
                    }
                    self.code.extend_from_slice(&[0x89, 0xCA, 0xEF]);
                } else {
                    let arg_registers_out = [
                        &[0x48, 0x89, 0xC7][..], // rdi
                        &[0x48, 0x89, 0xC6][..], // rsi
                        &[0x48, 0x89, 0xC2][..], // rdx
                        &[0x48, 0x89, 0xC1][..], // rcx
                        &[0x49, 0x89, 0xC0][..], // r8
                        &[0x49, 0x89, 0xC1][..], // r9
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
                | Expr::SignedNumber(_)
                | Expr::StringLit(_)
                | Expr::AddrOf(_)
                | Expr::AddrOfExpr(_)
                | Expr::FloatLit(_) => {}
                Expr::MemberAccess { .. } | Expr::Index { .. } | Expr::SectionAccess { .. } => {
                    if reg != 3 {
                        let low_reg = reg & 7;
                        let rex = if reg >= 8 { 0x49 } else { 0x48 };
                        let modrm = 0xC0 | (3 << 3) | low_reg;
                        self.code.extend_from_slice(&[rex, 0x89, modrm]);
                    }
                }
                Expr::Binary { .. } | Expr::Call { .. } => {
                    if reg != 0 {
                        let low_reg = reg & 7;
                        let rex = if reg >= 8 { 0x49 } else { 0x48 };
                        let modrm = 0xC0 | (0 << 3) | low_reg;
                        self.code.extend_from_slice(&[rex, 0x89, modrm]);
                    }
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

                    if modifier == PtrAccess::Input
                        || modifier == PtrAccess::Output
                        || modifier == PtrAccess::InputOutput
                        || is_pointer
                    {
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

                if let Some(base_type) = self.resolve_expr_type(base_expr) {
                    match base_type {
                        DataType::Array(elem, _) => {
                            elem_size = self.get_type_size_internal(&elem);
                        }
                        DataType::Pointer(elem) => {
                            elem_size = self.get_type_size_internal(&elem);
                        }
                        DataType::Typedef(name, _) => {
                            if let Some(DataType::Array(elem, _)) = self.typedefs_map.get(&name) {
                                elem_size = self.get_type_size_internal(elem);
                            }
                        }
                        _ => {}
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
                self.emit_rip_relative_lea(internal_reg, key);
            }

            Expr::AddrOf(_) => {
                self.compile_expr(expr, internal_reg, false);
            }

            Expr::AddrOfExpr(inner) => {
                self.compile_address(inner, internal_reg);
            }

            Expr::Binary { left, op, right } => match op.as_str() {
                "OpAdd" => {
                    self.compile_address(left, 3);
                    self.code.push(0x53);
                    self.compile_expr(right, 0, true);
                    self.code.push(0x5B);
                    self.code.extend_from_slice(&[0x48, 0x01, 0xC3]);

                    if internal_reg == 0 {
                        self.code.extend_from_slice(&[0x48, 0x89, 0xD8]);
                    }
                }

                "OpSub" => {
                    self.compile_address(left, 3);
                    self.code.push(0x53);
                    self.compile_expr(right, 0, true);
                    self.code.push(0x5B);
                    self.code.extend_from_slice(&[0x48, 0x29, 0xC3]);

                    if internal_reg == 0 {
                        self.code.extend_from_slice(&[0x48, 0x89, 0xD8]);
                    }
                }

                _ => {
                    self.compile_expr(expr, internal_reg, false);
                }
            },

            _ => {}
        }

        if reg != 0 && reg != 3 {
            let rex = if reg >= 8 { 0x49 } else { 0x48 };
            let modrm = 0xC0 | (3 << 3) | (reg & 7);
            self.code.extend_from_slice(&[rex, 0x89, modrm]);
        }
    }

    fn compile_bool_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Binary { left, op, right } => match op.as_str() {
                "OpAnd" | "&&" => {
                    self.compile_bool_expr(left);
                    self.code.extend_from_slice(&[0x48, 0x85, 0xC0]);

                    self.code.push(0x0F);
                    self.code.push(0x84);
                    let false_patch = self.code.len();
                    self.code.extend_from_slice(&[0, 0, 0, 0]);

                    self.compile_bool_expr(right);

                    self.code.push(0xE9);
                    let end_patch = self.code.len();
                    self.code.extend_from_slice(&[0, 0, 0, 0]);

                    let false_pos = self.code.len();
                    self.patch_address(
                        false_patch,
                        (false_pos as i32) - ((false_patch + 4) as i32),
                    );

                    self.code.extend_from_slice(&[0x48, 0x31, 0xC0]);

                    let end_pos = self.code.len();
                    self.patch_address(end_patch, (end_pos as i32) - ((end_patch + 4) as i32));
                }

                "OpOr" | "||" => {
                    self.compile_bool_expr(left);
                    self.code.extend_from_slice(&[0x48, 0x85, 0xC0]);

                    self.code.push(0x0F);
                    self.code.push(0x85);
                    let true_patch = self.code.len();
                    self.code.extend_from_slice(&[0, 0, 0, 0]);

                    self.compile_bool_expr(right);

                    self.code.push(0xE9);
                    let end_patch = self.code.len();
                    self.code.extend_from_slice(&[0, 0, 0, 0]);

                    let true_pos = self.code.len();
                    self.patch_address(true_patch, (true_pos as i32) - ((true_patch + 4) as i32));

                    self.code
                        .extend_from_slice(&[0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00]);

                    let end_pos = self.code.len();
                    self.patch_address(end_patch, (end_pos as i32) - ((end_patch + 4) as i32));
                }

                "OpEq" | "OpEqEq" | "==" | "OpNotEq" | "OpNe" | "!=" | "OpLt" | "Lt" | "<"
                | "OpLtEq" | "OpLe" | "<=" | "OpGt" | "Gt" | ">" | "OpGtEq" | "OpGe" | ">=" => {
                    self.compile_expr(left, 0, true);
                    self.code.push(0x50);
                    self.compile_expr(right, 0, true);
                    self.code.extend_from_slice(&[0x48, 0x89, 0xC3]);
                    self.code.push(0x58);
                    self.code.extend_from_slice(&[0x48, 0x39, 0xD8]);

                    let signed = self.is_signed_expr(left) || self.is_signed_expr(right);

                    let setcc = match op.as_str() {
                        "OpEq" | "OpEqEq" | "==" => 0x94,
                        "OpNotEq" | "OpNe" | "!=" => 0x95,
                        "OpLt" | "Lt" | "<" => {
                            if signed {
                                0x9C
                            } else {
                                0x92
                            }
                        }
                        "OpLtEq" | "OpLe" | "<=" => {
                            if signed {
                                0x9E
                            } else {
                                0x96
                            }
                        }
                        "OpGt" | "Gt" | ">" => {
                            if signed {
                                0x9F
                            } else {
                                0x97
                            }
                        }
                        "OpGtEq" | "OpGe" | ">=" => {
                            if signed {
                                0x9D
                            } else {
                                0x93
                            }
                        }
                        _ => 0x95,
                    };

                    self.code.extend_from_slice(&[0x0F, setcc, 0xC0]);
                    self.code.extend_from_slice(&[0x48, 0x0F, 0xB6, 0xC0]);
                }

                _ => {
                    self.compile_expr(expr, 0, true);
                    self.code.extend_from_slice(&[0x48, 0x85, 0xC0]);
                    self.code.extend_from_slice(&[0x0F, 0x95, 0xC0]);
                    self.code.extend_from_slice(&[0x48, 0x0F, 0xB6, 0xC0]);
                }
            },

            _ => {
                self.compile_expr(expr, 0, true);
                self.code.extend_from_slice(&[0x48, 0x85, 0xC0]);
                self.code.extend_from_slice(&[0x0F, 0x95, 0xC0]);
                self.code.extend_from_slice(&[0x48, 0x0F, 0xB6, 0xC0]);
            }
        }
    }

    fn compile_condition_helper(&mut self, cond: &Expr) -> u8 {
        self.compile_bool_expr(cond);
        self.code.extend_from_slice(&[0x48, 0x85, 0xC0]);
        0x84
    }

    fn emit_cmp_rax_imm(&mut self, val: u64) {
        if val <= 0x7FFFFFFF {
            self.code.extend_from_slice(&[0x48, 0x3D]);
            self.code.extend_from_slice(&(val as u32).to_le_bytes());
        } else {
            self.code.extend_from_slice(&[0x48, 0xBB]);
            self.code.extend_from_slice(&val.to_le_bytes());
            self.code.extend_from_slice(&[0x48, 0x39, 0xD8]);
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
                if let Some(&offset) = self.local_offsets.get(name) {
                    if modifier == PtrAccess::Output || modifier == PtrAccess::InputOutput {
                        self.emit_mem_load(3, offset, 8);
                        let elem_size =
                            if let Some(DataType::Pointer(inner)) = self.local_types.get(name) {
                                self.get_type_size_internal(inner)
                            } else {
                                8
                            };
                        match elem_size {
                            1 => self.code.extend_from_slice(&[0x88, 0x03]),
                            2 => self.code.extend_from_slice(&[0x66, 0x89, 0x03]),
                            4 => self.code.extend_from_slice(&[0x89, 0x03]),
                            _ => self.code.extend_from_slice(&[0x48, 0x89, 0x03]),
                        }
                    } else {
                        let var_size = self.get_expr_type_size(&Expr::Variable(name.clone()));
                        self.emit_mem_store(0, offset, var_size);
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
                self.emit_rip_relative_lea(3, key.clone());
                self.code.extend_from_slice(&[0x48, 0x89, 0x03]);
                let is_volatile = self.section_volatile.contains(&key);
                if is_volatile {
                    self.code.extend_from_slice(&[0x0F, 0xAE, 0xF0]);
                }
            }
            Expr::Index { .. } | Expr::MemberAccess { .. } => {
                self.code.push(0x50);
                self.compile_address(target, 3);
                self.code.push(0x58);
                let size = self.get_expr_type_size(target);

                match size {
                    1 => self.code.extend_from_slice(&[0x88, 0x03]),
                    2 => self.code.extend_from_slice(&[0x66, 0x89, 0x03]),
                    4 => self.code.extend_from_slice(&[0x89, 0x03]),
                    _ => self.code.extend_from_slice(&[0x48, 0x89, 0x03]),
                }

                if self.is_volatile_member_expr(target) {
                    self.code.extend_from_slice(&[0x0F, 0xAE, 0xF0]);
                }
            }
            _ => {}
        }
    }
    fn get_primitive_size(&self, name: &str) -> Option<u32> {
        match name {
            "u8" | "i8" => Some(1),
            "u16" | "i16" => Some(2),
            "u32" | "i32" => Some(4),
            "u64" | "i64" | "f64" | "ptr" => Some(8),
            "void" => Some(0),
            _ => None,
        }
    }

    fn resolve_type_size(&self, name: &str) -> Result<u64, String> {
        if let Some(size) = self.get_primitive_size(name) {
            return Ok(size as u64);
        }

        if let Some(dt) = self.typedefs_map.get(name) {
            return Ok(self.get_type_size_internal(dt) as u64);
        }

        if let Some((size, _)) = self.struct_layouts.get(name) {
            return Ok(*size as u64);
        }

        Err(format!("unknown type '{}'", name))
    }

    fn resolve_type_alignment_for_datatype(&self, dt: &DataType) -> Result<u32, String> {
        match dt {
            DataType::U8 | DataType::I8 => Ok(1),
            DataType::U16 | DataType::I16 => Ok(2),
            DataType::U32 | DataType::I32 => Ok(4),
            DataType::U64 | DataType::I64 | DataType::F64 => Ok(8),
            DataType::Void => Ok(1),
            DataType::Pointer(_) => Ok(8),
            DataType::Array(elem, _) => self.resolve_type_alignment_for_datatype(elem),
            DataType::Typedef(_, underlying) => {
                self.resolve_type_alignment_for_datatype(underlying)
            }
            DataType::Struct(name) => self
                .struct_alignments
                .get(name)
                .cloned()
                .ok_or_else(|| format!("unknown struct '{}'", name)),
        }
    }

    fn resolve_type_alignment(&self, name: &str) -> Result<u64, String> {
        if let Some(size) = self.get_primitive_size(name) {
            if size == 0 {
                return Ok(1);
            }
            return Ok(size as u64);
        }

        if let Some(dt) = self.typedefs_map.get(name) {
            return Ok(self.resolve_type_alignment_for_datatype(dt)? as u64);
        }

        if let Some(alignment) = self.struct_alignments.get(name) {
            return Ok(*alignment as u64);
        }

        Err(format!("unknown type '{}'", name))
    }

    fn eval_const_expr(&self, expr: &Expr) -> Result<u64, String> {
        self.eval_const_expr_depth(expr, 0)
    }

    fn eval_const_expr_depth(&self, expr: &Expr, depth: usize) -> Result<u64, String> {
        if depth > 64 {
            return Err("constant evaluation depth too high".to_string());
        }

        match expr {
            Expr::Number(n) => Ok(*n),
            Expr::SignedNumber(n) => Ok(*n as u64),
            Expr::Null => Ok(0),
            Expr::Variable(name) => {
                if let Some(value_expr) = self.constants.get(name) {
                    let value_expr = value_expr.clone();
                    return self.eval_const_expr_depth(&value_expr, depth + 1);
                }

                Err(format!("unknown constant '{}'", name))
            }
            Expr::SectionAccess { section, variable } => {
                if let Some(values) = self.enums.get(section) {
                    if let Some(value) = values.get(variable) {
                        return Ok(*value);
                    }
                }

                Err(format!("unknown enum value '{}:{}'", section, variable))
            }
            Expr::Binary { left, op, right } => {
                if op == "OpCastF64" || op == "OpCastInt" || op == "OpCast" {
                    return self.eval_const_expr_depth(left, depth + 1);
                }

                if op == "OpBitNot" {
                    let value = self.eval_const_expr_depth(left, depth + 1)?;
                    return Ok(!value);
                }

                let a = self.eval_const_expr_depth(left, depth + 1)?;
                let b = self.eval_const_expr_depth(right, depth + 1)?;

                match op.as_str() {
                    "OpAdd" => Ok(a.wrapping_add(b)),
                    "OpSub" => Ok(a.wrapping_sub(b)),
                    "OpMul" => Ok(a.wrapping_mul(b)),
                    "OpDiv" => {
                        if b == 0 {
                            return Err("division by zero in constant expression".to_string());
                        }
                        Ok(a / b)
                    }
                    "OpMod" => {
                        if b == 0 {
                            return Err("modulo by zero in constant expression".to_string());
                        }
                        Ok(a % b)
                    }
                    "OpBitAnd" => Ok(a & b),
                    "OpBitOr" => Ok(a | b),
                    "OpBitXor" => Ok(a ^ b),
                    "OpShl" => Ok(a.wrapping_shl(b as u32)),
                    "OpShr" => Ok(a.wrapping_shr(b as u32)),
                    "OpEq" | "OpEqEq" | "==" => Ok(if a == b { 1 } else { 0 }),
                    "OpNotEq" | "OpNe" | "!=" => Ok(if a != b { 1 } else { 0 }),
                    "OpLt" | "Lt" | "<" => Ok(if a < b { 1 } else { 0 }),
                    "OpLtEq" | "OpLe" | "<=" => Ok(if a <= b { 1 } else { 0 }),
                    "OpGt" | "Gt" | ">" => Ok(if a > b { 1 } else { 0 }),
                    "OpGtEq" | "OpGe" | ">=" => Ok(if a >= b { 1 } else { 0 }),
                    "OpAnd" | "&&" => Ok(if a != 0 && b != 0 { 1 } else { 0 }),
                    "OpOr" | "||" => Ok(if a != 0 || b != 0 { 1 } else { 0 }),
                    _ => Err(format!("unsupported constant operator '{}'", op)),
                }
            }
            Expr::Call { name, args } => match name.as_str() {
                "sizeof" => {
                    if let Some(Expr::Variable(type_name)) = args.first() {
                        return self.resolve_type_size(type_name);
                    }
                    if let Some(Expr::Variable(section_name)) = args.first() {
                        let mut total = 0u64;
                        for (key, &sz) in &self.section_var_sizes {
                            if key.starts_with(&format!("{}:", section_name)) {
                                total += sz as u64;
                            }
                        }
                        if total > 0 {
                            return Ok(total);
                        }
                    }
                    Err("sizeof requires a type name or section".to_string())
                }
                "alignof" => {
                    if let Some(Expr::Variable(type_name)) = args.first() {
                        return self.resolve_type_alignment(type_name);
                    }

                    Err("alignof requires a type name".to_string())
                }
                "versionof" => {
                    if let Some(Expr::Variable(type_name)) = args.first() {
                        if let Some(version) = self.struct_versions.get(type_name) {
                            return Ok(*version as u64);
                        }

                        if self.typedefs_map.contains_key(type_name) {
                            return Ok(1);
                        }

                        return Err(format!("unknown versioned type '{}'", type_name));
                    }

                    Err("versionof requires a type name".to_string())
                }
                "fieldsof" => {
                    if let Some(Expr::Variable(type_name)) = args.first() {
                        if let Some(fields) = self.struct_fields.get(type_name) {
                            return Ok(fields.len() as u64);
                        }

                        return Err(format!("unknown structure '{}'", type_name));
                    }

                    Err("fieldsof requires a type name".to_string())
                }
                "offsetof" => {
                    if let Some(Expr::SectionAccess { section, variable }) = args.first() {
                        if let Some((_, fields)) = self.struct_layouts.get(section) {
                            if let Some(offset) = fields.get(variable) {
                                return Ok(*offset as u64);
                            }
                        }

                        return Err(format!("unknown field '{}:{}'", section, variable));
                    }

                    Err("offsetof requires Struct:field".to_string())
                }
                _ => Err(format!("unsupported constant function '{}'", name)),
            },
            _ => Err("unsupported constant expression".to_string()),
        }
    }
    fn emit_function_epilogue(&mut self) {
        if self.current_is_irq {
            self.code.extend_from_slice(&[0x41, 0x5F]);
            self.code.extend_from_slice(&[0x41, 0x5E]);
            self.code.extend_from_slice(&[0x41, 0x5D]);
            self.code.extend_from_slice(&[0x41, 0x5C]);
            self.code.extend_from_slice(&[0x41, 0x5B]);
            self.code.extend_from_slice(&[0x41, 0x5A]);
            self.code.extend_from_slice(&[0x41, 0x59]);
            self.code.extend_from_slice(&[0x41, 0x58]);
            self.code.push(0x5F);
            self.code.push(0x5E);
            self.code.push(0x5A);
            self.code.push(0x59);
            self.code.push(0x58);
            self.code.extend_from_slice(&[0x48, 0xCF]);
        } else {
            self.code.push(0xC3);
        }
    }
    fn move_rax_to_reg(&mut self, reg: u8) {
        if reg != 0 {
            let rex = if reg >= 8 { 0x49 } else { 0x48 };
            let modrm = 0xC0 | (reg & 7);
            self.code.extend_from_slice(&[rex, 0x89, modrm]);
        }
    }
    fn elf_strtab_insert(strtab: &mut Vec<u8>, cache: &mut HashMap<String, u32>, s: &str) -> u32 {
        if let Some(off) = cache.get(s) {
            return *off;
        }

        let off = strtab.len() as u32;
        strtab.extend_from_slice(s.as_bytes());
        strtab.push(0);
        cache.insert(s.to_string(), off);
        off
    }

    fn elf_push_u16(buf: &mut Vec<u8>, value: u16) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn elf_push_u32(buf: &mut Vec<u8>, value: u32) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn elf_push_u64(buf: &mut Vec<u8>, value: u64) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn elf_push_i64(buf: &mut Vec<u8>, value: i64) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    #[allow(clippy::too_many_arguments)]
    fn elf_push_shdr(
        buf: &mut Vec<u8>,
        name: u32,
        sh_type: u32,
        flags: u64,
        addr: u64,
        offset: u64,
        size: u64,
        link: u32,
        info: u32,
        addralign: u64,
        entsize: u64,
    ) {
        Self::elf_push_u32(buf, name);
        Self::elf_push_u32(buf, sh_type);
        Self::elf_push_u64(buf, flags);
        Self::elf_push_u64(buf, addr);
        Self::elf_push_u64(buf, offset);
        Self::elf_push_u64(buf, size);
        Self::elf_push_u32(buf, link);
        Self::elf_push_u32(buf, info);
        Self::elf_push_u64(buf, addralign);
        Self::elf_push_u64(buf, entsize);
    }

    pub fn build_relocatable_elf(&self, global_data_bytes: &[u8], program: &Program) -> Vec<u8> {
        let mut shstrtab = Vec::new();
        shstrtab.push(0);
        let n_text = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".text\0");

        let mut section_names = Vec::new();
        let mut section_name_offsets = Vec::new();
        for sect in &program.sections {
            let elf_name = format!(".w.{}\0", sect.name);
            let off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(elf_name.as_bytes());
            section_names.push(sect.name.clone());
            section_name_offsets.push(off);
        }

        let n_rodata = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".rodata\0");
        let n_rela = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".rela.text\0");
        let n_symtab = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".symtab\0");
        let n_strtab = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".strtab\0");
        let n_shstrtab = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".shstrtab\0");
        let n_note = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".note.GNU-stack\0");

        let mut strtab = vec![0u8];
        let mut str_cache = HashMap::new();
        let text_name = Self::elf_strtab_insert(&mut strtab, &mut str_cache, ".text");

        let mut sect_data_ranges: Vec<(String, u32, u32, u32, bool, bool)> = Vec::new();
        let mut cur_off = 0u32;
        for sect in &program.sections {
            let start = cur_off;
            for var in &sect.variables {
                let var_size = self.get_type_size_internal(&var.data_type);
                cur_off += var_size;
            }
            let align = if sect.alignment > 0 {
                sect.alignment
            } else {
                8
            };
            sect_data_ranges.push((
                sect.name.clone(),
                start,
                cur_off - start,
                align,
                sect.is_ro,
                sect.is_noinit,
            ));
        }
        let user_data_end = cur_off;

        let mut symbols: Vec<(u32, u8, u8, u16, u64, u64)> = Vec::new();
        symbols.push((0, 0, 0, 0, 0, 0));
        symbols.push((text_name, 3, 0, 1, 0, 0));

        let mut data_sym: HashMap<String, u32> = HashMap::new();
        for (si, (sect_name, start, _size, _align, _ro, _noinit)) in
            sect_data_ranges.iter().enumerate()
        {
            let shndx = (2 + si) as u16;
            for var in &program.sections[si].variables {
                let key = format!("{}:{}", sect_name, var.name);
                let local_off = self.global_offsets.get(&key).cloned().unwrap_or(0) - start;
                let sym_name = format!("{}.{}", sect_name, var.name);
                let name_off = Self::elf_strtab_insert(&mut strtab, &mut str_cache, &sym_name);
                let idx = symbols.len() as u32;
                let var_size = self.get_type_size_internal(&var.data_type);
                symbols.push((name_off, 18, 0, shndx, local_off as u64, var_size as u64));
                data_sym.insert(key, idx);
            }
        }

        let rodata_shndx = (2 + program.sections.len()) as u16;
        let mut string_keys: Vec<(String, u32)> = Vec::new();
        let mut float_keys: Vec<(String, u32)> = Vec::new();
        for (key, &off) in &self.global_offsets {
            if off >= user_data_end {
                if key.starts_with("str:") {
                    string_keys.push((key.clone(), off));
                } else if key.starts_with("float:") {
                    float_keys.push((key.clone(), off));
                }
            }
        }
        string_keys.sort_by_key(|x| x.1);
        float_keys.sort_by_key(|x| x.1);
        for (i, (key, off)) in string_keys.iter().enumerate() {
            let sym_name = format!(".Lstr{}", i);
            let name_off = Self::elf_strtab_insert(&mut strtab, &mut str_cache, &sym_name);
            let idx = symbols.len() as u32;
            let local_off = off - user_data_end;
            symbols.push((name_off, 1, 0, rodata_shndx, local_off as u64, 0));
            data_sym.insert(key.clone(), idx);
        }
        for (i, (key, off)) in float_keys.iter().enumerate() {
            let sym_name = format!(".Lfloat{}", i);
            let name_off = Self::elf_strtab_insert(&mut strtab, &mut str_cache, &sym_name);
            let idx = symbols.len() as u32;
            let local_off = off - user_data_end;
            symbols.push((name_off, 1, 0, rodata_shndx, local_off as u64, 8));
            data_sym.insert(key.clone(), idx);
        }

        let has_explicit_exports = program
            .functions
            .iter()
            .any(|f| f.is_export && !f.is_extern && f.body.is_some());
        let mut func_sym: HashMap<String, u32> = HashMap::new();
        for func in &program.functions {
            if func.is_extern || func.body.is_none() {
                continue;
            }
            let is_global = !has_explicit_exports || func.is_export;
            if is_global {
                continue;
            }
            if let Some(&off) = self.function_offsets.get(&func.name) {
                let name_off = Self::elf_strtab_insert(&mut strtab, &mut str_cache, &func.name);
                let idx = symbols.len() as u32;
                symbols.push((name_off, 2, 0, 1, off as u64, 0));
                func_sym.insert(func.name.clone(), idx);
            }
        }
        let first_global = symbols.len() as u32;
        for func in &program.functions {
            if func.is_extern || func.body.is_none() {
                continue;
            }
            let is_global = !has_explicit_exports || func.is_export;
            if !is_global {
                continue;
            }
            if let Some(&off) = self.function_offsets.get(&func.name) {
                let name_off = Self::elf_strtab_insert(&mut strtab, &mut str_cache, &func.name);
                let idx = symbols.len() as u32;
                symbols.push((name_off, 18, 0, 1, off as u64, 0));
                func_sym.insert(func.name.clone(), idx);
            }
        }
        let mut undefined: Vec<String> = Vec::new();
        for (_, target) in &self.call_patches {
            if !self.function_offsets.contains_key(target) && !undefined.contains(target) {
                undefined.push(target.clone());
            }
        }
        for name in &undefined {
            let name_off = Self::elf_strtab_insert(&mut strtab, &mut str_cache, name);
            let idx = symbols.len() as u32;
            symbols.push((name_off, 16, 0, 0, 0, 0));
            func_sym.insert(name.clone(), idx);
        }

        let mut rela = Vec::new();
        for (patch_pos, target) in &self.call_patches {
            let sym = func_sym.get(target).cloned().unwrap_or(0);
            Self::elf_push_u64(&mut rela, *patch_pos as u64);
            Self::elf_push_u64(&mut rela, ((sym as u64) << 32) | 2u64);
            Self::elf_push_i64(&mut rela, -4);
        }
        for (patch_pos, key, is_rip) in &self.address_patches {
            let sym = data_sym.get(key).cloned().unwrap_or(0);
            let typ = if *is_rip { 2u64 } else { 1u64 };
            let addend = if *is_rip { -4i64 } else { 0i64 };
            Self::elf_push_u64(&mut rela, *patch_pos as u64);
            Self::elf_push_u64(&mut rela, ((sym as u64) << 32) | typ);
            Self::elf_push_i64(&mut rela, addend);
        }

        let mut symtab = Vec::new();
        for (name, info, other, shndx, value, size) in &symbols {
            Self::elf_push_u32(&mut symtab, *name);
            symtab.push(*info);
            symtab.push(*other);
            Self::elf_push_u16(&mut symtab, *shndx);
            Self::elf_push_u64(&mut symtab, *value);
            Self::elf_push_u64(&mut symtab, *size);
        }

        let text_size = self.code.len();
        let rela_size = rela.len();
        let symtab_size = symtab.len();
        let strtab_size = strtab.len();
        let shstrtab_size = shstrtab.len();

        let mut body = Vec::new();
        let text_off = 64usize + body.len();
        body.extend_from_slice(&self.code);
        while (64usize + body.len()) % 16 != 0 {
            body.push(0);
        }

        let mut sect_offsets = Vec::new();
        for (_si, (_name, start, size, align, _ro, noinit)) in sect_data_ranges.iter().enumerate() {
            let off = 64usize + body.len();
            if !noinit {
                let chunk = &global_data_bytes[*start as usize..(*start + *size) as usize];
                body.extend_from_slice(chunk);
            }
            let align_usize = *align as usize;
            while (64usize + body.len()) % align_usize.max(1) != 0 {
                body.push(0);
            }
            sect_offsets.push(off);
        }

        let rodata_off = 64usize + body.len();
        let rodata_data = &global_data_bytes[user_data_end as usize..];
        body.extend_from_slice(rodata_data);
        while (64usize + body.len()) % 8 != 0 {
            body.push(0);
        }

        let rela_off = 64usize + body.len();
        body.extend_from_slice(&rela);
        while (64usize + body.len()) % 8 != 0 {
            body.push(0);
        }
        let symtab_off = 64usize + body.len();
        body.extend_from_slice(&symtab);
        while (64usize + body.len()) % 8 != 0 {
            body.push(0);
        }
        let strtab_off = 64usize + body.len();
        body.extend_from_slice(&strtab);
        while (64usize + body.len()) % 8 != 0 {
            body.push(0);
        }
        let shstrtab_off = 64usize + body.len();
        body.extend_from_slice(&shstrtab);
        while (64usize + body.len()) % 8 != 0 {
            body.push(0);
        }
        let shoff = 64usize + body.len();

        let num_sect_headers = 1 + 1 + program.sections.len() + 1 + 1 + 1 + 1 + 1 + 1;

        let mut elf = Vec::new();
        elf.extend_from_slice(&[0x7F, b'E', b'L', b'F', 2, 1, 1, 0]);
        elf.extend_from_slice(&[0; 8]);
        Self::elf_push_u16(&mut elf, 1);
        Self::elf_push_u16(&mut elf, 62);
        Self::elf_push_u32(&mut elf, 1);
        Self::elf_push_u64(&mut elf, 0);
        Self::elf_push_u64(&mut elf, 0);
        Self::elf_push_u64(&mut elf, shoff as u64);
        Self::elf_push_u32(&mut elf, 0);
        Self::elf_push_u16(&mut elf, 64);
        Self::elf_push_u16(&mut elf, 0);
        Self::elf_push_u16(&mut elf, 0);
        Self::elf_push_u16(&mut elf, 64);
        Self::elf_push_u16(&mut elf, num_sect_headers as u16);
        Self::elf_push_u16(&mut elf, 6);

        elf.extend_from_slice(&body);

        Self::elf_push_shdr(&mut elf, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
        Self::elf_push_shdr(
            &mut elf,
            n_text,
            1,
            6,
            0,
            text_off as u64,
            text_size as u64,
            0,
            0,
            16,
            0,
        );

        let symtab_shndx = (2 + program.sections.len() + 1 + 1) as u32;
        let text_shndx = 1u32;

        for (si, (_name, _start, size, align, ro, noinit)) in sect_data_ranges.iter().enumerate() {
            let flags: u64 = if *noinit {
                3
            } else if *ro {
                2
            } else {
                3
            };
            let sh_type: u32 = if *noinit { 8 } else { 1 };
            let off = sect_offsets[si];
            let sz = if *noinit { *size as u64 } else { *size as u64 };
            Self::elf_push_shdr(
                &mut elf,
                section_name_offsets[si],
                sh_type,
                flags,
                0,
                off as u64,
                sz,
                0,
                0,
                *align as u64,
                0,
            );
        }

        let rodata_size = rodata_data.len();
        Self::elf_push_shdr(
            &mut elf,
            n_rodata,
            1,
            2,
            0,
            rodata_off as u64,
            rodata_size as u64,
            0,
            0,
            8,
            0,
        );

        let _rela_shndx = (2 + program.sections.len() + 1) as u32;
        Self::elf_push_shdr(
            &mut elf,
            n_rela,
            4,
            64,
            0,
            rela_off as u64,
            rela_size as u64,
            symtab_shndx,
            text_shndx,
            8,
            24,
        );
        Self::elf_push_shdr(
            &mut elf,
            n_symtab,
            2,
            0,
            0,
            symtab_off as u64,
            symtab_size as u64,
            symtab_shndx + 1,
            first_global,
            8,
            24,
        );
        Self::elf_push_shdr(
            &mut elf,
            n_strtab,
            3,
            0,
            0,
            strtab_off as u64,
            strtab_size as u64,
            0,
            0,
            1,
            0,
        );
        Self::elf_push_shdr(
            &mut elf,
            n_shstrtab,
            3,
            0,
            0,
            shstrtab_off as u64,
            shstrtab_size as u64,
            0,
            0,
            1,
            0,
        );
        Self::elf_push_shdr(&mut elf, n_note, 1, 0, 0, 0, 0, 0, 0, 1, 0);
        elf
    }
    fn emit_mov_imm64(&mut self, reg: u8, value: u64) {
        if value <= 0xFFFFFFFF {
            if reg >= 8 {
                self.code.push(0x41);
                self.code.push(0xB8 + (reg & 7));
            } else {
                self.code.push(0xB8 + reg);
            }

            self.code.extend_from_slice(&(value as u32).to_le_bytes());
        } else {
            if reg >= 8 {
                self.code.push(0x49);
                self.code.push(0xB8 + (reg & 7));
            } else {
                self.code.push(0x48);
                self.code.push(0xB8 + reg);
            }

            self.code.extend_from_slice(&value.to_le_bytes());
        }
    }
    fn unescape_wand_string(raw: &str) -> Vec<u8> {
        let bytes = raw.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0usize;

        while i < bytes.len() {
            if bytes[i] == 92 && i + 1 < bytes.len() {
                match bytes[i + 1] {
                    b'n' => {
                        out.push(10);
                        i += 2;
                    }
                    b't' => {
                        out.push(9);
                        i += 2;
                    }
                    b'r' => {
                        out.push(13);
                        i += 2;
                    }
                    b'"' => {
                        out.push(34);
                        i += 2;
                    }
                    b'\\' => {
                        out.push(92);
                        i += 2;
                    }
                    b'0'..=b'7' => {
                        let mut value = 0u8;
                        let mut count = 0usize;
                        let mut j = i + 1;

                        while j < bytes.len() && count < 3 && bytes[j] >= b'0' && bytes[j] <= b'7' {
                            value = value * 8 + (bytes[j] - b'0');
                            j += 1;
                            count += 1;
                        }

                        out.push(value);
                        i = j;
                    }
                    _ => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }

        out
    }
}
