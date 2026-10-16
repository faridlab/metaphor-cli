//! Container-image deny-list for `metaphor doctor`.
//!
//! Scans the compose files a workspace ships and fails on images that must not
//! come back: floating tags (`:latest`, or no tag at all, which means latest),
//! Redis 7 (7.4+ is not open source), MinIO server and client images (no longer
//! published or maintained), MailHog and Promtail (unmaintained / end of life),
//! and Node 20 (end of life). A floating tag lets a rebuild silently pull a
//! different major version, or an image that has since been withdrawn.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// One denied image reference found in a compose file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The compose file, as given to [`scan_file`].
    pub file: PathBuf,
    /// The compose service that uses the image.
    pub service: String,
    /// The image reference exactly as written.
    pub image: String,
    /// Why it is denied, and what to use instead.
    pub reason: &'static str,
}

/// A parsed image reference: registry-qualified repository, tag, digest.
struct ImageRef<'a> {
    /// Repository with `docker.io/` and `library/` stripped, e.g. `redis`,
    /// `grafana/promtail`, `quay.io/minio/mc`.
    name: String,
    tag: Option<&'a str>,
    has_digest: bool,
}

fn parse(image: &str) -> ImageRef<'_> {
    let (rest, has_digest) = match image.split_once('@') {
        Some((r, _)) => (r, true),
        None => (image, false),
    };
    // A `:` after the last `/` starts the tag; one before it is a registry port.
    let slash = rest.rfind('/').map_or(0, |i| i + 1);
    let (repo, tag) = match rest[slash..].find(':') {
        Some(i) => (&rest[..slash + i], Some(&rest[slash + i + 1..])),
        None => (rest, None),
    };
    let mut name = repo.strip_prefix("docker.io/").unwrap_or(repo);
    name = name.strip_prefix("library/").unwrap_or(name);
    ImageRef {
        name: name.to_string(),
        tag,
        has_digest,
    }
}

/// True when `tag` is `major` itself or starts a version in that major line
/// (`7`, `7.4`, `7-alpine`, `7.2.4-bookworm`).
fn in_major(tag: Option<&str>, major: &str) -> bool {
    tag.is_some_and(|t| {
        t.strip_prefix(major)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', '-']))
    })
}

/// Why `image` is denied, or `None` when it is allowed.
pub fn deny_reason(image: &str) -> Option<&'static str> {
    let r = parse(image);
    let name = r.name.as_str();
    if name.starts_with("minio/") || name.starts_with("quay.io/minio/") {
        return Some("MinIO no longer publishes or maintains its images: run RustFS (`rustfs/rustfs`) as the object store, and the self-built `ghcr.io/faridlab/mc` for the bucket bootstrap");
    }
    if name.contains("mailhog") {
        return Some("MailHog is unmaintained: use Mailpit (`axllent/mailpit`)");
    }
    if name == "grafana/promtail" {
        return Some("Promtail reached end of life on 2026-03-02: ship logs with Grafana Alloy (`grafana/alloy`)");
    }
    if name == "redis" && in_major(r.tag, "7") {
        return Some("Redis 7.4+ is licensed RSALv2/SSPL, not open source: use Valkey (`valkey/valkey`)");
    }
    if name == "node" && in_major(r.tag, "20") {
        return Some("Node.js 20 reached end of life on 2026-04-30: use Node 24 LTS");
    }
    // An interpolated tag (`:${SERVICE_TAG}`) is pinned by whoever sets it.
    if !r.has_digest && matches!(r.tag, None | Some("latest")) {
        return Some("floating tag: a rebuild can pull a different major version, or an image that was withdrawn; pin an exact version");
    }
    None
}

/// The compose file names Dependabot's `docker-compose` ecosystem reads:
/// `docker-compose.yml`, `compose.yaml`, `compose.dev.yaml`, `docker-compose-ci.yml`, …
pub fn is_compose_file_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some(stem) = lower
        .strip_suffix(".yml")
        .or_else(|| lower.strip_suffix(".yaml"))
    else {
        return false;
    };
    let stem = stem.strip_prefix("docker-").unwrap_or(stem);
    let Some(rest) = stem.strip_prefix("compose") else {
        return false;
    };
    let word = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    let (dash, dot) = match rest.split_once('.') {
        Some((d, x)) => (d, Some(x)),
        None => (rest, None),
    };
    let dash_ok = dash.is_empty() || dash.strip_prefix('-').is_some_and(word);
    let dot_ok = dot.is_none_or(|x| !x.is_empty() && x.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
    dash_ok && dot_ok
}

