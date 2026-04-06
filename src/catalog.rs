//! Pre-codegen action listing (`list_available_actions`).

use crate::discovery::RestDescription;
use crate::error::BuilderError;
use crate::fetch::DiscoveryFetcher;
use crate::{ActionSummary, ServiceSpec};

/// Walk Discovery resources and collect a flat list of actions (no codegen).
pub fn list_available_actions(
    services: &[ServiceSpec],
    fetcher: &dyn DiscoveryFetcher,
) -> Result<Vec<ActionSummary>, BuilderError> {
    let mut out = Vec::new();
    for spec in services {
        let raw = fetcher.fetch_document(&spec.name, &spec.version)?;
        let doc: RestDescription = serde_json::from_str(&raw).map_err(|e| BuilderError::Parse {
            service: spec.name.clone(),
            source: e,
        })?;
        walk_resources(&doc, &spec.name, "", &doc.resources, &mut out)?;
    }
    Ok(out)
}

fn walk_resources(
    _doc: &RestDescription,
    service: &str,
    prefix: &str,
    map: &std::collections::HashMap<String, crate::discovery::RestResource>,
    out: &mut Vec<ActionSummary>,
) -> Result<(), BuilderError> {
    for (key, res) in map {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        for (mname, m) in &res.methods {
            let id = m
                .id
                .clone()
                .unwrap_or_else(|| format!("{service}.{path}.{mname}"));
            out.push(ActionSummary {
                service: service.to_string(),
                resource_path: path.clone(),
                method: mname.clone(),
                id,
                http_method: m.http_method.clone(),
                description: m.description.clone().unwrap_or_default(),
                deprecated: false,
            });
        }
        walk_resources(_doc, service, &path, &res.resources, out)?;
    }
    Ok(())
}
