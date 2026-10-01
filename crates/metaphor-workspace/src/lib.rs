//! Workspace manifest (`metaphor.yaml`) schema and I/O.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const MANIFEST_FILE: &str = "metaphor.yaml";
pub const LOCK_FILE: &str = "metaphor.lock";
pub const CURRENT_VERSION: u32 = 1;
/// The newest manifest version this CLI reads. Version 2 adds `refs_from`; a manifest using it
/// must say so, because a CLI that predates it would ignore the key and sync every module that
/// has no `ref:` to its remote HEAD. Declaring 2 makes such a CLI refuse the manifest instead.
pub const MAX_MANIFEST_VERSION: u32 = 2;

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
    /// Where projects without a `ref:` take their revision from. Absent, such a project follows
    /// the remote's HEAD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refs_from: Option<RefsFrom>,
    #[serde(default)]
    pub projects: Vec<Project>,
}

/// Derive project refs from a Cargo.lock, so the version a consumer compiles is the one place a
/// module's version is written: a project whose crate the lock carries is synced to that version's
/// release tag, and its migrations and schema then match the compiled code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RefsFrom {
    /// The Cargo.lock to read, relative to the workspace root.
    pub cargo_lock: String,
    /// The tag a version is released under, with `{version}` replaced.
    #[serde(default = "default_release_tag")]
    pub tag: String,
}