/// The compose files directly inside `dir` and `dir/deployment`.
pub fn compose_files_in(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for d in [dir.to_path_buf(), dir.join("deployment")] {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_file()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(is_compose_file_name)
            {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// The denied images in one compose file. A file that does not parse yields
/// an error string instead; one without `services` yields nothing.
pub fn scan_file(path: &Path) -> Result<Vec<Finding>, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&raw).map_err(|e| e.to_string())?;
    let mut findings = Vec::new();
    let Some(services) = doc.get("services").and_then(|s| s.as_mapping()) else {
        return Ok(findings);
    };
    for (name, svc) in services {
        let Some(image) = svc.get("image").and_then(|i| i.as_str()) else {
            continue;
        };
        if let Some(reason) = deny_reason(image) {
            findings.push(Finding {
                file: path.to_path_buf(),
                service: name.as_str().unwrap_or("?").to_string(),
                image: image.to_string(),
                reason,
            });
        }
    }
    Ok(findings)
}

/// Every compose file in the workspace root and in each project root, once.
pub fn workspace_compose_files<'a>(
    workspace_root: &'a Path,
    project_roots: impl IntoIterator<Item = &'a Path>,
) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let roots = std::iter::once(workspace_root).chain(project_roots);
    for root in roots {
        for f in compose_files_in(root) {
            let key = f.canonicalize().unwrap_or_else(|_| f.clone());
            if seen.insert(key) {
                out.push(f);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floating_tags_are_denied() {
        for img in ["prom/prometheus:latest", "grafana/grafana", "localhost:5000/app", "caddy:latest"] {
            assert!(deny_reason(img).is_some(), "{img} should be denied");
        }
    }

    #[test]
    fn pinned_and_interpolated_tags_are_allowed() {
        for img in [
            "postgres:16.15-alpine",
            "valkey/valkey:9.0.6-alpine",
            "rustfs/rustfs:1.0.2",
            "grafana/alloy:v1.20.1@sha256:2aa2099af76c0098d4af7a4d6e48f86cb66dc1a000222ad927a1c67c6542d13f",
            "ghcr.io/faridlab/mc:RELEASE.2025-08-13T08-35-41Z",
            "ghcr.io/faridlab/app:${SERVICE_TAG:?must be set}",
            "localhost:5000/app:1.2.3",
            "node:24.22.0-alpine",
            "redis-commander:0.9",
            "node:200",
        ] {
            assert_eq!(deny_reason(img), None, "{img} should be allowed");
        }
    }

    #[test]
    fn replaced_components_are_denied_whatever_the_tag() {
        for img in [
            "redis:7-alpine",
            "redis:7",
            "docker.io/library/redis:7.4.1",
            "minio/minio:RELEASE.2025-01-01T00-00-00Z",
            "quay.io/minio/mc:RELEASE.2025-08-13T08-35-41Z",
            "mailhog/mailhog:v1.0.1",
            "grafana/promtail:3.5.0",
            "node:20-alpine",
            "node:20.18.1",
        ] {
            assert!(deny_reason(img).is_some(), "{img} should be denied");
        }
    }

    #[test]
    fn a_digest_alone_pins_an_image() {
        assert_eq!(deny_reason("caddy@sha256:d8542f48d34a"), None);
    }

    #[test]
    fn compose_file_names_match_dependabot() {
        for ok in ["docker-compose.yml", "compose.yaml", "compose.dev.yaml", "docker-compose-ci.yml", "Compose.YML", "compose.prod-eu.yaml"] {
            assert!(is_compose_file_name(ok), "{ok}");
        }
        for no in ["compose.fragment", "my-compose.yml", "compose-.yml", "compose..yaml", "values.yaml", "composer.json"] {
            assert!(!is_compose_file_name(no), "{no}");
        }
    }
}
