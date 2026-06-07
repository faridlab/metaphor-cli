//! `metaphor clean` — remove stale build-artifact directories across projects.
//!
//! Defaults to dry-run: the first invocation always reports *what would be*
//! freed. Destructive deletion requires `--apply`.
//!
//! What counts as a build artifact depends on the project type (see
//! [`targets_for`]) — the safelist is intentionally conservative. We only
//! delete directories with names we explicitly recognize, so a user's source
//! dir coincidentally named `build/` is not at risk.

use anyhow::{bail, Context, Result};
use metaphor_workspace::{Manifest, Project, ProjectType};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};
use walkdir::WalkDir;

/// Return the build-artifact directory names that are safe to delete for
/// projects of the given type. Names are matched literally against the
/// top-level entries of the project directory.
pub fn targets_for(t: ProjectType) -> &'static [&'static str] {
    match t {
        ProjectType::Crate | ProjectType::CliTool => &["target"],
        // Backend services are typically Rust/Go/Python/Node — generic
        // language-aware safelist. `.next/` is a Next.js web-frontend artifact;
        // deliberately excluded here.
        ProjectType::BackendService => &["target", "node_modules", "dist", "build", "__pycache__"],
        ProjectType::Webservice | ProjectType::Webapp | ProjectType::DocsSite => &[
            "node_modules",
            "dist",
            ".next",
            ".cache",
            "build",
            ".nuxt",
            ".parcel-cache",
        ],
        ProjectType::Mobileapp => &["build", ".gradle", "node_modules", "Pods", "DerivedData"],
        ProjectType::Desktopapp => &["target", "build", "dist", "node_modules"],
        ProjectType::Module => &["target", "node_modules", "build", "dist", "__pycache__"],
        ProjectType::Infra => &[".terraform"],
    }
}

#[derive(Debug, Clone)]
pub struct CleanCandidate {
    pub project: String,
    pub project_type: ProjectType,
    pub dir: PathBuf,
    pub bytes: u64,
    pub modified: Option<SystemTime>,
}

pub struct CleanOptions<'a> {
    pub older_than: Duration,
    pub project_filter: Option<&'a [String]>,
    pub apply: bool,
    pub json: bool,
    /// Skip per-directory sizing. Faster on cold caches / huge trees; reported
    /// sizes will read as 0.
    pub quick: bool,
    /// When `--apply` would free more than this many bytes, refuse unless
    /// `--yes` is also passed. `None` = no threshold check.
    pub confirm_over: Option<u64>,
    /// Bypass `--confirm-over` thresholds.
    pub yes: bool,
    /// Also reclaim this workspace's Docker build-cache volumes (e.g. the dev
    /// stack's `cargo_target`). Scoped to the workspace's compose project(s);
    /// data volumes (pgdata, miniodata, …) are never touched.
    pub docker: bool,
    /// When cleaning Docker volumes, also reclaim ones currently in use by a
    /// running container — by emptying them in place (forces a rebuild).
    /// Without this, in-use volumes are reported but left alone.
    pub include_running: bool,
}

pub fn cmd_clean(
    manifest: &Manifest,
    workspace_root: &Path,
    opts: &CleanOptions<'_>,
) -> Result<()> {
    let cutoff = SystemTime::now()
        .checked_sub(opts.older_than)
        .unwrap_or(SystemTime::UNIX_EPOCH);

    let selected = select(manifest, opts.project_filter)?;
    let mut candidates: Vec<CleanCandidate> = Vec::new();
    for p in selected {
        collect_candidates(p, workspace_root, cutoff, opts.quick, &mut candidates)?;
    }

    // Safety gate: when the user set a size threshold, a large --apply is
    // refused unless they also pass --yes. The dry-run path is exempt.
    if opts.apply {
        if let Some(limit) = opts.confirm_over {
            let total: u64 = candidates.iter().map(|c| c.bytes).sum();
            if total > limit && !opts.yes {
                bail!(
                    "refusing to delete {} (> --confirm-over={}); re-run with --yes to proceed",
                    human_bytes(total),
                    human_bytes(limit),
                );
            }
        }
    }

    if opts.apply {
        let (deleted, bytes, errors) = apply(&candidates);
        if opts.json {
            print_json(&candidates, opts, Some((deleted, bytes, errors.clone())))?;
        } else {
            print_text(&candidates, opts);
            println!(
                "Deleted {deleted} director{y} ({}).",
                human_bytes(bytes),
                y = if deleted == 1 { "y" } else { "ies" }
            );
            if !errors.is_empty() {
                eprintln!();
                for (path, msg) in &errors {
                    eprintln!("  failed: {} — {msg}", path.display());
                }
                bail!(
                    "{} director{} failed to delete",
                    errors.len(),
                    if errors.len() == 1 { "y" } else { "ies" }
                );
            }
        }
    } else if opts.json {
        print_json(&candidates, opts, None)?;
    } else {
        print_text(&candidates, opts);
        let total: u64 = candidates.iter().map(|c| c.bytes).sum();
        let count = candidates.len();
        println!(
            "Dry run — would free {} across {count} director{y}. Re-run with --apply to delete.",
            human_bytes(total),
            y = if count == 1 { "y" } else { "ies" }
        );
    }

    if opts.docker {
        clean_docker(workspace_root, opts)?;
    }
    Ok(())
}

