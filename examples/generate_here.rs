//! One-shot codegen into `./generated/gws_types` (uses embedded Discovery JSON; no network).
//!
//! Run from the crate root:
//! `cargo run --example generate_here`

use std::collections::HashMap;
use std::path::PathBuf;

use gws_builder::{generate, BuilderConfig, MapFetcher, RegenerationPolicy, ServiceSpec};

const MINIMAL: &str = r#"{
  "name": "testapi",
  "version": "v1",
  "rootUrl": "https://example.googleapis.com/",
  "servicePath": "test/v1/",
  "schemas": {
    "Thing": {
      "id": "Thing",
      "type": "object",
      "properties": {
        "name": { "type": "string" },
        "count": { "type": "string", "format": "int64" }
      }
    }
  },
  "resources": {
    "items": {
      "methods": {
        "list": {
          "id": "testapi.items.list",
          "httpMethod": "GET",
          "path": "items",
          "response": { "$ref": "Thing" }
        }
      }
    }
  }
}"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out_dir = root.join("generated/gws_types");

    let mut docs = HashMap::new();
    docs.insert(
        ("testapi".to_string(), "v1".to_string()),
        MINIMAL.to_string(),
    );

    let report = generate(BuilderConfig {
        services: vec![ServiceSpec::whitelist("testapi", "v1", vec!["items.*".into()])?],
        out_dir: out_dir.clone(),
        regeneration: RegenerationPolicy::Always,
        fetcher: Some(Box::new(MapFetcher { docs })),
        cache_dir: None,
    })?;

    println!("gws-builder: wrote generated files under {}", out_dir.display());
    println!(
        "  services: {:?}, skipped: {:?}, actions: {}, schemas: {}",
        report.services_generated,
        report.services_skipped,
        report.actions_emitted,
        report.schemas_emitted
    );
    Ok(())
}
