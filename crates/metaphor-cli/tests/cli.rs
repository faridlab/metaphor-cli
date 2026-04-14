//! End-to-end CLI tests for `metaphor`. Exercises the binary against
//! fixture workspaces — the harness that Phase B/C commands also use.
//!
//! The multi-project tests (`lint_*`) are gated on `#[cfg(unix)]` because
//! they write bash-shebang fake plugins. Porting to Windows means either
//! shipping cross-platform fake plugins (a tiny Rust helper binary) or
//! running under WSL; when we ship Windows releases we'll revisit.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

const MANIFEST: &str = r#"version: 1
projects:
  - name: domain
    type: module
    path: ./domain
  - name: api
    type: backend-service
    path: ./api
    depends_on: [domain]
  - name: web
    type: webapp
    path: ./web
    depends_on: [api, domain]
"#;

fn workspace_with(manifest: &str) -> TempDir {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("metaphor.yaml"), manifest).unwrap();
    tmp
}

fn metaphor() -> Command {
    Command::cargo_bin("metaphor").unwrap()
}

#[test]
fn init_creates_empty_manifest() {
    let tmp = TempDir::new().unwrap();
    metaphor()
        .current_dir(tmp.path())
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Initialized empty metaphor workspace",
        ));
    assert!(tmp.path().join("metaphor.yaml").exists());
}

