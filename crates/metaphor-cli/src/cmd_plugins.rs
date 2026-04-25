//! `metaphor plugins` — list plugin binaries visible to the current install.
//!
//! Minimal v1: enumerates the known plugins (`metaphor-schema`,
//! `metaphor-codegen`, `metaphor-dev`, `metaphor-agent`), resolves their paths
//! via the usual lookup (`$METAPHOR_PLUGIN_BIN_DIR` then `$PATH`), and runs
//! `--version` on each that's present. The richer "advertised ToolCapability
//! set" view depends on the in-process plugin registry (roadmap Phase 4).
//!
//! Output: friendly text by default, stable JSON envelope with `--json`.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Known plugin metadata — binary name, source repo, and the `metaphor`
/// subcommands each one backs. Kept in sync with the dispatch table in
/// `main.rs` and the install path in `cmd_plugin_add.rs`.
pub struct PluginSpec {
    pub name: &'static str,
    pub repo: &'static str,
    pub commands: &'static [&'static str],
}

pub const KNOWN_PLUGINS: &[PluginSpec] = &[
    PluginSpec {
        name: "metaphor-schema",
        repo: "faridlab/metaphor-plugin-schema",
        commands: &["schema", "webapp"],
    },
    PluginSpec {
        name: "metaphor-codegen",
        repo: "faridlab/metaphor-plugin-codegen",
        commands: &["make", "module", "apps", "proto", "migration", "seed"],
    },
    PluginSpec {
        name: "metaphor-dev",
        repo: "faridlab/metaphor-plugin-dev",
        commands: &["dev", "lint", "test", "docs", "config", "jobs", "docker", "deploy"],
    },
    PluginSpec {
        name: "metaphor-agent",
        repo: "faridlab/metaphor-skill-agents",
        commands: &["agent"],
    },
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
        .map(|p| discover(p.name, p.commands))
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

/// Default location where `metaphor plugin add` drops binaries when
/// `$METAPHOR_PLUGIN_BIN_DIR` isn't set. Shared between install and discovery
/// so the two sides of the CLI can't disagree. `None` only if we can't
/// resolve the home directory.
pub fn default_install_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".metaphor/bin"))
}

/// Find the plugin on disk. Returns `None` if the binary isn't installed in
/// `$METAPHOR_PLUGIN_BIN_DIR`, isn't on `$PATH`, and isn't in the default
/// install dir (`~/.metaphor/bin`). Shared with `cmd_doctor` and
/// `cmd_plugin_add`.
pub fn resolve_installed(name: &str) -> Option<PathBuf> {
    // 1. METAPHOR_PLUGIN_BIN_DIR — explicit user opt-in wins.
    if let Ok(dir) = std::env::var("METAPHOR_PLUGIN_BIN_DIR") {
        let candidate = PathBuf::from(dir).join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    // 2. $PATH — walk each entry and test for an executable file.
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(name);
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    // 3. ~/.metaphor/bin — the default install dir for `metaphor plugin add`.
    //    Letting discovery find it here means the install and list commands
    //    agree even before the user edits their shell PATH.
    if let Some(dir) = default_install_dir() {
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

pub fn query_version(path: &Path) -> Option<String> {
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