fn default_release_tag() -> String {
    "v{version}".to_string()
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
    /// The package name this project publishes, when it differs from `name`. Used to find the
    /// project in the `refs_from` Cargo.lock; a multi-crate repository names any one of its crates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "crate")]
    pub crate_name: Option<String>,
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
            refs_from: None,
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
    if !(CURRENT_VERSION..=MAX_MANIFEST_VERSION).contains(&manifest.version) {
        return Err(WorkspaceError::UnsupportedVersion {
            found: manifest.version,
            expected: MAX_MANIFEST_VERSION,
        }
        .into());
    }
    if manifest.refs_from.is_some() && manifest.version < 2 {
        anyhow::bail!(
            "metaphor.yaml uses refs_from, which needs `version: 2`: a CLI that predates refs_from \
             would ignore it and sync every module without a ref: to its remote HEAD"
        );
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

/// The ref each remote project syncs to, keyed by project name (`None` = the remote's HEAD).
///
/// A project's own `ref:` is used as written. Without one, and with `refs_from` set, the project
/// takes the release tag of the version the Cargo.lock carries for its crate (`crate:`, else its
/// name). Three cases are refused rather than guessed: a `ref:` that disagrees with the locked
/// version (the clone would carry migrations for code that is not compiled), a crate the lock
/// carries at more than one version, and a module or crate the lock does not carry at all.
pub fn effective_refs(
    manifest: &Manifest,
    workspace_root: &Path,
) -> Result<BTreeMap<String, Option<String>>> {
    let locked = match &manifest.refs_from {
        Some(from) => Some((from, locked_versions(&workspace_root.join(&from.cargo_lock))?)),
        None => None,
    };
    let mut refs = BTreeMap::new();
    for p in manifest.projects.iter().filter(|p| p.remote.is_some()) {
        let crate_name = p.crate_name.as_deref().unwrap_or(&p.name);
        let derived = match &locked {
            Some((from, versions)) => match versions.get(crate_name).map(Vec::as_slice) {
                Some([version]) => Some(from.tag.replace("{version}", version)),
                Some(many) => bail!(
                    "{} carries {crate_name} at {} versions ({}); project '{}' cannot be pinned to one",
                    from.cargo_lock,
                    many.len(),
                    many.join(", "),
                    p.name
                ),
                None => None,
            },
            None => None,
        };
        let effective = match (&p.git_ref, derived) {
            (Some(own), Some(derived)) if *own != derived => bail!(
                "project '{}' pins ref {own}, but {} compiles {crate_name} at {derived}; \
                 drop its ref: so the lock decides",
                p.name,
                manifest.refs_from.as_ref().map(|f| f.cargo_lock.as_str()).unwrap_or_default()
            ),
            (Some(own), _) => Some(own.clone()),
            (None, Some(derived)) => Some(derived),
            (None, None)
                if manifest.refs_from.is_some()
                    && matches!(p.project_type, ProjectType::Module | ProjectType::Crate) =>
            {
                bail!(
                    "project '{}' has no ref: and {} carries no crate named {crate_name}; \
                     set crate: to the package it publishes, or give it a ref:",
                    p.name,
                    manifest.refs_from.as_ref().map(|f| f.cargo_lock.as_str()).unwrap_or_default()
                )
            }
            (None, None) => None,
        };
        refs.insert(p.name.clone(), effective);
    }
    Ok(refs)
}

/// Every package a Cargo.lock carries, with the distinct versions it carries it at.
fn locked_versions(path: &Path) -> Result<BTreeMap<String, Vec<String>>> {
    #[derive(Deserialize)]
    struct CargoLock {
        #[serde(default)]
        package: Vec<LockedPackage>,
    }
    #[derive(Deserialize)]
    struct LockedPackage {
        name: String,
        version: String,
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {} (refs_from.cargo_lock)", path.display()))?;
    let lock: CargoLock =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    let mut versions: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pkg in lock.package {
        let entry = versions.entry(pkg.name).or_default();
        if !entry.contains(&pkg.version) {
            entry.push(pkg.version);
        }
    }
    Ok(versions)
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
            crate_name: None,
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn validate_accepts_valid_graph() {
        let m = Manifest {
            version: CURRENT_VERSION,
            refs_from: None,
            projects: vec![project("a", &[]), project("b", &["a"])],
        };
        m.validate().unwrap();
    }

    #[test]
    fn validate_rejects_unknown_dependency() {
        let m = Manifest {
            version: CURRENT_VERSION,
            refs_from: None,
            projects: vec![project("a", &["ghost"])],
        };
        let err = m.validate().unwrap_err();
        assert!(matches!(err, WorkspaceError::UnknownDependency { .. }));
    }

    #[test]
    fn validate_rejects_self_dependency() {
        let m = Manifest {
            version: CURRENT_VERSION,
            refs_from: None,
            projects: vec![project("a", &["a"])],
        };
        let err = m.validate().unwrap_err();
        assert!(matches!(err, WorkspaceError::SelfDependency(_)));
    }

    #[test]
    fn validate_rejects_duplicate_names() {
        let m = Manifest {
            version: CURRENT_VERSION,
            refs_from: None,
            projects: vec![project("a", &[]), project("a", &[])],
        };
        let err = m.validate().unwrap_err();
        assert!(matches!(err, WorkspaceError::DuplicateProject(_)));
    }

    #[test]
    fn current_project_matches_exact_path() {
        let m = Manifest {
            version: CURRENT_VERSION,
            refs_from: None,
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
            refs_from: None,
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
            refs_from: None,
            projects: vec![
                Project {
                    name: "outer".into(),
                    project_type: ProjectType::Module,
                    path: "./apps".into(),
                    remote: None,
                    git_ref: None,
                    crate_name: None,
                    depends_on: vec![],
                },
                Project {
                    name: "inner".into(),
                    project_type: ProjectType::Module,
                    path: "./apps/billing".into(),
                    remote: None,
                    git_ref: None,
                    crate_name: None,
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
            refs_from: None,
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
        // Nanoseconds alone collide between tests running in parallel; the counter does not.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        p.push(format!("metaphor-test-{}-{nanos}-{n}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn remote_project(name: &str, kind: ProjectType, git_ref: Option<&str>, crate_name: Option<&str>) -> Project {
        Project {
            name: name.into(),
            project_type: kind,
            path: format!("modules/{name}"),
            remote: Some(format!("https://example.invalid/{name}")),
            git_ref: git_ref.map(str::to_string),
            crate_name: crate_name.map(str::to_string),
            depends_on: vec![],
        }
    }

    /// A workspace whose `refs_from` lock carries the given (name, version) packages.
    fn locked_workspace(packages: &[(&str, &str)], projects: Vec<Project>) -> (PathBuf, Manifest) {
        let tmp = tempdir();
        let mut lock = String::from("version = 4\n");
        for (name, version) in packages {
            lock.push_str(&format!("\n[[package]]\nname = \"{name}\"\nversion = \"{version}\"\n"));
        }
        std::fs::create_dir_all(tmp.join("apps/svc")).unwrap();
        std::fs::write(tmp.join("apps/svc/Cargo.lock"), lock).unwrap();
        let manifest = Manifest {
            version: CURRENT_VERSION,
            refs_from: Some(RefsFrom { cargo_lock: "apps/svc/Cargo.lock".into(), tag: default_release_tag() }),
            projects,
        };
        (tmp, manifest)
    }

    #[test]
    fn refs_come_from_the_locked_version_by_name_or_by_crate() {
        let (tmp, m) = locked_workspace(
            &[("backbone-core", "2.7.35"), ("backbone-sapiens", "0.4.27"), ("backbone-cmms", "0.7.22")],
            vec![
                remote_project("backbone-framework", ProjectType::Module, None, Some("backbone-core")),
                remote_project("backbone-sapiens", ProjectType::Module, None, None),
                remote_project("backbone-maintenance", ProjectType::Module, None, Some("backbone-cmms")),
                remote_project("serpa-control-plane", ProjectType::BackendService, Some("main"), None),
            ],
        );
        let refs = effective_refs(&m, &tmp).unwrap();
        assert_eq!(refs["backbone-framework"].as_deref(), Some("v2.7.35"));
        assert_eq!(refs["backbone-sapiens"].as_deref(), Some("v0.4.27"));
        assert_eq!(refs["backbone-maintenance"].as_deref(), Some("v0.7.22"));
        assert_eq!(refs["serpa-control-plane"].as_deref(), Some("main"));
    }

    #[test]
    fn a_ref_that_agrees_with_the_lock_is_kept_and_one_that_disagrees_is_refused() {
        let pkgs = [("backbone-sapiens", "0.4.27")];
        let (tmp, m) = locked_workspace(&pkgs, vec![remote_project("backbone-sapiens", ProjectType::Module, Some("v0.4.27"), None)]);
        assert_eq!(effective_refs(&m, &tmp).unwrap()["backbone-sapiens"].as_deref(), Some("v0.4.27"));
        let (tmp, m) = locked_workspace(&pkgs, vec![remote_project("backbone-sapiens", ProjectType::Module, Some("v0.4.26"), None)]);
        let err = effective_refs(&m, &tmp).unwrap_err().to_string();
        assert!(err.contains("v0.4.26") && err.contains("v0.4.27") && err.contains("drop its ref"), "{err}");
    }

    #[test]
    fn a_module_the_lock_does_not_carry_is_refused_but_an_app_follows_head() {
        let (tmp, m) = locked_workspace(&[], vec![remote_project("backbone-orphan", ProjectType::Module, None, None)]);
        let err = effective_refs(&m, &tmp).unwrap_err().to_string();
        assert!(err.contains("backbone-orphan") && err.contains("crate:"), "{err}");
        let (tmp, m) = locked_workspace(&[], vec![remote_project("some-app", ProjectType::Webapp, None, None)]);
        assert_eq!(effective_refs(&m, &tmp).unwrap()["some-app"], None);
    }

    #[test]
    fn a_crate_locked_at_two_versions_is_refused() {
        let (tmp, m) = locked_workspace(
            &[("backbone-sapiens", "0.4.26"), ("backbone-sapiens", "0.4.27")],
            vec![remote_project("backbone-sapiens", ProjectType::Module, None, None)],
        );
        let err = effective_refs(&m, &tmp).unwrap_err().to_string();
        assert!(err.contains("2 versions"), "{err}");
    }

    #[test]
    fn without_refs_from_only_written_refs_apply() {
        let m = Manifest {
            version: CURRENT_VERSION,
            refs_from: None,
            projects: vec![
                remote_project("a", ProjectType::Module, Some("v1.0.0"), None),
                remote_project("b", ProjectType::Module, None, None),
            ],
        };
        let refs = effective_refs(&m, Path::new("/nonexistent")).unwrap();
        assert_eq!(refs["a"].as_deref(), Some("v1.0.0"));
        assert_eq!(refs["b"], None);
    }

    #[test]
    fn refs_from_and_crate_round_trip_through_yaml() {
        let yaml = "version: 1\nrefs_from:\n  cargo_lock: apps/svc/Cargo.lock\nprojects:\n- name: backbone-maintenance\n  type: module\n  path: modules/backbone-maintenance\n  remote: https://example.invalid/m\n  crate: backbone-cmms\n";
        let m: Manifest = serde_yaml::from_str(yaml).unwrap();
        let from = m.refs_from.as_ref().unwrap();
        assert_eq!((from.cargo_lock.as_str(), from.tag.as_str()), ("apps/svc/Cargo.lock", "v{version}"));
        assert_eq!(m.projects[0].crate_name.as_deref(), Some("backbone-cmms"));
        let again: Manifest = serde_yaml::from_str(&serde_yaml::to_string(&m).unwrap()).unwrap();
        assert_eq!(again.refs_from, m.refs_from);
        assert_eq!(again.projects[0].crate_name.as_deref(), Some("backbone-cmms"));
    }


    #[test]
    fn refs_from_requires_manifest_version_two_and_version_three_is_refused() {
        let body = "refs_from:\n  cargo_lock: Cargo.lock\nprojects: []\n";
        let dir = tempdir();
        std::fs::write(dir.join(MANIFEST_FILE), format!("version: 1\n{body}")).unwrap();
        let err = load(&dir).unwrap_err().to_string();
        assert!(err.contains("version: 2"), "{err}");
        std::fs::write(dir.join(MANIFEST_FILE), format!("version: 2\n{body}")).unwrap();
        assert!(load(&dir).unwrap().refs_from.is_some());
        std::fs::write(dir.join(MANIFEST_FILE), "version: 3\nprojects: []\n").unwrap();
        assert!(load(&dir).unwrap_err().to_string().contains("unsupported"));
    }
}
