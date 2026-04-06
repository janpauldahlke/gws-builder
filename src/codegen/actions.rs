//! `ActionDescriptor` / `ParamDescriptor` static data emission.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::ir::types::{IrField, IrMethod, IrType};

/// Shared descriptor structs (emit once per generated crate root).
pub fn emit_action_descriptor_types() -> TokenStream {
    quote! {
        /// Describes a single API endpoint for agent consumption.
        #[derive(Debug, Clone)]
        pub struct ActionDescriptor {
            pub id: &'static str,
            pub service: &'static str,
            pub resource_path: &'static str,
            pub method_name: &'static str,
            pub http_method: &'static str,
            pub description: &'static str,
            pub path_template: &'static str,
            pub base_url: &'static str,
            pub scopes: &'static [&'static str],
            pub parameters: &'static [ParamDescriptor],
            pub request_body_schema: Option<&'static str>,
            pub response_body_schema: Option<&'static str>,
            pub supports_pagination: bool,
            pub supports_media_upload: bool,
            pub supports_media_download: bool,
            pub deprecated: bool,
        }

        /// Describes a single parameter on an action.
        #[derive(Debug, Clone)]
        pub struct ParamDescriptor {
            pub name: &'static str,
            pub param_type: &'static str,
            pub location: &'static str,
            pub required: bool,
            pub description: &'static str,
            pub default_value: Option<&'static str>,
            pub enum_values: Option<&'static [&'static str]>,
            pub deprecated: bool,
        }
    }
}

/// One static `ActionDescriptor` for a method.
pub fn emit_method_action(
    m: &IrMethod,
    resource_path: &str,
    base_url: &str,
    service_name: &str,
) -> TokenStream {
    let const_name = format_ident!("{}_ACTION", m.rust_name.to_uppercase());
    let id = &m.id;
    let http = &m.http_method;
    let path_t = &m.path_template;
    let desc = m.doc.as_deref().unwrap_or("");
    let pag = m.supports_pagination;
    let up = m.supports_media_upload;
    let down = m.supports_media_download;
    let dep = m.deprecated;
    let method_name = m.rust_name.as_str();

    let req = schema_name(&m.request_type);
    let res = schema_name(&m.response_type);

    let scope_tokens: Vec<TokenStream> = m.scopes.iter().map(|s| quote!(#s)).collect();

    let mut param_items = Vec::new();
    for p in &m.path_params {
        param_items.push(emit_param_static(p, "path"));
    }
    for p in &m.query_params {
        param_items.push(emit_param_static(p, "query"));
    }

    quote! {
        pub static #const_name: ActionDescriptor = ActionDescriptor {
            id: #id,
            service: #service_name,
            resource_path: #resource_path,
            method_name: #method_name,
            http_method: #http,
            description: #desc,
            path_template: #path_t,
            base_url: #base_url,
            scopes: &[ #( #scope_tokens ),* ],
            parameters: &[ #( #param_items ),* ],
            request_body_schema: #req,
            response_body_schema: #res,
            supports_pagination: #pag,
            supports_media_upload: #up,
            supports_media_download: #down,
            deprecated: #dep,
        };
    }
}

fn schema_name(ty: &Option<IrType>) -> TokenStream {
    match ty {
        Some(IrType::Ref(n)) => quote!(Some(#n)),
        _ => quote!(None),
    }
}

fn emit_param_static(p: &IrField, location: &'static str) -> TokenStream {
    let name = &p.original_name;
    let loc = location;
    let ptype = param_type_str(&p.field_type);
    let req = p.required;
    let desc = p.doc.as_deref().unwrap_or("");
    let def = p
        .default_value
        .as_ref()
        .map(|d| quote!(Some(#d)))
        .unwrap_or(quote!(None));
    let dep = p.deprecated;

    quote! {
        ParamDescriptor {
            name: #name,
            param_type: #ptype,
            location: #loc,
            required: #req,
            description: #desc,
            default_value: #def,
            enum_values: None,
            deprecated: #dep,
        }
    }
}

fn param_type_str(ty: &IrType) -> &'static str {
    match ty {
        IrType::I32 | IrType::I64 | IrType::U32 | IrType::U64 | IrType::F32 | IrType::F64 => {
            "integer"
        }
        IrType::Bool => "boolean",
        _ => "string",
    }
}
