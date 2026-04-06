//! Fetch Discovery docs for Drive, Gmail, and Calendar; emit into `./generated/gws_types`.
//!
//! Requires network access to `https://www.googleapis.com`.
//!
//! ```bash
//! cargo run --example generate_google --release
//! ```

use std::path::PathBuf;

use gws_builder::{
    generate, ActionFilter, BuilderConfig, RegenerationPolicy, ServiceSpec,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out_dir = root.join("generated/gws_types");

    let report = generate(BuilderConfig {
        services: vec![
            ServiceSpec {
                name: "drive".into(),
                version: "v3".into(),
                filter: ActionFilter::All,
            },
            ServiceSpec {
                name: "gmail".into(),
                version: "v1".into(),
                filter: ActionFilter::All,
            },
            ServiceSpec {
                name: "calendar".into(),
                version: "v3".into(),
                filter: ActionFilter::All,
            },
        ],
        out_dir: out_dir.clone(),
        regeneration: RegenerationPolicy::Always,
        fetcher: None,
        cache_dir: Some(root.join("generated/discovery_cache")),
    })?;

    println!("gws-builder: wrote generated files under {}", out_dir.display());
    println!(
        "  generated: {:?}, skipped: {:?}, actions: {}, schemas: {}",
        report.services_generated,
        report.services_skipped,
        report.actions_emitted,
        report.schemas_emitted
    );
    Ok(())
}
