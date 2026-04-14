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
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

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

/// Captured result of a plugin invocation. Used by run-many to buffer output
/// per-project so parallel runs stay readable.
pub struct CapturedRun {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Spawn a plugin binary with captured stdio in the given working directory.
/// `argv` is the full argument list (subcommand + user args, if any).
pub fn run_captured(binary: &str, argv: &[String], cwd: &Path) -> Result<CapturedRun> {
    let bin = plugin_binary(binary)?;
    let output = Command::new(&bin)
        .args(argv)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to spawn {} — is it installed?", binary))?;
    Ok(CapturedRun {
        status: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

// --- Synthetic ExitStatus helpers ---------------------------------------
//
// Cache replays and pre-spawn errors need an `ExitStatus` value even though
// no real process exited. These helpers fabricate one per platform.

#[cfg(unix)]
pub fn success_status() -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from_raw(0)
}

#[cfg(unix)]
pub fn failed_status() -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    // Unix wait-status encoding: high byte = exit code, low byte = signal.
    // 1 << 8 == exit status 1 with no signal.
    ExitStatus::from_raw(1 << 8)
}

#[cfg(windows)]
pub fn success_status() -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    ExitStatus::from_raw(0)
}

#[cfg(windows)]
pub fn failed_status() -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    ExitStatus::from_raw(1)
}

#[cfg(not(any(unix, windows)))]
pub fn success_status() -> ExitStatus {
    Command::new("true")
        .status()
        .expect("no way to synthesize a success ExitStatus on this platform")
}

#[cfg(not(any(unix, windows)))]
pub fn failed_status() -> ExitStatus {
    Command::new("false").status().unwrap_or_else(|_| {
        Command::new("sh")
            .args(["-c", "exit 1"])
            .status()
            .expect("no way to synthesize a failed ExitStatus on this platform")
    })
}
