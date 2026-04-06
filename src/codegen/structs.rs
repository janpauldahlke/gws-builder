//! `struct` emission via `quote`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::codegen::naming::{dedupe_field_names, field_ident_with_rename};
use crate::ir::types::{IrField, IrStruct, IrType};

/// Emit a Rust struct with serde attributes.
pub fn emit_struct(s: &IrStruct) -> TokenStream {
    let name = format_ident!("{}", s.name);
    let doc = s.doc.as_ref().map(|d| quote!(#[doc = #d]));

    let originals: Vec<String> = s.fields.iter().map(|f| f.original_name.clone()).collect();
    let deduped = dedupe_field_names(&originals);

    let fields: Vec<TokenStream> = s
        .fields
        .iter()
        .zip(deduped.iter())
        .map(|(f, rust)| {
            let needs_rename = rust != &f.rust_name;
            emit_field(f, rust.as_str(), needs_rename)
        })
        .collect();

    quote! {
        #doc
        #[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct #name {
            #( #fields )*
        }
    }
}

fn emit_field(f: &IrField, rust_name_override: &str, dedupe_rename: bool) -> TokenStream {
    let doc = f.doc.as_ref().map(|d| quote!(#[doc = #d]));
    let deprec = f.deprecated.then(|| quote!(#[deprecated]));
    let (rust, serde_kw) = field_ident_with_rename(rust_name_override, &f.original_name);
    let serde_rename = if let Some(ts) = serde_kw {
        Some(ts)
    } else if dedupe_rename {
        let orig = f.original_name.as_str();
        Some(quote!(#[serde(rename = #orig)]))
    } else {
        None
    }
    .unwrap_or_else(|| quote!());
    let inner = ir_type_tokens(&f.field_type, f.needs_box);
    let serde_int64 = (!f.serde_flatten && matches!(&f.field_type, IrType::I64)).then(|| {
        quote!(#[serde(default, deserialize_with = "super::serde_helpers::string_to_i64")])
    });
    let serde_u64 = (!f.serde_flatten && matches!(&f.field_type, IrType::U64)).then(|| {
        quote!(#[serde(default, deserialize_with = "super::serde_helpers::string_to_u64")])
    });
    let serde_bytes = (!f.serde_flatten && matches!(&f.field_type, IrType::Bytes)).then(|| {
        quote!(#[serde(default, deserialize_with = "super::serde_helpers::deserialize_bytes_base64")])
    });
    let flatten = f.serde_flatten.then(|| quote!(#[serde(flatten)]));
    let skip = if f.serde_flatten {
        quote!(#[serde(skip_serializing_if = "Option::is_none")])
    } else {
        quote!(#[serde(skip_serializing_if = "Option::is_none")])
    };

    quote! {
        #doc
        #deprec
        #flatten
        #serde_rename
        #serde_int64
        #serde_u64
        #serde_bytes
        #skip
        pub #rust: Option<#inner>,
    }
}

fn ir_type_tokens(ty: &IrType, needs_box: bool) -> TokenStream {
    let core = match ty {
        IrType::String => quote!(String),
        IrType::I32 => quote!(i32),
        IrType::I64 => quote!(i64),
        IrType::U32 => quote!(u32),
        IrType::U64 => quote!(u64),
        IrType::F32 => quote!(f32),
        IrType::F64 => quote!(f64),
        IrType::Bool => quote!(bool),
        IrType::Bytes => quote!(Vec<u8>),
        IrType::DateTime | IrType::Date => quote!(String),
        IrType::Any => quote!(serde_json::Value),
        IrType::Array(inner) => {
            let inner = ir_type_tokens(inner, false);
            quote!(Vec<#inner>)
        }
        IrType::Map(inner) => {
            let inner = ir_type_tokens(inner, false);
            quote!(std::collections::HashMap<String, #inner>)
        }
        IrType::Ref(name) => {
            let id = format_ident!("{}", name);
            quote!(#id)
        }
        IrType::Struct(_) | IrType::Enum(_) => quote!(serde_json::Value),
    };

    if needs_box {
        quote!(Box<#core>)
    } else {
        core
    }
}
