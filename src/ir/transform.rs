//! Discovery document → IR.

use std::collections::{HashMap, HashSet};

use heck::{ToPascalCase, ToSnakeCase};

use crate::discovery::{
    JsonSchema, JsonSchemaProperty, MethodParameter, RestDescription, RestMethod, RestResource,
    SchemaRef,
};
use crate::error::BuilderError;
use crate::ir::types::{
    IrEnum, IrEnumVariant, IrField, IrMethod, IrResource, IrService, IrStruct, IrType,
};

/// Convert a parsed Discovery document into IR for one API.
pub fn discovery_to_ir(doc: &RestDescription) -> Result<IrService, BuilderError> {
    let base_url = compute_base_url(doc);
    let title = doc.title.clone().or_else(|| doc.description.clone());

    let mut structs: Vec<IrStruct> = Vec::new();
    let mut enums: Vec<IrEnum> = Vec::new();
    let mut emitted_names: HashSet<String> = HashSet::new();

    let schema_map = &doc.schemas;
    for (name, schema) in schema_map {
        if schema.enum_values.is_some() && schema.properties.is_empty() {
            if let Some(e) = schema_to_enum(name, schema)? {
                if emitted_names.insert(e.name.clone()) {
                    enums.push(e);
                }
            }
        } else if schema.schema_type.as_deref() == Some("object") || !schema.properties.is_empty() {
            let st = schema_to_struct(
                name,
                schema,
                schema_map,
                &mut structs,
                &mut enums,
                &mut emitted_names,
            )?;
            if emitted_names.insert(st.name.clone()) {
                structs.push(st);
            }
        }
    }

    let mut resources: Vec<IrResource> = Vec::new();
    for (key, res) in &doc.resources {
        resources.push(resource_to_ir(
            key,
            res,
            doc,
            &mut structs,
            &mut enums,
            &mut emitted_names,
            schema_map,
        )?);
    }

    Ok(IrService {
        name: doc.name.clone(),
        version: doc.version.clone(),
        doc: title,
        base_url,
        structs,
        enums,
        resources,
    })
}

fn compute_base_url(doc: &RestDescription) -> String {
    if let Some(b) = &doc.base_url {
        return b.clone();
    }
    format!(
        "{}{}",
        doc.root_url.trim_end_matches('/'),
        doc.service_path.trim_start_matches('/')
    )
}

fn resource_to_ir(
    key: &str,
    res: &RestResource,
    doc: &RestDescription,
    structs: &mut Vec<IrStruct>,
    enums: &mut Vec<IrEnum>,
    emitted: &mut HashSet<String>,
    schema_map: &HashMap<String, JsonSchema>,
) -> Result<IrResource, BuilderError> {
    let mut methods: Vec<IrMethod> = Vec::new();
    for (mname, m) in &res.methods {
        methods.push(method_to_ir(
            key,
            mname,
            m,
            doc,
            structs,
            enums,
            emitted,
            schema_map,
        )?);
    }
    let mut sub: Vec<IrResource> = Vec::new();
    for (sk, sr) in &res.resources {
        sub.push(resource_to_ir(
            sk,
            sr,
            doc,
            structs,
            enums,
            emitted,
            schema_map,
        )?);
    }
    Ok(IrResource {
        name: key.to_string(),
        rust_name: key.to_snake_case(),
        methods,
        sub_resources: sub,
    })
}

fn method_to_ir(
    resource_key: &str,
    method_key: &str,
    m: &RestMethod,
    doc: &RestDescription,
    structs: &mut Vec<IrStruct>,
    enums: &mut Vec<IrEnum>,
    emitted: &mut HashSet<String>,
    schema_map: &HashMap<String, JsonSchema>,
) -> Result<IrMethod, BuilderError> {
    let id = m
        .id
        .clone()
        .unwrap_or_else(|| format!("{}.{}.{}", doc.name, resource_key, method_key));

    let mut path_params: Vec<IrField> = Vec::new();
    let mut query_params: Vec<IrField> = Vec::new();

    for (pk, p) in &m.parameters {
        let location = p.location.as_deref().unwrap_or("");
        let field = method_param_to_field(pk, p, schema_map, structs, enums, emitted)?;
        match location {
            "path" => path_params.push(field),
            _ => query_params.push(field),
        }
    }

    // Path template `{foo}` params may be listed only in `path` string — ensure coverage.
    for seg in path_param_names(&m.path) {
        if !path_params.iter().any(|f| f.original_name == seg) {
            if let Some(p) = m.parameters.get(&seg) {
                path_params.push(method_param_to_field(
                    &seg,
                    p,
                    schema_map,
                    structs,
                    enums,
                    emitted,
                )?);
            }
        }
    }

    let request_type = m
        .request
        .as_ref()
        .and_then(|r| schema_ref_to_type(r, schema_map));
    let response_type = m
        .response
        .as_ref()
        .and_then(|r| schema_ref_to_type(r, schema_map));

    let supports_pagination = m
        .parameters
        .get("pageToken")
        .map(|_| true)
        .unwrap_or(false)
        || query_params.iter().any(|f| f.original_name == "pageToken");

    Ok(IrMethod {
        id,
        rust_name: method_key.to_snake_case(),
        doc: m.description.clone(),
        http_method: m.http_method.clone(),
        path_template: m.path.clone(),
        path_params,
        query_params,
        request_type,
        response_type,
        scopes: m.scopes.clone(),
        supports_pagination,
        supports_media_upload: m.supports_media_upload,
        supports_media_download: m.supports_media_download,
        deprecated: false,
    })
}