#[test]
fn init_refuses_to_overwrite() {
    let tmp = workspace_with("version: 1\nprojects: []\n");
    metaphor()
        .current_dir(tmp.path())
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn list_prints_projects() {
    let tmp = workspace_with(MANIFEST);
    metaphor()
        .current_dir(tmp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("3 project(s):"))
        .stdout(predicate::str::contains("domain [Module]"))
        .stdout(predicate::str::contains("web [Webapp]"));
}

#[test]
fn graph_prints_tree() {
    let tmp = workspace_with(MANIFEST);
    metaphor()
        .current_dir(tmp.path())
        .arg("graph")
        .assert()
        .success()
        .stdout(predicate::str::contains("api"))
        .stdout(predicate::str::contains("└─ domain"))
        .stdout(predicate::str::contains("├─ api"));
}

#[test]
fn graph_focus_filters_subgraph() {
    let tmp = workspace_with(MANIFEST);
    metaphor()
        .current_dir(tmp.path())
        .args(["graph", "--focus", "api"])
        .assert()
        .success()
        .stdout(predicate::str::contains("api"))
        .stdout(predicate::str::contains("domain"))
        .stdout(predicate::str::contains("web").not());
}

#[test]
fn graph_json_envelope_is_stable() {
    let tmp = workspace_with(MANIFEST);
    let out = metaphor()
        .current_dir(tmp.path())
        .args(["graph", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    // Strip the banner and parse the JSON payload.
    let text = String::from_utf8(out).unwrap();
    let json_start = text.find('{').expect("no json payload in output");
    let v: serde_json::Value = serde_json::from_str(&text[json_start..]).unwrap();
    assert_eq!(v["version"], 1);
    assert!(v["data"]["nodes"].is_array());
    assert!(v["data"]["edges"].is_array());
}

#[test]
fn show_project_prints_detail() {
    let tmp = workspace_with(MANIFEST);
    metaphor()
        .current_dir(tmp.path())
        .args(["show", "project", "web"])
        .assert()
        .success()
        .stdout(predicate::str::contains("name:        web"))
        .stdout(predicate::str::contains("depends_on:  api, domain"));
}

#[test]
fn show_project_unknown_errors() {
    let tmp = workspace_with(MANIFEST);
    metaphor()
        .current_dir(tmp.path())
        .args(["show", "project", "ghost"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("'ghost' not found"));
}

#[test]
fn show_project_json_includes_resolved_path() {
    let tmp = workspace_with(MANIFEST);
    let out = metaphor()
        .current_dir(tmp.path())
        .args(["show", "project", "api", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    let json_start = text.find('{').unwrap();
    let v: serde_json::Value = serde_json::from_str(&text[json_start..]).unwrap();
    assert_eq!(v["version"], 1);
    assert_eq!(v["data"]["project"]["name"], "api");
    let resolved = v["data"]["resolved_path"].as_str().unwrap();
    assert!(
        !resolved.contains("/./"),
        "resolved_path should be normalized: {resolved}"
    );
}

#[test]
fn load_rejects_unknown_dependency() {
    let tmp = workspace_with(
        "version: 1\nprojects:\n  - name: a\n    type: module\n    path: ./a\n    depends_on: [ghost]\n",
    );
    metaphor()
        .current_dir(tmp.path())
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown project 'ghost'"));
}

// --------------------------------------------------------------------------
// Phase B: metaphor add
// --------------------------------------------------------------------------

#[test]
fn add_registers_project() {
    let tmp = workspace_with("version: 1\nprojects: []\n");
    metaphor()
        .current_dir(tmp.path())
        .args([
            "add",
            "api",
            "--project-type",
            "backend-service",
            "--path",
            "./api",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added project 'api'"));

    // Verify list sees it.
    metaphor()
        .current_dir(tmp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("api [BackendService]"));
}

#[test]
fn add_rejects_duplicate_name() {
    let tmp = workspace_with(
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n",
    );
    metaphor()
        .current_dir(tmp.path())
        .args(["add", "api", "--project-type", "module", "--path", "./api2"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate project name 'api'"));
}

#[test]
fn add_rejects_unknown_depends_on() {
    let tmp = workspace_with("version: 1\nprojects: []\n");
    metaphor()
        .current_dir(tmp.path())
        .args([
            "add",
            "api",
            "--project-type",
            "backend-service",
            "--path",
            "./api",
            "--depends-on",
            "ghost",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown project 'ghost'"));
}

// --------------------------------------------------------------------------
// Phase B: run-many (--all / --projects / --continue-on-error)
// --------------------------------------------------------------------------

/// Create a small shebang "plugin" in `dir` that echoes its cwd and returns
/// the given exit code. The script filename is `name` (matches what
/// `metaphor` looks up via `$METAPHOR_PLUGIN_BIN_DIR/<name>`).
fn write_fake_plugin(dir: &std::path::Path, name: &str, body: &str) {
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
    }
}

fn echoing_plugin() -> &'static str {
    "#!/bin/bash\necho \"pwd=$PWD args=$*\"\n"
}

fn failing_on_api_plugin() -> &'static str {
    "#!/bin/bash\necho \"pwd=$PWD\"\nif [[ \"$PWD\" == *\"/api\" ]]; then echo fail >&2; exit 1; fi\n"
}

fn workspace_with_three_projects() -> (TempDir, TempDir) {
    let tmp = workspace_with(MANIFEST);
    for p in ["domain", "api", "web"] {
        fs::create_dir_all(tmp.path().join(p)).unwrap();
    }
    let bin_dir = TempDir::new().unwrap();
    (tmp, bin_dir)
}

#[cfg(unix)]
#[test]
fn lint_all_fans_out_in_topo_order() {
    let (tmp, bin_dir) = workspace_with_three_projects();
    write_fake_plugin(bin_dir.path(), "metaphor-dev", echoing_plugin());
    let out = metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .args(["lint", "--all"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    let d = text.find("== domain ==").unwrap();
    let a = text.find("== api ==").unwrap();
    let w = text.find("== web ==").unwrap();
    assert!(d < a && a < w, "topological order violated:\n{text}");
}

#[cfg(unix)]
#[test]
fn lint_projects_filters_subset() {
    let (tmp, bin_dir) = workspace_with_three_projects();
    write_fake_plugin(bin_dir.path(), "metaphor-dev", echoing_plugin());
    metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .args(["lint", "--projects=api,web"])
        .assert()
        .success()
        .stdout(predicate::str::contains("== api ==").and(predicate::str::contains("== web ==")))
        .stdout(predicate::str::contains("== domain ==").not());
}

#[cfg(unix)]
#[test]
fn lint_continue_on_error_runs_everyone() {
    let (tmp, bin_dir) = workspace_with_three_projects();
    write_fake_plugin(bin_dir.path(), "metaphor-dev", failing_on_api_plugin());
    let out = metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .args(["lint", "--all", "--continue-on-error"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    // All three projects ran even though api failed.
    assert!(stdout.contains("== domain =="));
    assert!(stdout.contains("== api =="));
    assert!(stdout.contains("== web =="));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("failed in 1 project(s): api"));
}

#[cfg(unix)]
#[test]
fn lint_parallel_runs_all_projects() {
    // With N>1 workers we lose sequential guarantees but every project
    // must still run exactly once and succeed.
    let (tmp, bin_dir) = workspace_with_three_projects();
    write_fake_plugin(bin_dir.path(), "metaphor-dev", echoing_plugin());
    let out = metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .args(["lint", "--all", "--parallel=3"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    for name in ["domain", "api", "web"] {
        assert!(
            text.contains(&format!("== {name} ==")),
            "missing {name} in parallel run:\n{text}"
        );
    }
}

#[cfg(unix)]
#[test]
fn parallel_without_selector_errors() {
    let (tmp, _bin_dir) = workspace_with_three_projects();
    metaphor()
        .current_dir(tmp.path())
        .args(["lint", "--parallel=4"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--parallel requires one of --all"));
}

#[cfg(unix)]
#[test]
fn affected_selects_changed_project_and_dependents() {
    use std::process::Command as StdCommand;

    let (tmp, bin_dir) = workspace_with_three_projects();
    write_fake_plugin(bin_dir.path(), "metaphor-dev", echoing_plugin());

    // Seed git repo with an initial commit.
    let run_git = |args: &[&str]| {
        let status = StdCommand::new("git")
            .args(args)
            .current_dir(tmp.path())
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    };
    run_git(&["init", "-q"]);
    run_git(&["add", "-A"]);
    run_git(&["commit", "-q", "-m", "init"]);

    // Mutate a file under api/ and commit, so `git diff HEAD~1..HEAD`
    // reports the change.
    fs::write(tmp.path().join("api").join("new.txt"), "x").unwrap();
    run_git(&["add", "-A"]);
    run_git(&["commit", "-q", "-m", "touch api"]);

    let out = metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .args(["lint", "--affected", "--base=HEAD~1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    // api changed → api runs. web depends on api → web runs.
    // domain is neither changed nor a dependent → must NOT run.
    assert!(text.contains("== api =="), "api missing:\n{text}");
    assert!(text.contains("== web =="), "web missing:\n{text}");
    assert!(
        !text.contains("== domain =="),
        "domain should not be affected:\n{text}"
    );
}

// --------------------------------------------------------------------------
// Phase C: metaphor plugins + task cache
// --------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn plugins_shows_installed_and_missing() {
    let tmp = TempDir::new().unwrap();
    let bin_dir = TempDir::new().unwrap();
    write_fake_plugin(
        bin_dir.path(),
        "metaphor-dev",
        "#!/bin/bash\n[ \"$1\" = \"--version\" ] && echo \"metaphor-dev 9.9.9\"\n",
    );
    // Intentionally do NOT install metaphor-schema or metaphor-codegen.
    metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .env("PATH", bin_dir.path()) // keep discovery sandboxed
        .arg("plugins")
        .assert()
        .success()
        .stdout(predicate::str::contains("✓ metaphor-dev"))
        .stdout(predicate::str::contains("metaphor-dev 9.9.9"))
        .stdout(predicate::str::contains("✗ metaphor-schema"))
        .stdout(predicate::str::contains("(not installed)"));
}

#[cfg(unix)]
#[test]
fn lint_second_run_is_cached() {
    let (tmp, bin_dir) = workspace_with_three_projects();
    // Plugin writes a unique marker each invocation so we can tell a real
    // run apart from a cache replay.
    write_fake_plugin(
        bin_dir.path(),
        "metaphor-dev",
        "#!/bin/bash\n[ \"$1\" = \"--version\" ] && echo v1 && exit 0\necho \"ran-$$\"\n",
    );

    let first = metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .args(["lint", "--projects=domain"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let first = String::from_utf8(first).unwrap();
    assert!(first.contains("ran-"));
    assert!(!first.contains("(cached)"));

    let second = metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .args(["lint", "--projects=domain"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let second = String::from_utf8(second).unwrap();
    assert!(second.contains("(cached)"), "expected cache hit:\n{second}");
    // stdout replayed from the cache must match the first run's body.
    let first_marker = first
        .lines()
        .find(|l| l.starts_with("ran-"))
        .expect("first run marker");
    assert!(second.contains(first_marker), "replay mismatch");
}

#[cfg(unix)]
#[test]
fn no_cache_bypasses_reads_and_writes() {
    let (tmp, bin_dir) = workspace_with_three_projects();
    write_fake_plugin(
        bin_dir.path(),
        "metaphor-dev",
        "#!/bin/bash\n[ \"$1\" = \"--version\" ] && echo v1 && exit 0\necho \"ran-$$\"\n",
    );

    // Warm the cache.
    metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .args(["lint", "--projects=domain"])
        .assert()
        .success();

    // --no-cache must re-run and NOT print "(cached)".
    metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .args(["lint", "--projects=domain", "--no-cache"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(cached)").not());
}

#[cfg(unix)]
#[test]
fn failed_run_is_not_cached() {
    let (tmp, bin_dir) = workspace_with_three_projects();
    write_fake_plugin(
        bin_dir.path(),
        "metaphor-dev",
        "#!/bin/bash\n[ \"$1\" = \"--version\" ] && echo v1 && exit 0\necho boom >&2\nexit 1\n",
    );

    for _ in 0..2 {
        metaphor()
            .current_dir(tmp.path())
            .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
            .args(["lint", "--projects=domain"])
            .assert()
            .failure()
            .stdout(predicate::str::contains("(cached)").not());
    }
}

#[cfg(unix)]
#[test]
fn no_cache_requires_selector() {
    let (tmp, _bin) = workspace_with_three_projects();
    metaphor()
        .current_dir(tmp.path())
        .args(["lint", "--no-cache"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--no-cache requires one of --all"));
}

#[cfg(unix)]
#[test]
fn plugin_version_change_busts_cache() {
    let (tmp, bin_dir) = workspace_with_three_projects();

    // v1: prints "ran-v1".
    write_fake_plugin(
        bin_dir.path(),
        "metaphor-dev",
        "#!/bin/bash\n[ \"$1\" = \"--version\" ] && echo v1 && exit 0\necho ran-v1\n",
    );
    metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .args(["lint", "--projects=domain"])
        .assert()
        .success();

    // Swap plugin for v2 (same contents except --version output and marker).
    write_fake_plugin(
        bin_dir.path(),
        "metaphor-dev",
        "#!/bin/bash\n[ \"$1\" = \"--version\" ] && echo v2 && exit 0\necho ran-v2\n",
    );
    let out = metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .args(["lint", "--projects=domain"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(
        !text.contains("(cached)"),
        "version bump should bust the cache:\n{text}"
    );
    assert!(text.contains("ran-v2"), "expected v2 marker:\n{text}");
}

#[cfg(unix)]
#[test]
fn cache_works_with_parallel() {
    // Warm cache sequentially, then invoke with --parallel=3 and expect
    // every project to replay from cache.
    let (tmp, bin_dir) = workspace_with_three_projects();
    write_fake_plugin(
        bin_dir.path(),
        "metaphor-dev",
        "#!/bin/bash\n[ \"$1\" = \"--version\" ] && echo v1 && exit 0\necho ran\n",
    );
    metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .args(["lint", "--all"])
        .assert()
        .success();

    let out = metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .args(["lint", "--all", "--parallel=3"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    for name in ["domain", "api", "web"] {
        assert!(
            text.contains(&format!("== {name} == (cached)")),
            "expected cache hit for {name} under --parallel:\n{text}"
        );
    }
}

#[cfg(unix)]
#[test]
fn cache_stats_and_clear() {
    let (tmp, bin_dir) = workspace_with_three_projects();
    write_fake_plugin(bin_dir.path(), "metaphor-dev", echoing_plugin());

    // Warm the cache with one entry.
    metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .args(["lint", "--projects=domain"])
        .assert()
        .success();

    metaphor()
        .current_dir(tmp.path())
        .args(["cache", "stats"])
        .assert()
        .success()
        .stdout(predicate::str::contains("entries: 1"));

    metaphor()
        .current_dir(tmp.path())
        .args(["cache", "clear"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleared 1 entries"));

    metaphor()
        .current_dir(tmp.path())
        .args(["cache", "stats"])
        .assert()
        .success()
        .stdout(predicate::str::contains("entries: 0"));
}

#[cfg(unix)]
#[test]
fn lint_fail_fast_stops_after_first_failure() {
    let (tmp, bin_dir) = workspace_with_three_projects();
    write_fake_plugin(bin_dir.path(), "metaphor-dev", failing_on_api_plugin());
    let out = metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .args(["lint", "--all"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    // web should NOT have run.
    assert!(stdout.contains("== domain =="));
    assert!(stdout.contains("== api =="));
    assert!(
        !stdout.contains("== web =="),
        "fail-fast violated:\n{stdout}"
    );
}
