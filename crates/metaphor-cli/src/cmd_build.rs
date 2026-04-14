//! `metaphor build` — `docker build` per project with consistent tagging.
//!
//! Reuses the Phase B fan-out helpers so `--all`, `--projects`, `--affected`,
//! `--parallel`, and `--continue-on-error` work uniformly. Each project's
//! `Dockerfile` (or `--dockerfile` override) is built from the project's
//! `resolved_path` and tagged with `{name}:{git_sha}` by default. `--push`
//! pushes after a successful build. `--tag` is repeatable and accepts
//! `{name}` / `{git_sha}` / `{version}` placeholders.
//!
//! This is a thin coordinator: it delegates the actual docker invocation
//! to `docker` on the user's `$PATH`. Not a Docker-in-Rust implementation.

use crate::affected;
use crate::graph::Graph;
use crate::plugin_env::{failed_status, success_status, CapturedRun};
use anyhow::{bail, Context, Result};
use clap::Args;
use colored::*;
use metaphor_workspace::{Manifest, Project};
use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

/// Optional per-project file: `<project>/metaphor.build.yaml`. Overrides
/// the workspace-wide `--dockerfile` default and adds project-specific tags.
#[derive(Debug, Default, Deserialize)]
struct BuildConfig {
    #[serde(default)]
    dockerfile: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

impl BuildConfig {
    fn load(project_root: &Path) -> Self {
        let path = project_root.join("metaphor.build.yaml");
        fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_yaml::from_str(&raw).ok())
            .unwrap_or_default()
    }
}

/// Flags shared between `build` and future image-producing commands.
/// Mirrors the shape of `RunFlags` but with build-specific extras.
#[derive(Args, Debug, Clone)]
pub struct BuildFlags {
    /// Build every registered project
    #[arg(long, conflicts_with_all = ["projects", "affected"])]
    pub all: bool,

    /// Build only these projects (comma-separated)
    #[arg(long, value_delimiter = ',', conflicts_with = "affected")]
    pub projects: Vec<String>,

    /// Build only projects affected by git changes (and dependents)
    #[arg(long)]
    pub affected: bool,

    /// Base git ref for --affected
    #[arg(long, default_value = "main")]
    pub base: String,

    /// Head git ref for --affected
    #[arg(long, default_value = "HEAD")]
    pub head: String,

    /// Max concurrent project builds (default: 1)
    #[arg(long, default_value_t = 1)]
    pub parallel: usize,

    /// Keep building remaining projects on failure
    #[arg(long)]
    pub continue_on_error: bool,

    /// Image tag template(s). Repeatable. Placeholders: {name}, {git_sha}, {version}
    #[arg(long = "tag", value_name = "TEMPLATE")]
    pub tag_templates: Vec<String>,

    /// Override the Dockerfile name (relative to each project's root)
    #[arg(long, default_value = "Dockerfile")]
    pub dockerfile: String,

    /// Push every successfully built tag after build
    #[arg(long)]
    pub push: bool,

    /// Dry-run: print the docker commands that would run
    #[arg(long)]
    pub dry_run: bool,
}

impl BuildFlags {
    pub fn has_selector(&self) -> bool {
        self.all || !self.projects.is_empty() || self.affected
    }
}

pub fn cmd_build(manifest: &Manifest, workspace_root: &Path, flags: &BuildFlags) -> Result<()> {
    if !flags.has_selector() {
        bail!("metaphor build requires one of --all, --projects, or --affected");
    }

    let selected = select_projects(manifest, workspace_root, flags)?;
    if selected.is_empty() {
        println!("No projects matched.");
        return Ok(());
    }

    // Fallback sha for projects whose own git rev-parse fails.
    let workspace_sha = resolve_git_sha(workspace_root).unwrap_or_else(|_| "unknown".into());
    let cli_templates: Vec<String> = if flags.tag_templates.is_empty() {
        vec!["{name}:{git_sha}".to_string()]
    } else {
        flags.tag_templates.clone()
    };

    let parallel = flags.parallel.max(1);
    let mut failures: Vec<String> = Vec::new();

    if parallel == 1 {
        for p in &selected {
            let outcome = build_one(p, workspace_root, &cli_templates, &workspace_sha, flags);
            print_outcome(&outcome);
            if !outcome.success {
                failures.push(outcome.project_name.clone());
                if !flags.continue_on_error {
                    break;
                }
            }
        }
    } else {
        let workers = parallel.min(selected.len());
        let next = Mutex::new(0usize);
        let results: Vec<Mutex<Option<BuildOutcome>>> =
            (0..selected.len()).map(|_| Mutex::new(None)).collect();
        std::thread::scope(|s| {
            for _ in 0..workers {
                s.spawn(|| loop {
                    let idx = {
                        let mut n = next.lock().unwrap();
                        let i = *n;
                        *n += 1;
                        i
                    };
                    if idx >= selected.len() {
                        break;
                    }
                    let p = selected[idx];
                    let outcome =
                        build_one(p, workspace_root, &cli_templates, &workspace_sha, flags);
                    *results[idx].lock().unwrap() = Some(outcome);
                });
            }
        });
        for slot in &results {
            if let Some(outcome) = slot.lock().unwrap().take() {
                let name = outcome.project_name.clone();
                let ok = outcome.success;
                print_outcome(&outcome);
                if !ok {
                    failures.push(name);
                }
            }
        }
    }

    if !failures.is_empty() {
        bail!(
            "build failed in {} project(s): {}",
            failures.len(),
            failures.join(", ")
        );
    }
    Ok(())
}

