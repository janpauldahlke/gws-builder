//! Rust identifier naming: case conversion, keywords, collisions.
#![allow(dead_code)]
// Helpers like `dedupe_field_names` are used by upcoming collision work.

use std::collections::HashMap;

use heck::{ToPascalCase, ToSnakeCase};
use proc_macro2::Ident;
use quote::format_ident;

const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use",
    "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final",
    "macro", "override", "priv", "try", "typeof", "unsized", "virtual", "yield",
];

/// Convert camelCase / PascalCase JSON names to `snake_case` field names.
pub fn to_snake_case(name: &str) -> String {
    name.to_snake_case()
}

/// Convert to `PascalCase` for Rust types.
pub fn to_pascal_case(name: &str) -> String {
    name.to_pascal_case()
}

/// Field identifier (`snake_case`), escaping reserved words with raw identifiers.
pub fn field_ident(name: &str) -> Ident {
    let s = to_snake_case(name);
    if RUST_KEYWORDS.contains(&s.as_str()) {
        format_ident!("r#{}", s)
    } else {
        format_ident!("{}", s)
    }
}

/// Escape reserved words as a string (for diagnostics).
pub fn escape_keyword(ident: &str) -> String {
    let s = to_snake_case(ident);
    if RUST_KEYWORDS.contains(&s.as_str()) {
        format!("r#{s}")
    } else {
        s
    }
}

/// Deduplicate snake_case names within a struct field list.
pub fn dedupe_field_names(names: &[String]) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::with_capacity(names.len());
    for n in names {
        let base = to_snake_case(n);
        let entry = counts.entry(base.clone()).or_insert(0);
        let resolved = if *entry == 0 {
            base.clone()
        } else {
            format!("{base}_{}", *entry)
        };
        *entry += 1;
        out.push(resolved);
    }
    out
}
