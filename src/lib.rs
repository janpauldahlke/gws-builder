//! Static Rust codegen from Google Discovery documents (build-time / `build.rs`).
//!
//! See crate `docs/BUILD_PLAN.md` for design.

mod catalog;
mod codegen;
pub mod discovery;
mod emit;
mod error;
mod fetch;
pub mod ir;
mod manifest;

pub use emit::{emit_service_rust, write_file as emit_write_file};

pub use catalog::list_available_actions;
pub use error::BuilderError;
pub use fetch::{DiscoveryFetcher, HttpFetcher, MapFetcher};

use std::path::{Path, PathBuf};

use crate::discovery::RestDescription;
use crate::emit::{
    emit_serde_helpers, write_file, write_generation_manifest, write_mod_rs,
};
use crate::fetch::{read_cache, validate_api_identifier, write_cache};
use crate::ir::{apply_filter, discovery_to_ir, resolve_service};
use crate::manifest::{
    entry_for_service, load as load_manifest, service_unchanged, sha256_json, GenerationManifest,
};

/// Which API methods to include in generated code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionFilter {
    /// Include all resources and methods.
    All,
    /// Include only methods matching these patterns (`files.*`, `files.list`, `users.**`).
    Whitelist(Vec<String>),
    /// Include everything except methods matching these patterns.
    Blacklist(Vec<String>),
}

/// One Google API (e.g. Drive v3) to generate.
///
/// **Only APIs you list in [`BuilderConfig::services`] are codegenned.** To keep binaries small and
/// scopes minimal, give each API a [`ActionFilter::Whitelist`] (see [`ServiceSpec::whitelist`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSpec {
    pub name: String,
    pub version: String,
    pub filter: ActionFilter,
}

impl ServiceSpec {
    /// Emit only methods matching these patterns; see [`ActionFilter`] and the crate README.
    ///
    /// Returns an error if `patterns` is empty — use [`list_available_actions`] to discover ids,
    /// then add patterns such as `files.*` or `users.messages.list`.
    pub fn whitelist(
        name: impl Into<String>,
        version: impl Into<String>,
        patterns: Vec<String>,
    ) -> Result<Self, BuilderError> {
        if patterns.is_empty() {
            return Err(BuilderError::Resolution(
                "whitelist patterns must not be empty: list APIs in BuilderConfig.services, then \
                 add one or more patterns per API (use list_available_actions() to discover them)"
                    .into(),
            ));
        }
        Ok(Self {
            name: name.into(),
            version: version.into(),
            filter: ActionFilter::Whitelist(patterns),
        })
    }
}

/// When to fetch Discovery docs and regenerate Rust sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RegenerationPolicy {
    /// Always fetch and regenerate.
    Always,
    /// Regenerate when revision or checksum (or filter) changes vs manifest.
    #[default]
    IfChanged,
    /// Generate only when manifest or outputs are missing.
    IfMissing,
    /// Never fetch; require existing outputs.
    Never,
}

/// Configuration for [`generate`].
pub struct BuilderConfig {
    pub services: Vec<ServiceSpec>,
    pub out_dir: PathBuf,
    pub regeneration: RegenerationPolicy,
    pub fetcher: Option<Box<dyn DiscoveryFetcher>>,
    /// Optional cache directory for raw Discovery JSON (used when fetch fails).
    pub cache_dir: Option<PathBuf>,
}

/// Summary of one available REST method (for whitelisting).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionSummary {
    pub service: String,
    pub resource_path: String,
    pub method: String,
    pub id: String,
    pub http_method: String,
    pub description: String,
    pub deprecated: bool,
}

/// Result of a [`generate`] run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationReport {
    pub services_generated: Vec<String>,
    pub services_skipped: Vec<String>,
    pub actions_emitted: usize,
    pub schemas_emitted: usize,
}

