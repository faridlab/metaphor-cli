//! Resolve backbone tool binaries at invocation time.
//!
//! Lookup order:
//! 1. `$METAPHOR_BACKBONE_BIN_DIR/<name>` if the env var is set
//! 2. plain `<name>` (relies on `$PATH`)
//!
//! This keeps metaphor decoupled from where the backbone tools live. For
//! local development you can point `METAPHOR_BACKBONE_BIN_DIR` at the
//! `monorepo-backbone/target/debug/` directory and metaphor will use the
//! binaries built there.

use anyhow::Result;
use std::path::PathBuf;

pub fn backbone_binary(name: &str) -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("METAPHOR_BACKBONE_BIN_DIR") {
        let candidate = PathBuf::from(dir).join(name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Ok(PathBuf::from(name))
}