struct BuildOutcome {
    project_name: String,
    tags: Vec<String>,
    runs: Vec<(String, CapturedRun)>, // (step label, captured output)
    success: bool,
}

fn build_one(
    project: &Project,
    workspace_root: &Path,
    cli_templates: &[String],
    workspace_sha: &str,
    flags: &BuildFlags,
) -> BuildOutcome {
    let cwd = project.resolved_path(workspace_root);
    let config = BuildConfig::load(&cwd);
    let dockerfile = config
        .dockerfile
        .clone()
        .unwrap_or_else(|| flags.dockerfile.clone());

    // Project-specific sha falls back to workspace sha when the project
    // directory isn't its own git repo. Matches the "independent repos"
    // model where each project *may* have its own history.
    let git_sha = resolve_git_sha(&cwd).unwrap_or_else(|_| workspace_sha.to_string());

    // Tags = CLI templates ∪ per-project `tags:` list. Dedup preserving order.
    let mut seen = std::collections::BTreeSet::new();
    let mut tags: Vec<String> = Vec::new();
    for t in cli_templates.iter().chain(config.tags.iter()) {
        let expanded = expand_template(t, &project.name, &git_sha);
        if seen.insert(expanded.clone()) {
            tags.push(expanded);
        }
    }

    if flags.dry_run {
        let argv = docker_build_argv(&dockerfile, &tags);
        let mut out = format!("would run: docker {}\n", shell_quote_argv(&argv));
        if flags.push {
            for t in &tags {
                out.push_str(&format!("would run: docker push {}\n", shell_quote(t)));
            }
        }
        return BuildOutcome {
            project_name: project.name.clone(),
            tags,
            runs: vec![(
                "dry-run".into(),
                CapturedRun {
                    status: success_status(),
                    stdout: out.into_bytes(),
                    stderr: Vec::new(),
                },
            )],
            success: true,
        };
    }

    let mut runs = Vec::new();
    let build_argv = docker_build_argv(&dockerfile, &tags);
    let build_run = run_docker(&build_argv, &cwd);
    let build_ok = build_run.status.success();
    runs.push(("build".into(), build_run));

    let mut success = build_ok;
    if flags.push && build_ok {
        for tag in &tags {
            let push_run = run_docker(&["push".into(), tag.clone()], &cwd);
            if !push_run.status.success() {
                success = false;
            }
            runs.push((format!("push {tag}"), push_run));
            if !success {
                break;
            }
        }
    }

    BuildOutcome {
        project_name: project.name.clone(),
        tags,
        runs,
        success,
    }
}

