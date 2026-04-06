//! Per-method query/path parameter structs.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::codegen::naming::field_ident;
use crate::ir::types::{IrField, IrMethod, IrType};

/// Emit `Params` structs for each method (grouped later by resource module in actions).
pub fn emit_method_param_structs(methods: &[IrMethod]) -> Vec<TokenStream> {
    methods.iter().map(emit_one_method_params).collect()
}

fn emit_one_method_params(m: &IrMethod) -> TokenStream {
    let struct_name = format_ident!("{}Params", to_pascal(&m.rust_name));
    let doc = format!(
        "Query/path parameters for `{}`.",
        m.id
    );
    let mut fields: Vec<TokenStream> = Vec::new();
    for f in m.path_params.iter().chain(m.query_params.iter()) {
        fields.push(emit_param_field(f));
    }

    quote! {
        #[doc = #doc]
        #[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct #struct_name {
            #( #fields )*
        }
    }
}

fn emit_param_field(f: &IrField) -> TokenStream {
    let rust = field_ident(&f.rust_name);
    let inner = ir_param_type(&f.field_type);
    let skip = quote!(#[serde(skip_serializing_if = "Option::is_none")]);
    quote! {
        #skip
        pub #rust: Option<#inner>,
    }
}

fn ir_param_type(ty: &IrType) -> TokenStream {
    match ty {
        IrType::String => quote!(String),
        IrType::I32 => quote!(i32),
        IrType::I64 => quote!(i64),
        IrType::U32 => quote!(u32),
        IrType::U64 => quote!(u64),
        IrType::F32 => quote!(f32),
        IrType::F64 => quote!(f64),
        IrType::Bool => quote!(bool),
        _ => quote!(String),
    }
}

fn to_pascal(s: &str) -> String {
    use heck::ToPascalCase;
    s.to_pascal_case()
}
