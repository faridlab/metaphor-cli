//! `metaphor pull` — clone or fast-forward every working clone of the workspace.
//!
//! A working clone is a project with a `remote` and `pin: false` in the manifest: an app, or a
//! module in a framework workspace, developed in place. `metaphor sync` serves the pinned
//! projects — it pins their clones to `metaphor.lock` and fills registry modules from their
//! released crate — so every pinned project is left to `sync` here.
//!
//! For each working clone:
//!   - its path is missing → `git clone` (on its `ref` when the manifest names one);
//!   - it has uncommitted changes to tracked files → left alone, reported;
//!   - its HEAD is detached, or its branch has no upstream → fetched only, reported;
//!   - otherwise → fetched and fast-forwarded to its upstream; a branch that has diverged
//!     is left alone and reported rather than merged.
//!
//! The lock is never written: a working clone follows its branch. `--dry-run` reports what
//! would happen from the local state alone, with nothing fetched or written.

use anyhow::{bail, Context, Result};
use colored::*;
use metaphor_workspace::{Manifest, Project};
use serde::Serialize;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

pub struct PullOptions {
    /// Only these projects (empty = every working clone).
    pub projects: Vec<String>,
    /// How many repositories to work on at once.
    pub parallel: usize,
    /// Report what would happen; fetch and write nothing.
    pub dry_run: bool,
    /// Print one JSON array instead of lines.
    pub json: bool,
}

/// What happened to one project.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Cloned,
    Updated,
    UpToDate,
    /// Uncommitted changes to tracked files: not touched.
    Dirty,
    /// The local branch and its upstream both have commits the other lacks: not merged.
    Diverged,
    /// Detached HEAD or no upstream: fetched, nothing moved.
    Fetched,
    /// `--dry-run`: the path is missing and would be cloned.
    WouldClone,
    /// `--dry-run`: the clone is clean and would be fetched and fast-forwarded.
    WouldUpdate,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub name: String,
    pub path: String,
    pub action: Action,
    pub detail: String,
}

impl Action {
    fn is_failure(&self) -> bool {
        matches!(self, Action::Failed)
    }
    fn needs_attention(&self) -> bool {
        matches!(self, Action::Dirty | Action::Diverged | Action::Fetched | Action::Failed)
    }
}

pub fn cmd_pull(manifest: &Manifest, workspace_root: &Path, opts: &PullOptions) -> Result<()> {
    // Projects `sync` fills from a released crate are not git clones; they stay with `sync`,
    // as does every pinned project.
    let crates = metaphor_workspace::registry_crates(manifest, workspace_root)?;
    let unknown: Vec<&String> = opts
        .projects
        .iter()
        .filter(|n| !manifest.projects.iter().any(|p| &p.name == *n))
        .collect();
    if !unknown.is_empty() {
        bail!(
            "not in this workspace's manifest: {}",
            unknown.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        );
    }
    let targets: Vec<&Project> = manifest
        .projects
        .iter()
        .filter(|p| p.remote.is_some() && !p.pin && !crates.contains_key(&p.name))
        .filter(|p| opts.projects.is_empty() || opts.projects.iter().any(|n| n == &p.name))
        .collect();

    if targets.is_empty() {
        if opts.json {
            println!("[]");
        } else {
            println!(
                "No working clones to pull: mark a project with a remote `pin: false` to have `metaphor pull` clone and fast-forward it."
            );
        }
        return Ok(());
    }

    let outcomes = run_all(&targets, workspace_root, opts);

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&outcomes)?);
    } else {
        for o in &outcomes {
            print_line(o);
        }
        print_summary(&outcomes, opts.dry_run);
    }

    let failed: Vec<&str> =
        outcomes.iter().filter(|o| o.action.is_failure()).map(|o| o.name.as_str()).collect();
    if !failed.is_empty() {
        bail!("failed in {} project(s): {}", failed.len(), failed.join(", "));
    }
    Ok(())
}

/// Work on every target, `opts.parallel` at a time, and return the outcomes in manifest order.
fn run_all(targets: &[&Project], workspace_root: &Path, opts: &PullOptions) -> Vec<Outcome> {
    let workers = opts.parallel.max(1).min(targets.len());
    let next = Mutex::new(0usize);
    let slots: Vec<Mutex<Option<Outcome>>> = (0..targets.len()).map(|_| Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let idx = {
                    let mut n = next.lock().unwrap();
                    let i = *n;
                    *n += 1;
                    i
                };
                if idx >= targets.len() {
                    break;
                }
                let outcome = pull_one(targets[idx], workspace_root, opts.dry_run);
                *slots[idx].lock().unwrap() = Some(outcome);
            });
        }
    });
    slots.into_iter().filter_map(|s| s.into_inner().unwrap()).collect()
}