fn path_param_names(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find('}') {
            out.push(after[..end].to_string());
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    out
}

fn method_param_to_field(
    name: &str,
    p: &MethodParameter,
    schema_map: &HashMap<String, JsonSchema>,
    _structs: &mut Vec<IrStruct>,
    _enums: &mut Vec<IrEnum>,
    _emitted: &mut HashSet<String>,
) -> Result<IrField, BuilderError> {
    let ty = method_param_type(p, schema_map);
    Ok(IrField {
        original_name: name.to_string(),
        rust_name: name.to_snake_case(),
        doc: p.description.clone(),
        field_type: ty,
        required: p.required,
        read_only: false,
        deprecated: p.deprecated,
        default_value: p.default.as_ref().map(|v| v.to_string()),
        needs_box: false,
    })
}

fn method_param_type(p: &MethodParameter, _schema_map: &HashMap<String, JsonSchema>) -> IrType {
    if p.enum_values.is_some() {
        return IrType::String;
    }
    match p.param_type.as_deref() {
        Some("string") => match p.format.as_deref() {
            Some("int64") => IrType::I64,
            Some("uint64") => IrType::U64,
            _ => IrType::String,
        },
        Some("integer") => match p.format.as_deref() {
            Some("uint32") => IrType::U32,
            _ => IrType::I32,
        },
        Some("number") => match p.format.as_deref() {
            Some("float") => IrType::F32,
            _ => IrType::F64,
        },
        Some("boolean") => IrType::Bool,
        _ => IrType::String,
    }
}

fn schema_ref_to_type(r: &SchemaRef, _schema_map: &HashMap<String, JsonSchema>) -> Option<IrType> {
    r.schema_ref
        .as_ref()
        .map(|name| IrType::Ref(name.clone()))
}

fn schema_to_enum(name: &str, schema: &JsonSchema) -> Result<Option<IrEnum>, BuilderError> {
    let values = match &schema.enum_values {
        Some(v) if !v.is_empty() => v,
        _ => return Ok(None),
    };
    let descs: Vec<Option<String>> = schema
        .enum_descriptions
        .clone()
        .map(|d| d.into_iter().map(Some).collect())
        .unwrap_or_else(|| vec![None; values.len()]);

    let mut variants = Vec::new();
    for (i, val) in values.iter().enumerate() {
        let rust = enum_variant_rust_name(val);
        variants.push(IrEnumVariant {
            original_value: val.clone(),
            rust_name: rust,
            doc: descs.get(i).cloned().flatten(),
        });
    }
    Ok(Some(IrEnum {
        name: name.to_string(),
        doc: schema.description.clone(),
        variants,
    }))
}

fn enum_variant_rust_name(raw: &str) -> String {
    let base = raw.to_pascal_case();
    let mut s = if base.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        format!("_{base}")
    } else {
        base
    };
    if s.is_empty() {
        s = "UnknownVariant".into();
    }
    s
}

fn schema_to_struct(
    name: &str,
    schema: &JsonSchema,
    schema_map: &HashMap<String, JsonSchema>,
    structs: &mut Vec<IrStruct>,
    enums: &mut Vec<IrEnum>,
    emitted: &mut HashSet<String>,
) -> Result<IrStruct, BuilderError> {
    let mut fields = Vec::new();
    for (fname, prop) in &schema.properties {
        let ft = property_to_ir_type(
            name,
            fname,
            prop,
            schema_map,
            structs,
            enums,
            emitted,
        )?;
        let required = schema.required.iter().any(|r| r == fname);
        fields.push(IrField {
            original_name: fname.clone(),
            rust_name: fname.to_snake_case(),
            doc: prop.description.clone(),
            field_type: ft,
            required,
            read_only: prop.read_only,
            deprecated: false,
            default_value: prop.default.as_ref().map(|v| v.to_string()),
            needs_box: false,
        });
    }
    Ok(IrStruct {
        name: name.to_string(),
        doc: schema.description.clone(),
        fields,
        is_recursive: false,
    })
}

