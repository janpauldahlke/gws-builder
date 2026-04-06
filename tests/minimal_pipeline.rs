//! Full pipeline on a tiny hand-written Discovery document.

use std::collections::HashMap;
use std::fs;

use gws_builder::{
    generate, list_available_actions, ActionFilter, BuilderConfig, BuilderError, MapFetcher,
    RegenerationPolicy, ServiceSpec,
};

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

#[test]
fn generate_with_map_fetcher_emits_rust() {
    let mut m = HashMap::new();
    m.insert(("testapi".into(), "v1".into()), MINIMAL.to_string());
    let tmp = tempfile::tempdir().expect("tmp");
    let out = tmp.path().join("gws_types");
    let report = generate(BuilderConfig {
        services: vec![ServiceSpec::whitelist("testapi", "v1", vec!["items.*".into()]).expect(
            "whitelist",
        )],
        out_dir: out.clone(),
        regeneration: RegenerationPolicy::Always,
        fetcher: Some(Box::new(MapFetcher { docs: m })),
        cache_dir: None,
    })
    .expect("generate");
    assert_eq!(report.services_generated, vec!["testapi"]);
    let src = fs::read_to_string(out.join("testapi.rs")).expect("read");
    assert!(src.contains("pub struct Thing"));
    assert!(src.contains("LIST_ACTION"));
    assert!(src.contains("ALL_ACTIONS"));
    assert!(out.join("serde_helpers.rs").exists());
    assert!(out.join("mod.rs").exists());
}

#[test]
fn list_available_actions_smoke() {
    let mut m = HashMap::new();
    m.insert(("testapi".into(), "v1".into()), MINIMAL.to_string());
    let fetcher = MapFetcher { docs: m };
    let actions = list_available_actions(
        &[ServiceSpec {
            name: "testapi".into(),
            version: "v1".into(),
            filter: ActionFilter::All,
        }],
        &fetcher,
    )
    .expect("catalog");
    assert!(actions.iter().any(|a| a.id.contains("items.list")));
}

#[test]
fn empty_whitelist_is_rejected() {
    let e = ServiceSpec::whitelist("drive", "v3", vec![]).expect_err("empty");
    assert!(matches!(e, BuilderError::Resolution(_)));
}
