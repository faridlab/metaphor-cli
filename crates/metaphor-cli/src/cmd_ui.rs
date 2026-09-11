//! `metaphor ui` — launch the interactive terminal UI.
//!
//! The UI itself is the separate `metaphor-ui` npm package (Ink 7). This
//! command resolves that executable with the same lookup order plugins use
//! — plus a `METAPHOR_UI_BIN` override that exists so tests (and power
//! users) can point at a specific binary — and spawns it with inherited
//! stdio, exporting `METAPHOR_LAUNCHER` so the UI always queries the binary
//! that launched it rather than whatever `metaphor` is first on `$PATH`.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;

pub const UI_BINARY: &str = "metaphor-ui";

pub const INSTALL_HINT: &str = "`metaphor-ui` is not installed.

Install it with:
  npm install -g @metaphor/metaphor-ui   (requires Node.js >= 20)

Or point METAPHOR_UI_BIN at an existing metaphor-ui binary.
Or run `metaphor repl` for the classic interactive REPL.";

/// Locate the UI executable. Order: `METAPHOR_UI_BIN` override (only when it
/// actually exists — a stale value falls through to normal resolution), then
/// the shared plugin resolution (`$METAPHOR_PLUGIN_BIN_DIR` → `$PATH` →
/// `~/.metaphor/bin`).
pub fn resolve_ui_binary() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("METAPHOR_UI_BIN") {
        let path = PathBuf::from(p);
        if path.exists() {
            return Some(path);
        }
    }
    crate::cmd_plugins::resolve_installed(UI_BINARY)
}

/// Spawn the UI and wait. Inherited stdio keeps the UI fully interactive;
/// the child's exit code is surfaced unchanged.
pub fn spawn_ui(bin: &PathBuf) -> Result<()> {
    let launcher = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .unwrap_or_else(|| PathBuf::from("metaphor"));
    let status = Command::new(bin)
        .env("METAPHOR_LAUNCHER", &launcher)
        .status()
        .with_context(|| format!("failed to spawn {} — is node on $PATH?", bin.display()))?;
    if !status.success() {
        bail!("metaphor-ui exited with status: {}", status);
    }
    Ok(())
}

/// `metaphor ui` — resolve and launch, bailing with the install hint when
/// the UI is missing.
pub fn cmd_ui() -> Result<()> {
    match resolve_ui_binary() {
        Some(bin) => spawn_ui(&bin),
        None => bail!("{INSTALL_HINT}"),
    }
}