fn property_to_ir_type(
    parent_schema: &str,
    field_name: &str,
    prop: &JsonSchemaProperty,
    schema_map: &HashMap<String, JsonSchema>,
    structs: &mut Vec<IrStruct>,
    enums: &mut Vec<IrEnum>,
    emitted: &mut HashSet<String>,
) -> Result<IrType, BuilderError> {
    if let Some(r) = &prop.schema_ref {
        return Ok(IrType::Ref(r.clone()));
    }

    if let Some(ev) = &prop.enum_values {
        if !ev.is_empty() && prop.prop_type.as_deref() == Some("string") {
            let ename = format!("{}{}", parent_schema, field_name.to_pascal_case());
            let en = IrEnum {
                name: ename.clone(),
                doc: prop.description.clone(),
                variants: ev
                    .iter()
                    .map(|val| IrEnumVariant {
                        original_value: val.clone(),
                        rust_name: enum_variant_rust_name(val),
                        doc: None,
                    })
                    .collect(),
            };
            if emitted.insert(en.name.clone()) {
                enums.push(en);
            }
            return Ok(IrType::Ref(ename));
        }
    }

    match prop.prop_type.as_deref() {
        Some("string") => match prop.format.as_deref() {
            Some("int64") => Ok(IrType::I64),
            Some("uint64") => Ok(IrType::U64),
            Some("byte") => Ok(IrType::Bytes),
            Some("date-time") => Ok(IrType::DateTime),
            Some("date") => Ok(IrType::Date),
            _ => Ok(IrType::String),
        },
        Some("integer") => match prop.format.as_deref() {
            Some("uint32") => Ok(IrType::U32),
            _ => Ok(IrType::I32),
        },
        Some("number") => match prop.format.as_deref() {
            Some("float") => Ok(IrType::F32),
            _ => Ok(IrType::F64),
        },
        Some("boolean") => Ok(IrType::Bool),
        Some("any") => Ok(IrType::Any),
        Some("array") => {
            let inner = prop
                .items
                .as_ref()
                .map(|b| {
                    property_to_ir_type(
                        parent_schema,
                        field_name,
                        b,
                        schema_map,
                        structs,
                        enums,
                        emitted,
                    )
                })
                .transpose()?
                .unwrap_or(IrType::Any);
            Ok(IrType::Array(Box::new(inner)))
        }
        Some("object") | None => {
            if !prop.properties.is_empty() {
                let synth = format!("{}{}", parent_schema, field_name.to_pascal_case());
                let fake = JsonSchema {
                    id: Some(synth.clone()),
                    schema_type: Some("object".into()),
                    description: prop.description.clone(),
                    deprecated: false,
                    properties: prop.properties.clone(),
                    schema_ref: None,
                    items: None,
                    required: vec![],
                    additional_properties: prop.additional_properties.clone(),
                    enum_values: None,
                    enum_descriptions: None,
                };
                let st = schema_to_struct(&synth, &fake, schema_map, structs, enums, emitted)?;
                if emitted.insert(st.name.clone()) {
                    structs.push(st);
                }
                return Ok(IrType::Ref(synth));
            }
            additional_props_map(prop, parent_schema, field_name, schema_map, structs, enums, emitted)
        }
        _ => Ok(IrType::Any),
    }
}

fn additional_props_map(
    prop: &JsonSchemaProperty,
    parent_schema: &str,
    field_name: &str,
    schema_map: &HashMap<String, JsonSchema>,
    structs: &mut Vec<IrStruct>,
    enums: &mut Vec<IrEnum>,
    emitted: &mut HashSet<String>,
) -> Result<IrType, BuilderError> {
    let ap = match &prop.additional_properties {
        Some(v) => v,
        None => return Ok(IrType::Any),
    };

    if ap.is_boolean() {
        return if ap.as_bool() == Some(true) {
            Ok(IrType::Map(Box::new(IrType::Any)))
        } else {
            Ok(IrType::Any)
        };
    }

    if let Some(obj) = ap.as_object() {
        if let Some(t) = obj.get("type").and_then(|x| x.as_str()) {
            let fake_prop = JsonSchemaProperty {
                prop_type: Some(t.to_string()),
                description: None,
                schema_ref: obj
                    .get("$ref")
                    .and_then(|r| r.as_str())
                    .map(|s| s.to_string()),
                format: obj
                    .get("format")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string()),
                items: None,
                properties: HashMap::new(),
                read_only: false,
                default: None,
                enum_values: None,
                enum_descriptions: None,
                additional_properties: None,
                annotations: None,
            };
            let inner = property_to_ir_type(
                parent_schema,
                field_name,
                &fake_prop,
                schema_map,
                structs,
                enums,
                emitted,
            )?;
            return Ok(IrType::Map(Box::new(inner)));
        }
    }

    Ok(IrType::Map(Box::new(IrType::Any)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_params() {
        let p = "files/{fileId}/permissions/{permissionId}";
        let n = path_param_names(p);
        assert_eq!(n, vec!["fileId", "permissionId"]);
    }
}
