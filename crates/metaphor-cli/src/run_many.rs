//! Fan-out dispatch: run a plugin command across many projects.
//!
//! Invoked when any of `--all`, `--projects`, or `--affected` is passed on a
//! passthrough command. Without any of those flags, the caller uses the
//! existing single-shot `plugin_env::passthrough*` helpers instead.

use crate::affected;
use crate::cache;
use crate::graph::Graph;
use crate::plugin_env::{self, failed_status, success_status};
use anyhow::{bail, Result};
use clap::Args;
use colored::*;
use metaphor_workspace::{Manifest, Project};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

/// Flags shared by every passthrough command. Flattened into each command
/// variant with `#[command(flatten)]`.
#[derive(Args, Debug, Clone)]
pub struct RunFlags {
    /// Run across every registered project
    #[arg(long, conflicts_with_all = ["projects", "affected"])]
    pub all: bool,

    /// Run across only these projects (comma-separated)
    #[arg(long, value_delimiter = ',', conflicts_with = "affected")]
    pub projects: Vec<String>,

    /// Run only on projects affected by git changes (and their dependents)
    #[arg(long)]
    pub affected: bool,

    /// Base git ref for --affected (default: main)
    #[arg(long, default_value = "main")]
    pub base: String,

    /// Head git ref for --affected (default: HEAD)
    #[arg(long, default_value = "HEAD")]
    pub head: String,

    /// Max concurrent project invocations (default: 1, sequential)
    #[arg(long, default_value_t = 1)]
    pub parallel: usize,

    /// Continue running remaining projects on failure; exit non-zero if any failed
    #[arg(long)]
    pub continue_on_error: bool,

    /// Bypass the task result cache (neither read nor write)
    #[arg(long)]
    pub no_cache: bool,
}

impl RunFlags {
    pub fn is_multi(&self) -> bool {
        self.all || !self.projects.is_empty() || self.affected
    }
}

/// Compute the ordered list of projects to dispatch against.
/// Order is topological (dependencies before dependents).
pub fn select_projects<'a>(
    manifest: &'a Manifest,
    workspace_root: &Path,
    flags: &RunFlags,
) -> Result<Vec<&'a Project>> {
    let graph = Graph::from_manifest(manifest);
    let order = graph.topo_sort()?;

    let filter: BTreeSet<String> = if flags.all {
        manifest.projects.iter().map(|p| p.name.clone()).collect()
    } else if !flags.projects.is_empty() {
        let wanted: BTreeSet<String> = flags.projects.iter().cloned().collect();
        // Validate every requested name exists.
        for name in &wanted {
            manifest.find_project(name)?;
        }
        wanted
    } else if flags.affected {
        affected::affected_projects(manifest, &graph, workspace_root, &flags.base, &flags.head)?
    } else {
        bail!("select_projects called without a multi-project flag");
    };

    let mut selected = Vec::new();
    for name in order {
        if filter.contains(&name) {
            // Safe: `name` came from the manifest.
            selected.push(manifest.find_project(&name).unwrap());
        }
    }
    Ok(selected)
}

/// Build the full argv for a plugin invocation: optional subcommand prefix
/// followed by the user's trailing args.
fn build_argv(subcommand: Option<&str>, extra_args: &[String]) -> Vec<String> {
    let mut argv = Vec::with_capacity(extra_args.len() + 1);
    if let Some(sub) = subcommand {
        argv.push(sub.to_string());
    }
    argv.extend(extra_args.iter().cloned());
    argv
}

/// Dispatch a plugin command across `projects`. Handles sequential and
/// parallel paths, per-project output buffering, `--continue-on-error`, and
/// the task-result cache.
pub fn dispatch(
    binary: &str,
    subcommand: Option<&str>,
    extra_args: &[String],
    projects: &[&Project],
    workspace_root: &Path,
    flags: &RunFlags,
) -> Result<()> {
    if projects.is_empty() {
        println!("No projects matched.");
        return Ok(());
    }

    let argv = build_argv(subcommand, extra_args);
    let parallel = flags.parallel.max(1);
    let mut failures: Vec<String> = Vec::new();

    // Cache bookkeeping. `--no-cache` disables entirely. Otherwise, a failure
    // to open the cache (unwritable `.metaphor/cache/`, etc.) is logged to
    // stderr and we proceed without caching — rather than silently making
    // every invocation look uncacheable.
    let ctx = if flags.no_cache {
        None
    } else {
        match CacheCtx::open(binary, workspace_root) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("{}: cache disabled: {}", "warning".yellow().bold(), e);
                None
            }
        }
    };

    if parallel == 1 {
        for p in projects {
            let outcome = run_one(binary, &argv, p, workspace_root, ctx.as_ref());
            print_outcome(&p.name, &outcome);
            if !outcome.run.status.success() {
                failures.push(p.name.clone());
                if !flags.continue_on_error {
                    break;
                }
            }
        }
    } else {
        // Bounded worker pool via std::thread::scope. Each worker pulls an
        // index from a shared counter and writes its result slot.
        let workers = parallel.min(projects.len());
        let next = Mutex::new(0usize);
        let results: Vec<Mutex<Option<RunOutcome>>> =
            (0..projects.len()).map(|_| Mutex::new(None)).collect();
        let ctx_ref = ctx.as_ref();

        std::thread::scope(|s| {
            for _ in 0..workers {
                s.spawn(|| loop {
                    let idx = {
                        let mut n = next.lock().unwrap();
                        let i = *n;
                        *n += 1;
                        i
                    };
                    if idx >= projects.len() {
                        break;
                    }
                    let p = projects[idx];
                    let outcome = run_one(binary, &argv, p, workspace_root, ctx_ref);
                    *results[idx].lock().unwrap() = Some(outcome);
                });
            }
        });

        for slot in &results {
            if let Some(outcome) = slot.lock().unwrap().take() {
                let name = outcome.project_name.clone();
                print_outcome(&name, &outcome);
                if !outcome.run.status.success() {
                    failures.push(name);
                }
            }
        }
    }

    if !failures.is_empty() {
        bail!(
            "failed in {} project(s): {}",
            failures.len(),
            failures.join(", ")
        );
    }
    Ok(())
}

