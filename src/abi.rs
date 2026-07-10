#![allow(dead_code)]

use std::collections::HashMap;

pub const P46_SECTION_TYPES: u32 = 1;
pub const P46_SECTION_EXPORTS: u32 = 2;
pub const P46_SECTION_REFLECT: u32 = 3;
pub const P46_SECTION_IMPORTS: u32 = 4;
pub const P46_SECTION_DEPENDENCIES: u32 = 5;

pub const P46_EXPORT_KIND_FUNCTION: u8 = 1;
pub const P46_EXPORT_KIND_VARIABLE: u8 = 2;
pub const P46_EXPORT_KIND_TYPE: u8 = 3;

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
    types_section: Vec<u8>,
    exports_section: Vec<u8>,
    exports_count: u32,
    imports_section: Vec<u8>,
    imports_count: u32,
    deps_section: Vec<u8>,
    deps_count: u32,
    reflect_section: Vec<u8>,
    reflect_count: u32,
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
        }
    }

    pub fn add_dependency(&mut self, name: &str, major: u32, minor: u32) {
        if self.deps_count == 0 {
            self.deps_section.extend_from_slice(&0u32.to_le_bytes());
        }
        let name_offset = self.strtab.insert(name);
        self.deps_section
            .extend_from_slice(&name_offset.to_le_bytes());
        self.deps_section.extend_from_slice(&major.to_le_bytes());
        self.deps_section.extend_from_slice(&minor.to_le_bytes());
        self.deps_section.extend_from_slice(&0u32.to_le_bytes()); // patch
        self.deps_section.extend_from_slice(&0u32.to_le_bytes()); // build
        self.deps_count += 1;
    }

    pub fn add_type_record(&mut self, type_id: u16, payload: &[u8]) {
        self.types_section.extend_from_slice(&type_id.to_le_bytes());
        self.types_section
            .extend_from_slice(&(payload.len() as u32).to_le_bytes());
        self.types_section.extend_from_slice(payload);
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
        self.exports_section
            .extend_from_slice(&address.to_le_bytes());
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
            self.exports_section.extend_from_slice(&0u32.to_le_bytes()); // param_count
            self.exports_section.extend_from_slice(&0u32.to_le_bytes()); // return_type
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

    pub fn build_binary_image(&mut self) -> Vec<u8> {
        if self.deps_count > 0 {
            let count_bytes = self.deps_count.to_le_bytes();
            self.deps_section[0..4].copy_from_slice(&count_bytes);
        }
        if self.exports_count > 0 {
            let count_bytes = self.exports_count.to_le_bytes();
            self.exports_section[0..4].copy_from_slice(&count_bytes);
        }
        if self.imports_count > 0 {
            let count_bytes = self.imports_count.to_le_bytes();
            self.imports_section[0..4].copy_from_slice(&count_bytes);
        }
        if self.reflect_count > 0 {
            let count_bytes = self.reflect_count.to_le_bytes();
            self.reflect_section[0..4].copy_from_slice(&count_bytes);
        }

        let mut image = Vec::new();

        let header_size = 24;
        let section_desc_size = 12 * 5;
        let sections_start = header_size + section_desc_size;

        let types_offset = sections_start;
        let types_size = self.types_section.len() as u32;

        let exports_offset = types_offset + types_size;
        let exports_size = self.exports_section.len() as u32;

        let reflect_offset = exports_offset + exports_size;
        let reflect_size = self.reflect_section.len() as u32;

        let imports_offset = reflect_offset + reflect_size;
        let imports_size = self.imports_section.len() as u32;

        let deps_offset = imports_offset + imports_size;
        let deps_size = self.deps_section.len() as u32;

        let strtab_offset = deps_offset + deps_size;
        let strtab_size = self.strtab.as_bytes().len() as u32;

        image.extend_from_slice(&[0x50, 0x34, 0x36, 0x00]); // magic
        image.push(1); // format_major
        image.push(5); // format_minor
        image.push(0); // format_patch
        image.push(1); // endianness (little-endian)
        image.push(8); // pointer_size (64-bit)
        image.extend_from_slice(&[0, 0, 0]); // reserved
        image.extend_from_slice(&5u32.to_le_bytes()); // section_count: 5
        image.extend_from_slice(&strtab_offset.to_le_bytes());
        image.extend_from_slice(&strtab_size.to_le_bytes());

        image.extend_from_slice(&types_offset.to_le_bytes());
        image.extend_from_slice(&types_size.to_le_bytes());
        image.extend_from_slice(&P46_SECTION_TYPES.to_le_bytes());

        image.extend_from_slice(&exports_offset.to_le_bytes());
        image.extend_from_slice(&exports_size.to_le_bytes());
        image.extend_from_slice(&P46_SECTION_EXPORTS.to_le_bytes());

        image.extend_from_slice(&reflect_offset.to_le_bytes());
        image.extend_from_slice(&reflect_size.to_le_bytes());
        image.extend_from_slice(&P46_SECTION_REFLECT.to_le_bytes());

        image.extend_from_slice(&imports_offset.to_le_bytes());
        image.extend_from_slice(&imports_size.to_le_bytes());
        image.extend_from_slice(&P46_SECTION_IMPORTS.to_le_bytes());

        image.extend_from_slice(&deps_offset.to_le_bytes());
        image.extend_from_slice(&deps_size.to_le_bytes());
        image.extend_from_slice(&P46_SECTION_DEPENDENCIES.to_le_bytes());

        image.extend_from_slice(&self.types_section);
        image.extend_from_slice(&self.exports_section);
        image.extend_from_slice(&self.reflect_section);
        image.extend_from_slice(&self.imports_section);
        image.extend_from_slice(&self.deps_section);
        image.extend_from_slice(self.strtab.as_bytes());

        image
    }
}
