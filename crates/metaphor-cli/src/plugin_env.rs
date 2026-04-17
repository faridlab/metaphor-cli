//! Resolve plugin tool binaries at invocation time.
//!
//! Shares its lookup with `cmd_plugins::resolve_installed` so the dispatch
//! path (`metaphor dev …`) and the listing path (`metaphor plugins`) can't
//! disagree about whether a plugin is installed. See that function for the
//! full lookup order (`$METAPHOR_PLUGIN_BIN_DIR` → `$PATH` → default
//! `~/.metaphor/bin`). Falls back to a bare name so the OS can have a final
//! try at `$PATH` resolution when the shared resolver says "not found" —
//! that way a clear "failed to spawn" error still comes out of `Command`.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use crate::cmd_plugins::resolve_installed;

pub fn plugin_binary(name: &str) -> Result<PathBuf> {
    Ok(resolve_installed(name).unwrap_or_else(|| PathBuf::from(name)))
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
