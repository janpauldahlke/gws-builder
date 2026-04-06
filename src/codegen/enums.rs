//! String enum emission.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::ir::types::IrEnum;

pub fn emit_enum(e: &IrEnum) -> TokenStream {
    let name = format_ident!("{}", e.name);
    let doc = e.doc.as_ref().map(|d| quote!(#[doc = #d]));

    let variants: Vec<TokenStream> = e
        .variants
        .iter()
        .map(|v| {
            let id = format_ident!("{}", v.rust_name);
            let rename = &v.original_value;
            let vdoc = v.doc.as_ref().map(|d| quote!(#[doc = #d]));
            quote! {
                #vdoc
                #[serde(rename = #rename)]
                #id,
            }
        })
        .collect();

    quote! {
        #doc
        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        pub enum #name {
            #( #variants )*
            #[serde(other)]
            Unknown(String),
        }
    }
}
