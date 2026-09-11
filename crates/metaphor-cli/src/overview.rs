//! `metaphor overview` — workspace state at first sight.
//!
//! Assembles, in one pass, everything the interactive UI's overview screen
//! (and any script) needs to orient: workspace identity, the current
//! project, runtime apps with their local version and git state, the
//! deployment environments defined in `metaphor.deploy.yaml` joined with
//! the last successful record from `deployment/history/<env>.jsonl`
//! (which service is running which version where), plugin install state,
//! and a doctor-style health tally.
//!
//! Read-only by design: git state is read with porcelain commands; deploy
//! metadata is parsed minimally (version-gated); env-file contents are
//! NEVER read or emitted — paths and names only, they carry secrets.
//! Missing pieces become `null` sections, never errors.

use anyhow::Result;
use metaphor_workspace::{Manifest, ProjectType};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Runtime project types shown in the apps list. Modules/crates/infra are
/// libraries or build inputs, not things a user runs or deploys.
fn is_runtime_type(t: ProjectType) -> bool {
    matches!(
        t,
        ProjectType::BackendService
            | ProjectType::Webservice
            | ProjectType::Webapp
            | ProjectType::Mobileapp
            | ProjectType::Desktopapp
    )
}

fn type_label(t: ProjectType) -> &'static str {
    match t {
        ProjectType::BackendService => "backend-service",
        ProjectType::Webservice => "webservice",
        ProjectType::Webapp => "webapp",
        ProjectType::Mobileapp => "mobileapp",
        ProjectType::Desktopapp => "desktopapp",
        ProjectType::Module => "module",
        ProjectType::Crate => "crate",
        ProjectType::CliTool => "cli-tool",
        ProjectType::Infra => "infra",
        ProjectType::DocsSite => "docs-site",
    }
}

pub fn cmd_overview(manifest: &Manifest, root: &Path, cwd: &Path, json: bool) -> Result<()> {
    let payload = build_overview(manifest, root, cwd);
    if json {
        let payload = crate::json_envelope(payload);
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        print_text(&payload);
    }
    Ok(())
}

