//! `metaphor deploy` — delegate to the workspace's infra project.
//!
//! Finds the `infra` project in `metaphor.yaml`, then runs the first of:
//!   1. `./deploy.sh` (if present and executable)
//!   2. `make deploy` (if a `Makefile` is present)
//!
//! Any trailing `-- <args>` are forwarded to the chosen command. Metaphor
//! doesn't know what "deploy" means — that's the infra repo's concern.
//! This is the thinnest-possible glue.

use anyhow::{bail, Context, Result};
use metaphor_workspace::{Manifest, Project, ProjectType};
use std::path::Path;
use std::process::Command;

pub struct DeployOptions<'a> {
    /// Select a specific infra project by name. Required when more than one
    /// project has `type: infra`.
    pub infra: Option<&'a str>,
    /// Extra args forwarded to the chosen deploy command.
    pub args: &'a [String],
}

pub fn cmd_deploy(
    manifest: &Manifest,
    workspace_root: &Path,
    opts: &DeployOptions<'_>,
) -> Result<()> {
    let infra = find_infra_project(manifest, opts.infra)?;
    let dir = infra.resolved_path(workspace_root);
    if !dir.is_dir() {
        bail!(
            "infra project '{}' not found on disk at {}",
            infra.name,
            dir.display()
        );
    }

    let script = dir.join("deploy.sh");
    let makefile = dir.join("Makefile");

    let (label, status) = if is_executable(&script) {
        let mut cmd = Command::new(&script);
        cmd.current_dir(&dir).args(opts.args);
        ("./deploy.sh", cmd.status())
    } else if makefile.exists() {
        let mut cmd = Command::new("make");
        cmd.current_dir(&dir).arg("deploy").args(opts.args);
        ("make deploy", cmd.status())
    } else {
        bail!(
            "infra project '{}' has no deploy.sh or Makefile; add one and try again",
            infra.name
        );
    };

    let status = status.with_context(|| format!("spawning {label}"))?;
    if !status.success() {
        bail!("{label} exited with status: {status}");
    }
    Ok(())
}

fn find_infra_project<'a>(manifest: &'a Manifest, name: Option<&str>) -> Result<&'a Project> {
    let infras: Vec<&Project> = manifest
        .projects
        .iter()
        .filter(|p| p.project_type == ProjectType::Infra)
        .collect();

    if let Some(name) = name {
        let p = manifest.find_project(name)?;
        if p.project_type != ProjectType::Infra {
            bail!(
                "project '{}' is type {:?}, not 'infra'",
                p.name,
                p.project_type
            );
        }
        return Ok(p);
    }

    match infras.len() {
        0 => bail!("no project with type: infra in this workspace"),
        1 => Ok(infras[0]),
        n => bail!(
            "{n} infra projects registered ({}); disambiguate with --infra=<name>",
            infras
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(p) {
        Ok(md) => md.is_file() && md.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    std::fs::metadata(p).map(|md| md.is_file()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use metaphor_workspace::CURRENT_VERSION;

    fn proj(name: &str, t: ProjectType) -> Project {
        Project {
            name: name.into(),
            project_type: t,
            path: format!("./{name}"),
            remote: None,
            depends_on: vec![],
        }
    }

    #[test]
    fn picks_the_sole_infra_project() {
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![
                proj("api", ProjectType::BackendService),
                proj("infra", ProjectType::Infra),
            ],
        };
        let p = find_infra_project(&m, None).unwrap();
        assert_eq!(p.name, "infra");
    }

    #[test]
    fn errors_when_none() {
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![proj("api", ProjectType::BackendService)],
        };
        let e = find_infra_project(&m, None).unwrap_err();
        assert!(e.to_string().contains("no project with type: infra"));
    }

    #[test]
    fn requires_disambiguation_when_multiple() {
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![
                proj("infra-staging", ProjectType::Infra),
                proj("infra-prod", ProjectType::Infra),
            ],
        };
        let e = find_infra_project(&m, None).unwrap_err();
        assert!(e.to_string().contains("--infra="));
        let p = find_infra_project(&m, Some("infra-prod")).unwrap();
        assert_eq!(p.name, "infra-prod");
    }

    #[test]
    fn rejects_non_infra_name() {
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![
                proj("api", ProjectType::BackendService),
                proj("infra", ProjectType::Infra),
            ],
        };
        let e = find_infra_project(&m, Some("api")).unwrap_err();
        assert!(e.to_string().contains("not 'infra'"));
    }
}
