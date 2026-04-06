//! Intermediate representation types for codegen.

/// Resolved or built-in type in the IR.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrType {
    String,
    I32,
    I64,
    U32,
    U64,
    F32,
    F64,
    Bool,
    Bytes,
    DateTime,
    Date,
    Any,
    Array(Box<IrType>),
    Map(Box<IrType>),
    /// Named schema reference (top-level `schemas` key).
    Ref(String),
    Struct(IrStruct),
    Enum(IrEnum),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrStruct {
    pub name: String,
    pub doc: Option<String>,
    pub fields: Vec<IrField>,
    pub is_recursive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrField {
    pub original_name: String,
    pub rust_name: String,
    pub doc: Option<String>,
    pub field_type: IrType,
    pub required: bool,
    pub read_only: bool,
    pub deprecated: bool,
    pub default_value: Option<String>,
    /// Set by `resolve` when breaking a cyclic `$ref`.
    pub needs_box: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrEnum {
    pub name: String,
    pub doc: Option<String>,
    pub variants: Vec<IrEnumVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrEnumVariant {
    pub original_value: String,
    pub rust_name: String,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrMethod {
    pub id: String,
    pub rust_name: String,
    pub doc: Option<String>,
    pub http_method: String,
    pub path_template: String,
    pub path_params: Vec<IrField>,
    pub query_params: Vec<IrField>,
    pub request_type: Option<IrType>,
    pub response_type: Option<IrType>,
    pub scopes: Vec<String>,
    pub supports_pagination: bool,
    pub supports_media_upload: bool,
    pub supports_media_download: bool,
    pub deprecated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrService {
    pub name: String,
    pub version: String,
    pub doc: Option<String>,
    pub base_url: String,
    pub structs: Vec<IrStruct>,
    pub enums: Vec<IrEnum>,
    pub resources: Vec<IrResource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrResource {
    pub name: String,
    pub rust_name: String,
    pub methods: Vec<IrMethod>,
    pub sub_resources: Vec<IrResource>,
}
