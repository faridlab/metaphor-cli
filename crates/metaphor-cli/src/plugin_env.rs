//! Resolve plugin tool binaries at invocation time.
//!
//! Lookup order:
//! 1. `$METAPHOR_PLUGIN_BIN_DIR/<name>` if the env var is set
//! 2. plain `<name>` (relies on `$PATH`)
//!
//! This keeps metaphor decoupled from where the plugin tools live. Each
//! plugin (metaphor-schema, metaphor-plugin-mobilegen, metaphor-plugin-webgen)
//! is its own standalone repo and produces a binary by the same name. For
//! local development you can point
//! `METAPHOR_PLUGIN_BIN_DIR` at a directory containing those binaries
//! (typically multiple `target/debug/` symlinked together, or a single
//! install dir).

use anyhow::Result;
use std::path::PathBuf;

pub fn plugin_binary(name: &str) -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("METAPHOR_PLUGIN_BIN_DIR") {
        let candidate = PathBuf::from(dir).join(name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Ok(PathBuf::from(name))
}