/// Build the overview payload (unwrapped — `cmd_overview` applies the envelope).
pub fn build_overview(manifest: &Manifest, root: &Path, cwd: &Path) -> serde_json::Value {
    // Current project: where in the workspace are we, and what would a
    // change here ripple into? (Same logic as `metaphor info`.)
    let current_project = manifest.current_project(root, cwd);
    let current_project_json = current_project.map(|p| {
        let depended_by: Vec<&str> = manifest
            .projects
            .iter()
            .filter(|other| other.depends_on.iter().any(|d| d == &p.name))
            .map(|other| other.name.as_str())
            .collect();
        json!({
            "name": p.name,
            "type": type_label(p.project_type),
            "path": p.path,
            "depends_on": p.depends_on,
            "depended_by": depended_by,
        })
    });

    // Apps: runtime projects only — enough to orient, cheap enough to
    // compute on every launch (fs metadata + two git calls per app).
    let apps: Vec<serde_json::Value> = manifest
        .projects
        .iter()
        .filter(|p| is_runtime_type(p.project_type))
        .map(|p| {
            let dir = root.join(&p.path);
            let exists = dir.is_dir();
            json!({
                "name": p.name,
                "type": type_label(p.project_type),
                "path": p.path,
                "exists": exists,
                "version": if exists { read_project_version(&dir) } else { None },
                "git_branch": if exists { git(&["rev-parse", "--abbrev-ref", "HEAD"], &dir) } else { None },
                "git_dirty": if exists { git(&["status", "--porcelain"], &dir).map(|s| !s.is_empty()) } else { None },
            })
        })
        .collect();

    // Deployment environments + currently deployed versions.
    let deploy_file = root.join("metaphor.deploy.yaml");
    let (environments, recent_deployments, deploy_hint): (
        serde_json::Value,
        Vec<serde_json::Value>,
        Option<String>,
    ) = if !deploy_file.exists() {
        (
            serde_json::Value::Null,
            Vec::new(),
            Some(
                "no metaphor.deploy.yaml at the workspace root — `metaphor deploy --help` explains the format"
                    .to_string(),
            ),
        )
    } else {
        match read_deploy_file(&deploy_file) {
            Some(deploy) => {
                let history_dir = root.join("deployment/history");
                let mut envs = Vec::new();
                let mut recent: Vec<serde_json::Value> = Vec::new();

                for (name, env) in deploy.environments.iter() {
                    let records = read_history(&history_dir.join(format!("{}.jsonl", name)));

                    // Currently deployed version per service: the LAST
                    // successful record that mentions the service wins
                    // (history is append-only and chronological).
                    let mut current: BTreeMap<&str, &HistoryLite> = BTreeMap::new();
                    for r in records.iter() {
                        if r.status != "success" {
                            continue;
                        }
                        for svc in r.image_tags.keys() {
                            current.insert(svc.as_str(), r);
                        }
                    }

                    // Every service the env defines gets a row; ones never
                    // deployed show as null.
                    let services: Vec<serde_json::Value> = env
                        .images
                        .keys()
                        .map(|svc| match current.get(svc.as_str()) {
                            Some(r) => json!({
                                "service": svc,
                                "tag": r.tag,
                                "deployed_at": r.ts,
                                "deployed_by": r.deployer,
                            }),
                            None => json!({
                                "service": svc,
                                "tag": serde_json::Value::Null,
                                "deployed_at": serde_json::Value::Null,
                                "deployed_by": serde_json::Value::Null,
                            }),
                        })
                        .collect();

                    envs.push(json!({
                        "name": name,
                        "host": env.host,
                        "services": services,
                    }));

                    for r in records.iter().rev().take(10) {
                        recent.push(json!({
                            "env": name,
                            "ts": r.ts,
                            "action": r.action,
                            "status": r.status,
                            "tag": r.tag,
                            "deployer": r.deployer,
                        }));
                    }
                }

                recent.sort_by(|a, b| {
                    b["ts"]
                        .as_str()
                        .unwrap_or("")
                        .cmp(a["ts"].as_str().unwrap_or(""))
                });
                recent.truncate(10);

                (json!(envs), recent, None)
            }
            None => (
                serde_json::Value::Null,
                Vec::new(),
                Some(format!(
                    "{} could not be parsed (unsupported version?) — deploy sections omitted",
                    deploy_file.display()
                )),
            ),
        }
    };

    // Plugin install state.
    let plugins: Vec<serde_json::Value> = crate::cmd_plugins::KNOWN_PLUGINS
        .iter()
        .map(|spec| {
            let installed = crate::cmd_plugins::resolve_installed(spec.name).is_some();
            json!({
                "name": spec.name,
                "commands": spec.commands,
                "installed": installed,
            })
        })
        .collect();

    // Health tally from the same checks `metaphor doctor` runs.
    let checks = crate::cmd_doctor::run_checks(manifest, root);
    let (ok, warn, fail) = crate::cmd_doctor::tally(&checks);
    let worst: Vec<String> = checks
        .iter()
        .filter(|c| {
            c.status == crate::cmd_doctor::Status::Fail || c.status == crate::cmd_doctor::Status::Warn
        })
        .map(|c| format!("[{}] {}", c.status.as_str(), c.name))
        .take(8)
        .collect();

    json!({
        "workspace": {
            "root": root.display().to_string(),
            "project_count": manifest.projects.len(),
        },
        "current_project": current_project_json,
        "apps": apps,
        "environments": environments,
        "recent_deployments": recent_deployments,
        "deploy_hint": deploy_hint,
        "plugins": plugins,
        "health": {
            "ok": ok,
            "warn": warn,
            "fail": fail,
            "worst": worst,
        },
    })
}

// --- Deploy file + history (minimal parse) --------------------------------

#[derive(serde::Deserialize)]
struct DeployFile {
    version: u32,
    #[serde(default)]
    environments: BTreeMap<String, DeployEnv>,
}

#[derive(serde::Deserialize)]
struct DeployEnv {
    #[serde(default)]
    host: Option<String>,
    /// Service names only — image build details belong to the dev plugin.
    #[serde(default)]
    images: BTreeMap<String, serde_yaml::Value>,
}

#[derive(serde::Deserialize)]
struct HistoryLite {
    ts: String,
    action: String,
    status: String,
    tag: String,
    #[serde(default)]
    image_tags: BTreeMap<String, String>,
    deployer: String,
}

