//! `metaphor info` — show workspace root + the project cwd is inside.
//!
//! Ergonomic helper so a user `cd`-ing into a project dir can confirm it's
//! registered without running `metaphor list` and eyeballing paths.

use anyhow::Result;
use metaphor_workspace::Manifest;
use std::path::Path;

pub fn cmd_info(manifest: &Manifest, workspace_root: &Path, cwd: &Path, json: bool) -> Result<()> {
    let current = manifest.current_project(workspace_root, cwd);
    // Projects that list the current one in their `depends_on` — useful for
    // understanding "who will break if I change this".
    let depended_by: Vec<&str> = match current {
        Some(p) => manifest
            .projects
            .iter()
            .filter(|other| other.depends_on.iter().any(|d| d == &p.name))
            .map(|p| p.name.as_str())
            .collect(),
        None => Vec::new(),
    };

    if json {
        let payload = crate::json_envelope(serde_json::json!({
            "workspace_root": workspace_root.display().to_string(),
            "current_project": current.map(|p| serde_json::json!({
                "name": p.name,
                "type": format!("{:?}", p.project_type),
                "path": p.path,
                "resolved_path": p.resolved_path(workspace_root).display().to_string(),
                "depends_on": p.depends_on,
                "depended_by": depended_by,
            })),
            "projects_registered": manifest.projects.len(),
        }));
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    println!("workspace: {}", workspace_root.display());
    match current {
        Some(p) => {
            println!("current project: {} ({:?})", p.name, p.project_type);
            println!("  path: {}", p.path);
            println!("  resolved: {}", p.resolved_path(workspace_root).display());
            if p.depends_on.is_empty() {
                println!("  depends_on: (none)");
            } else {
                println!("  depends_on: {}", p.depends_on.join(", "));
            }
            if depended_by.is_empty() {
                println!("  depended-by: (none)");
            } else {
                println!("  depended-by: {}", depended_by.join(", "));
            }
        }
        None => {
            println!("current project: (not inside any registered project)");
        }
    }
    println!("projects: {} registered", manifest.projects.len());
    Ok(())
}
