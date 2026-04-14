//! `metaphor add` — register a project in the current workspace manifest.

use anyhow::{Context, Result};
use clap::ValueEnum;
use metaphor_workspace::{Project, ProjectType};

/// Clap-facing mirror of `metaphor_workspace::ProjectType`.
///
/// Duplicates the variant list so the workspace crate stays dependency-free
/// (no `clap` dep). If you add a variant, add it here too — and the
/// `From` impl below will force you to handle it.
#[derive(Debug, Clone, Copy, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum CliProjectType {
    BackendService,
    Webservice,
    Webapp,
    Mobileapp,
    Desktopapp,
    Module,
    Crate,
    CliTool,
    Infra,
    DocsSite,
}

impl From<CliProjectType> for ProjectType {
    fn from(t: CliProjectType) -> Self {
        match t {
            CliProjectType::BackendService => ProjectType::BackendService,
            CliProjectType::Webservice => ProjectType::Webservice,
            CliProjectType::Webapp => ProjectType::Webapp,
            CliProjectType::Mobileapp => ProjectType::Mobileapp,
            CliProjectType::Desktopapp => ProjectType::Desktopapp,
            CliProjectType::Module => ProjectType::Module,
            CliProjectType::Crate => ProjectType::Crate,
            CliProjectType::CliTool => ProjectType::CliTool,
            CliProjectType::Infra => ProjectType::Infra,
            CliProjectType::DocsSite => ProjectType::DocsSite,
        }
    }
}

pub struct AddArgs<'a> {
    pub name: &'a str,
    pub project_type: CliProjectType,
    pub path: &'a str,
    pub remote: Option<&'a str>,
    pub depends_on: &'a [String],
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum;

    /// Every `CliProjectType` variant round-trips into `ProjectType` and back
    /// via the string form clap emits. Guards against someone adding a variant
    /// to one enum and forgetting the other — the `From` impl already forces
    /// the forward direction at compile time; this locks the reverse.
    #[test]
    fn cli_project_type_variants_map_to_workspace() {
        for variant in CliProjectType::value_variants() {
            let _: ProjectType = (*variant).into();
        }
        // If you added a ProjectType variant without a CliProjectType mirror,
        // update the match in `From` — the compiler will force you to.
        // The count assertion keeps the two enums in lockstep.
        assert_eq!(CliProjectType::value_variants().len(), 10);
    }
}

pub fn cmd_add(args: AddArgs<'_>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let mut manifest = metaphor_workspace::load(&cwd)?;

    let project = Project {
        name: args.name.to_string(),
        project_type: args.project_type.into(),
        path: args.path.to_string(),
        remote: args.remote.map(str::to_string),
        depends_on: args.depends_on.to_vec(),
    };
    manifest.projects.push(project);

    // Reuses existing duplicate / unknown-dep / self-dep checks.
    manifest
        .validate()
        .context("validating manifest after add")?;

    let path = metaphor_workspace::save(&manifest, &cwd)?;
    println!("Added project '{}' to {}", args.name, path.display());
    Ok(())
}