fn pull_one(project: &Project, workspace_root: &Path, dry_run: bool) -> Outcome {
    let dir = project.resolved_path(workspace_root);
    let outcome = |action: Action, detail: String| Outcome {
        name: project.name.clone(),
        path: project.path.clone(),
        action,
        detail,
    };
    let remote = project.remote.as_deref().unwrap_or_default();
    let result = if !dir.exists() {
        if dry_run {
            Ok((Action::WouldClone, format!("from {remote}")))
        } else {
            clone(remote, &dir, project.git_ref.as_deref())
                .and_then(|_| head_summary(&dir))
                .map(|head| (Action::Cloned, head))
        }
    } else if !dir.join(".git").exists() {
        Err(anyhow::anyhow!("{} exists but is not a git clone", dir.display()))
    } else {
        update(&dir, dry_run)
    };
    match result {
        Ok((action, detail)) => outcome(action, detail),
        Err(e) => outcome(Action::Failed, format!("{e:#}")),
    }
}

fn clone(remote: &str, dir: &Path, git_ref: Option<&str>) -> Result<()> {
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut args = vec!["clone".to_string()];
    // A branch or tag clones straight onto it; a commit SHA is checked out after.
    let sha = git_ref.filter(|r| looks_like_sha(r));
    if let Some(r) = git_ref.filter(|r| !looks_like_sha(r)) {
        args.push("--branch".into());
        args.push(r.to_string());
    }
    args.push(remote.to_string());
    args.push(dir.display().to_string());
    git(None, &args.iter().map(|s| s.as_str()).collect::<Vec<_>>())?;
    if let Some(r) = sha {
        git(Some(dir), &["checkout", r])?;
    }
    Ok(())
}

fn update(dir: &Path, dry_run: bool) -> Result<(Action, String)> {
    let dirty = git(Some(dir), &["status", "--porcelain", "--untracked-files=no"])?;
    if !dirty.trim().is_empty() {
        let n = dirty.lines().count();
        return Ok((Action::Dirty, format!("{n} changed file(s); commit or stash, then pull again")));
    }
    let branch = git(Some(dir), &["symbolic-ref", "--quiet", "--short", "HEAD"]).ok();
    let upstream = branch.as_ref().and_then(|_| {
        git(Some(dir), &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]).ok()
    });
    if dry_run {
        return Ok(match (&branch, &upstream) {
            (Some(b), Some(u)) => (Action::WouldUpdate, format!("{} ← {}", b.trim(), u.trim())),
            (Some(b), None) => (Action::Fetched, format!("{}: no upstream; would fetch only", b.trim())),
            (None, _) => (Action::Fetched, "detached HEAD; would fetch only".into()),
        });
    }
    git(Some(dir), &["fetch", "--prune", "--tags"])?;
    let (Some(branch), Some(upstream)) = (branch, upstream) else {
        let head = head_summary(dir)?;
        return Ok((Action::Fetched, format!("detached HEAD or no upstream; fetched, left at {head}")));
    };
    let (branch, upstream) = (branch.trim().to_string(), upstream.trim().to_string());
    let counts = git(Some(dir), &["rev-list", "--left-right", "--count", &format!("HEAD...{upstream}")])?;
    let mut parts = counts.split_whitespace().map(|n| n.parse::<u64>().unwrap_or(0));
    let (ahead, behind) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    if behind == 0 {
        let note = if ahead > 0 { format!(", {ahead} local commit(s) not pushed") } else { String::new() };
        return Ok((Action::UpToDate, format!("{branch}{note}")));
    }
    if ahead > 0 {
        return Ok((
            Action::Diverged,
            format!("{branch}: {ahead} local and {behind} upstream commit(s); merge or rebase by hand"),
        ));
    }
    git(Some(dir), &["merge", "--ff-only", &upstream])?;
    Ok((Action::Updated, format!("{branch}: {behind} new commit(s)")))
}

fn head_summary(dir: &Path) -> Result<String> {
    let branch = git(Some(dir), &["symbolic-ref", "--quiet", "--short", "HEAD"]).ok();
    let sha = git(Some(dir), &["rev-parse", "--short=12", "HEAD"])?;
    Ok(match branch {
        Some(b) => format!("{} at {}", b.trim(), sha.trim()),
        None => sha.trim().to_string(),
    })
}

/// A full or abbreviated commit hash, which `git clone --branch` cannot take.
fn looks_like_sha(r: &str) -> bool {
    (7..=40).contains(&r.len()) && r.chars().all(|c| c.is_ascii_hexdigit())
}

