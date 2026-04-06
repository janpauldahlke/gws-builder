//! Write generated Rust sources and manifest.

use std::fs;
use std::path::Path;

use proc_macro2::TokenStream;
use quote::quote;

use crate::codegen::{
    emit_action_descriptor_types, emit_enum, emit_method_action, emit_method_param_structs,
    emit_struct,
};
use crate::error::BuilderError;
use crate::ir::types::{IrMethod, IrResource, IrService};
use crate::manifest::GenerationManifest;

fn pretty_format(ts: TokenStream) -> Result<String, BuilderError> {
    let file = syn::parse2(ts).map_err(|e| BuilderError::Codegen(format!("syn parse: {e}")))?;
    let formatted = prettyplease::unparse(&file);
    Ok(formatted)
}

fn walk_resource_methods(
    res: &IrResource,
    prefix: &str,
    out: &mut Vec<(String, IrMethod)>,
) {
    let path = if prefix.is_empty() {
        res.name.clone()
    } else {
        format!("{prefix}.{}", res.name)
    };
    for m in &res.methods {
        out.push((path.clone(), m.clone()));
    }
    for sub in &res.sub_resources {
        walk_resource_methods(sub, &path, out);
    }
}

fn flatten_methods(service: &IrService) -> Vec<(String, IrMethod)> {
    let mut v = Vec::new();
    for r in &service.resources {
        walk_resource_methods(r, "", &mut v);
    }
    v
}

/// Emit one service file (`drive.rs`, etc.) as formatted source.
pub fn emit_service_rust(service: &IrService) -> Result<String, BuilderError> {
    let mut streams: Vec<TokenStream> = Vec::new();
    streams.push(emit_action_descriptor_types());

    for e in &service.enums {
        streams.push(emit_enum(e));
    }
    for s in &service.structs {
        streams.push(emit_struct(s));
    }

    let flat = flatten_methods(service);
    let methods: Vec<IrMethod> = flat.iter().map(|(_, m)| m.clone()).collect();
    let param_streams = emit_method_param_structs(&methods);
    streams.extend(param_streams);

    for (res_path, m) in &flat {
        streams.push(emit_method_action(
            m,
            res_path,
            &service.base_url,
            &service.name,
        ));
    }

    let action_refs: Vec<_> = flat
        .iter()
        .map(|(_, m)| {
            let id = quote::format_ident!("{}_ACTION", m.rust_name.to_uppercase());
            quote! { &#id }
        })
        .collect();

    streams.push(quote! {
        pub static ALL_ACTIONS: &[&ActionDescriptor] = &[ #( #action_refs ),* ];
    });

    let combined = quote! { #( #streams )* };
    pretty_format(combined)
}

pub fn emit_serde_helpers() -> &'static str {
    r#"//! serde helpers for Google JSON string encodings.

use serde::Deserialize;

pub fn string_to_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    use serde::de::Visitor;
    use std::fmt;

    struct I64Visitor;

    impl<'de> Visitor<'de> for I64Visitor {
        type Value = i64;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a string or integer i64")
        }

        fn visit_str<E: Error>(self, v: &str) -> Result<i64, E> {
            v.parse().map_err(Error::custom)
        }

        fn visit_i64<E: Error>(self, v: i64) -> Result<i64, E> {
            Ok(v)
        }

        fn visit_u64<E: Error>(self, v: u64) -> Result<i64, E> {
            i64::try_from(v).map_err(Error::custom)
        }
    }

    deserializer.deserialize_any(I64Visitor)
}

pub fn string_to_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    use serde::de::Visitor;
    use std::fmt;

    struct U64Visitor;

    impl<'de> Visitor<'de> for U64Visitor {
        type Value = u64;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a string or integer u64")
        }

        fn visit_str<E: Error>(self, v: &str) -> Result<u64, E> {
            v.parse().map_err(Error::custom)
        }

        fn visit_u64<E: Error>(self, v: u64) -> Result<u64, E> {
            Ok(v)
        }

        fn visit_i64<E: Error>(self, v: i64) -> Result<u64, E> {
            u64::try_from(v).map_err(Error::custom)
        }
    }

    deserializer.deserialize_any(U64Visitor)
}
"#
}

pub fn write_mod_rs(
    out_dir: &Path,
    service_modules: &[String],
) -> Result<(), BuilderError> {
    let mut lines = Vec::new();
    for s in service_modules {
        lines.push(format!("pub mod {s};"));
    }
    lines.push("mod serde_helpers;".into());
    lines.push(String::new());
    lines.push("pub use serde_helpers::*;".into());
    if let Some(first) = service_modules.first() {
        lines.push(format!(
            "pub use {first}::{{ActionDescriptor, ParamDescriptor}};"
        ));
    }
    lines.push(String::new());
    lines.push("/// Returns all action descriptors across generated services.".into());
    lines.push("pub fn all_actions() -> Vec<&'static ActionDescriptor> {".into());
    lines.push("    let mut all = Vec::new();".into());
    for s in service_modules {
        lines.push(format!("    all.extend_from_slice({s}::ALL_ACTIONS);"));
    }
    lines.push("    all".into());
    lines.push("}".into());

    let path = out_dir.join("mod.rs");
    atomic_write(&path, lines.join("\n").as_bytes())?;
    Ok(())
}

pub fn write_generation_manifest(
    path: &Path,
    manifest: &GenerationManifest,
) -> Result<(), BuilderError> {
    crate::manifest::save(path, manifest)
}

fn atomic_write(path: &Path, data: &[u8]) -> Result<(), BuilderError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| BuilderError::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data).map_err(|e| BuilderError::Io {
        path: tmp.clone(),
        source: e,
    })?;
    fs::rename(&tmp, path).map_err(|e| BuilderError::Io {
        path: path.to_path_buf(),
        source: e,
    })
}

pub fn write_file(path: &Path, content: &str) -> Result<(), BuilderError> {
    atomic_write(path, content.as_bytes())
}
