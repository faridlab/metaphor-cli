//! `metaphor plugin add <name>[@<version>]` — install a known plugin binary
//! from its GitHub release.
//!
//! v1 restricts `<name>` to the entries in [`KNOWN_PLUGINS`]. Arbitrary
//! plugins wait on the in-process registry (see roadmap.md).
//!
//! Download mechanism: shells out to `curl` and `tar`, matching `install.sh`
//! so the same tools cover CLI and plugin installation with no new Rust deps.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cmd_plugins::{default_install_dir, query_version, PluginSpec, KNOWN_PLUGINS};

pub fn cmd_plugin_add(spec_str: &str) -> Result<()> {
    let (name, version) = parse_spec(spec_str)?;
    let plugin = find_known_plugin(&name)?;
    let target = detect_target()?;
    let url = asset_url(plugin, &version, target);
    let install_dir = resolve_install_dir()?;

    println!("Downloading {} ({target}) from {url}", plugin.name);
    download_and_install(&url, &install_dir, plugin.name)?;

    let installed = install_dir.join(plugin.name);
    let version_str = query_version(&installed).unwrap_or_else(|| "(unknown)".into());
    println!("Installed {} to {}", plugin.name, installed.display());
    println!("  version: {version_str}");
    maybe_print_path_tip(&install_dir, plugin.name);
    Ok(())
}

/// Split `<name>[@<version>]`. An absent `@` defaults to `latest`.
fn parse_spec(s: &str) -> Result<(String, String)> {
    match s.split_once('@') {
        None if !s.is_empty() => Ok((s.into(), "latest".into())),
        Some((name, ver)) if !name.is_empty() && !ver.is_empty() => {
            Ok((name.into(), ver.into()))
        }
        _ => bail!("invalid plugin spec '{s}' (expected <name>[@<version>])"),
    }
}

fn find_known_plugin(name: &str) -> Result<&'static PluginSpec> {
    KNOWN_PLUGINS.iter().find(|p| p.name == name).ok_or_else(|| {
        let valid: Vec<_> = KNOWN_PLUGINS.iter().map(|p| p.name).collect();
        anyhow::anyhow!(
            "unknown plugin '{name}'. Known plugins: {}",
            valid.join(", ")
        )
    })
}

/// Map the current host to a release-asset target triple.
fn detect_target() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        (os, arch) => bail!("unsupported platform: {os}-{arch}"),
    }
}

/// Release-asset URL. `latest` uses GitHub's auto-redirect endpoint; any
/// other value is treated as a tag, with a leading `v` added if missing.
fn asset_url(plugin: &PluginSpec, version: &str, target: &str) -> String {
    let asset = format!("{}-{target}.tar.gz", plugin.name);
    if version == "latest" {
        format!(
            "https://github.com/{repo}/releases/latest/download/{asset}",
            repo = plugin.repo,
        )
    } else {
        let tag = if version.starts_with('v') {
            version.to_string()
        } else {
            format!("v{version}")
        };
        format!(
            "https://github.com/{repo}/releases/download/{tag}/{asset}",
            repo = plugin.repo,
        )
    }
}

fn resolve_install_dir() -> Result<PathBuf> {
    let dir = if let Ok(custom) = std::env::var("METAPHOR_PLUGIN_BIN_DIR") {
        PathBuf::from(custom)
    } else {
        default_install_dir().context("could not determine home directory")?
    };
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create {}", dir.display()))?;
    Ok(dir)
}

