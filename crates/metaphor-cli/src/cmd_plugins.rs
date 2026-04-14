//! `metaphor plugins` — list plugin binaries visible to the current install.
//!
//! Minimal v1: enumerates the three known plugins (`metaphor-schema`,
//! `metaphor-codegen`, `metaphor-dev`), resolves their paths via the usual
//! lookup (`$METAPHOR_PLUGIN_BIN_DIR` then `$PATH`), and runs `--version` on
//! each that's present. The richer "advertised ToolCapability set" view
//! depends on the in-process plugin registry (roadmap Phase 4).
//!
//! Output: friendly text by default, stable JSON envelope with `--json`.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Known plugin names and the `metaphor` subcommands they back.
/// Kept in sync with the dispatch table in `main.rs` — if a new plugin lands,
/// add it here too.
pub const KNOWN_PLUGINS: &[(&str, &[&str])] = &[
    ("metaphor-schema", &["schema", "webapp"]),
    (
        "metaphor-codegen",
        &["make", "module", "apps", "proto", "migration", "seed"],
    ),
    (
        "metaphor-dev",
        &["dev", "lint", "test", "docs", "config", "jobs"],
    ),
];

pub struct PluginInfo {
    pub name: &'static str,
    pub commands: &'static [&'static str],
    pub path: Option<PathBuf>,
    pub version: Option<String>,
}

pub fn cmd_plugins(json: bool) -> Result<()> {
    let infos: Vec<PluginInfo> = KNOWN_PLUGINS
        .iter()
        .map(|(name, commands)| discover(name, commands))
        .collect();

    if json {
        let data: Vec<_> = infos
            .iter()
            .map(|i| {
                serde_json::json!({
                    "name": i.name,
                    "commands": i.commands,
                    "path": i.path.as_ref().map(|p| p.display().to_string()),
                    "version": i.version,
                    "installed": i.path.is_some(),
                })
            })
            .collect();
        let payload = crate::json_envelope(serde_json::json!({ "plugins": data }));
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("Known plugins:");
        for i in &infos {
            let cmds = i.commands.join(", ");
            match (&i.path, &i.version) {
                (Some(p), Some(v)) => println!(
                    "  \u{2713} {name} [{cmds}]\n      path:    {path}\n      version: {version}",
                    name = i.name,
                    cmds = cmds,
                    path = p.display(),
                    version = v,
                ),
                (Some(p), None) => println!(
                    "  \u{2713} {name} [{cmds}]\n      path:    {path}\n      version: (unknown)",
                    name = i.name,
                    cmds = cmds,
                    path = p.display(),
                ),
                (None, _) => println!(
                    "  \u{2717} {name} [{cmds}]  (not installed)",
                    name = i.name,
                    cmds = cmds,
                ),
            }
        }
    }
    Ok(())
}

fn discover(name: &'static str, commands: &'static [&'static str]) -> PluginInfo {
    let path = resolve_installed(name);
    let version = path.as_deref().and_then(query_version);
    PluginInfo {
        name,
        commands,
        path,
        version,
    }
}

/// Find the plugin on disk. Returns `None` if the binary isn't installed in
/// `$METAPHOR_PLUGIN_BIN_DIR` and isn't on `$PATH`. Shared with `cmd_doctor`.
pub fn resolve_installed(name: &str) -> Option<PathBuf> {
    // 1. METAPHOR_PLUGIN_BIN_DIR
    if let Ok(dir) = std::env::var("METAPHOR_PLUGIN_BIN_DIR") {
        let candidate = PathBuf::from(dir).join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    // 2. $PATH — walk each entry and test for an executable file.
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(p) {
        Ok(md) => md.is_file() && md.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    // On Windows (or anything non-unix) we rely on `.exists()` + is_file —
    // the OS enforces executability via file extension.
    std::fs::metadata(p).map(|md| md.is_file()).unwrap_or(false)
}

fn query_version(path: &Path) -> Option<String> {
    let output = Command::new(path).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}
