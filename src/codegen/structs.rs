//! `struct` emission via `quote`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::codegen::naming::field_ident;
use crate::ir::types::{IrField, IrStruct, IrType};

/// Emit a Rust struct with serde attributes.
pub fn emit_struct(s: &IrStruct) -> TokenStream {
    let name = format_ident!("{}", s.name);
    let doc = s.doc.as_ref().map(|d| quote!(#[doc = #d]));

    let fields: Vec<TokenStream> = s.fields.iter().map(emit_field).collect();

    quote! {
        #doc
        #[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct #name {
            #( #fields )*
        }
    }
}

fn emit_field(f: &IrField) -> TokenStream {
    let doc = f.doc.as_ref().map(|d| quote!(#[doc = #d]));
    let deprec = f.deprecated.then(|| quote!(#[deprecated]));
    let rust = field_ident(&f.rust_name);
    let inner = ir_type_tokens(&f.field_type, f.needs_box);
    let serde_int64 = matches!(&f.field_type, IrType::I64).then(|| {
        quote!(#[serde(default, deserialize_with = "super::serde_helpers::string_to_i64")])
    });
    let serde_u64 = matches!(&f.field_type, IrType::U64).then(|| {
        quote!(#[serde(default, deserialize_with = "super::serde_helpers::string_to_u64")])
    });
    let skip = quote!(#[serde(skip_serializing_if = "Option::is_none")]);

    quote! {
        #doc
        #deprec
        #serde_int64
        #serde_u64
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