fn download_and_install(url: &str, install_dir: &Path, binary_name: &str) -> Result<()> {
    let tmp = tempfile::tempdir().context("failed to create temp dir")?;
    let tarball = tmp.path().join("plugin.tar.gz");

    let status = Command::new("curl")
        .args(["-fsSL", url, "-o"])
        .arg(&tarball)
        .status()
        .context("failed to spawn curl — is it installed?")?;
    if !status.success() {
        bail!("download failed: {url}");
    }

    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&tarball)
        .arg("-C")
        .arg(tmp.path())
        .status()
        .context("failed to spawn tar — is it installed?")?;
    if !status.success() {
        bail!("failed to extract {}", tarball.display());
    }

    let extracted = tmp.path().join(binary_name);
    if !extracted.exists() {
        bail!(
            "tarball did not contain '{binary_name}' at its root — \
             check the release assets follow the contract in docs/plugins.md"
        );
    }

    // Stage as <name>.new then rename so a failed copy can't corrupt an
    // existing install.
    let dest = install_dir.join(binary_name);
    let staging = install_dir.join(format!("{binary_name}.new"));
    std::fs::copy(&extracted, &staging).with_context(|| {
        format!("failed to copy binary into {}", install_dir.display())
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&staging)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&staging, perms)?;
    }
    std::fs::rename(&staging, &dest)
        .with_context(|| format!("failed to place binary at {}", dest.display()))?;
    Ok(())
}

fn maybe_print_path_tip(dir: &Path, binary_name: &str) {
    // The resolver already checks $METAPHOR_PLUGIN_BIN_DIR and the default
    // ~/.metaphor/bin, so `metaphor <subcommand>` will find the plugin
    // without any further setup. The only reason to add it to $PATH is if
    // the user wants to invoke the plugin binary directly from their shell.
    if std::env::var_os("METAPHOR_PLUGIN_BIN_DIR").is_some() {
        return;
    }
    let on_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d == dir))
        .unwrap_or(false);
    if !on_path {
        println!();
        println!(
            "Tip: add {} to your PATH to run `{}` directly from your shell:",
            dir.display(),
            binary_name,
        );
        println!("  export PATH=\"{}:$PATH\"", dir.display());
        println!("(metaphor itself already finds the plugin without this.)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_spec_defaults_to_latest() {
        assert_eq!(
            parse_spec("metaphor-dev").unwrap(),
            ("metaphor-dev".into(), "latest".into())
        );
    }

    #[test]
    fn parse_spec_takes_explicit_version() {
        assert_eq!(
            parse_spec("metaphor-dev@0.1.0").unwrap(),
            ("metaphor-dev".into(), "0.1.0".into())
        );
        assert_eq!(
            parse_spec("metaphor-dev@latest").unwrap(),
            ("metaphor-dev".into(), "latest".into())
        );
    }

    #[test]
    fn parse_spec_rejects_malformed() {
        assert!(parse_spec("").is_err());
        assert!(parse_spec("@1.0.0").is_err());
        assert!(parse_spec("metaphor-dev@").is_err());
    }

    #[test]
    fn find_known_plugin_rejects_unknown() {
        let err = find_known_plugin("metaphor-bogus")
            .map(|_| ())
            .unwrap_err()
            .to_string();
        assert!(err.contains("metaphor-bogus"));
        assert!(err.contains("metaphor-dev"));
    }

    #[test]
    fn asset_url_uses_latest_download_path() {
        let dev = find_known_plugin("metaphor-dev").unwrap();
        let url = asset_url(dev, "latest", "aarch64-apple-darwin");
        assert_eq!(
            url,
            "https://github.com/faridlab/metaphor-plugin-dev/releases/latest/download/\
             metaphor-dev-aarch64-apple-darwin.tar.gz"
        );
    }

    #[test]
    fn asset_url_prefixes_v_when_missing() {
        let dev = find_known_plugin("metaphor-dev").unwrap();
        let url = asset_url(dev, "0.1.0", "x86_64-unknown-linux-gnu");
        assert!(url.contains("/releases/download/v0.1.0/"));
    }

    #[test]
    fn asset_url_keeps_existing_v_prefix() {
        let dev = find_known_plugin("metaphor-dev").unwrap();
        let url = asset_url(dev, "v0.1.0", "x86_64-unknown-linux-gnu");
        assert!(url.contains("/releases/download/v0.1.0/"));
        assert!(!url.contains("/vv0.1.0/"));
    }
}