/// Parse `metaphor.deploy.yaml`. Returns None when missing/unparsable or
/// when the file version is not one we understand.
fn read_deploy_file(path: &Path) -> Option<DeployFile> {
    let text = fs::read_to_string(path).ok()?;
    let deploy: DeployFile = serde_yaml::from_str(&text).ok()?;
    if deploy.version != 1 {
        return None;
    }
    Some(deploy)
}

/// Read a history JSONL file. Lines that fail to parse are skipped — a
/// partial or corrupt line must never take down the whole overview.
fn read_history(path: &Path) -> Vec<HistoryLite> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    text.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

// --- Small helpers --------------------------------------------------------

/// Run a git command in `dir`, returning trimmed stdout on success.
fn git(args: &[&str], dir: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Local version of a project: Cargo.toml `[package] version` or package.json
/// `version` — whichever exists. `version.workspace = true` inheritance is
/// deliberately not chased (the line doesn't parse as a plain version).
fn read_project_version(dir: &Path) -> Option<String> {
    if let Ok(text) = fs::read_to_string(dir.join("Cargo.toml")) {
        let mut in_package = false;
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with('[') {
                in_package = t == "[package]";
                continue;
            }
            if in_package {
                if let Some(rest) = t.strip_prefix("version") {
                    let rest = rest.trim_start();
                    if let Some(v) = rest.strip_prefix('=') {
                        let v = v.trim().trim_matches('"');
                        if !v.is_empty() {
                            return Some(v.to_string());
                        }
                    }
                }
            }
        }
    }
    if let Ok(text) = fs::read_to_string(dir.join("package.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(s) = v.get("version").and_then(|x| x.as_str()) {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// Compact human summary (default output without `--json`).
fn print_text(payload: &serde_json::Value) {
    let ws = &payload["workspace"];
    println!(
        "workspace: {} ({} project(s))",
        ws["root"].as_str().unwrap_or("?"),
        ws["project_count"]
    );
    match payload["current_project"].as_object() {
        Some(cp) => println!(
            "current project: {} ({}) — depends_on: {}, depended-by: {}",
            cp["name"].as_str().unwrap_or("?"),
            cp["type"].as_str().unwrap_or("?"),
            cp["depends_on"].as_array().map(|a| a.len()).unwrap_or(0),
            cp["depended_by"].as_array().map(|a| a.len()).unwrap_or(0),
        ),
        None => println!("current project: (not inside any registered project)"),
    }
    println!("apps:");
    for app in payload["apps"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        println!(
            "  {:<24} {:<16} {}{}",
            app["name"].as_str().unwrap_or("?"),
            app["type"].as_str().unwrap_or("?"),
            app["version"]
                .as_str()
                .map(|v| format!("v{} ", v))
                .unwrap_or_default(),
            app["git_branch"]
                .as_str()
                .map(|b| format!("({})", b))
                .unwrap_or_default(),
        );
    }
    println!("environments:");
    match payload["environments"].as_array() {
        Some(envs) if !envs.is_empty() => {
            for env in envs {
                let host = env["host"]
                    .as_str()
                    .map(|h| h.to_string())
                    .unwrap_or_else(|| "local".to_string());
                println!("  {:<12} {}", env["name"].as_str().unwrap_or("?"), host);
                for svc in env["services"]
                    .as_array()
                    .map(|a| a.as_slice())
                    .unwrap_or(&[])
                {
                    println!(
                        "    {:<20} {}",
                        svc["service"].as_str().unwrap_or("?"),
                        match svc["tag"].as_str() {
                            Some(tag) => format!(
                                "{} (by {}, at {})",
                                tag,
                                svc["deployed_by"].as_str().unwrap_or("?"),
                                svc["deployed_at"].as_str().unwrap_or("?")
                            ),
                            None => "never deployed".to_string(),
                        }
                    );
                }
            }
        }
        _ => {
            if let Some(hint) = payload["deploy_hint"].as_str() {
                println!("  (none — {})", hint);
            } else {
                println!("  (none defined)");
            }
        }
    }
    let health = &payload["health"];
    println!(
        "health: {} ok, {} warn, {} fail",
        health["ok"], health["warn"], health["fail"]
    );
}
