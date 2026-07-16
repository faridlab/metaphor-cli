//! `metaphor sync` — clone or update remote projects to their pinned ref.
//!
//! For each project that has a `remote` URL:
//!   1. If the local `path` doesn't exist → `git clone`.
//!   2. If it exists → `git fetch` + `git checkout`.
//!
//! After each project is synced the resolved commit hash is recorded in
//! `metaphor.lock` so the exact state is reproducible.

use anyhow::{bail, Context, Result};
use colored::*;
use metaphor_workspace::{LockFile, Manifest, Project};
use std::io::Write;
use std::path::Path;
use std::process::Command;

use crate::graph::Graph;

pub struct SyncOptions {
    /// Re-resolve refs even if the lock file already has an entry.
    pub update: bool,
    /// Only sync these projects (empty = all with remotes).
    pub projects: Vec<String>,
}

pub fn cmd_sync(manifest: &Manifest, workspace_root: &Path, opts: &SyncOptions) -> Result<()> {
    let mut lock = metaphor_workspace::load_lock(workspace_root)?;

    // Use topological order so dependencies are synced before dependents.
    let graph = Graph::from_manifest(manifest);
    let topo_order = graph.topo_sort()?;

    let targets: Vec<&Project> = topo_order
        .iter()
        .filter_map(|name| manifest.projects.iter().find(|p| p.name == *name))
        .filter(|p| p.remote.is_some())
        .filter(|p| {
            if opts.projects.is_empty() {
                true
            } else {
                opts.projects.iter().any(|n| n == &p.name)
            }
        })
        .collect();

    if targets.is_empty() {
        println!("No projects with a remote to sync.");
        return Ok(());
    }

    let mut synced = 0u32;
    let mut failed = 0u32;

    for project in &targets {
        let remote = project.remote.as_deref().unwrap();
        let target_dir = project.resolved_path(workspace_root);
        let git_ref = project.git_ref.as_deref();

        print!(
            "{} {} ...",
            "syncing".bright_blue().bold(),
            project.name.bold()
        );
        std::io::stdout().flush().ok();

        match sync_one(project, remote, &target_dir, git_ref, &lock, opts.update) {
            Ok(resolved) => {
                lock.upsert(&project.name, git_ref, &resolved);
                let short = &resolved[..resolved.len().min(12)];
                println!(
                    " {} ({})",
                    "ok".bright_green().bold(),
                    short.dimmed()
                );
                synced += 1;
            }
            Err(e) => {
                println!(" {}: {e:#}", "FAILED".bright_red().bold());
                failed += 1;
            }
        }
    }

    // Write lock file even if some failed — partial progress is better
    // than losing everything.
    let lock_path = metaphor_workspace::save_lock(&lock, workspace_root)?;

    println!();
    println!(
        "Synced {synced} project(s), {failed} failed. Lock written to {}",
        lock_path.display()
    );

    if failed > 0 {
        bail!("{failed} project(s) failed to sync");
    }
    Ok(())
}

/// Sync a single project. Returns the resolved full commit hash.
fn sync_one(
    project: &Project,
    remote: &str,
    target_dir: &Path,
    git_ref: Option<&str>,
    lock: &LockFile,
    update: bool,
) -> Result<String> {
    if !target_dir.exists() {
        // Fresh clone
        clone_project(remote, target_dir, git_ref)?;
    } else {
        // Already cloned — decide whether to update
        let ref_changed = lock
            .find_project(&project.name)
            .map(|l| l.git_ref.as_deref() != git_ref)
            .unwrap_or(true);
        let needs_update = update || ref_changed;

        if needs_update {
            fetch_and_checkout(target_dir, git_ref)?;
        }
    }

    resolve_head(target_dir)
}

/// Clone a remote repository into `target_dir`, optionally checking out a
/// specific ref. Used by both `metaphor sync` and `metaphor add --clone`.
pub(crate) fn clone_project(
    remote: &str,
    target_dir: &Path,
    git_ref: Option<&str>,
) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = target_dir.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent dir {}", parent.display()))?;
    }

    let output = Command::new("git")
        .args(["clone", remote])
        .arg(target_dir)
        .output()
        .with_context(|| format!("spawning git clone for {remote}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git clone failed for {remote}:\n{stderr}");
    }

    // Checkout the specific ref if provided
    if let Some(r) = git_ref {
        checkout(target_dir, r)?;
    }
    Ok(())
}

fn fetch_and_checkout(target_dir: &Path, git_ref: Option<&str>) -> Result<()> {
    // Fetch all refs (tags + branches)
    let output = Command::new("git")
        .args(["fetch", "--tags", "--prune"])
        .current_dir(target_dir)
        .output()
        .context("spawning git fetch")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git fetch failed in {}:\n{stderr}", target_dir.display());
    }

    if let Some(r) = git_ref {
        checkout(target_dir, r)?;
        // `git fetch` advanced the remote-tracking ref (`origin/<r>`) but left the local branch
        // where it was — so a bare `git checkout <r>` lands on the *stale* local branch, and the
        // resolved HEAD never moves to the commit we just fetched. That was the `sync --update`
        // bug: it fetched the new tip but re-recorded the old SHA. When `<r>` is a branch,
        // fast-forward the local branch to the fetched tip; when it is a tag or a raw SHA there is
        // no `origin/<r>`, so this is a no-op and the immutable pin stays put.
        fast_forward_to_remote(target_dir, r)?;
    } else {
        // No ref pinned — pull the current branch
        let output = Command::new("git")
            .args(["pull", "--ff-only"])
            .current_dir(target_dir)
            .output()
            .context("spawning git pull")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git pull failed in {}:\n{stderr}", target_dir.display());
        }
    }
    Ok(())
}

/// If `git_ref` names a remote branch, fast-forward the checked-out local branch to
/// `origin/<git_ref>`. For a tag or a raw commit SHA there is no `origin/<git_ref>`, so HEAD is
/// left untouched — those are immutable pins and must not move.
fn fast_forward_to_remote(target_dir: &Path, git_ref: &str) -> Result<()> {
    let remote_ref = format!("origin/{git_ref}");

    // Does `origin/<ref>` resolve? If not, `<ref>` is a tag or a raw SHA — nothing to advance.
    let exists = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", &remote_ref])
        .current_dir(target_dir)
        .output()
        .context("spawning git rev-parse for remote-branch check")?;
    if !exists.status.success() {
        return Ok(());
    }

    // Fast-forward only: advance a clean branch, but refuse (rather than clobber) a local branch
    // that has diverged from the remote, surfacing the conflict instead of silently eating it.
    let output = Command::new("git")
        .args(["merge", "--ff-only", &remote_ref])
        .current_dir(target_dir)
        .output()
        .context("spawning git merge --ff-only")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git merge --ff-only {remote_ref} failed in {} \
             (local branch has diverged from the remote?):\n{stderr}",
            target_dir.display()
        );
    }
    Ok(())
}

fn checkout(target_dir: &Path, git_ref: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["checkout", git_ref])
        .current_dir(target_dir)
        .output()
        .with_context(|| format!("checking out {git_ref}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git checkout {git_ref} failed in {}:\n{stderr}",
            target_dir.display()
        );
    }
    Ok(())
}

/// Resolve the current HEAD to a full commit hash.
pub(crate) fn resolve_head(target_dir: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(target_dir)
        .output()
        .context("spawning git rev-parse HEAD")?;
    if !output.status.success() {
        bail!(
            "git rev-parse HEAD failed in {}",
            target_dir.display()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