struct RunOutcome {
    project_name: String,
    run: plugin_env::CapturedRun,
    cache_hit: bool,
}

struct CacheCtx {
    cache: cache::Cache,
    binary_path: std::path::PathBuf,
    plugin_version: String,
}

impl CacheCtx {
    fn open(binary: &str, workspace_root: &Path) -> Result<Self> {
        let binary_path = plugin_env::plugin_binary(binary)?;
        let plugin_version = std::process::Command::new(&binary_path)
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "unknown".to_string());
        let cache = cache::Cache::open(workspace_root)?;
        Ok(Self {
            cache,
            binary_path,
            plugin_version,
        })
    }

    fn key_for(&self, argv: &[String], project: &Project, cwd: &Path) -> Option<cache::CacheKey> {
        let tree = cache::hash_project_tree(cwd).ok()?;
        let inputs = cache::KeyInputs {
            plugin_binary: &self.binary_path,
            plugin_version: &self.plugin_version,
            argv,
            project_name: &project.name,
            project_tree_hash: tree,
        };
        Some(inputs.compute_key())
    }
}

fn run_one(
    binary: &str,
    argv: &[String],
    project: &Project,
    workspace_root: &Path,
    ctx: Option<&CacheCtx>,
) -> RunOutcome {
    let cwd = project.resolved_path(workspace_root);

    // Compute the cache key once so a miss doesn't re-walk the project tree
    // on write. `None` means the key couldn't be computed (project dir gone,
    // tree hash failed) — we still run the plugin but skip cache writes.
    let key = ctx.and_then(|c| c.key_for(argv, project, &cwd));

    // Cache lookup.
    if let (Some(ctx), Some(key)) = (ctx, key) {
        if let Some(entry) = ctx.cache.get(key) {
            return RunOutcome {
                project_name: project.name.clone(),
                run: entry_to_captured(entry),
                cache_hit: true,
            };
        }
    }

    // Cache miss (or disabled) — actually run the plugin.
    let run = match plugin_env::run_captured(binary, argv, &cwd) {
        Ok(r) => r,
        Err(e) => plugin_env::CapturedRun {
            status: failed_status(),
            stdout: Vec::new(),
            stderr: format!("spawn error: {e}\n").into_bytes(),
        },
    };

    // Only cache successful runs — don't memoize flaky failures.
    if let (Some(ctx), Some(key)) = (ctx, key) {
        if run.status.success() {
            let entry = captured_to_entry(&run);
            let _ = ctx.cache.put(key, &entry); // best effort
        }
    }

    RunOutcome {
        project_name: project.name.clone(),
        run,
        cache_hit: false,
    }
}

fn entry_to_captured(entry: cache::CacheEntry) -> plugin_env::CapturedRun {
    let status = if entry.exit_code == 0 {
        success_status()
    } else {
        failed_status()
    };
    plugin_env::CapturedRun {
        status,
        stdout: entry.stdout,
        stderr: entry.stderr,
    }
}

fn captured_to_entry(run: &plugin_env::CapturedRun) -> cache::CacheEntry {
    cache::CacheEntry {
        exit_code: run.status.code().unwrap_or(0),
        stdout: run.stdout.clone(),
        stderr: run.stderr.clone(),
    }
}

fn print_outcome(name: &str, outcome: &RunOutcome) {
    let suffix = if outcome.cache_hit { " (cached)" } else { "" };
    let header = format!("== {name} =={suffix}");
    if outcome.run.status.success() {
        println!("{}", header.green().bold());
    } else {
        println!("{}", header.red().bold());
    }
    let _ = std::io::stdout().write_all(&outcome.run.stdout);
    let _ = std::io::stderr().write_all(&outcome.run.stderr);
    if !outcome.run.status.success() {
        eprintln!("  (exit status: {})", outcome.run.status);
    }
    println!();
}

// ExitStatus helpers for cache replay / pre-spawn errors live in
// `plugin_env` (shared across run_many and cmd_build).
