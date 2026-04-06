//! `generation_manifest.json` read/write and filter fingerprinting.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::BuilderError;
use crate::{ActionFilter, ServiceSpec};

/// On-disk record of a generation run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerationManifest {
    pub gws_builder_version: String,
    pub generated_at: String,
    pub services: HashMap<String, ServiceManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceManifestEntry {
    pub revision: String,
    pub json_sha256: String,
    pub actions_emitted: usize,
    pub schemas_emitted: usize,
    pub filter_applied: String,
}

pub fn filter_fingerprint(filter: &ActionFilter) -> String {
    match filter {
        ActionFilter::All => "All".into(),
        ActionFilter::Whitelist(p) => format!("Whitelist({p:?})"),
        ActionFilter::Blacklist(p) => format!("Blacklist({p:?})"),
    }
}

pub fn load(path: &Path) -> Result<Option<GenerationManifest>, BuilderError> {
    if !path.exists() {
        return Ok(None);
    }
    let s = fs::read_to_string(path).map_err(|e| BuilderError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let m: GenerationManifest = serde_json::from_str(&s).map_err(|e| BuilderError::Parse {
        service: "manifest".into(),
        source: e,
    })?;
    Ok(Some(m))
}

pub fn save(path: &Path, manifest: &GenerationManifest) -> Result<(), BuilderError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| BuilderError::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    let json = serde_json::to_string_pretty(manifest).map_err(|e| BuilderError::Codegen(
        format!("manifest serialize: {e}"),
    ))?;
    atomic_write(path, json.as_bytes())
}

pub fn sha256_json(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn atomic_write(path: &Path, data: &[u8]) -> Result<(), BuilderError> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, data).map_err(|e| BuilderError::Io {
        path: tmp.clone(),
        source: e,
    })?;
    fs::rename(&tmp, path).map_err(|e| BuilderError::Io {
        path: path.to_path_buf(),
        source: e,
    })
}

/// True if manifest lists the service with matching revision, checksum, and filter.
pub fn service_unchanged(
    entry: &ServiceManifestEntry,
    revision: &str,
    json_sha256: &str,
    filter: &ActionFilter,
) -> bool {
    entry.revision == revision
        && entry.json_sha256 == json_sha256
        && entry.filter_applied == filter_fingerprint(filter)
}

/// Build manifest entry fields from raw JSON and counts.
pub fn entry_for_service(
    raw_json: &str,
    revision: Option<&str>,
    actions_emitted: usize,
    schemas_emitted: usize,
    spec: &ServiceSpec,
) -> ServiceManifestEntry {
    ServiceManifestEntry {
        revision: revision.unwrap_or("").to_string(),
        json_sha256: sha256_json(raw_json),
        actions_emitted,
        schemas_emitted,
        filter_applied: filter_fingerprint(&spec.filter),
    }
}
