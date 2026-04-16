//! Compute the affected project set for `--affected`.
//!
//! Strategy:
//! 1. Run `git diff --name-only <base>...<head>` to list changed files.
//! 2. Map each changed file to a project by longest matching `path` prefix.
//! 3. Close the resulting set under reverse-dependency edges (so touching
//!    a shared module also picks up everything that depends on it).

use crate::graph::Graph;
use anyhow::{bail, Context, Result};
use metaphor_workspace::Manifest;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn affected_projects(
    manifest: &Manifest,
    graph: &Graph,
    workspace_root: &Path,
    base: &str,
    head: &str,
) -> Result<BTreeSet<String>> {
    let changed = git_changed_files(workspace_root, base, head)?;
    let directly_affected = map_files_to_projects(manifest, workspace_root, &changed);

    let mut closure = BTreeSet::new();
    for name in &directly_affected {
        let rev = graph.reverse_deps(name)?;
        closure.extend(rev);
    }
    Ok(closure)
}

fn git_changed_files(workspace_root: &Path, base: &str, head: &str) -> Result<Vec<PathBuf>> {
    // Two-dot (..) range: commits reachable from `head` but not `base`.
    // Matches Nx's `nx affected` convention. Use `git diff a..b` (not `a...b`)
    // so that base moving doesn't change the affected set.
    let range = format!("{base}..{head}");
    let output = Command::new("git")
        .args(["diff", "--name-only", &range])
        .current_dir(workspace_root)
        .output()
        .context("failed to invoke `git diff` — is git installed and is this a git workspace?")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "`git diff {range}` failed (exit {}): {}",
            output.status,
            stderr.trim()
        );
    }
    // Note: only tracked files show up here. A newly-created, never-staged
    // file won't mark its project as affected — same semantics as
    // `nx affected`.
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| workspace_root.join(l))
        .collect())
}

/// For each changed file, find the project whose `resolved_path` is the
/// longest prefix of the file. Files outside any project are ignored.
///
/// Prefix matching is component-aware because `PathBuf::starts_with` compares
/// whole path components — `/ws/api` is *not* a prefix of `/ws/api-v2`.
fn map_files_to_projects(
    manifest: &Manifest,
    workspace_root: &Path,
    changed: &[PathBuf],
) -> BTreeSet<String> {
    let mut affected = BTreeSet::new();
    for file in changed {
        let mut best: Option<(usize, &str)> = None;
        for p in &manifest.projects {
            let project_root = p.resolved_path(workspace_root);
            if file.starts_with(&project_root) {
                let depth = project_root.components().count();
                match best {
                    Some((d, _)) if d >= depth => {}
                    _ => best = Some((depth, p.name.as_str())),
                }
            }
        }
        if let Some((_, name)) = best {
            affected.insert(name.to_string());
        }
    }
    affected
}

#[cfg(test)]
mod tests {
    use super::*;
    use metaphor_workspace::{Project, ProjectType, CURRENT_VERSION};

    fn proj(name: &str, path: &str, deps: &[&str]) -> Project {
        Project {
            name: name.to_string(),
            project_type: ProjectType::Module,
            path: path.to_string(),
            remote: None,
            git_ref: None,
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn longest_prefix_wins() {
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![
                proj("outer", "./apps", &[]),
                proj("inner", "./apps/billing", &[]),
            ],
        };
        let root = Path::new("/ws");
        let changed = vec![PathBuf::from("/ws/apps/billing/src/lib.rs")];
        let got = map_files_to_projects(&m, root, &changed);
        assert_eq!(got, BTreeSet::from(["inner".to_string()]));
    }

    #[test]
    fn files_outside_projects_are_ignored() {
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![proj("api", "./api", &[])],
        };
        let root = Path::new("/ws");
        let changed = vec![PathBuf::from("/ws/README.md")];
        assert!(map_files_to_projects(&m, root, &changed).is_empty());
    }

    #[test]
    fn similar_names_do_not_collide() {
        // `./api` must not match files under `./api-v2` — path-component-aware.
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![proj("api", "./api", &[]), proj("api-v2", "./api-v2", &[])],
        };
        let root = Path::new("/ws");
        let changed = vec![PathBuf::from("/ws/api-v2/src/main.rs")];
        let got = map_files_to_projects(&m, root, &changed);
        assert_eq!(got, BTreeSet::from(["api-v2".to_string()]));
    }
}