// ── Docker build-cache volumes ────────────────────────────────────────────────
//
// The dev stack keeps Rust build artifacts in named Docker volumes (e.g.
// `<project>_cargo_target`), which `metaphor clean` can't see from the host —
// that 100 GB+ volume is the usual cause of a "No space left on device" that
// takes Postgres down. This reclaims them, scoped strictly to the workspace's
// own compose project(s) via the `com.docker.compose.project` label, and only
// for build-cache volumes — never data volumes.

/// Volume short-names (the `com.docker.compose.volume` part, i.e. the suffix
/// after `<project>_`) that hold rebuildable build caches. SAFELIST: anything
/// not listed (pgdata, miniodata, redisdata, …) is never removed or emptied.
const DOCKER_CACHE_VOLUMES: &[&str] = &[
    "cargo_target",
    "cargo_registry",
    "cargo_git",
    "target",
    "node_modules",
    "gradle_cache",
    "build_cache",
];

struct DockerVolume {
    name: String,
    short: String,
    size_bytes: u64,
    size_human: String,
    in_use: bool,
    container: Option<String>,
}

fn clean_docker(workspace_root: &Path, opts: &CleanOptions<'_>) -> Result<()> {
    println!();
    if !docker_available() {
        println!("Docker: not available (daemon not running?) — skipping volume cleanup.");
        return Ok(());
    }
    let projects = workspace_compose_projects(workspace_root);
    if projects.is_empty() {
        println!(
            "Docker: no compose project name found under {}/deployment — skipping.",
            workspace_root.display()
        );
        return Ok(());
    }

    let sizes = docker_volume_sizes()?; // name -> (bytes, human, links)
    let mut vols: Vec<DockerVolume> = Vec::new();
    for project in &projects {
        for name in docker_project_volumes(project)? {
            let short = name
                .strip_prefix(&format!("{project}_"))
                .unwrap_or(&name)
                .to_string();
            if !DOCKER_CACHE_VOLUMES.contains(&short.as_str()) {
                continue; // not a build cache — leave it (data volumes, etc.)
            }
            let (size_bytes, size_human, links) =
                sizes.get(&name).cloned().unwrap_or((0, "0 B".into(), 0));
            vols.push(DockerVolume {
                container: if links > 0 { docker_volume_container(&name) } else { None },
                in_use: links > 0,
                name,
                short,
                size_bytes,
                size_human,
            });
        }
    }

    if vols.is_empty() {
        println!("Docker: no build-cache volumes for project(s) {}.", projects.join(", "));
        return Ok(());
    }

    println!("Docker build-cache volumes ({}):", projects.join(", "));
    for v in &vols {
        let state = match &v.container {
            Some(c) => format!("in use by {c}"),
            None => "idle".to_string(),
        };
        println!("  {:<28} {:>10}   {state}", v.short, v.size_human);
    }
    println!();

    if !opts.apply {
        let reclaimable: u64 = vols
            .iter()
            .filter(|v| !v.in_use || opts.include_running)
            .map(|v| v.size_bytes)
            .sum();
        let blocked = vols.iter().filter(|v| v.in_use && !opts.include_running).count();
        println!(
            "Dry run — would reclaim {} from {} Docker volume(s). Re-run with --apply --docker to do it.",
            human_bytes(reclaimable),
            vols.iter().filter(|v| !v.in_use || opts.include_running).count(),
        );
        if blocked > 0 {
            println!(
                "  ({blocked} in use by a running container — add --include-running to empty them, which forces a rebuild.)"
            );
        }
        return Ok(());
    }

    // Threshold guard mirrors the host path.
    if let Some(limit) = opts.confirm_over {
        let total: u64 = vols
            .iter()
            .filter(|v| !v.in_use || opts.include_running)
            .map(|v| v.size_bytes)
            .sum();
        if total > limit && !opts.yes {
            bail!(
                "refusing to reclaim {} of Docker cache (> --confirm-over={}); re-run with --yes",
                human_bytes(total),
                human_bytes(limit),
            );
        }
    }

    let mut freed = 0u64;
    for v in &vols {
        if v.in_use {
            if !opts.include_running {
                println!("  skip {} — in use by {} (use --include-running)", v.short, v.container.as_deref().unwrap_or("?"));
                continue;
            }
            match empty_in_use_volume(v) {
                Ok(()) => {
                    freed += v.size_bytes;
                    println!("  emptied {} ({}) in {}", v.short, v.size_human, v.container.as_deref().unwrap_or("?"));
                }
                Err(e) => eprintln!("  failed to empty {}: {e}", v.short),
            }
        } else {
            match run_docker(&["volume", "rm", &v.name]) {
                Ok(_) => {
                    freed += v.size_bytes;
                    println!("  removed volume {} ({})", v.short, v.size_human);
                }
                Err(e) => eprintln!("  failed to remove {}: {e}", v.short),
            }
        }
    }
    println!("Reclaimed ~{} of Docker build cache.", human_bytes(freed));
    Ok(())
}

