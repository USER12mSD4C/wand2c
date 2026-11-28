#![allow(dead_code)]
use crate::ast::{DataType, EnumDecl, Program, StructDecl};
use crate::codegen::CodegenOutput;
use std::collections::HashMap;

pub const P46_SECTION_TYPES: u32 = 1;
pub const P46_SECTION_EXPORTS: u32 = 2;
pub const P46_SECTION_REFLECT: u32 = 3;
pub const P46_SECTION_IMPORTS: u32 = 4;
pub const P46_SECTION_DEPENDENCIES: u32 = 5;

/* Wand continuation-object extensions (vendor range, section 3.6 of the
 * Standard 4/6 spec). These are ordinary P46 sections; a loader that does not
 * implement continuations can still read the five mandatory sections. */
pub const P46_SECTION_WEXP_IMAGE: u32 = 100;
pub const P46_SECTION_WEXP_RELOCS: u32 = 101;

pub const WEXP_RELOC_64: u32 = 1;
pub const WEXP_RELOC_PC32: u32 = 4;
pub const WEXP_RELOC_ENTRY_SIZE: usize = 24;

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
    pub vendor_sections: Vec<(u32, Vec<u8>)>,
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
            vendor_sections: Vec::new(),
            address_size: ADDRESS_SIZE,
        }
    }

    pub fn add_vendor_section(&mut self, section_type: u32, data: Vec<u8>) {
        debug_assert!(section_type >= 100);
        self.vendor_sections.push((section_type, data));
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
        fn set_count(section: &mut Vec<u8>, count: u32) {
            if section.len() < 4 {
                section.resize(4, 0);
            }
            section[0..4].copy_from_slice(&count.to_le_bytes());
        }
        set_count(&mut self.deps_section, self.deps_count);
        set_count(&mut self.exports_section, self.exports_count);
        set_count(&mut self.imports_section, self.imports_count);
        set_count(&mut self.reflect_section, self.reflect_count);
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
    gen: &CodegenOutput,
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

    let has_explicit_exports = program
        .functions
        .iter()
        .any(|f| f.is_export && !f.is_extern && f.body.is_some());

    for func in &program.functions {
        if func.is_extern || func.body.is_none() {
            continue;
        }

        if has_explicit_exports && !func.is_export {
            continue;
        }

        let local_offset = match gen.function_offsets.get(&func.name) {
            Some(offset) => *offset,
            None => continue,
        };

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
        .map(|s| {
            s.trim_matches(|c| c == '<' || c == '>' || c == '"')
                .to_string()
        })
        .unwrap_or_else(|| "libc.ko".to_string());
    let import_versions: HashMap<&str, u32> = program
        .functions
        .iter()
        .map(|f| (f.name.as_str(), f.version))
        .collect();
    for name in &unresolved_calls {
        let version = import_versions.get(name.as_str()).copied().unwrap_or(1);
        builder.add_import(name, &module_name, version, 0);
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

/* ------------------------------------------------------------------------- */
/* Continuation objects (-fw)                                                */
/* ------------------------------------------------------------------------- */

const STRUCT_TYPE_ID_BASE: u32 = 1000;
const ENUM_TYPE_ID_BASE: u32 = 2000;
const TYPEDEF_TYPE_ID_BASE: u32 = 3000;

fn align_up(value: u32, align: u32) -> u32 {
    let a = align.max(1);
    (value + a - 1) & !(a - 1)
}

fn primitive_type_id(dt: &DataType) -> u32 {
    match dt {
        DataType::U8 => 1,
        DataType::U16 => 2,
        DataType::U32 => 3,
        DataType::U64 => 4,
        DataType::I8 => 5,
        DataType::I16 => 6,
        DataType::I32 => 7,
        DataType::I64 => 8,
        DataType::F64 => 10,
        DataType::Pointer(_) => 11,
        DataType::Void => 0,
        DataType::Array(..) => 6,
        DataType::Struct(..) | DataType::Typedef(..) => 0,
    }
}

fn struct_index(program: &Program, name: &str) -> Option<usize> {
    program.structs.iter().position(|s| s.name == name)
}

fn typedef_index(program: &Program, name: &str) -> Option<usize> {
    program.typedefs.iter().position(|(n, _)| n == name)
}

fn type_id(dt: &DataType, program: &Program) -> u32 {
    match dt {
        DataType::Struct(name) => struct_index(program, name)
            .map(|i| STRUCT_TYPE_ID_BASE + i as u32)
            .unwrap_or(STRUCT_TYPE_ID_BASE),
        DataType::Typedef(name, _) => typedef_index(program, name)
            .map(|i| TYPEDEF_TYPE_ID_BASE + i as u32)
            .unwrap_or(TYPEDEF_TYPE_ID_BASE),
        _ => primitive_type_id(dt),
    }
}

fn struct_alignment(s: &StructDecl, program: &Program, depth: usize) -> u32 {
    if s.is_packed || s.alignment == 1 {
        return 1;
    }
    let natural = s
        .fields
        .iter()
        .map(|f| type_alignment(&f.data_type, program, depth + 1))
        .max()
        .unwrap_or(1)
        .min(8);
    if s.alignment > 1 {
        natural.max(s.alignment.min(8))
    } else {
        natural
    }
}

fn type_alignment(dt: &DataType, program: &Program, depth: usize) -> u32 {
    if depth > 16 {
        return 8;
    }
    match dt {
        DataType::U8 | DataType::I8 => 1,
        DataType::U16 | DataType::I16 => 2,
        DataType::U32 | DataType::I32 => 4,
        DataType::U64 | DataType::I64 | DataType::F64 => 8,
        DataType::Pointer(_) => POINTER_SIZE as u32,
        DataType::Array(elem, _) => type_alignment(elem, program, depth + 1),
        DataType::Struct(name) => program
            .structs
            .iter()
            .find(|s| &s.name == name)
            .map(|s| struct_alignment(s, program, depth + 1))
            .unwrap_or(1),
        DataType::Typedef(_, inner) => type_alignment(inner, program, depth + 1),
        DataType::Void => 1,
    }
}

fn struct_layout(s: &StructDecl, program: &Program, depth: usize) -> (u32, Vec<u32>) {
    let align = struct_alignment(s, program, depth);
    let mut offsets = Vec::with_capacity(s.fields.len());
    let mut cursor = 0u32;
    let mut max_size = 0u32;
    for field in &s.fields {
        let size = type_size(&field.data_type, program, depth + 1);
        if s.is_union {
            offsets.push(0);
            max_size = max_size.max(size);
        } else {
            let field_align = if s.is_packed {
                1
            } else {
                type_alignment(&field.data_type, program, depth + 1).min(align)
            };
            cursor = align_up(cursor, field_align);
            offsets.push(cursor);
            cursor = cursor.saturating_add(size);
        }
    }
    let size = if s.is_union {
        align_up(max_size, align)
    } else {
        align_up(cursor, align)
    };
    (size, offsets)
}

fn type_size(dt: &DataType, program: &Program, depth: usize) -> u32 {
    if depth > 16 {
        return 8;
    }
    match dt {
        DataType::U8 | DataType::I8 => 1,
        DataType::U16 | DataType::I16 => 2,
        DataType::U32 | DataType::I32 => 4,
        DataType::U64 | DataType::I64 | DataType::F64 => 8,
        DataType::Pointer(_) => POINTER_SIZE as u32,
        DataType::Array(elem, count) => type_size(elem, program, depth + 1)
            .saturating_mul(*count as u32),
        DataType::Struct(name) => program
            .structs
            .iter()
            .find(|s| &s.name == name)
            .map(|s| struct_layout(s, program, depth + 1).0)
            .unwrap_or(16),
        DataType::Typedef(_, inner) => type_size(inner, program, depth + 1),
        DataType::Void => 0,
    }
}

fn enum_size(e: &EnumDecl) -> u32 {
    if e.values.len() <= 256 {
        1
    } else if e.values.len() <= 65536 {
        2
    } else {
        4
    }
}

fn import_module(target: &str, program: &Program) -> String {
    if target == "__wand_jmpto_loader" {
        return "wand.rt".to_string();
    }
    program
        .imports
        .first()
        .map(|s| {
            s.trim_matches(|c| c == '<' || c == '>' || c == '"' || c == '\'')
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "libc.ko".to_string())
}

fn read_u64_at(data: &[u8], pos: usize) -> u64 {
    if pos + 8 > data.len() {
        return 0;
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&data[pos..pos + 8]);
    u64::from_le_bytes(bytes)
}

/// Build a continuation object: a flat Standard 4/6 Raw Binary image with no
/// ELF header, no process entry stub and no fixed load address. Code and data
/// live in a vendor section; a second vendor section carries the relocations
/// the loader must apply after choosing a module base.
pub fn generate_wexp_image(
    payload_bytes: &[u8],
    program: &Program,
    gen: &CodegenOutput,
    module_name: &str,
) -> Vec<u8> {
    let mut builder = BinaryBuilder::new();

    for imp in &program.imports {
        builder.add_dependency(imp, 1, 0, 0, 0);
    }

    let mut type_record_index = 0u32;
    for s in &program.structs {
        let name_off = builder.strtab.insert(&s.name);
        let (size, offsets) = struct_layout(s, program, 0);
        let mut payload = Vec::new();
        payload.extend_from_slice(&name_off.to_le_bytes());
        payload.extend_from_slice(&s.version.to_le_bytes());
        payload.extend_from_slice(&size.to_le_bytes());
        payload.extend_from_slice(&(s.fields.len() as u32).to_le_bytes());
        for (field, offset) in s.fields.iter().zip(offsets.iter()) {
            let field_name = builder.strtab.insert(&field.name);
            payload.extend_from_slice(&field_name.to_le_bytes());
            payload.extend_from_slice(&type_id(&field.data_type, program).to_le_bytes());
            payload.extend_from_slice(&offset.to_le_bytes());
            payload.extend_from_slice(&field.version_added.to_le_bytes());
            payload.extend_from_slice(&field.version_removed.to_le_bytes());
        }
        builder.add_type_record(if s.is_union { 3 } else { 1 }, &payload);
        builder.add_reflect_entry(
            &format!("{}:{}", module_name, s.name),
            1,
            type_record_index,
            s.version,
        );
        type_record_index += 1;
    }

    for e in &program.enums {
        let name_off = builder.strtab.insert(&e.name);
        let size = enum_size(e);
        let mut payload = Vec::new();
        payload.extend_from_slice(&name_off.to_le_bytes());
        payload.extend_from_slice(&e.version.to_le_bytes());
        payload.extend_from_slice(&size.to_le_bytes());
        payload.extend_from_slice(&(e.values.len() as u32).to_le_bytes());
        for value in &e.values {
            let value_name = builder.strtab.insert(&value.name);
            payload.extend_from_slice(&value_name.to_le_bytes());
            payload.extend_from_slice(&value.value.to_le_bytes());
            payload.extend_from_slice(&value.version_added.to_le_bytes());
            payload.extend_from_slice(&value.version_removed.to_le_bytes());
        }
        builder.add_type_record(2, &payload);
        builder.add_reflect_entry(
            &format!("{}:{}", module_name, e.name),
            1,
            type_record_index,
            e.version,
        );
        type_record_index += 1;
    }

    for (name, dt) in &program.typedefs {
        let alias_off = builder.strtab.insert(name);
        let mut payload = Vec::new();
        payload.extend_from_slice(&alias_off.to_le_bytes());
        payload.extend_from_slice(&type_id(dt, program).to_le_bytes());
        builder.add_type_record(9, &payload);
        builder.add_reflect_entry(
            &format!("{}:{}", module_name, name),
            1,
            type_record_index,
            1,
        );
        type_record_index += 1;
    }

    let has_explicit_exports = program
        .functions
        .iter()
        .any(|f| f.is_export && !f.is_extern && f.body.is_some());

    let mut export_index = 0u32;
    for func in &program.functions {
        if func.is_extern || func.body.is_none() {
            continue;
        }
        if has_explicit_exports && !func.is_export && func.name != "main" {
            continue;
        }
        let local_offset = match gen.function_offsets.get(&func.name) {
            Some(offset) => *offset,
            None => continue,
        };

        let return_type = func
            .return_types
            .first()
            .map(|dt| type_id(dt, program))
            .unwrap_or(0);
        let param_types: Vec<u32> = func
            .params
            .iter()
            .map(|(dt, _, _)| type_id(dt, program))
            .collect();

        let mut fn_payload = Vec::new();
        let name_off = builder.strtab.insert(&func.name);
        fn_payload.extend_from_slice(&name_off.to_le_bytes());
        fn_payload.extend_from_slice(&func.version.to_le_bytes());
        fn_payload.extend_from_slice(&return_type.to_le_bytes());
        fn_payload.extend_from_slice(&(param_types.len() as u32).to_le_bytes());
        for p in &param_types {
            fn_payload.extend_from_slice(&p.to_le_bytes());
        }
        builder.add_type_record(4, &fn_payload);

        builder.add_export(
            &func.name,
            module_name,
            func.version,
            P46_EXPORT_KIND_FUNCTION,
            local_offset as u64,
            return_type,
            &param_types,
            return_type,
        );
        builder.add_reflect_entry(
            &format!("{}:{}", module_name, func.name),
            2,
            export_index,
            func.version,
        );
        export_index += 1;
    }

    let mut unresolved_calls = Vec::new();
    for (_, target_name) in &gen.call_patches {
        if !gen.function_offsets.contains_key(target_name) && !unresolved_calls.contains(target_name) {
            unresolved_calls.push(target_name.clone());
        }
    }
    let import_versions: HashMap<&str, u32> = program
        .functions
        .iter()
        .map(|f| (f.name.as_str(), f.version))
        .collect();
    for name in &unresolved_calls {
        let version = import_versions.get(name.as_str()).copied().unwrap_or(1);
        builder.add_import(name, &import_module(name, program), version, 0);
    }

    let func_addr_patches = gen
        .addr_patches
        .iter()
        .filter(|(_, key, is_rip)| *is_rip && key.starts_with("fn:"))
        .count();
    let mut relocs = Vec::with_capacity(
        (gen.call_patches.len()
            + gen.addr_patches.iter().filter(|(_, _, is_rip)| !*is_rip).count()
            + func_addr_patches)
            * WEXP_RELOC_ENTRY_SIZE,
    );
    for (pos, name) in &gen.call_patches {
        if gen.function_offsets.contains_key(name) {
            continue;
        }
        let name_off = builder.strtab.insert(name);
        relocs.extend_from_slice(&(*pos as u64).to_le_bytes());
        relocs.extend_from_slice(&WEXP_RELOC_PC32.to_le_bytes());
        relocs.extend_from_slice(&name_off.to_le_bytes());
        relocs.extend_from_slice(&(-4i64).to_le_bytes());
    }
    for (pos, key, is_rip) in &gen.addr_patches {
        if *is_rip {
            if let Some(name) = key.strip_prefix("fn:") {
                let name_off = builder.strtab.insert(name);
                relocs.extend_from_slice(&(*pos as u64).to_le_bytes());
                relocs.extend_from_slice(&WEXP_RELOC_PC32.to_le_bytes());
                relocs.extend_from_slice(&name_off.to_le_bytes());
                relocs.extend_from_slice(&(-4i64).to_le_bytes());
            }
            continue;
        }
        let target = read_u64_at(payload_bytes, *pos);
        let symbol = format!("@offset:{}", target);
        let name_off = builder.strtab.insert(&symbol);
        relocs.extend_from_slice(&(*pos as u64).to_le_bytes());
        relocs.extend_from_slice(&WEXP_RELOC_64.to_le_bytes());
        relocs.extend_from_slice(&name_off.to_le_bytes());
        relocs.extend_from_slice(&0i64.to_le_bytes());
    }

    builder.add_vendor_section(P46_SECTION_WEXP_IMAGE, payload_bytes.to_vec());
    builder.add_vendor_section(P46_SECTION_WEXP_RELOCS, relocs);
    builder.patch_section_counts();

    let mut sections = Vec::new();
    sections.push((P46_SECTION_TYPES, builder.types_section.clone()));
    sections.push((P46_SECTION_EXPORTS, builder.exports_section.clone()));
    sections.push((P46_SECTION_REFLECT, builder.reflect_section.clone()));
    sections.push((P46_SECTION_IMPORTS, builder.imports_section.clone()));
    sections.push((P46_SECTION_DEPENDENCIES, builder.deps_section.clone()));
    for (ty, data) in &builder.vendor_sections {
        sections.push((*ty, data.clone()));
    }

    let sections_start = HEADER_SIZE as u64 + SECTION_DESC_SIZE as u64 * sections.len() as u64;
    let mut cursor = sections_start;
    let mut descriptors = Vec::with_capacity(sections.len());
    for (ty, data) in &sections {
        descriptors.push((cursor, data.len() as u64, *ty));
        cursor += data.len() as u64;
    }
    let strtab_offset = cursor;
    let strtab_bytes = builder.strtab.as_bytes();
    let strtab_size = strtab_bytes.len() as u64;

    let mut header = Vec::with_capacity(HEADER_SIZE + SECTION_DESC_SIZE * sections.len());
    header.extend_from_slice(&[0x50, 0x34, 0x36, 0x00]);
    header.push(1);
    header.push(6);
    header.push(0);
    header.push(1);
    header.push(POINTER_SIZE);
    header.push(ADDRESS_SIZE);
    header.extend_from_slice(&[0, 0]);
    header.extend_from_slice(&0x01000000u32.to_le_bytes());
    header.extend_from_slice(&(sections.len() as u32).to_le_bytes());
    header.extend_from_slice(&strtab_offset.to_le_bytes());
    header.extend_from_slice(&strtab_size.to_le_bytes());
    for (offset, size, ty) in descriptors {
        header.extend_from_slice(&offset.to_le_bytes());
        header.extend_from_slice(&size.to_le_bytes());
        header.extend_from_slice(&ty.to_le_bytes());
    }

    let mut image = header;
    for (_, data) in sections {
        image.extend_from_slice(&data);
    }
    image.extend_from_slice(strtab_bytes);
    image
}
