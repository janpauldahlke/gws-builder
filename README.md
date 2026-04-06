# gws-builder

Build-time Rust code generation from [Google API Discovery](https://developers.google.com/discovery) documents: typed structs, enums, method parameter helpers, and static [`ActionDescriptor`](docs/BUILD_PLAN.md) data for agents. The default `HttpFetcher` uses blocking HTTP (`ureq`), suitable for `build.rs`.

Design details and roadmap are in [`docs/BUILD_PLAN.md`](docs/BUILD_PLAN.md).

## Whitelist-driven generation (recommended)

**Only what you configure is generated:**

1. **`BuilderConfig::services`** — list **only** the Google APIs you need (e.g. `drive` + `gmail`), each with a **version** (`v3`, `v1`, …). APIs not listed are not emitted at all.
2. **Per-API whitelist** — use [`ServiceSpec::whitelist`] so each service includes **only** the REST methods matching your patterns. Unused methods and unreachable schemas are dropped (smaller crates, fewer OAuth scopes to reason about).

Discover method ids first, then encode them as patterns (`files.*`, `users.messages.list`, `users.**`):

```rust
use gws_builder::{list_available_actions, HttpFetcher, ServiceSpec, ActionFilter};

let actions = list_available_actions(
    &[ServiceSpec {
        name: "drive".into(),
        version: "v3".into(),
        filter: ActionFilter::All, // catalog ignores filter; any placeholder is fine
    }],
    &HttpFetcher::new(),
)?;
for a in &actions {
    println!("{}", a.id); // e.g. drive.files.list → use patterns like "files.*"
}
```

Empty whitelist patterns are **rejected** at `generate()` time.

## Add to your crate

In the **consumer** crate that will run codegen (often the same crate that owns `build.rs`):

```toml
[build-dependencies]
gws-builder = { path = "../gws-builder" }   # or version from crates.io when published
```

## Minimal `build.rs` (whitelist)

```rust
use std::path::PathBuf;

use gws_builder::{
    generate, BuilderConfig, RegenerationPolicy, ServiceSpec,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let report = generate(BuilderConfig {
        services: vec![
            // Only these APIs appear under `generated/gws_types/`.
            ServiceSpec::whitelist(
                "drive",
                "v3",
                vec!["files.*".into(), "permissions.list".into()],
            )?,
            // Add more `ServiceSpec::whitelist(...)` entries as needed.
        ],
        out_dir: PathBuf::from("generated/gws_types"),
        regeneration: RegenerationPolicy::IfChanged,
        fetcher: None,
        cache_dir: None,
    })?;

    eprintln!(
        "gws-builder: generated {} actions, {} schemas; skipped: {:?}",
        report.actions_emitted,
        report.schemas_emitted,
        report.services_skipped
    );
    Ok(())
}
```

`ActionFilter::All` still exists for bring-up or tooling, but **downstream products should prefer whitelists** so generated code matches the surface you actually ship.

Generation runs when you build; `cargo build` must be able to reach `https://www.googleapis.com` (or use a cache—see below).

## Use the generated code

Point a module at the generated `mod.rs` (Pattern B: checked into the repo, one directory per `out_dir`):

```rust
// In src/lib.rs or src/main.rs of the consumer crate:
#[path = "../generated/gws_types/mod.rs"]
mod gws_types;

use gws_types::drive::File;
use gws_types::all_actions;
```

Adjust the `#[path = ...]` relative to the file that contains it.

## API keys, OAuth, and “which Workspace user?”

**Handle this in the downstream consumer, not in `gws-builder`.**

`gws-builder` only downloads **public** Discovery documents and emits Rust types plus static `ActionDescriptor` metadata. It does **not** call Google APIs on behalf of an account, and it has no notion of API keys, OAuth clients, refresh tokens, or a target user or domain. Adding those concerns here would mix **build-time codegen** with **runtime credentials** and would encourage secrets in the wrong place.

**What the consumer owns:**

| Concern | Where it lives |
| --------|---------------|
| Google Cloud project, OAuth client ID/secret (or service account JSON) | Consumer app config / secret store (env, vault), never in generated code |
| Which human or mailbox “this run” acts as | The identity behind **OAuth** (user consent) or **domain-wide delegation** (service account impersonating `user@example.com`, configured by a Workspace admin) |
| Access tokens, refresh tokens, `Authorization: Bearer …` | Your HTTP client at runtime (`reqwest`, `hyper`, etc.) |
| Choosing scopes | Match [`ActionDescriptor::scopes`](docs/BUILD_PLAN.md) (and Google’s docs) to what you request at consent time |

**API keys alone** are usually **not** enough for private Workspace data (Drive files, Gmail, Calendar events). Google expects **OAuth 2.0** (user or delegated) or **service accounts** with appropriate Workspace admin setup. Your agent crate should use the official patterns from [Google’s OAuth documentation](https://developers.google.com/identity/protocols/oauth2).

### Example: realistic downstream layout

Imagine an internal **agent** binary `workspace-agent` that lists Drive files for the signed-in user:

1. **Build time** — `build.rs` runs `gws_builder::generate` (as above) and checks in `generated/gws_types/`.
2. **Runtime** — `main` loads client credentials from the environment, runs an OAuth flow (or reads a stored refresh token), obtains an access token, and calls REST endpoints that match the generated types.

```text
workspace-agent/
  build.rs                 # gws-builder only: types + ActionDescriptors
  src/
    main.rs                # OAuth + HTTP; uses generated modules
  generated/gws_types/       # committed (Pattern B) or generated in CI
```

```rust
// Pseudocode — illustrative only; use a real OAuth crate and token storage.
// After codegen: mod gws_types { ... }

use gws_types::drive; // generated structs + LIST_ACTION, etc.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // From env or secret manager — NOT from gws-builder:
    let access_token = oauth::user_access_token_from_env().await?;

    // "Whose Workspace?" = whoever completed OAuth consent for these scopes.
    let url = "https://www.googleapis.com/drive/v3/files?pageSize=10";
    let client = reqwest::Client::new();
    let body = client
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await?
        .text()
        .await?;

    let list: drive::FileList = serde_json::from_str(&body)?;
    // Use drive::files::LIST_ACTION.scopes etc. to document required OAuth scopes in your UI.
    println!("{:?}", list);
    Ok(())
}
```

The generated `ActionDescriptor` values (scopes, path templates, HTTP methods) are there so your consumer can **drive consent prompts, tool routing, and request construction** — not so `gws-builder` can store tenants or keys.

## Output layout

- **`{out_dir}/`** — e.g. `generated/gws_types/`: `mod.rs`, `serde_helpers.rs`, one `*.rs` per API (e.g. `drive.rs`).
- **`generation_manifest.json`** — written next to the parent of `out_dir` (e.g. `generated/generation_manifest.json`), used for `RegenerationPolicy::IfChanged` / `IfMissing`.

## Action filters

`ActionFilter` controls which REST methods are kept (and which schemas survive pruning):

- **`All`** — every method.
- **`Whitelist(vec)`** — patterns such as:
  - `files.list` — resource `files`, method `list`
  - `files.*` — all methods on resource `files`
  - `users.messages.*` — all methods on nested resource `users.messages`
  - `users.**` — all methods on `users` and any sub-resource under it
- **`Blacklist(vec)`** — same pattern syntax, methods excluded.

Example:

```rust
filter: ActionFilter::Whitelist(vec![
    "files.*".into(),
    "permissions.list".into(),
]),
```

## Regeneration policy

- **`IfChanged`** (default) — fetch Discovery JSON; if revision, checksum, and filter fingerprint match the manifest and the service file exists, skip that service.
- **`Always`** — always fetch and regenerate.
- **`IfMissing`** — skip when the manifest already lists the service and the service `.rs` file exists.
- **`Never`** — do not fetch; require existing `generation_manifest.json` and the service `.rs` file (fail if missing).

## Listing actions before you whitelist

Use this from a small binary or a one-off `build.rs` helper to print available methods (no code generation):

```rust
use gws_builder::{list_available_actions, HttpFetcher, ServiceSpec, ActionFilter};

let actions = list_available_actions(
    &[ServiceSpec {
        name: "drive".into(),
        version: "v3".into(),
        filter: ActionFilter::All,
    }],
    &HttpFetcher::new(),
)?;

for a in &actions {
    println!("{} {} {}", a.id, a.http_method, a.description);
}
```

## Caching Discovery JSON

If the network fetch fails, you can set **`cache_dir`** on `BuilderConfig` to a directory where `gws-builder` will write `{service}_{version}.json` after a successful fetch and read it back on failure.

## Tests and development

This repository’s crate:

```bash
cargo test
cargo clippy --all-targets
```

To smoke-test codegen into `./generated/gws_types` (embedded sample Discovery JSON, no network):

```bash
cargo run --example generate_here
```

To fetch **Drive v3**, **Gmail v1**, and **Calendar v3** with **full** surface (`ActionFilter::All`, for exploration or baselines):

```bash
cargo run --example generate_google --release
```

For a **consumer-style** run (only the APIs and method patterns you list — recommended for real apps):

```bash
cargo run --example consumer_whitelist --release
```

That output path is listed in `.gitignore` so generated files stay local unless you force-add them.
