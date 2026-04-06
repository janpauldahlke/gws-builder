//! `$ref` graph: cycle detection and `Box` insertion for recursive schemas.

use std::collections::HashMap;

use crate::error::BuilderError;
use crate::ir::types::{IrService, IrStruct, IrType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Color {
    White,
    Gray,
    Black,
}

/// Mark fields that need `Box<...>` to break cyclic `$ref` chains between named structs.
pub fn resolve_service(service: &mut IrService) -> Result<(), BuilderError> {
    let index: HashMap<String, usize> = service
        .structs
        .iter()
        .enumerate()
        .map(|(i, s)| (s.name.clone(), i))
        .collect();

    let mut color: HashMap<String, Color> = HashMap::new();
    for s in &service.structs {
        color.insert(s.name.clone(), Color::White);
    }

    let names: Vec<String> = service.structs.iter().map(|s| s.name.clone()).collect();
    for name in names {
        if color.get(&name) == Some(&Color::White) {
            dfs_visit(&mut service.structs, &index, &mut color, &name)?;
        }
    }

    for s in &mut service.structs {
        s.is_recursive = s.fields.iter().any(|f| f.needs_box);
    }

    Ok(())
}

fn dfs_visit(
    structs: &mut [IrStruct],
    index: &HashMap<String, usize>,
    color: &mut HashMap<String, Color>,
    name: &str,
) -> Result<(), BuilderError> {
    *color.get_mut(name).unwrap() = Color::Gray;
    let si = *index
        .get(name)
        .ok_or_else(|| BuilderError::Resolution(format!("unknown struct {name} in resolve")))?;

    let n = structs[si].fields.len();
    for i in 0..n {
        let mut ty = std::mem::replace(
            &mut structs[si].fields[i].field_type,
            IrType::String,
        );
        let boxed = resolve_field_type(&mut ty, structs, index, color, name)?;
        structs[si].fields[i].field_type = ty;
        if boxed {
            structs[si].fields[i].needs_box = true;
        }
    }

    *color.get_mut(name).unwrap() = Color::Black;
    Ok(())
}

fn resolve_field_type(
    ty: &mut IrType,
    structs: &mut [IrStruct],
    index: &HashMap<String, usize>,
    color: &mut HashMap<String, Color>,
    _current: &str,
) -> Result<bool, BuilderError> {
    match ty {
        IrType::Ref(r) => {
            if !index.contains_key(r) {
                return Ok(false);
            }
            match color.get(r).copied() {
                Some(Color::Gray) => Ok(true),
                Some(Color::White) => {
                    dfs_visit(structs, index, color, r)?;
                    Ok(false)
                }
                Some(Color::Black) => Ok(false),
                None => Ok(false),
            }
        }
        IrType::Array(inner) => resolve_field_type(inner, structs, index, color, _current),
        IrType::Map(inner) => resolve_field_type(inner, structs, index, color, _current),
        IrType::Struct(st) => {
            for f in &mut st.fields {
                let mut inner = std::mem::replace(
                    &mut f.field_type,
                    IrType::String,
                );
                let bx = resolve_field_type(&mut inner, structs, index, color, _current)?;
                f.field_type = inner;
                if bx {
                    f.needs_box = true;
                }
            }
            Ok(false)
        }
        IrType::Enum(_) => Ok(false),
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::types::{IrField, IrStruct};

    #[test]
    fn boxes_cycle() {
        let a = IrStruct {
            name: "A".into(),
            doc: None,
            fields: vec![IrField {
                original_name: "b".into(),
                rust_name: "b".into(),
                doc: None,
                field_type: IrType::Ref("B".into()),
                required: false,
                read_only: false,
                deprecated: false,
                default_value: None,
                needs_box: false,
                serde_flatten: false,
            }],
            is_recursive: false,
        };
        let b = IrStruct {
            name: "B".into(),
            doc: None,
            fields: vec![IrField {
                original_name: "a".into(),
                rust_name: "a".into(),
                doc: None,
                field_type: IrType::Ref("A".into()),
                required: false,
                read_only: false,
                deprecated: false,
                default_value: None,
                needs_box: false,
                serde_flatten: false,
            }],
            is_recursive: false,
        };
        let mut svc = IrService {
            name: "s".into(),
            version: "v1".into(),
            doc: None,
            base_url: "https://example.com/".into(),
            structs: vec![a, b],
            enums: vec![],
            resources: vec![],
        };
        resolve_service(&mut svc).expect("resolve");
        assert!(svc.structs[0].fields[0].needs_box || svc.structs[1].fields[0].needs_box);
    }
}