/// Compose project name(s) declared in the workspace's `deployment/compose*.y*ml`
/// `name:` field — the scope for which Docker volumes we may touch.
fn workspace_compose_projects(workspace_root: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let dir = workspace_root.join("deployment");
    let Ok(entries) = fs::read_dir(&dir) else {
        return names;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !(fname.starts_with("compose") && (fname.ends_with(".yml") || fname.ends_with(".yaml"))) {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            if let Some(n) = parse_compose_project_name(&text) {
                if !names.contains(&n) {
                    names.push(n);
                }
            }
        }
    }
    names
}

/// Extract the top-level Compose `name:` (project name) from a compose file's
/// text. Only matches an unindented `name:` so a service-level `name:` nested
/// under `services:` is never mistaken for the project name.
fn parse_compose_project_name(text: &str) -> Option<String> {
    for line in text.lines() {
        // Top-level keys start at column 0 (no leading whitespace).
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        if let Some(rest) = line.strip_prefix("name:") {
            let n = rest.trim().trim_matches(['"', '\'']).to_string();
            if !n.is_empty() {
                return Some(n);
            }
        }
    }
    None
}

fn docker_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run_docker(args: &[&str]) -> Result<String> {
    let out = Command::new("docker")
        .args(args)
        .output()
        .with_context(|| format!("running docker {}", args.join(" ")))?;
    if !out.status.success() {
        bail!("docker {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn docker_project_volumes(project: &str) -> Result<Vec<String>> {
    let out = run_docker(&[
        "volume",
        "ls",
        "--filter",
        &format!("label=com.docker.compose.project={project}"),
        "--format",
        "{{.Name}}",
    ])?;
    Ok(out.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
}

/// name -> (bytes, human, links) from `docker system df -v`.
fn docker_volume_sizes() -> Result<std::collections::HashMap<String, (u64, String, i64)>> {
    let out = run_docker(&["system", "df", "-v", "--format", "{{json .Volumes}}"])?;
    let mut map = std::collections::HashMap::new();
    let parsed: serde_json::Value = serde_json::from_str(out.trim()).unwrap_or(serde_json::Value::Null);
    if let Some(arr) = parsed.as_array() {
        for v in arr {
            let name = v.get("Name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let human = v.get("Size").and_then(|x| x.as_str()).unwrap_or("0B").to_string();
            let links = v
                .get("Links")
                .map(|x| x.as_i64().unwrap_or_else(|| x.as_str().and_then(|s| s.parse().ok()).unwrap_or(0)))
                .unwrap_or(0);
            let bytes = parse_size(&human).unwrap_or(0);
            if !name.is_empty() {
                map.insert(name, (bytes, human, links));
            }
        }
    }
    Ok(map)
}

fn docker_volume_container(volume: &str) -> Option<String> {
    run_docker(&["ps", "-a", "--filter", &format!("volume={volume}"), "--format", "{{.Names}}"])
        .ok()
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .filter(|s| !s.is_empty())
}

/// Empty an in-use volume in place: find where the container mounts it, then
/// delete its contents (the volume and container stay; the build is rebuilt).
fn empty_in_use_volume(v: &DockerVolume) -> Result<()> {
    let container = v.container.as_deref().context("no container for in-use volume")?;
    let dest = run_docker(&[
        "inspect",
        container,
        "--format",
        &format!("{{{{range .Mounts}}}}{{{{if eq .Name \"{}\"}}}}{{{{.Destination}}}}{{{{end}}}}{{{{end}}}}", v.name),
    ])?
    .trim()
    .to_string();
    if dest.is_empty() {
        bail!("could not find mount path of {} in {container}", v.name);
    }
    // Safety: only ever operate on the recognized cache mountpoint.
    run_docker(&["exec", container, "sh", "-c", &format!("find {dest} -mindepth 1 -delete")])?;
    Ok(())
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

fn collect_candidates(
    project: &Project,
    workspace_root: &Path,
    cutoff: SystemTime,
    quick: bool,
    out: &mut Vec<CleanCandidate>,
) -> Result<()> {
    let root = project.resolved_path(workspace_root);
    if !root.is_dir() {
        // Project directory is missing or not yet cloned; skip silently.
        return Ok(());
    }
    for name in targets_for(project.project_type) {
        let dir = root.join(name);
        if !dir.is_dir() {
            continue;
        }
        // Conservative: if mtime is missing or newer than cutoff, skip the
        // dir. Only delete when we have a timestamp AND it predates cutoff.
        let modified = fs::metadata(&dir).ok().and_then(|m| m.modified().ok());
        match modified {
            Some(t) if t <= cutoff => { /* eligible — fall through */ }
            _ => continue,
        }
        let bytes = if quick {
            0
        } else {
            dir_size(&dir).unwrap_or(0)
        };
        out.push(CleanCandidate {
            project: project.name.clone(),
            project_type: project.project_type,
            dir,
            bytes,
            modified,
        });
    }
    Ok(())
}

fn dir_size(root: &Path) -> Result<u64> {
    let mut total = 0u64;
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.with_context(|| format!("walking {}", root.display()))?;
        if entry.file_type().is_file() {
            if let Ok(md) = entry.metadata() {
                total = total.saturating_add(md.len());
            }
        }
    }
    Ok(total)
}

fn apply(candidates: &[CleanCandidate]) -> (usize, u64, Vec<(PathBuf, String)>) {
    let mut deleted = 0usize;
    let mut bytes = 0u64;
    let mut errors = Vec::new();
    for c in candidates {
        match fs::remove_dir_all(&c.dir) {
            Ok(_) => {
                deleted += 1;
                bytes += c.bytes;
            }
            Err(e) => errors.push((c.dir.clone(), e.to_string())),
        }
    }
    (deleted, bytes, errors)
}

fn print_text(candidates: &[CleanCandidate], opts: &CleanOptions<'_>) {
    if candidates.is_empty() {
        println!(
            "No stale build artifacts found older than {}.",
            human_duration(opts.older_than)
        );
        return;
    }
    let mut current: Option<&str> = None;
    for c in candidates {
        if current != Some(c.project.as_str()) {
            if current.is_some() {
                println!();
            }
            println!("{} ({:?}):", c.project, c.project_type);
            current = Some(c.project.as_str());
        }
        let ago = c
            .modified
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .map(|d| format!("{} ago", human_duration(d)))
            .unwrap_or_else(|| "unknown".to_string());
        println!(
            "  {:<20} {:>10}   modified {ago}",
            display_relative(&c.dir),
            human_bytes(c.bytes),
        );
    }
    println!();
}

fn print_json(
    candidates: &[CleanCandidate],
    opts: &CleanOptions<'_>,
    applied: Option<(usize, u64, Vec<(PathBuf, String)>)>,
) -> Result<()> {
    let items: Vec<_> = candidates
        .iter()
        .map(|c| {
            serde_json::json!({
                "project": c.project,
                "project_type": format!("{:?}", c.project_type),
                "dir": c.dir.display().to_string(),
                "bytes": c.bytes,
                "modified_unix_seconds": c.modified.and_then(|t| {
                    t.duration_since(SystemTime::UNIX_EPOCH).ok().map(|d| d.as_secs())
                }),
            })
        })
        .collect();

    let total_bytes: u64 = candidates.iter().map(|c| c.bytes).sum();
    let mut data = serde_json::json!({
        "older_than_seconds": opts.older_than.as_secs(),
        "dry_run": !opts.apply,
        "candidates": items,
        "total_bytes": total_bytes,
    });
    if let Some((deleted, bytes, errors)) = applied {
        data["deleted"] = serde_json::json!(deleted);
        data["deleted_bytes"] = serde_json::json!(bytes);
        data["errors"] = serde_json::json!(errors
            .into_iter()
            .map(|(p, m)| serde_json::json!({ "dir": p.display().to_string(), "error": m }))
            .collect::<Vec<_>>());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&crate::json_envelope(data))?
    );
    Ok(())
}

/// Show the target path in the most readable form for the user — the final
/// two components (e.g. `api/target`) rather than a long absolute path.
fn display_relative(dir: &Path) -> String {
    let comps: Vec<_> = dir.components().collect();
    if comps.len() >= 2 {
        let n = comps.len();
        format!(
            "{}/{}",
            comps[n - 2].as_os_str().to_string_lossy(),
            comps[n - 1].as_os_str().to_string_lossy()
        )
    } else {
        dir.display().to_string()
    }
}

fn human_bytes(n: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

fn human_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let days = secs / 86_400;
    if days >= 365 {
        format!("{} year{}", days / 365, plural(days / 365))
    } else if days >= 30 {
        format!("{} month{}", days / 30, plural(days / 30))
    } else if days >= 1 {
        format!("{days} day{}", plural(days))
    } else {
        let hours = secs / 3600;
        if hours >= 1 {
            format!("{hours} hour{}", plural(hours))
        } else {
            format!("{} minute{}", secs / 60, plural(secs / 60))
        }
    }
}

fn plural(n: u64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Minimum allowed `--older-than`. Protects against "`--older-than=0` just
/// wiped my workspace." One hour is short enough for legitimate CI pre-warm
/// use, long enough to rule out typos.
const MIN_OLDER_THAN: Duration = Duration::from_secs(3600);

/// Parse values like `30d`, `6w`, `2m`, `1y`, `6h`, or a bare number
/// (interpreted as days). Accepts `h` (hours), `d` (days), `w` (weeks),
/// `m` (months = 30 days), `y` (years = 365 days). Rejects anything shorter
/// than `MIN_OLDER_THAN`.
pub fn parse_older_than(s: &str) -> Result<Duration> {
    let s = s.trim();
    if s.is_empty() {
        bail!("empty --older-than value");
    }
    let (num_str, unit): (&str, char) = match s.chars().last().unwrap() {
        c if c.is_ascii_digit() => (s, 'd'),
        c => (&s[..s.len() - c.len_utf8()], c),
    };
    let n: u64 = num_str
        .parse()
        .with_context(|| format!("parsing number from --older-than={s}"))?;
    let seconds = match unit {
        'h' => n.saturating_mul(3_600),
        'd' => n.saturating_mul(86_400),
        'w' => n.saturating_mul(7 * 86_400),
        'm' => n.saturating_mul(30 * 86_400),
        'y' => n.saturating_mul(365 * 86_400),
        other => {
            bail!(
                "unknown duration unit '{other}' — use h, d, w, m, or y (e.g. 6h, 30d, 6w, 2m, 1y)"
            )
        }
    };
    let d = Duration::from_secs(seconds);
    if d < MIN_OLDER_THAN {
        bail!(
            "--older-than={s} is too small; minimum is {} (guard against accidental wipe)",
            human_duration(MIN_OLDER_THAN)
        );
    }
    Ok(d)
}

/// Parse a human size like `10GB`, `500MB`, `1.5TB`, or a bare byte count.
/// Units are IEC-ish (1 KB = 1024 bytes).
pub fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim();
    if s.is_empty() {
        bail!("empty size");
    }
    let upper = s.to_ascii_uppercase();
    let (num, mult): (&str, u64) = if let Some(rest) = upper.strip_suffix("TB") {
        (rest.trim(), 1024u64.pow(4))
    } else if let Some(rest) = upper.strip_suffix("GB") {
        (rest.trim(), 1024u64.pow(3))
    } else if let Some(rest) = upper.strip_suffix("MB") {
        (rest.trim(), 1024u64.pow(2))
    } else if let Some(rest) = upper.strip_suffix("KB") {
        (rest.trim(), 1024)
    } else if let Some(rest) = upper.strip_suffix('B') {
        (rest.trim(), 1)
    } else {
        (upper.as_str(), 1)
    };
    let value: f64 = num
        .parse()
        .with_context(|| format!("parsing number from size={s}"))?;
    if !value.is_finite() || value < 0.0 {
        bail!("invalid size '{s}'");
    }
    Ok((value * mult as f64) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_older_than_defaults_to_days() {
        assert_eq!(
            parse_older_than("30").unwrap(),
            Duration::from_secs(30 * 86400)
        );
        assert_eq!(
            parse_older_than("30d").unwrap(),
            Duration::from_secs(30 * 86400)
        );
    }

    #[test]
    fn parse_older_than_units() {
        assert_eq!(
            parse_older_than("2w").unwrap(),
            Duration::from_secs(14 * 86400)
        );
        assert_eq!(
            parse_older_than("6m").unwrap(),
            Duration::from_secs(180 * 86400)
        );
        assert_eq!(
            parse_older_than("1y").unwrap(),
            Duration::from_secs(365 * 86400)
        );
    }

    #[test]
    fn parse_older_than_rejects_bad_input() {
        assert!(parse_older_than("").is_err());
        assert!(parse_older_than("10q").is_err());
        assert!(parse_older_than("abc").is_err());
    }

    #[test]
    fn parse_older_than_supports_hours() {
        assert_eq!(
            parse_older_than("6h").unwrap(),
            Duration::from_secs(6 * 3600)
        );
    }

    #[test]
    fn parse_older_than_rejects_below_minimum() {
        // 0, sub-hour minutes, 30 minutes — all must error so a typo doesn't
        // wipe the workspace.
        assert!(parse_older_than("0").is_err());
        assert!(parse_older_than("0d").is_err());
        assert!(parse_older_than("0h").is_err());
    }

    #[test]
    fn parse_size_units() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
        assert_eq!(parse_size("1KB").unwrap(), 1024);
        assert_eq!(parse_size("10MB").unwrap(), 10 * 1024 * 1024);
        assert_eq!(parse_size("2GB").unwrap(), 2 * 1024u64.pow(3));
        assert_eq!(parse_size("1.5GB").unwrap(), (1.5 * 1024f64.powi(3)) as u64);
    }

    #[test]
    fn parse_size_rejects_bad_input() {
        assert!(parse_size("").is_err());
        assert!(parse_size("abc").is_err());
        assert!(parse_size("-5MB").is_err());
    }

    #[test]
    fn targets_are_non_empty() {
        for t in [
            ProjectType::Crate,
            ProjectType::CliTool,
            ProjectType::BackendService,
            ProjectType::Webservice,
            ProjectType::Webapp,
            ProjectType::Mobileapp,
            ProjectType::Desktopapp,
            ProjectType::Module,
            ProjectType::DocsSite,
            ProjectType::Infra,
        ] {
            assert!(!targets_for(t).is_empty(), "{t:?} has empty target list");
        }
    }

    #[test]
    fn human_bytes_reads_well() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1023), "1023 B");
        assert_eq!(human_bytes(2048), "2.0 KB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn compose_project_name_top_level_only() {
        let yaml = "version: '3'\nname: bersihir-metaphora\nservices:\n  api:\n    name: should-not-match\n";
        assert_eq!(parse_compose_project_name(yaml).as_deref(), Some("bersihir-metaphora"));
    }

    #[test]
    fn compose_project_name_quoted_and_absent() {
        assert_eq!(parse_compose_project_name("name: \"quoted-proj\"\n").as_deref(), Some("quoted-proj"));
        assert_eq!(parse_compose_project_name("services:\n  api: {}\n"), None);
        // A nested (indented) name: must not be picked up as the project name.
        assert_eq!(parse_compose_project_name("services:\n  api:\n    name: svc\n"), None);
    }

    #[test]
    fn docker_cache_safelist_excludes_data_volumes() {
        // build caches are listed; data volumes are not (never touched).
        assert!(DOCKER_CACHE_VOLUMES.contains(&"cargo_target"));
        assert!(DOCKER_CACHE_VOLUMES.contains(&"node_modules"));
        for data in ["pgdata", "miniodata", "redisdata"] {
            assert!(!DOCKER_CACHE_VOLUMES.contains(&data), "{data} must not be in the safelist");
        }
    }
}