/// Run fetch → parse → filter → IR → codegen → emit for each configured service.
pub fn generate(config: BuilderConfig) -> Result<GenerationReport, BuilderError> {
    for spec in &config.services {
        validate_service_spec(spec)?;
    }

    let fetcher: Box<dyn DiscoveryFetcher> = config
        .fetcher
        .unwrap_or_else(|| Box::new(HttpFetcher::new()));

    let manifest_path = manifest_path_for_out(&config.out_dir);
    let existing = load_manifest(&manifest_path)?;

    let mut report = GenerationReport {
        services_generated: Vec::new(),
        services_skipped: Vec::new(),
        actions_emitted: 0,
        schemas_emitted: 0,
    };

    let mut manifest = existing.unwrap_or_else(|| GenerationManifest {
        gws_builder_version: env!("CARGO_PKG_VERSION").to_string(),
        generated_at: String::new(),
        services: std::collections::HashMap::new(),
    });

    for spec in &config.services {
        validate_api_identifier(&spec.name)?;
        validate_api_identifier(&spec.version)?;

        let out_file = config.out_dir.join(format!("{}.rs", spec.name));

        match config.regeneration {
            RegenerationPolicy::Never => {
                if !manifest_path.exists() || !out_file.exists() {
                    return Err(BuilderError::Resolution(format!(
                        "RegenerationPolicy::Never requires existing manifest and {}.rs",
                        spec.name
                    )));
                }
                report
                    .services_skipped
                    .push(format!("{}/{}", spec.name, spec.version));
                continue;
            }
            RegenerationPolicy::IfMissing => {
                if manifest.services.contains_key(&spec.name) && out_file.exists() {
                    report
                        .services_skipped
                        .push(format!("{}/{}", spec.name, spec.version));
                    continue;
                }
            }
            _ => {}
        }

        let raw = match fetcher.fetch_document(&spec.name, &spec.version) {
            Ok(s) => s,
            Err(e) => {
                if let Some(ref dir) = config.cache_dir {
                    if let Some(cached) = read_cache(dir, &spec.name, &spec.version) {
                        eprintln!(
                            "gws-builder: using cached Discovery JSON for {}/{} ({e})",
                            spec.name, spec.version
                        );
                        cached
                    } else {
                        return Err(e);
                    }
                } else {
                    return Err(e);
                }
            }
        };

        if let Some(ref dir) = config.cache_dir {
            write_cache(dir, &spec.name, &spec.version, &raw);
        }

        let doc: RestDescription = serde_json::from_str(&raw).map_err(|e| BuilderError::Parse {
            service: spec.name.clone(),
            source: e,
        })?;

        let revision = doc.revision.clone().unwrap_or_default();
        let checksum = sha256_json(&raw);

        if matches!(config.regeneration, RegenerationPolicy::IfChanged) {
            if let Some(entry) = manifest.services.get(&spec.name) {
                if service_unchanged(entry, &revision, &checksum, &spec.filter) && out_file.exists()
                {
                    report
                        .services_skipped
                        .push(format!("{}/{}", spec.name, spec.version));
                    continue;
                }
            }
        }

        let mut ir = discovery_to_ir(&doc)?;
        apply_filter(&mut ir, &spec.filter)?;
        resolve_service(&mut ir)?;

        let rust = emit_service_rust(&ir)?;
        write_file(&out_file, &rust)?;

        let actions = count_actions(&ir);
        let schemas = ir.structs.len() + ir.enums.len();

        manifest.services.insert(
            spec.name.clone(),
            entry_for_service(&raw, Some(&revision), actions, schemas, spec),
        );

        report.actions_emitted += actions;
        report.schemas_emitted += schemas;
        report.services_generated.push(spec.name.clone());
    }

    let helpers_path = config.out_dir.join("serde_helpers.rs");
    write_file(&helpers_path, emit_serde_helpers())?;

    let modules = list_existing_service_modules(&config.out_dir, &config.services);
    write_mod_rs(&config.out_dir, &modules)?;

    manifest.generated_at = now_iso8601();
    write_generation_manifest(&manifest_path, &manifest)?;

    Ok(report)
}

fn now_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Seconds since epoch is stable without extra deps; consumers can format if needed.
    format!("{secs}")
}

fn list_existing_service_modules(out_dir: &Path, specs: &[ServiceSpec]) -> Vec<String> {
    specs
        .iter()
        .filter(|s| out_dir.join(format!("{}.rs", s.name)).exists())
        .map(|s| s.name.clone())
        .collect()
}

fn count_actions(service: &crate::ir::IrService) -> usize {
    fn walk(res: &crate::ir::IrResource) -> usize {
        let mut n = res.methods.len();
        for s in &res.sub_resources {
            n += walk(s);
        }
        n
    }
    service.resources.iter().map(walk).sum()
}

fn manifest_path_for_out(out_dir: &Path) -> PathBuf {
    out_dir
        .parent()
        .unwrap_or(Path::new("."))
        .join("generation_manifest.json")
}

fn validate_service_spec(spec: &ServiceSpec) -> Result<(), BuilderError> {
    if let ActionFilter::Whitelist(p) = &spec.filter {
        if p.is_empty() {
            return Err(BuilderError::Resolution(format!(
                "service `{}`: whitelist patterns must not be empty (remove the API or add patterns)",
                spec.name
            )));
        }
    }
    Ok(())
}
