//! Example **consumer-style** `generate` call: only the APIs listed below are emitted, and each API
//! only includes methods matching the whitelist patterns (smaller crates, fewer OAuth scopes).
//!
//! Adjust patterns to match what your app needs; use `list_available_actions()` first to discover ids.
//!
//! ```bash
//! cargo run --example consumer_whitelist --release
//! ```

use std::path::PathBuf;

use gws_builder::{
    generate, BuilderConfig, RegenerationPolicy, ServiceSpec,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out_dir = root.join("generated/gws_types");

    let services = vec![
        // Drive: only file + permission surface (tighten further per product).
        ServiceSpec::whitelist(
            "drive",
            "v3",
            vec![
                "files.*".into(),
                "permissions.*".into(),
            ],
        )?,
        // Gmail: messages under users only (add threads.* etc. as needed).
        ServiceSpec::whitelist(
            "gmail",
            "v1",
            vec!["users.messages.*".into(), "users.labels.*".into()],
        )?,
        // Calendar: calendars + events.
        ServiceSpec::whitelist(
            "calendar",
            "v3",
            vec!["calendars.*".into(), "events.*".into()],
        )?,
    ];

    let report = generate(BuilderConfig {
        services,
        out_dir: out_dir.clone(),
        regeneration: RegenerationPolicy::IfChanged,
        fetcher: None,
        cache_dir: Some(root.join("generated/discovery_cache")),
    })?;

    println!("gws-builder: wrote under {}", out_dir.display());
    println!(
        "  generated: {:?}, skipped: {:?}, actions: {}, schemas: {}",
        report.services_generated,
        report.services_skipped,
        report.actions_emitted,
        report.schemas_emitted
    );
    Ok(())
}