/// Shell-quote a single token so the dry-run output is copy-pasteable.
fn shell_quote(s: &str) -> String {
    // Use single quotes; escape internal `'` as `'\''`. Numbers, letters,
    // `/.:_-+=@,{}` are safe; otherwise quote.
    let safe = |c: char| {
        c.is_ascii_alphanumeric()
            || matches!(
                c,
                '/' | '.' | ':' | '_' | '-' | '+' | '=' | '@' | ',' | '{' | '}'
            )
    };
    if !s.is_empty() && s.chars().all(safe) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

fn shell_quote_argv(argv: &[String]) -> String {
    argv.iter()
        .map(|s| shell_quote(s))
        .collect::<Vec<_>>()
        .join(" ")
}

fn docker_build_argv(dockerfile: &str, tags: &[String]) -> Vec<String> {
    let mut argv = vec!["build".into(), "-f".into(), dockerfile.into()];
    for t in tags {
        argv.push("-t".into());
        argv.push(t.clone());
    }
    argv.push(".".into());
    argv
}

fn run_docker(args: &[String], cwd: &Path) -> CapturedRun {
    match Command::new("docker").args(args).current_dir(cwd).output() {
        Ok(out) => CapturedRun {
            status: out.status,
            stdout: out.stdout,
            stderr: out.stderr,
        },
        Err(e) => CapturedRun {
            status: failed_status(),
            stdout: Vec::new(),
            stderr: format!("docker spawn error: {e}\n").into_bytes(),
        },
    }
}

fn resolve_git_sha(workspace_root: &Path) -> Result<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .current_dir(workspace_root)
        .output()
        .context("invoking git rev-parse")?;
    if !out.status.success() {
        bail!("git rev-parse failed");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn expand_template(template: &str, name: &str, git_sha: &str) -> String {
    template
        .replace("{name}", name)
        .replace("{git_sha}", git_sha)
        .replace("{version}", git_sha) // v1: same as git_sha; real semver later
}

fn select_projects<'a>(
    manifest: &'a Manifest,
    workspace_root: &Path,
    flags: &BuildFlags,
) -> Result<Vec<&'a Project>> {
    let graph = Graph::from_manifest(manifest);
    let order = graph.topo_sort()?;
    use std::collections::BTreeSet;
    let filter: BTreeSet<String> = if flags.all {
        manifest.projects.iter().map(|p| p.name.clone()).collect()
    } else if !flags.projects.is_empty() {
        let wanted: BTreeSet<String> = flags.projects.iter().cloned().collect();
        for n in &wanted {
            manifest.find_project(n)?;
        }
        wanted
    } else if flags.affected {
        affected::affected_projects(manifest, &graph, workspace_root, &flags.base, &flags.head)?
    } else {
        bail!("select_projects called without a selector flag");
    };
    let mut out = Vec::new();
    for name in order {
        if filter.contains(&name) {
            out.push(manifest.find_project(&name).unwrap());
        }
    }
    Ok(out)
}

fn print_outcome(outcome: &BuildOutcome) {
    let name = &outcome.project_name;
    let header = if outcome.success {
        format!("== {name} ==").green().bold()
    } else {
        format!("== {name} ==").red().bold()
    };
    println!("{header}");
    if !outcome.tags.is_empty() {
        println!("  tags: {}", outcome.tags.join(", "));
    }
    for (label, run) in &outcome.runs {
        if !run.stdout.is_empty() || !run.stderr.is_empty() {
            println!("  [{label}]");
            let _ = std::io::stdout().write_all(&run.stdout);
            let _ = std::io::stderr().write_all(&run.stderr);
        }
        if !run.status.success() {
            eprintln!("  [{label}] exit: {}", run.status);
        }
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_template_replaces_placeholders() {
        assert_eq!(
            expand_template("{name}:{git_sha}", "api", "abc123"),
            "api:abc123"
        );
        assert_eq!(
            expand_template("reg.example.com/{name}:v-{git_sha}", "web", "deadbeef"),
            "reg.example.com/web:v-deadbeef"
        );
    }

    #[test]
    fn docker_build_argv_shape() {
        let argv = docker_build_argv("Dockerfile", &["api:abc".into(), "registry/api:abc".into()]);
        assert_eq!(argv[0], "build");
        assert_eq!(argv[1], "-f");
        assert_eq!(argv[2], "Dockerfile");
        assert!(argv.iter().any(|a| a == "api:abc"));
        assert!(argv.iter().any(|a| a == "registry/api:abc"));
        assert_eq!(argv.last().unwrap(), ".");
    }

    #[test]
    fn shell_quote_passes_safe_tokens_through() {
        assert_eq!(shell_quote("api:abc"), "api:abc");
        assert_eq!(
            shell_quote("registry.example.com/api:1.2.3"),
            "registry.example.com/api:1.2.3"
        );
        assert_eq!(shell_quote("Dockerfile"), "Dockerfile");
    }

    #[test]
    fn shell_quote_wraps_dangerous_tokens() {
        assert_eq!(shell_quote("foo; rm -rf /"), "'foo; rm -rf /'");
        assert_eq!(shell_quote("name with spaces"), "'name with spaces'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote(""), "''");
    }
}
