//! Resolve plugin tool binaries at invocation time.
//!
//! Lookup order:
//! 1. `$METAPHOR_PLUGIN_BIN_DIR/<name>` if the env var is set
//! 2. plain `<name>` (relies on `$PATH`)
//!
//! This keeps metaphor decoupled from where the plugin tools live. Each
//! plugin (e.g. metaphor-schema) is its own standalone repo and produces
//! a binary by the same name. For
//! local development you can point
//! `METAPHOR_PLUGIN_BIN_DIR` at a directory containing those binaries
//! (typically multiple `target/debug/` symlinked together, or a single
//! install dir).

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

pub fn plugin_binary(name: &str) -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("METAPHOR_PLUGIN_BIN_DIR") {
        let candidate = PathBuf::from(dir).join(name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Ok(PathBuf::from(name))
}

/// Run a plugin binary with the given subcommand and args.
pub fn passthrough(binary: &str, subcommand: &str, args: &[String]) -> Result<()> {
    let bin = plugin_binary(binary)?;
    let status = Command::new(&bin)
        .arg(subcommand)
        .args(args)
        .status()
        .with_context(|| format!("failed to spawn {} — is it installed?", binary))?;
    if !status.success() {
        anyhow::bail!("{} exited with status: {}", binary, status);
    }
    Ok(())
}

/// Run a plugin binary forwarding all args directly (no subcommand prefix).
pub fn passthrough_raw(binary: &str, args: &[String]) -> Result<()> {
    let bin = plugin_binary(binary)?;
    let status = Command::new(&bin)
        .args(args)
        .status()
        .with_context(|| format!("failed to spawn {} — is it installed?", binary))?;
    if !status.success() {
        anyhow::bail!("{} exited with status: {}", binary, status);
    }
    Ok(())
}
