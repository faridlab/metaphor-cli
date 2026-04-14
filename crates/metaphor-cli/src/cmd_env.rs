//! `metaphor env check` — validate every project's declared env vars are set.
//!
//! Each project may ship a `metaphor.env.yaml` declaring the env vars it
//! consumes. This command walks each selected project, parses its schema,
//! and verifies every *required* var has a value — consulting, in order:
//!
//! 1. The current process environment.
//! 2. A workspace-root `.env` file (simple `KEY=VALUE` lines).
//!
//! The command is read-only: it never writes secrets, never modifies
//! anything, and exits non-zero if any required var is missing.

use anyhow::{bail, Context, Result};
use metaphor_workspace::{Manifest, Project};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub const SCHEMA_FILE: &str = "metaphor.env.yaml";
pub const WORKSPACE_ENV: &str = ".env";

#[derive(Debug, Deserialize)]
struct Schema {
    #[serde(default)]
    env: Vec<Var>,
}

#[derive(Debug, Deserialize)]
struct Var {
    name: String,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    secret: bool,
}

pub struct EnvCheckOptions<'a> {
    pub project_filter: Option<&'a [String]>,
    pub json: bool,
}

pub fn cmd_env_check(
    manifest: &Manifest,
    workspace_root: &Path,
    opts: &EnvCheckOptions<'_>,
) -> Result<()> {
    let workspace_env = load_env_file(&workspace_root.join(WORKSPACE_ENV));
    let selected = select(manifest, opts.project_filter)?;
    let mut reports: Vec<ProjectReport> = Vec::new();

    for p in &selected {
        let project_root = p.resolved_path(workspace_root);
        let schema_path = project_root.join(SCHEMA_FILE);
        if !schema_path.exists() {
            reports.push(ProjectReport {
                project: p.name.clone(),
                schema_present: false,
                missing: Vec::new(),
                present: Vec::new(),
            });
            continue;
        }
        // Per-project .env takes priority over the workspace .env so
        // "this service needs THIS value" always wins.
        let project_env = load_env_file(&project_root.join(".env"));
        let schema = load_schema(&schema_path)?;
        let mut missing = Vec::new();
        let mut present = Vec::new();
        for v in &schema.env {
            let source = if std::env::var(&v.name).is_ok() {
                Some("environment")
            } else if project_env.contains_key(&v.name) {
                Some("project .env")
            } else if workspace_env.contains_key(&v.name) {
                Some("workspace .env")
            } else if v.default.is_some() {
                Some("default")
            } else {
                None
            };
            match source {
                Some(src) => present.push(VarStatus {
                    name: v.name.clone(),
                    source: src.into(),
                    secret: v.secret,
                }),
                None if v.required => missing.push(VarStatus {
                    name: v.name.clone(),
                    source: "(missing)".into(),
                    secret: v.secret,
                }),
                None => {}
            }
        }
        reports.push(ProjectReport {
            project: p.name.clone(),
            schema_present: true,
            missing,
            present,
        });
    }

    let missing_names: Vec<String> = reports
        .iter()
        .flat_map(|r| {
            r.missing
                .iter()
                .map(move |v| format!("{}::{}", r.project, v.name))
        })
        .collect();
    let total_missing = missing_names.len();

    if opts.json {
        let data = serde_json::json!({
            "reports": reports.iter().map(|r| {
                serde_json::json!({
                    "project": r.project,
                    "schema_present": r.schema_present,
                    "present": r.present.iter().map(|v| serde_json::json!({
                        "name": v.name,
                        "source": v.source,
                        "secret": v.secret,
                    })).collect::<Vec<_>>(),
                    "missing": r.missing.iter().map(|v| serde_json::json!({
                        "name": v.name,
                        "secret": v.secret,
                    })).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
            "missing_count": total_missing,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&crate::json_envelope(data))?
        );
    } else {
        print_text(&reports);
    }

    if total_missing > 0 {
        bail!(
            "{} required env var(s) missing: {}",
            total_missing,
            missing_names.join(", ")
        );
    }
    Ok(())
}

fn print_text(reports: &[ProjectReport]) {
    for r in reports {
        println!("{}:", r.project);
        if !r.schema_present {
            println!("  (no {} — skipped)", SCHEMA_FILE);
            continue;
        }
        if r.present.is_empty() && r.missing.is_empty() {
            println!("  (schema empty)");
            continue;
        }
        for v in &r.present {
            let secret_mark = if v.secret { " [secret]" } else { "" };
            println!("  OK     {} ({}){secret_mark}", v.name, v.source);
        }
        for v in &r.missing {
            let secret_mark = if v.secret { " [secret]" } else { "" };
            println!("  MISS   {}{secret_mark}", v.name);
        }
    }
}

fn select<'a>(manifest: &'a Manifest, filter: Option<&'a [String]>) -> Result<Vec<&'a Project>> {
    match filter {
        None => Ok(manifest.projects.iter().collect()),
        Some(names) => {
            let mut out = Vec::with_capacity(names.len());
            for n in names {
                out.push(manifest.find_project(n)?);
            }
            Ok(out)
        }
    }
}

fn load_schema(path: &Path) -> Result<Schema> {
    let raw = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_yaml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
}

/// Parse a `.env`-style file. Handles:
/// - blank lines and full-line `#` comments
/// - optional `export ` prefix on the key
/// - double- or single-quoted values (quotes stripped, inner `#` kept literal)
/// - unquoted values with trailing `# comment` stripped
///
/// Not a fully shell-compliant parser (no escape sequences, no variable
/// interpolation) — enough to match what most CI/dev `.env` files contain
/// without pulling in a crate.
fn load_env_file(path: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(raw) = fs::read_to_string(path) else {
        return out;
    };
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim();
        if k.is_empty() {
            continue;
        }
        let v = parse_env_value(v);
        out.insert(k.to_string(), v);
    }
    out
}

fn parse_env_value(raw: &str) -> String {
    let raw = raw.trim_start();
    if let Some(stripped) = raw.strip_prefix('"') {
        if let Some(end) = stripped.find('"') {
            return stripped[..end].to_string();
        }
    }
    if let Some(stripped) = raw.strip_prefix('\'') {
        if let Some(end) = stripped.find('\'') {
            return stripped[..end].to_string();
        }
    }
    // Unquoted: strip an inline `# comment` starting at a whitespace-preceded `#`.
    let mut end = raw.len();
    let bytes = raw.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'#' && (i == 0 || bytes[i - 1].is_ascii_whitespace()) {
            end = i;
            break;
        }
    }
    raw[..end].trim_end().to_string()
}

struct ProjectReport {
    project: String,
    schema_present: bool,
    missing: Vec<VarStatus>,
    present: Vec<VarStatus>,
}

struct VarStatus {
    name: String,
    source: String,
    secret: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use metaphor_workspace::{ProjectType, CURRENT_VERSION};
    use tempfile::TempDir;

    fn proj(name: &str) -> Project {
        Project {
            name: name.into(),
            project_type: ProjectType::BackendService,
            path: format!("./{name}"),
            remote: None,
            depends_on: vec![],
        }
    }

    #[test]
    fn env_file_parses_common_shapes() {
        let tmp = TempDir::new().unwrap();
        let body = r#"
# header comment
FOO=bar
QUOTED="with spaces"
SINGLE='single-quoted'
INLINE=plain # trailing comment stripped
EXPORTED=yes-exported
export PREFIXED=also-parsed
EMPTY=
HASH_INSIDE="value#with#hash"
"#;
        fs::write(tmp.path().join(".env"), body).unwrap();
        let got = load_env_file(&tmp.path().join(".env"));
        assert_eq!(got.get("FOO").unwrap(), "bar");
        assert_eq!(got.get("QUOTED").unwrap(), "with spaces");
        assert_eq!(got.get("SINGLE").unwrap(), "single-quoted");
        assert_eq!(got.get("INLINE").unwrap(), "plain");
        assert_eq!(got.get("EXPORTED").unwrap(), "yes-exported");
        assert_eq!(got.get("PREFIXED").unwrap(), "also-parsed");
        assert_eq!(got.get("EMPTY").unwrap(), "");
        assert_eq!(got.get("HASH_INSIDE").unwrap(), "value#with#hash");
        assert!(!got.contains_key("# header comment"));
    }

    #[test]
    fn missing_required_var_reports() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("api")).unwrap();
        fs::write(
            tmp.path().join("api").join(SCHEMA_FILE),
            "env:\n  - name: NOT_SET_ANYWHERE_PLEASE\n    required: true\n",
        )
        .unwrap();
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![proj("api")],
        };
        let err = cmd_env_check(
            &m,
            tmp.path(),
            &EnvCheckOptions {
                project_filter: None,
                json: false,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn default_value_satisfies_required() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("api")).unwrap();
        fs::write(
            tmp.path().join("api").join(SCHEMA_FILE),
            "env:\n  - name: LOG_LEVEL\n    required: true\n    default: info\n",
        )
        .unwrap();
        let m = Manifest {
            version: CURRENT_VERSION,
            projects: vec![proj("api")],
        };
        cmd_env_check(
            &m,
            tmp.path(),
            &EnvCheckOptions {
                project_filter: None,
                json: false,
            },
        )
        .unwrap();
    }
}
