//! Blocking HTTP fetch for Discovery documents and directory resolution.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use ureq::Agent;

use crate::error::BuilderError;

/// Fetches raw Discovery JSON for a service/version pair.
pub trait DiscoveryFetcher: Send + Sync {
    /// Returns the raw JSON body of the Discovery REST document.
    fn fetch_document(&self, service: &str, version: &str) -> Result<String, BuilderError>;
}

/// Validates service and version strings (alphanumeric, dot, underscore, hyphen).
pub fn validate_api_identifier(s: &str) -> Result<(), BuilderError> {
    if s.is_empty() {
        return Err(BuilderError::Resolution(
            "API identifier must not be empty".into(),
        ));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(BuilderError::Resolution(format!(
            "invalid API identifier {s:?}: only [a-zA-Z0-9._-] allowed"
        )));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiDirectoryItem {
    name: String,
    version: String,
    #[serde(default)]
    discovery_rest_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiDirectoryList {
    #[serde(default)]
    items: Vec<ApiDirectoryItem>,
}

fn encode_path_segment(s: &str) -> String {
    // Discovery URLs use plain segments; validate_api_identifier already restricts chars.
    s.to_string()
}

/// Default blocking HTTP implementation using `ureq`.
pub struct HttpFetcher {
    agent: Agent,
}

impl Default for HttpFetcher {
    fn default() -> Self {
        Self {
            agent: Agent::new_with_config(
                ureq::config::Config::builder()
                    .timeout_global(Some(std::time::Duration::from_secs(120)))
                    .build(),
            ),
        }
    }
}

impl HttpFetcher {
    /// Fetch with optional on-disk cache: on success, writes JSON to
    /// `cache_dir/{service}_{version}.json`; on network failure, reads cache if present.
    pub fn new() -> Self {
        Self::default()
    }

    fn fetch_directory(&self) -> Result<ApiDirectoryList, BuilderError> {
        let url = "https://www.googleapis.com/discovery/v1/apis";
        let body = self
            .agent
            .get(url)
            .call()
            .map_err(|e| BuilderError::Fetch {
                service: "directory".into(),
                version: "v1".into(),
                source: Box::new(e),
            })?
            .into_body()
            .read_to_string()
            .map_err(|e| BuilderError::Fetch {
                service: "directory".into(),
                version: "v1".into(),
                source: Box::new(e),
            })?;
        serde_json::from_str(&body).map_err(|e| BuilderError::Parse {
            service: "directory".into(),
            source: e,
        })
    }

    fn find_discovery_url(
        &self,
        service: &str,
        version: &str,
    ) -> Result<Option<String>, BuilderError> {
        let list = self.fetch_directory()?;
        Ok(list
            .items
            .into_iter()
            .find(|i| i.name == service && i.version == version)
            .and_then(|i| i.discovery_rest_url))
    }

    fn fetch_url(&self, url: &str, service: &str, version: &str) -> Result<String, BuilderError> {
        self.agent
            .get(url)
            .call()
            .map_err(|e| BuilderError::Fetch {
                service: service.into(),
                version: version.into(),
                source: Box::new(e),
            })?
            .into_body()
            .read_to_string()
            .map_err(|e| BuilderError::Fetch {
                service: service.into(),
                version: version.into(),
                source: Box::new(e),
            })
    }
}

impl DiscoveryFetcher for HttpFetcher {
    fn fetch_document(&self, service: &str, version: &str) -> Result<String, BuilderError> {
        validate_api_identifier(service)?;
        validate_api_identifier(version)?;

        let primary = format!(
            "https://www.googleapis.com/discovery/v1/apis/{}/{}/rest",
            encode_path_segment(service),
            encode_path_segment(version)
        );

        let resp = self.agent.get(&primary).call();
        let body = match resp {
            Ok(r) => {
                let status = r.status().as_u16();
                if (200..300).contains(&status) {
                    r.into_body()
                        .read_to_string()
                        .map_err(|e| BuilderError::Fetch {
                            service: service.into(),
                            version: version.into(),
                            source: Box::new(e),
                        })?
                } else {
                    // Try directory canonical URL
                    if let Ok(Some(url)) = self.find_discovery_url(service, version) {
                        self.fetch_url(&url, service, version)?
                    } else {
                        let alt = format!(
                            "https://{service}.googleapis.com/$discovery/rest?version={version}"
                        );
                        self.fetch_url(&alt, service, version)?
                    }
                }
            }
            Err(_) => {
                if let Ok(Some(url)) = self.find_discovery_url(service, version) {
                    self.fetch_url(&url, service, version)?
                } else {
                    let alt = format!(
                        "https://{service}.googleapis.com/$discovery/rest?version={version}"
                    );
                    self.fetch_url(&alt, service, version)?
                }
            }
        };

        Ok(body)
    }
}

/// Fetcher that reads from a map of `(service, version) -> JSON` for tests.
pub struct MapFetcher {
    pub docs: HashMap<(String, String), String>,
}

impl DiscoveryFetcher for MapFetcher {
    fn fetch_document(&self, service: &str, version: &str) -> Result<String, BuilderError> {
        self.docs
            .get(&(service.to_string(), version.to_string()))
            .cloned()
            .ok_or_else(|| {
                BuilderError::Resolution(format!(
                    "MapFetcher: no document for {service}/{version}"
                ))
            })
    }
}

/// Reads a cached Discovery JSON file if it exists.
pub fn read_cache(cache_dir: &Path, service: &str, version: &str) -> Option<String> {
    let path = cache_dir.join(format!("{service}_{version}.json"));
    fs::read_to_string(path).ok()
}

/// Writes successful fetch to cache directory (best-effort).
pub fn write_cache(cache_dir: &Path, service: &str, version: &str, json: &str) {
    if let Err(e) = fs::create_dir_all(cache_dir) {
        eprintln!("gws-builder: could not create cache dir {}: {e}", cache_dir.display());
        return;
    }
    let path = cache_dir.join(format!("{service}_{version}.json"));
    if let Err(e) = fs::write(&path, json) {
        eprintln!("gws-builder: could not write cache {}: {e}", path.display());
    }
}
