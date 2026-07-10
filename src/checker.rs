#![allow(dead_code)]

use crate::ast::*;
use std::collections::HashMap;

pub struct TypeChecker {
    structs: HashMap<String, StructDecl>,
    sections: HashMap<String, SectionDecl>,
    functions: HashMap<String, FuncDecl>,
    typedefs: HashMap<String, DataType>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            structs: HashMap::new(),
            sections: HashMap::new(),
            functions: HashMap::new(),
            typedefs: HashMap::new(),
        }
    }

    pub fn populate_symbols(&mut self, program: &Program) {
        for s in &program.structs {
            self.structs.insert(s.name.clone(), s.clone());
        }
        for sect in &program.sections {
            self.sections.insert(sect.name.clone(), sect.clone());
        }
        for func in &program.functions {
            self.functions.insert(func.name.clone(), func.clone());
        }
        for (name, dt) in &program.typedefs {
            self.typedefs.insert(name.clone(), dt.clone());
        }
    }

    pub fn calculate_struct_layout(&self, struct_name: &str) -> Result<(u32, Vec<u32>), String> {
        let s = self
            .structs
            .get(struct_name)
            .ok_ok_or_else(|| format!("Struct {} not defined", struct_name))?;

        let mut offsets = Vec::new();
        let mut current_offset = 0;
        let mut max_alignment = 1;

        for field in &s.fields {
            let size = self.get_type_size(&field.data_type)?;
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

            offsets.push(current_offset);
            current_offset += size;
        }

        if current_offset % max_alignment != 0 {
            current_offset += max_alignment - (current_offset % max_alignment);
        }

        Ok((current_offset, offsets))
    }

    fn get_type_size(&self, dt: &DataType) -> Result<u32, String> {
        match dt {
            DataType::U8 | DataType::I8 => Ok(1),
            DataType::U16 | DataType::I16 => Ok(2),
            DataType::U32 | DataType::I32 => Ok(4),
            DataType::U64 | DataType::I64 => Ok(8),
            DataType::Void => Ok(0),
            DataType::Pointer(_) => Ok(8),
            DataType::Array(elem, count) => {
                let size = self.get_type_size(elem)?;
                Ok(size * (*count as u32))
            }
            DataType::Typedef(_, underlying) => self.get_type_size(underlying),
            DataType::Struct(name) => {
                if let Some(alias) = self.typedefs.get(name) {
                    self.get_type_size(alias)
                } else {
                    let (size, _) = self.calculate_struct_layout(name)?;
                    Ok(size)
                }
            }
        }
    }
}

trait OkOr {
    type Value;
    fn ok_ok_or_else<F, E>(self, f: F) -> Result<Self::Value, E>
    where
        F: FnOnce() -> E;
}

impl<T> OkOr for Option<T> {
    type Value = T;
    fn ok_ok_or_else<F, E>(self, f: F) -> Result<Self::Value, E>
    where
        F: FnOnce() -> E,
    {
        self.ok_or_else(f)
    }
}
