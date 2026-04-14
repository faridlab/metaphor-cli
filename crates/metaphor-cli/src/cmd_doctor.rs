//! `metaphor doctor` — diagnostic runner.
//!
//! Coordinates the existing validation primitives (manifest validate, plugin
//! discovery, env schema parse, etc.) and reports per-check `[OK]`/`[WARN]`/
//! `[FAIL]` lines with actionable hints. Exits non-zero iff any check fails.

use crate::cmd_plugins::{self, KNOWN_PLUGINS};
use anyhow::Result;
use colored::*;
use metaphor_workspace::Manifest;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Warn,
    Fail,
}

impl Status {
    fn tag(self) -> ColoredString {
        match self {
            Status::Ok => "[OK]  ".green().bold(),
            Status::Warn => "[WARN]".yellow().bold(),
            Status::Fail => "[FAIL]".red().bold(),
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Warn => "warn",
            Status::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Check {
    pub category: &'static str,
    pub name: String,
    pub status: Status,
    pub detail: Option<String>,
    pub hint: Option<String>,
}

impl Check {
    fn ok(category: &'static str, name: impl Into<String>) -> Self {
        Self {
            category,
            name: name.into(),
            status: Status::Ok,
            detail: None,
            hint: None,
        }
    }
    fn warn(
        category: &'static str,
        name: impl Into<String>,
        detail: impl Into<String>,
        hint: Option<&str>,
    ) -> Self {
        Self {
            category,
            name: name.into(),
            status: Status::Warn,
            detail: Some(detail.into()),
            hint: hint.map(str::to_string),
        }
    }
    fn fail(
        category: &'static str,
        name: impl Into<String>,
        detail: impl Into<String>,
        hint: Option<&str>,
    ) -> Self {
        Self {
            category,
            name: name.into(),
            status: Status::Fail,
            detail: Some(detail.into()),
            hint: hint.map(str::to_string),
        }
    }
}

pub fn cmd_doctor(manifest: &Manifest, workspace_root: &Path, json: bool) -> Result<()> {
    let checks = run_checks(manifest, workspace_root);
    let (ok, warn, fail) = tally(&checks);

    if json {
        print_json(workspace_root, &checks, ok, warn, fail)?;
    } else {
        print_text(workspace_root, &checks, ok, warn, fail);
    }

    if fail > 0 {
        anyhow::bail!("{} check(s) failed", fail);
    }
    Ok(())
}

fn run_checks(manifest: &Manifest, workspace_root: &Path) -> Vec<Check> {
    let mut checks = Vec::new();

    // ---- workspace ----
    checks.push(Check::ok(
        "workspace",
        format!("manifest valid ({} project(s))", manifest.projects.len()),
    ));

    if which("git") {
        checks.push(Check::ok("workspace", "git available"));
    } else {
        checks.push(Check::warn(
            "workspace",
            "git not found",
            "`metaphor build` tags with git SHAs and `--affected` needs git diff",
            Some("install git via your package manager"),
        ));
    }

    // Docker is only flagged when at least one project has a Dockerfile.
    let needs_docker = manifest
        .projects
        .iter()
        .any(|p| p.resolved_path(workspace_root).join("Dockerfile").exists());
    if needs_docker {
        if which("docker") {
            checks.push(Check::ok("workspace", "docker available"));
        } else {
            checks.push(Check::warn(
                "workspace",
                "docker not found (Dockerfiles detected)",
                "at least one project has a Dockerfile; `metaphor build` won't work without docker",
                Some("https://docs.docker.com/engine/install/"),
            ));
        }
    }

    // ---- projects ----
    for p in &manifest.projects {
        let root = p.resolved_path(workspace_root);
        let ctx = format!("{}", p.name);
        if !root.is_dir() {
            checks.push(Check::fail(
                "projects",
                format!("{ctx}: directory missing"),
                format!("expected at {}", root.display()),
                Some("clone the project into place, or update `path:` in metaphor.yaml"),
            ));
            // Skip per-file checks for missing projects.
            continue;
        }

        // Dockerfile / .dockerignore pair
        if root.join("Dockerfile").exists() && !root.join(".dockerignore").exists() {
            checks.push(Check::warn(
                "projects",
                format!("{ctx}: missing .dockerignore"),
                "Dockerfile present but no .dockerignore — the build context may include stray artifacts (target/, node_modules/, ...)",
                Some("add .dockerignore with: target/ node_modules/ build/ .git/ .metaphor/"),
            ));
        }

        // Convention files that parse as YAML
        yaml_parse_check(&mut checks, &ctx, &root, "metaphor.env.yaml");
        yaml_parse_check(&mut checks, &ctx, &root, "metaphor.build.yaml");
        yaml_parse_check(&mut checks, &ctx, &root, "compose.fragment.yml");
    }

    // ---- plugins ----
    for (name, _commands) in KNOWN_PLUGINS {
        let resolved = cmd_plugins::resolve_installed(name);
        if resolved.is_some() {
            checks.push(Check::ok("plugins", format!("{name} installed")));
        } else {
            checks.push(Check::warn(
                "plugins",
                format!("{name} not installed"),
                "metaphor subcommands that forward to this plugin will fail",
                Some("see docs/plugins.md for install options"),
            ));
        }
    }

    checks
}

fn yaml_parse_check(checks: &mut Vec<Check>, project_name: &str, project_root: &Path, file: &str) {
    let path = project_root.join(file);
    if !path.exists() {
        return;
    }
    let display = format!("{project_name}: {file}");
    match fs::read_to_string(&path) {
        Err(e) => checks.push(Check::warn(
            "projects",
            format!("{display} unreadable"),
            e.to_string(),
            None,
        )),
        Ok(raw) => {
            if let Err(e) = serde_yaml::from_str::<serde_yaml::Value>(&raw) {
                checks.push(Check::warn(
                    "projects",
                    format!("{display} invalid YAML"),
                    e.to_string(),
                    Some("fix the YAML syntax — commands consuming this file will fail"),
                ));
            }
        }
    }
}

fn which(bin: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path) {
        if dir.join(bin).exists() {
            return true;
        }
        // macOS / BSD: also try .exe on Windows (cross-platform safety)
        #[cfg(windows)]
        if dir.join(format!("{bin}.exe")).exists() {
            return true;
        }
    }
    false
}

fn tally(checks: &[Check]) -> (usize, usize, usize) {
    let mut ok = 0;
    let mut warn = 0;
    let mut fail = 0;
    for c in checks {
        match c.status {
            Status::Ok => ok += 1,
            Status::Warn => warn += 1,
            Status::Fail => fail += 1,
        }
    }
    (ok, warn, fail)
}

fn print_text(workspace_root: &Path, checks: &[Check], ok: usize, warn: usize, fail: usize) {
    println!(
        "metaphor doctor — checking workspace at {}",
        workspace_root.display()
    );
    println!();

    let mut current: &str = "";
    for c in checks {
        if c.category != current {
            if !current.is_empty() {
                println!();
            }
            println!("{}:", c.category);
            current = c.category;
        }
        println!("  {} {}", c.status.tag(), c.name);
        if let Some(d) = &c.detail {
            println!("         {d}");
        }
        if let Some(h) = &c.hint {
            println!("         {} {h}", "hint:".dimmed());
        }
    }
    println!();
    println!(
        "Summary: {} ok / {} warning{} / {} failure{}",
        ok,
        warn,
        if warn == 1 { "" } else { "s" },
        fail,
        if fail == 1 { "" } else { "s" },
    );
}

fn print_json(
    workspace_root: &Path,
    checks: &[Check],
    ok: usize,
    warn: usize,
    fail: usize,
) -> Result<()> {
    let items: Vec<_> = checks
        .iter()
        .map(|c| {
            serde_json::json!({
                "category": c.category,
                "name": c.name,
                "status": c.status.as_str(),
                "detail": c.detail,
                "hint": c.hint,
            })
        })
        .collect();
    let payload = crate::json_envelope(serde_json::json!({
        "workspace_root": workspace_root.display().to_string(),
        "checks": items,
        "summary": { "ok": ok, "warn": warn, "fail": fail },
    }));
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

// Used only for the `which` fallback; silence the unused warning when we
// never reach the windows-only block in cross-compilation.
#[allow(dead_code)]
fn _unused(_: PathBuf) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_finds_sh_on_unix_like_systems() {
        // `sh` is guaranteed on any unix host we care about.
        #[cfg(unix)]
        assert!(which("sh"));
    }

    #[test]
    fn which_returns_false_for_obvious_nonexistent_binary() {
        assert!(!which("this-binary-does-not-exist-metaphor-xyz"));
    }

    #[test]
    fn tally_counts_statuses() {
        let checks = vec![
            Check::ok("x", "one"),
            Check::warn("x", "two", "d", None),
            Check::fail("x", "three", "d", None),
            Check::ok("x", "four"),
        ];
        assert_eq!(tally(&checks), (2, 1, 1));
    }
}
