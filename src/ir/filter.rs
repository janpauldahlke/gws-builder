//! Action filtering and dead schema pruning.

use std::collections::HashSet;

use crate::error::BuilderError;
use crate::ir::types::{IrField, IrMethod, IrResource, IrService, IrType};
use crate::ActionFilter;

/// Apply whitelist/blacklist and remove unreferenced structs/enums.
pub fn apply_filter(service: &mut IrService, filter: &ActionFilter) -> Result<(), BuilderError> {
    match filter {
        ActionFilter::All => {}
        ActionFilter::Whitelist(patterns) => {
            filter_resources_whitelist(&mut service.resources, patterns);
            if !patterns.is_empty() && count_methods(&service.resources) == 0 {
                return Err(BuilderError::Resolution(format!(
                    "whitelist matched no methods for service {}; check patterns {patterns:?}",
                    service.name
                )));
            }
        }
        ActionFilter::Blacklist(patterns) => {
            filter_resources_blacklist(&mut service.resources, patterns);
        }
    }

    let mut closure = HashSet::new();
    for res in &service.resources {
        walk_resource_methods(res, &mut |m| {
            refs_from_method(m, &mut closure);
        });
    }

    let mut changed = true;
    while changed {
        changed = false;
        let mut add = HashSet::new();
        for name in &closure {
            if let Some(st) = service.structs.iter().find(|s| &s.name == name) {
                for f in &st.fields {
                    collect_ref_names(&f.field_type, &mut add);
                }
            }
        }
        for n in add {
            if closure.insert(n) {
                changed = true;
            }
        }
    }

    service.structs.retain(|s| closure.contains(&s.name));
    service.enums.retain(|e| closure.contains(&e.name));
    Ok(())
}

fn collect_ref_names(ty: &IrType, out: &mut HashSet<String>) {
    match ty {
        IrType::Ref(r) => {
            out.insert(r.clone());
        }
        IrType::Array(inner) => collect_ref_names(inner, out),
        IrType::Map(inner) => collect_ref_names(inner, out),
        IrType::Struct(st) => {
            for f in &st.fields {
                collect_ref_names(&f.field_type, out);
            }
        }
        IrType::Enum(_) => {}
        _ => {}
    }
}

fn count_methods(resources: &[IrResource]) -> usize {
    let mut n = 0;
    for r in resources {
        n += r.methods.len();
        n += count_methods(&r.sub_resources);
    }
    n
}

fn walk_resource_methods(res: &IrResource, f: &mut dyn FnMut(&IrMethod)) {
    for m in &res.methods {
        f(m);
    }
    for sub in &res.sub_resources {
        walk_resource_methods(sub, f);
    }
}

fn refs_from_method(m: &IrMethod, live: &mut HashSet<String>) {
    for p in &m.path_params {
        refs_from_field(p, live);
    }
    for p in &m.query_params {
        refs_from_field(p, live);
    }
    if let Some(t) = &m.request_type {
        collect_refs(t, live);
    }
    if let Some(t) = &m.response_type {
        collect_refs(t, live);
    }
}

fn refs_from_field(f: &IrField, live: &mut HashSet<String>) {
    collect_refs(&f.field_type, live);
}

fn collect_refs(ty: &IrType, live: &mut HashSet<String>) {
    match ty {
        IrType::Ref(r) => {
            live.insert(r.clone());
        }
        IrType::Array(inner) => collect_refs(inner, live),
        IrType::Map(inner) => collect_refs(inner, live),
        IrType::Struct(st) => {
            for f in &st.fields {
                collect_refs(&f.field_type, live);
            }
        }
        IrType::Enum(_) => {}
        _ => {}
    }
}

fn filter_resources_whitelist(resources: &mut Vec<IrResource>, patterns: &[String]) {
    for res in resources.iter_mut() {
        filter_resource_whitelist(res, patterns, "");
    }
    resources.retain(|r| !r.methods.is_empty() || !r.sub_resources.is_empty());
}

fn filter_resource_whitelist(res: &mut IrResource, patterns: &[String], prefix: &str) {
    let path = if prefix.is_empty() {
        res.name.clone()
    } else {
        format!("{prefix}.{}", res.name)
    };

    res.methods.retain(|m| {
        patterns
            .iter()
            .any(|p| pattern_matches(p, &path, &m.rust_name))
    });

    for sub in res.sub_resources.iter_mut() {
        filter_resource_whitelist(sub, patterns, &path);
    }
    res.sub_resources
        .retain(|r| !r.methods.is_empty() || !r.sub_resources.is_empty());
}

fn filter_resources_blacklist(resources: &mut Vec<IrResource>, patterns: &[String]) {
    for res in resources.iter_mut() {
        filter_resource_blacklist(res, patterns, "");
    }
    resources.retain(|r| !r.methods.is_empty() || !r.sub_resources.is_empty());
}

fn filter_resource_blacklist(res: &mut IrResource, patterns: &[String], prefix: &str) {
    let path = if prefix.is_empty() {
        res.name.clone()
    } else {
        format!("{prefix}.{}", res.name)
    };

    res.methods.retain(|m| {
        !patterns
            .iter()
            .any(|p| pattern_matches(p, &path, &m.rust_name))
    });

    for sub in res.sub_resources.iter_mut() {
        filter_resource_blacklist(sub, patterns, &path);
    }
    res.sub_resources
        .retain(|r| !r.methods.is_empty() || !r.sub_resources.is_empty());
}

/// `resource_path` is the dotted path from the root resource (e.g. `files`, `users.messages`).
/// `method_name` is the REST method key (`list`, `get`).
pub fn pattern_matches(pattern: &str, resource_path: &str, method_name: &str) -> bool {
    if pattern.ends_with(".**") {
        let prefix = pattern.trim_end_matches(".**");
        return resource_path == prefix
            || resource_path.starts_with(&format!("{prefix}."));
    }
    if pattern.ends_with(".*") {
        let prefix = pattern.trim_end_matches(".*");
        return resource_path == prefix;
    }
    if let Some(pos) = pattern.rfind('.') {
        let res = &pattern[..pos];
        let method = &pattern[pos + 1..];
        if !method.contains('*') {
            return resource_path == res && method_name == method;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_exact() {
        assert!(pattern_matches("files.list", "files", "list"));
        assert!(!pattern_matches("files.list", "files", "get"));
    }

    #[test]
    fn pattern_star_resource() {
        assert!(pattern_matches("files.*", "files", "list"));
        assert!(pattern_matches("files.*", "files", "get"));
        assert!(!pattern_matches("files.*", "about", "get"));
    }

    #[test]
    fn pattern_nested_star() {
        assert!(pattern_matches(
            "users.messages.*",
            "users.messages",
            "list"
        ));
        assert!(!pattern_matches("users.messages.*", "users", "list"));
    }

    #[test]
    fn pattern_recursive() {
        assert!(pattern_matches("users.**", "users", "get"));
        assert!(pattern_matches(
            "users.**",
            "users.messages",
            "list"
        ));
        assert!(!pattern_matches("users.**", "other", "get"));
    }
}
