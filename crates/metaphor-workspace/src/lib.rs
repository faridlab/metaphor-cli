//! Workspace manifest (`metaphor.yaml`) schema and I/O.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MANIFEST_FILE: &str = "metaphor.yaml";
pub const LOCK_FILE: &str = "metaphor.lock";
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

    #[error("duplicate project name '{0}'")]
    DuplicateProject(String),

    #[error("project '{project}' depends_on unknown project '{missing}'")]
    UnknownDependency { project: String, missing: String },

    #[error("project '{0}' lists itself in depends_on")]
    SelfDependency(String),
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
    /// Git ref to pin this project to: a tag (`v1.2.0`), branch (`main`),
    /// or commit hash. Only meaningful when `remote` is set. Defaults to
    /// the remote's HEAD when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
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

    /// Identify the project `cwd` sits inside. The winner is the project
    /// whose `resolved_path(workspace_root)` is the longest path-component-
    /// wise prefix of `cwd`. Returns `None` if no project matches.
    ///
    /// Uses `PathBuf::starts_with`, which compares whole components — so
    /// `/ws/api` is not a prefix of `/ws/api-v2`.
    pub fn current_project(&self, workspace_root: &Path, cwd: &Path) -> Option<&Project> {
        let mut best: Option<(usize, &Project)> = None;
        for p in &self.projects {
            let root = p.resolved_path(workspace_root);
            if cwd.starts_with(&root) {
                let depth = root.components().count();
                match best {
                    Some((d, _)) if d >= depth => {}
                    _ => best = Some((depth, p)),
                }
            }
        }
        best.map(|(_, p)| p)
    }

    /// Validate cross-project invariants: unique names, every `depends_on`
    /// entry resolves, no self-dependency.
    pub fn validate(&self) -> Result<(), WorkspaceError> {
        let mut seen = std::collections::HashSet::new();
        for p in &self.projects {
            if !seen.insert(p.name.as_str()) {
                return Err(WorkspaceError::DuplicateProject(p.name.clone()));
            }
        }
        for p in &self.projects {
            for dep in &p.depends_on {
                if dep == &p.name {
                    return Err(WorkspaceError::SelfDependency(p.name.clone()));
                }
                if !seen.contains(dep.as_str()) {
                    return Err(WorkspaceError::UnknownDependency {
                        project: p.name.clone(),
                        missing: dep.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

impl Project {
    /// Resolve this project's path against the workspace root. Absolute paths
    /// are returned as-is; relative paths are joined to `workspace_root`.
    /// `.` components are normalized away so the result is display-friendly
    /// (no `/./` segments).
    pub fn resolved_path(&self, workspace_root: &Path) -> PathBuf {
        let p = PathBuf::from(&self.path);
        let joined = if p.is_absolute() {
            p
        } else {
            workspace_root.join(p)
        };
        joined
            .components()
            .filter(|c| !matches!(c, std::path::Component::CurDir))
            .collect()
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
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let manifest: Manifest = serde_yaml::from_str(&raw).context("parsing metaphor.yaml")?;
    if manifest.version != CURRENT_VERSION {
        return Err(WorkspaceError::UnsupportedVersion {
            found: manifest.version,
            expected: CURRENT_VERSION,
        }
        .into());
    }
    manifest.validate()?;
    Ok(manifest)
}

/// Walk up from `start` looking for `metaphor.yaml`. Returns the manifest
/// and the directory it was found in. Validation happens transitively via
/// [`load`] — if you refactor this, preserve that invariant.
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

// ── Lock file ───────────────────────────────────────────────────────────

/// `metaphor.lock` records the exact commit hash each remote project was
/// synced to, making builds reproducible across machines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockFile {
    pub version: u32,
    #[serde(default)]
    pub projects: Vec<LockedProject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedProject {
    pub name: String,
    /// The `ref` value from `metaphor.yaml` at sync time (tag, branch, or
    /// commit). `None` means HEAD was used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
    /// The full commit hash that `ref` resolved to.
    pub resolved: String,
}

impl LockFile {
    pub fn empty() -> Self {
        Self {
            version: CURRENT_VERSION,
            projects: Vec::new(),
        }
    }

    pub fn find_project(&self, name: &str) -> Option<&LockedProject> {
        self.projects.iter().find(|p| p.name == name)
    }

    /// Insert or update the entry for `name`.
    pub fn upsert(&mut self, name: &str, git_ref: Option<&str>, resolved: &str) {
        if let Some(existing) = self.projects.iter_mut().find(|p| p.name == name) {
            existing.git_ref = git_ref.map(str::to_string);
            existing.resolved = resolved.to_string();
        } else {
            self.projects.push(LockedProject {
                name: name.to_string(),
                git_ref: git_ref.map(str::to_string),
                resolved: resolved.to_string(),
            });
        }
    }
}

/// Load `metaphor.lock` from `dir`. Returns an empty lock if the file
/// doesn't exist yet (first sync).
pub fn load_lock(dir: &Path) -> Result<LockFile> {
    let path = dir.join(LOCK_FILE);
    if !path.exists() {
        return Ok(LockFile::empty());
    }
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let lock: LockFile = serde_yaml::from_str(&raw).context("parsing metaphor.lock")?;
    Ok(lock)
}

/// Write `metaphor.lock` to `dir`.
pub fn save_lock(lock: &LockFile, dir: &Path) -> Result<PathBuf> {
    let path = dir.join(LOCK_FILE);
    let yaml = serde_yaml::to_string(lock).context("serializing lock file")?;
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

    fn project(name: &str, deps: &[&str]) -> Project {
        Project {
            name: name.to_string(),
            project_type: ProjectType::Module,
            path: format!("./{name}"),
            remote: None,
            git_ref: None,
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn validate_accepts_valid_graph() {
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![project("a", &[]), project("b", &["a"])],
        };
        m.validate().unwrap();
    }

    #[test]
    fn validate_rejects_unknown_dependency() {
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![project("a", &["ghost"])],
        };
        let err = m.validate().unwrap_err();
        assert!(matches!(err, WorkspaceError::UnknownDependency { .. }));
    }

    #[test]
    fn validate_rejects_self_dependency() {
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![project("a", &["a"])],
        };
        let err = m.validate().unwrap_err();
        assert!(matches!(err, WorkspaceError::SelfDependency(_)));
    }

    #[test]
    fn validate_rejects_duplicate_names() {
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![project("a", &[]), project("a", &[])],
        };
        let err = m.validate().unwrap_err();
        assert!(matches!(err, WorkspaceError::DuplicateProject(_)));
    }

    #[test]
    fn current_project_matches_exact_path() {
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![project("api", &[])],
        };
        let root = Path::new("/ws");
        let api = Path::new("/ws/api");
        assert_eq!(m.current_project(root, api).unwrap().name, "api");
    }

    #[test]
    fn current_project_matches_nested_cwd() {
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![project("api", &[])],
        };
        let root = Path::new("/ws");
        let deep = Path::new("/ws/api/src/handlers");
        assert_eq!(m.current_project(root, deep).unwrap().name, "api");
    }

    #[test]
    fn current_project_prefers_longest_prefix() {
        // When `inner/` lives under `outer/`, cwd inside inner matches inner.
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![
                Project {
                    name: "outer".into(),
                    project_type: ProjectType::Module,
                    path: "./apps".into(),
                    remote: None,
                    git_ref: None,
                    depends_on: vec![],
                },
                Project {
                    name: "inner".into(),
                    project_type: ProjectType::Module,
                    path: "./apps/billing".into(),
                    remote: None,
                    git_ref: None,
                    depends_on: vec![],
                },
            ],
        };
        let root = Path::new("/ws");
        let cwd = Path::new("/ws/apps/billing/src");
        assert_eq!(m.current_project(root, cwd).unwrap().name, "inner");
    }

    #[test]
    fn current_project_none_outside_workspace() {
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![project("api", &[])],
        };
        let root = Path::new("/ws");
        assert!(m.current_project(root, Path::new("/elsewhere")).is_none());
        // Component-aware: `/ws/api` is NOT a prefix of `/ws/api-v2`.
        assert!(m
            .current_project(root, Path::new("/ws/api-v2/src"))
            .is_none());
    }

    #[test]
    fn load_rejects_unknown_dependency() {
        let tmp = tempdir();
        let yaml = "version: 1\nprojects:\n  - name: a\n    type: module\n    path: ./a\n    depends_on: [ghost]\n";
        std::fs::write(tmp.join(MANIFEST_FILE), yaml).unwrap();
        let err = load(&tmp).unwrap_err();
        assert!(err.to_string().contains("unknown project 'ghost'"));
    }

    #[test]
    fn load_accepts_cycles_at_manifest_layer() {
        // Two-node cycles pass `validate` (both names exist, no self-ref).
        // Cycle detection is the graph layer's responsibility.
        let tmp = tempdir();
        let yaml = "version: 1\nprojects:\n  - name: a\n    type: module\n    path: ./a\n    depends_on: [b]\n  - name: b\n    type: module\n    path: ./b\n    depends_on: [a]\n";
        std::fs::write(tmp.join(MANIFEST_FILE), yaml).unwrap();
        let m = load(&tmp).unwrap();
        assert_eq!(m.projects.len(), 2);
    }

    #[test]
    fn lock_file_round_trip() {
        let tmp = tempdir();
        let mut lock = LockFile::empty();
        lock.upsert("sapiens", Some("v1.0.0"), "abc123def456");
        lock.upsert("bucket", None, "deadbeef0000");
        save_lock(&lock, &tmp).unwrap();

        let loaded = load_lock(&tmp).unwrap();
        assert_eq!(loaded.projects.len(), 2);

        let s = loaded.find_project("sapiens").unwrap();
        assert_eq!(s.git_ref.as_deref(), Some("v1.0.0"));
        assert_eq!(s.resolved, "abc123def456");

        let b = loaded.find_project("bucket").unwrap();
        assert!(b.git_ref.is_none());
        assert_eq!(b.resolved, "deadbeef0000");
    }

    #[test]
    fn lock_file_upsert_updates_existing() {
        let mut lock = LockFile::empty();
        lock.upsert("sapiens", Some("v1.0.0"), "aaa");
        lock.upsert("sapiens", Some("v2.0.0"), "bbb");
        assert_eq!(lock.projects.len(), 1);
        assert_eq!(lock.projects[0].resolved, "bbb");
        assert_eq!(lock.projects[0].git_ref.as_deref(), Some("v2.0.0"));
    }

    #[test]
    fn load_lock_missing_file_returns_empty() {
        let tmp = tempdir();
        let lock = load_lock(&tmp).unwrap();
        assert!(lock.projects.is_empty());
    }

    #[test]
    fn manifest_with_ref_round_trips() {
        let tmp = tempdir();
        let yaml = "version: 1\nprojects:\n  - name: sapiens\n    type: module\n    path: ./sapiens\n    remote: https://github.com/faridlab/backbone-sapiens\n    ref: v1.0.0\n";
        std::fs::write(tmp.join(MANIFEST_FILE), yaml).unwrap();
        let m = load(&tmp).unwrap();
        assert_eq!(m.projects[0].git_ref.as_deref(), Some("v1.0.0"));

        // Save and reload
        save(&m, &tmp).unwrap();
        let m2 = load(&tmp).unwrap();
        assert_eq!(m2.projects[0].git_ref.as_deref(), Some("v1.0.0"));
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