fn git(dir: Option<&Path>, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    // Never stop to ask for credentials in a fan-out; a repo that needs them fails and says so.
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    let out = cmd.output().with_context(|| format!("spawning git {}", args.join(" ")))?;
    if !out.status.success() {
        bail!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn print_line(o: &Outcome) {
    let label = match o.action {
        Action::Cloned => "cloned".bright_green().bold(),
        Action::Updated => "updated".bright_green().bold(),
        Action::UpToDate => "up to date".dimmed(),
        Action::Dirty => "dirty".yellow().bold(),
        Action::Diverged => "diverged".yellow().bold(),
        Action::Fetched => "fetched".yellow(),
        Action::WouldClone => "would clone".bright_blue(),
        Action::WouldUpdate => "would update".bright_blue(),
        Action::Failed => "FAILED".bright_red().bold(),
    };
    let line = format!("{:<12} {:<34} {}", label, o.name.bold(), o.detail);
    if o.action.is_failure() {
        eprintln!("{line}");
    } else {
        println!("{line}");
    }
}

fn print_summary(outcomes: &[Outcome], dry_run: bool) {
    let count = |a: Action| outcomes.iter().filter(|o| o.action == a).count();
    println!();
    if dry_run {
        println!(
            "Dry run: {} would be cloned, {} would be updated; nothing was fetched or written.",
            count(Action::WouldClone),
            count(Action::WouldUpdate)
        );
    } else {
        println!(
            "{} cloned, {} updated, {} up to date.",
            count(Action::Cloned),
            count(Action::Updated),
            count(Action::UpToDate)
        );
    }
    let attention = outcomes.iter().filter(|o| o.action.needs_attention()).count();
    if attention > 0 {
        println!("{attention} need a look (dirty, diverged, detached or failed): listed above, left as they were.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) {
        let out = Command::new("git").args(args).current_dir(dir).output().unwrap();
        assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
    }

    /// A bare "remote" with one commit on main, and a working clone of it.
    fn fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let origin = tmp.path().join("origin.git");
        let seed = tmp.path().join("seed");
        std::fs::create_dir_all(&seed).unwrap();
        sh(tmp.path(), &["init", "--bare", "-b", "main", origin.to_str().unwrap()]);
        sh(&seed, &["init", "-b", "main"]);
        sh(&seed, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "--allow-empty", "-m", "one"]);
        sh(&seed, &["remote", "add", "origin", origin.to_str().unwrap()]);
        sh(&seed, &["push", "-u", "origin", "main"]);
        (tmp, origin, seed)
    }

    fn commit(dir: &Path, msg: &str) {
        sh(dir, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "--allow-empty", "-m", msg]);
    }

    #[test]
    fn a_missing_clone_is_cloned_and_a_clean_one_fast_forwards() {
        let (tmp, origin, seed) = fixture();
        let work = tmp.path().join("work");
        clone(origin.to_str().unwrap(), &work, None).unwrap();
        assert_eq!(update(&work, false).unwrap().0, Action::UpToDate);

        commit(&seed, "two");
        sh(&seed, &["push"]);
        let (action, detail) = update(&work, false).unwrap();
        assert_eq!(action, Action::Updated);
        assert!(detail.contains("1 new commit"), "{detail}");
    }

    #[test]
    fn a_dirty_or_diverged_clone_is_left_alone() {
        let (tmp, origin, seed) = fixture();
        let work = tmp.path().join("work");
        clone(origin.to_str().unwrap(), &work, None).unwrap();
        std::fs::write(work.join("f.txt"), "x").unwrap();
        sh(&work, &["add", "f.txt"]);
        assert_eq!(update(&work, false).unwrap().0, Action::Dirty);
        sh(&work, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-m", "local"]);

        commit(&seed, "upstream");
        sh(&seed, &["push"]);
        let before = git(Some(&work), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(update(&work, false).unwrap().0, Action::Diverged);
        assert_eq!(git(Some(&work), &["rev-parse", "HEAD"]).unwrap(), before, "nothing moved");
    }

    #[test]
    fn a_dry_run_fetches_nothing() {
        let (tmp, origin, seed) = fixture();
        let work = tmp.path().join("work");
        clone(origin.to_str().unwrap(), &work, None).unwrap();
        commit(&seed, "two");
        sh(&seed, &["push"]);
        assert_eq!(update(&work, true).unwrap().0, Action::WouldUpdate);
        let counts = git(Some(&work), &["rev-list", "--count", "origin/main"]).unwrap();
        assert_eq!(counts.trim(), "1", "the new upstream commit was not fetched");
    }

    #[test]
    fn a_commit_sha_is_not_a_branch_name() {
        assert!(looks_like_sha("41d4a744aa33"));
        assert!(!looks_like_sha("main"));
        assert!(!looks_like_sha("v0.6.1"));
    }
}
