//! Workspace manifest (`metaphor.yaml`) schema and I/O.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MANIFEST_FILE: &str = "metaphor.yaml";
pub const CURRENT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("metaphor.yaml already exists at {0}")]
    AlreadyInitialized(PathBuf),

    #[error("metaphor.yaml not found in {0} or any parent directory")]
    NotFound(PathBuf),

    #[error("unsupported metaphor.yaml version: {found} (expected {expected})")]
    UnsupportedVersion { found: u32, expected: u32 },

    #[error("project '{0}' not found in workspace")]
    ProjectNotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    #[serde(default)]
    pub projects: Vec<Project>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    #[serde(rename = "type")]
    pub project_type: ProjectType,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectType {
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

impl Manifest {
    pub fn empty() -> Self {
        Self {
            version: CURRENT_VERSION,
            projects: Vec::new(),
        }
    }

    pub fn find_project(&self, name: &str) -> Result<&Project, WorkspaceError> {
        self.projects
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| WorkspaceError::ProjectNotFound(name.to_string()))
    }
}

impl Project {
    /// Resolve this project's path against the workspace root. Absolute paths
    /// are returned as-is; relative paths are joined to `workspace_root`.
    pub fn resolved_path(&self, workspace_root: &Path) -> PathBuf {
        let p = PathBuf::from(&self.path);
        if p.is_absolute() {
            p
        } else {
            workspace_root.join(p)
        }
    }
}

impl ProjectType {
    pub fn to_plugin_api(self) -> metaphor_plugin_api::ProjectType {
        use metaphor_plugin_api::ProjectType as P;
        match self {
            ProjectType::BackendService => P::BackendService,
            ProjectType::Webservice => P::Webservice,
            ProjectType::Webapp => P::Webapp,
            ProjectType::Mobileapp => P::Mobileapp,
            ProjectType::Desktopapp => P::Desktopapp,
            ProjectType::Module => P::Module,
            ProjectType::Crate => P::Crate,
            ProjectType::CliTool => P::CliTool,
            ProjectType::Infra => P::Infra,
            ProjectType::DocsSite => P::DocsSite,
        }
    }
}

/// Initialize a new workspace by writing an empty `metaphor.yaml` into `dir`.
pub fn init(dir: &Path) -> Result<PathBuf> {
    let path = dir.join(MANIFEST_FILE);
    if path.exists() {
        return Err(WorkspaceError::AlreadyInitialized(path).into());
    }
    let manifest = Manifest::empty();
    let yaml = serde_yaml::to_string(&manifest).context("serializing manifest")?;
    std::fs::write(&path, yaml).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// Load a workspace manifest from `dir/metaphor.yaml`.
pub fn load(dir: &Path) -> Result<Manifest> {
    let path = dir.join(MANIFEST_FILE);
    if !path.exists() {
        return Err(WorkspaceError::NotFound(dir.to_path_buf()).into());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    let manifest: Manifest = serde_yaml::from_str(&raw).context("parsing metaphor.yaml")?;
    if manifest.version != CURRENT_VERSION {
        return Err(WorkspaceError::UnsupportedVersion {
            found: manifest.version,
            expected: CURRENT_VERSION,
        }
        .into());
    }
    Ok(manifest)
}

/// Walk up from `start` looking for `metaphor.yaml`. Returns the manifest
/// and the directory it was found in.
pub fn find_and_load(start: &Path) -> Result<(Manifest, PathBuf)> {
    let mut dir = start.to_path_buf();
    loop {
        let candidate = dir.join(MANIFEST_FILE);
        if candidate.exists() {
            let manifest = load(&dir)?;
            return Ok((manifest, dir));
        }
        if !dir.pop() {
            break;
        }
    }
    Err(WorkspaceError::NotFound(start.to_path_buf()).into())
}

/// Save a manifest back to `dir/metaphor.yaml`.
pub fn save(manifest: &Manifest, dir: &Path) -> Result<PathBuf> {
    let path = dir.join(MANIFEST_FILE);
    let yaml = serde_yaml::to_string(manifest).context("serializing manifest")?;
    std::fs::write(&path, yaml).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_writes_empty_manifest() {
        let tmp = tempdir();
        let path = init(&tmp).unwrap();
        assert!(path.exists());
        let manifest = load(&tmp).unwrap();
        assert_eq!(manifest.version, CURRENT_VERSION);
        assert!(manifest.projects.is_empty());
    }

    #[test]
    fn init_twice_errors() {
        let tmp = tempdir();
        init(&tmp).unwrap();
        let err = init(&tmp).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    fn tempdir() -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("metaphor-test-{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
