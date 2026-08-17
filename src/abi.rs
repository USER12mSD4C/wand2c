#![allow(dead_code)]
use crate::ast::{DataType, Program};
use crate::codegen::NativeGenerator;
use std::collections::HashMap;

pub const P46_SECTION_TYPES: u32 = 1;
pub const P46_SECTION_EXPORTS: u32 = 2;
pub const P46_SECTION_REFLECT: u32 = 3;
pub const P46_SECTION_IMPORTS: u32 = 4;
pub const P46_SECTION_DEPENDENCIES: u32 = 5;

pub const P46_EXPORT_KIND_FUNCTION: u8 = 1;
pub const P46_EXPORT_KIND_VARIABLE: u8 = 2;
pub const P46_EXPORT_KIND_TYPE: u8 = 3;

pub const HEADER_SIZE: usize = 36;
pub const SECTION_DESC_SIZE: usize = 20;
pub const ADDRESS_SIZE: u8 = 8;
pub const POINTER_SIZE: u8 = 8;

pub struct Strtab {
    data: Vec<u8>,
    offsets: HashMap<String, u32>,
}

impl Strtab {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            offsets: HashMap::new(),
        }
    }

    pub fn insert(&mut self, s: &str) -> u32 {
        if let Some(&offset) = self.offsets.get(s) {
            return offset;
        }
        let offset = self.data.len() as u32;
        self.data.extend_from_slice(s.as_bytes());
        self.data.push(0);
        self.offsets.insert(s.to_string(), offset);
        offset
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

pub struct BinaryBuilder {
    pub strtab: Strtab,
    pub types_section: Vec<u8>,
    pub exports_section: Vec<u8>,
    pub exports_count: u32,
    pub imports_section: Vec<u8>,
    pub imports_count: u32,
    pub deps_section: Vec<u8>,
    pub deps_count: u32,
    pub reflect_section: Vec<u8>,
    pub reflect_count: u32,
    pub address_size: u8,
}

impl BinaryBuilder {
    pub fn new() -> Self {
        Self {
            strtab: Strtab::new(),
            types_section: Vec::new(),
            exports_section: Vec::new(),
            exports_count: 0,
            imports_section: Vec::new(),
            imports_count: 0,
            deps_section: Vec::new(),
            deps_count: 0,
            reflect_section: Vec::new(),
            reflect_count: 0,
            address_size: ADDRESS_SIZE,
        }
    }

    pub fn add_dependency(&mut self, name: &str, major: u32, minor: u32, patch: u32, build: u32) {
        if self.deps_count == 0 {
            self.deps_section.extend_from_slice(&0u32.to_le_bytes());
        }
        let name_offset = self.strtab.insert(name);
        self.deps_section
            .extend_from_slice(&name_offset.to_le_bytes());
        self.deps_section.extend_from_slice(&major.to_le_bytes());
        self.deps_section.extend_from_slice(&minor.to_le_bytes());
        self.deps_section.extend_from_slice(&patch.to_le_bytes());
        self.deps_section.extend_from_slice(&build.to_le_bytes());
        self.deps_count += 1;
    }

    pub fn add_type_record(&mut self, type_id: u16, payload: &[u8]) {
        self.types_section.extend_from_slice(&type_id.to_le_bytes());
        self.types_section
            .extend_from_slice(&(payload.len() as u32).to_le_bytes());
        self.types_section.extend_from_slice(payload);
    }

    fn write_address(address_size: u8, buf: &mut Vec<u8>, address: u64) {
        match address_size {
            1 => buf.push(address as u8),
            2 => buf.extend_from_slice(&(address as u16).to_le_bytes()),
            4 => buf.extend_from_slice(&(address as u32).to_le_bytes()),
            _ => buf.extend_from_slice(&address.to_le_bytes()),
        }
    }

    pub fn add_export(
        &mut self,
        name: &str,
        module_name: &str,
        version: u32,
        kind: u8,
        address: u64,
        type_id: u32,
        param_types: &[u32],
        return_type: u32,
    ) {
        if self.exports_count == 0 {
            self.exports_section.extend_from_slice(&0u32.to_le_bytes());
        }
        let name_offset = self.strtab.insert(name);
        let mod_offset = self.strtab.insert(module_name);
        self.exports_section
            .extend_from_slice(&name_offset.to_le_bytes());
        self.exports_section
            .extend_from_slice(&mod_offset.to_le_bytes());
        self.exports_section
            .extend_from_slice(&version.to_le_bytes());
        self.exports_section.push(kind);
        Self::write_address(self.address_size, &mut self.exports_section, address);
        self.exports_section
            .extend_from_slice(&type_id.to_le_bytes());
        if kind == P46_EXPORT_KIND_FUNCTION {
            self.exports_section
                .extend_from_slice(&(param_types.len() as u32).to_le_bytes());
            self.exports_section
                .extend_from_slice(&return_type.to_le_bytes());
            for p_type in param_types {
                self.exports_section
                    .extend_from_slice(&p_type.to_le_bytes());
            }
        } else {
            self.exports_section.extend_from_slice(&0u32.to_le_bytes());
            self.exports_section.extend_from_slice(&0u32.to_le_bytes());
        }
        self.exports_count += 1;
    }

    pub fn add_import(
        &mut self,
        name: &str,
        module_name: &str,
        required_version: u32,
        required_type_version: u32,
    ) {
        if self.imports_count == 0 {
            self.imports_section.extend_from_slice(&0u32.to_le_bytes());
        }
        let name_offset = self.strtab.insert(name);
        let mod_offset = self.strtab.insert(module_name);
        self.imports_section
            .extend_from_slice(&name_offset.to_le_bytes());
        self.imports_section
            .extend_from_slice(&mod_offset.to_le_bytes());
        self.imports_section
            .extend_from_slice(&required_version.to_le_bytes());
        self.imports_section
            .extend_from_slice(&required_type_version.to_le_bytes());
        self.imports_count += 1;
    }

    pub fn add_reflect_entry(
        &mut self,
        qualified_name: &str,
        target_kind: u8,
        target_index: u32,
        version: u32,
    ) {
        if self.reflect_count == 0 {
            self.reflect_section.extend_from_slice(&0u32.to_le_bytes());
        }
        let name_offset = self.strtab.insert(qualified_name);
        self.reflect_section
            .extend_from_slice(&name_offset.to_le_bytes());
        self.reflect_section.push(target_kind);
        self.reflect_section
            .extend_from_slice(&target_index.to_le_bytes());
        self.reflect_section
            .extend_from_slice(&version.to_le_bytes());
        self.reflect_count += 1;
    }

    fn patch_section_counts(&mut self) {
        if self.deps_count > 0 {
            self.deps_section[0..4].copy_from_slice(&self.deps_count.to_le_bytes());
        }
        if self.exports_count > 0 {
            self.exports_section[0..4].copy_from_slice(&self.exports_count.to_le_bytes());
        }
        if self.imports_count > 0 {
            self.imports_section[0..4].copy_from_slice(&self.imports_count.to_le_bytes());
        }
        if self.reflect_count == 0 {
            self.reflect_section.extend_from_slice(&0u32.to_le_bytes());
        } else {
            self.reflect_section[0..4].copy_from_slice(&self.reflect_count.to_le_bytes());
        }
    }

    fn build_header_and_descriptors(&self, sections_start: u64) -> (Vec<u8>, Vec<u64>, Vec<u64>) {
        let mut header = Vec::with_capacity(HEADER_SIZE);

        header.extend_from_slice(&[0x50, 0x34, 0x36, 0x00]);
        header.push(1);
        header.push(6);
        header.push(0);
        header.push(1);
        header.push(POINTER_SIZE);
        header.push(self.address_size);
        header.extend_from_slice(&[0, 0]);
        header.extend_from_slice(&0x01000000u32.to_le_bytes());
        header.extend_from_slice(&5u32.to_le_bytes());

        let header_end = sections_start;
        let types_offset = header_end;
        let types_size = self.types_section.len() as u64;
        let exports_offset = types_offset + types_size;
        let exports_size = self.exports_section.len() as u64;
        let reflect_offset = exports_offset + exports_size;
        let reflect_size = self.reflect_section.len() as u64;
        let imports_offset = reflect_offset + reflect_size;
        let imports_size = self.imports_section.len() as u64;
        let deps_offset = imports_offset + imports_size;
        let deps_size = self.deps_section.len() as u64;
        let strtab_offset = deps_offset + deps_size;
        let strtab_size = self.strtab.as_bytes().len() as u64;

        header.extend_from_slice(&strtab_offset.to_le_bytes());
        header.extend_from_slice(&strtab_size.to_le_bytes());

        let offsets = vec![
            types_offset,
            exports_offset,
            reflect_offset,
            imports_offset,
            deps_offset,
        ];
        let sizes = vec![
            types_size,
            exports_size,
            reflect_size,
            imports_size,
            deps_size,
        ];

        let section_types = [
            P46_SECTION_TYPES,
            P46_SECTION_EXPORTS,
            P46_SECTION_REFLECT,
            P46_SECTION_IMPORTS,
            P46_SECTION_DEPENDENCIES,
        ];

        for i in 0..5 {
            header.extend_from_slice(&offsets[i].to_le_bytes());
            header.extend_from_slice(&sizes[i].to_le_bytes());
            header.extend_from_slice(&section_types[i].to_le_bytes());
        }

        (header, offsets, sizes)
    }

    pub fn build_binary_image(&mut self) -> Vec<u8> {
        self.patch_section_counts();

        let sections_start = HEADER_SIZE as u64 + (SECTION_DESC_SIZE as u64 * 5);
        let (header, _offsets, _sizes) = self.build_header_and_descriptors(sections_start);

        let mut image = Vec::new();
        image.extend_from_slice(&header);
        image.extend_from_slice(&self.types_section);
        image.extend_from_slice(&self.exports_section);
        image.extend_from_slice(&self.reflect_section);
        image.extend_from_slice(&self.imports_section);
        image.extend_from_slice(&self.deps_section);
        image.extend_from_slice(self.strtab.as_bytes());
        image
    }
}

pub fn generate_elf64_binary(
    payload_bytes: &[u8],
    program: &Program,
    gen: &NativeGenerator,
) -> Vec<u8> {
    let mut builder = BinaryBuilder::new();

    for imp in &program.imports {
        builder.add_dependency(imp, 1, 0, 0, 0);
    }

    for s in &program.structs {
        let name_off = builder.strtab.insert(&s.name);
        let mut fields_data = Vec::new();
        for field in &s.fields {
            let f_name_off = builder.strtab.insert(&field.name);
            fields_data.extend_from_slice(&f_name_off.to_le_bytes());
            let type_id = match &field.data_type {
                DataType::U8 | DataType::I8 => 1u32,
                DataType::U16 | DataType::I16 => 2u32,
                DataType::U32 | DataType::I32 => 3u32,
                DataType::U64 | DataType::I64 => 4u32,
                DataType::F64 => 10u32,
                DataType::Pointer(_) => 11u32,
                DataType::Array(..) => 6u32,
                DataType::Typedef(..) => 9u32,
                _ => 5u32,
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
        builder.add_type_record(1, &val);
    }

    for (name, dt) in &program.typedefs {
        let alias_off = builder.strtab.insert(name);
        let underlying_id = match dt {
            DataType::Array(..) => 6u32,
            DataType::U64 | DataType::I64 => 4u32,
            DataType::U32 | DataType::I32 => 3u32,
            DataType::F64 => 10u32,
            _ => 5u32,
        };
        let mut val = Vec::new();
        val.extend_from_slice(&alias_off.to_le_bytes());
        val.extend_from_slice(&underlying_id.to_le_bytes());
        builder.add_type_record(9, &val);
    }

    for func in &program.functions {
        let local_offset = gen.function_offsets.get(&func.name).cloned().unwrap_or(0);
        let abs_addr = 0x400078u64 + (local_offset as u64);
        let mut param_types = Vec::new();
        for _ in &func.params {
            param_types.push(4u32);
        }
        builder.add_export(
            &func.name,
            "main_module",
            1,
            P46_EXPORT_KIND_FUNCTION,
            abs_addr,
            4,
            &param_types,
            4,
        );
    }

    let mut unresolved_calls = Vec::new();
    for (_, target_name) in &gen.call_patches {
        if !gen.function_offsets.contains_key(target_name) {
            if !unresolved_calls.contains(target_name) {
                unresolved_calls.push(target_name.clone());
            }
        }
    }
    let module_name = program
        .imports
        .first()
        .map(|s| s.trim_matches(|c| c == '<' || c == '>').to_string())
        .unwrap_or_else(|| "libc.ko".to_string());
    for name in &unresolved_calls {
        builder.add_import(name, &module_name, 1, 0);
    }

    builder.patch_section_counts();

    let p46_types = &builder.types_section;
    let p46_exports = &builder.exports_section;
    let p46_reflect = &builder.reflect_section;
    let p46_imports = &builder.imports_section;
    let p46_deps = &builder.deps_section;
    let p46_strtab = builder.strtab.as_bytes();

    let text_offset = 120usize;
    let text_size = payload_bytes.len();
    let p46_hdr_offset = text_offset + text_size;
    let p46_hdr_size = HEADER_SIZE + (SECTION_DESC_SIZE * 5);

    let sections_start = p46_hdr_offset + p46_hdr_size;
    let p46_types_offset = sections_start;
    let p46_types_size = p46_types.len();
    let p46_exp_offset = p46_types_offset + p46_types_size;
    let p46_exp_size = p46_exports.len();
    let p46_refl_offset = p46_exp_offset + p46_exp_size;
    let p46_refl_size = p46_reflect.len();
    let p46_imp_offset = p46_refl_offset + p46_refl_size;
    let p46_imp_size = p46_imports.len();
    let p46_deps_offset = p46_imp_offset + p46_imp_size;
    let p46_deps_size = p46_deps.len();
    let p46_strtab_offset = p46_deps_offset + p46_deps_size;
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

    let mut p46_header = Vec::with_capacity(p46_hdr_size);
    p46_header.extend_from_slice(&[0x50, 0x34, 0x36, 0x00]);
    p46_header.push(1);
    p46_header.push(6);
    p46_header.push(0);
    p46_header.push(1);
    p46_header.push(POINTER_SIZE);
    p46_header.push(ADDRESS_SIZE);
    p46_header.extend_from_slice(&[0, 0]);
    p46_header.extend_from_slice(&0x01000000u32.to_le_bytes());
    p46_header.extend_from_slice(&5u32.to_le_bytes());
    p46_header.extend_from_slice(&(p46_strtab_offset as u64).to_le_bytes());
    p46_header.extend_from_slice(&(p46_strtab_size as u64).to_le_bytes());

    let section_offsets = [
        p46_types_offset,
        p46_exp_offset,
        p46_refl_offset,
        p46_imp_offset,
        p46_deps_offset,
    ];
    let section_sizes = [
        p46_types_size,
        p46_exp_size,
        p46_refl_size,
        p46_imp_size,
        p46_deps_size,
    ];
    let section_types = [
        P46_SECTION_TYPES,
        P46_SECTION_EXPORTS,
        P46_SECTION_REFLECT,
        P46_SECTION_IMPORTS,
        P46_SECTION_DEPENDENCIES,
    ];

    for i in 0..5 {
        p46_header.extend_from_slice(&(section_offsets[i] as u64).to_le_bytes());
        p46_header.extend_from_slice(&(section_sizes[i] as u64).to_le_bytes());
        p46_header.extend_from_slice(&section_types[i].to_le_bytes());
    }

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
    elf.extend_from_slice(p46_types);
    elf.extend_from_slice(p46_exports);
    elf.extend_from_slice(p46_reflect);
    elf.extend_from_slice(p46_imports);
    elf.extend_from_slice(p46_deps);
    elf.extend_from_slice(p46_strtab);
    elf.extend_from_slice(&shstrtab);

    let build_shdr =
        |name: u32, ty: u32, flags: u64, addr: u64, offset: u64, size: u64| -> Vec<u8> {
            let mut shdr = Vec::new();
            shdr.extend_from_slice(&name.to_le_bytes());
            shdr.extend_from_slice(&ty.to_le_bytes());
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
