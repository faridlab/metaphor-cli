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
fn plugins_finds_default_install_dir() {
    // Simulates the state right after `metaphor plugin add` on a fresh
    // install: the binary lives in ~/.metaphor/bin, but the user hasn't
    // edited $PATH and hasn't set $METAPHOR_PLUGIN_BIN_DIR. The resolver
    // must still find it via the default-dir fallback.
    let tmp = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let default_bin = home.path().join(".metaphor/bin");
    std::fs::create_dir_all(&default_bin).unwrap();
    write_fake_plugin(
        &default_bin,
        "metaphor-dev",
        "#!/bin/bash\n[ \"$1\" = \"--version\" ] && echo \"metaphor-dev 9.9.9\"\n",
    );
    metaphor()
        .current_dir(tmp.path())
        .env("HOME", home.path())
        .env("PATH", "/usr/bin:/bin")
        .env_remove("METAPHOR_PLUGIN_BIN_DIR")
        .arg("plugins")
        .assert()
        .success()
        .stdout(predicate::str::contains("✓ metaphor-dev"))
        .stdout(predicate::str::contains("metaphor-dev 9.9.9"));
}

#[cfg(unix)]
#[test]
fn dispatch_finds_plugin_in_default_install_dir() {
    // The counterpart to `plugins_finds_default_install_dir`: the dispatch
    // path (`metaphor dev …`) must also fall back to ~/.metaphor/bin when
    // neither $METAPHOR_PLUGIN_BIN_DIR nor $PATH points there. Regression
    // test for the v0.1.5 bug where `plugins` showed ✓ but `dev` still
    // failed with "failed to spawn metaphor-dev".
    let tmp = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let default_bin = home.path().join(".metaphor/bin");
    std::fs::create_dir_all(&default_bin).unwrap();
    write_fake_plugin(
        &default_bin,
        "metaphor-dev",
        "#!/bin/bash\necho \"dev-ran args=$*\"\n",
    );
    metaphor()
        .current_dir(tmp.path())
        .env("HOME", home.path())
        .env("PATH", "/usr/bin:/bin")
        .env_remove("METAPHOR_PLUGIN_BIN_DIR")
        .args(["dev", "hello"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dev-ran args=dev hello"));
}

#[cfg(unix)]
#[test]
fn plugin_list_matches_plugins_alias() {
    let tmp = TempDir::new().unwrap();
    let bin_dir = TempDir::new().unwrap();
    write_fake_plugin(
        bin_dir.path(),
        "metaphor-dev",
        "#!/bin/bash\n[ \"$1\" = \"--version\" ] && echo \"metaphor-dev 9.9.9\"\n",
    );
    metaphor()
        .current_dir(tmp.path())
        .env("METAPHOR_PLUGIN_BIN_DIR", bin_dir.path())
        .env("PATH", bin_dir.path())
        .args(["plugin", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("✓ metaphor-dev"))
        .stdout(predicate::str::contains("metaphor-dev 9.9.9"));
}

#[test]
fn plugin_add_rejects_unknown_plugin() {
    let tmp = TempDir::new().unwrap();
    metaphor()
        .current_dir(tmp.path())
        .args(["plugin", "add", "metaphor-bogus"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown plugin 'metaphor-bogus'"))
        .stderr(predicate::str::contains("metaphor-dev"));
}

#[test]
fn plugin_add_rejects_malformed_spec() {
    let tmp = TempDir::new().unwrap();
    metaphor()
        .current_dir(tmp.path())
        .args(["plugin", "add", "metaphor-dev@"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid plugin spec"));
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

// --------------------------------------------------------------------------
// Phase D: build / compose generate / env check / deploy
// --------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn build_requires_a_selector() {
    let tmp = workspace_with(MANIFEST);
    metaphor()
        .current_dir(tmp.path())
        .args(["build"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "requires one of --all, --projects, or --affected",
        ));
}

#[cfg(unix)]
#[test]
fn build_dry_run_prints_docker_command() {
    let tmp = workspace_with(MANIFEST);
    for p in ["domain", "api", "web"] {
        fs::create_dir_all(tmp.path().join(p)).unwrap();
        fs::write(tmp.path().join(p).join("Dockerfile"), "FROM alpine\n").unwrap();
    }
    let out = metaphor()
        .current_dir(tmp.path())
        .args(["build", "--all", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("would run: docker build"));
    assert!(text.contains("-t domain:"));
    assert!(text.contains("-t api:"));
    assert!(text.contains("-t web:"));
}

#[cfg(unix)]
#[test]
fn build_custom_tag_template() {
    let tmp = workspace_with(MANIFEST);
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    fs::write(tmp.path().join("api").join("Dockerfile"), "FROM alpine\n").unwrap();
    metaphor()
        .current_dir(tmp.path())
        .args([
            "build",
            "--projects=api",
            "--dry-run",
            "--tag=registry.example.com/{name}:latest",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "-t registry.example.com/api:latest",
        ));
}

#[test]
fn compose_generate_dry_run_emits_services_block() {
    let tmp = workspace_with(
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n",
    );
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    fs::write(
        tmp.path().join("api").join("compose.fragment.yml"),
        "image: example/api:dev\nports: ['8080:8080']\n",
    )
    .unwrap();
    metaphor()
        .current_dir(tmp.path())
        .args(["compose", "generate"])
        .assert()
        .success()
        .stdout(predicate::str::contains("services:"))
        .stdout(predicate::str::contains("api:"))
        .stdout(predicate::str::contains("example/api:dev"))
        .stdout(predicate::str::contains("(dry run"));
    assert!(
        !tmp.path().join("docker-compose.yml").exists(),
        "dry-run must not write the file"
    );
}

#[test]
fn compose_generate_write_flag_writes_file() {
    let tmp = workspace_with(
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n",
    );
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    fs::write(
        tmp.path().join("api").join("compose.fragment.yml"),
        "image: example/api:dev\n",
    )
    .unwrap();
    metaphor()
        .current_dir(tmp.path())
        .args(["compose", "generate", "--write"])
        .assert()
        .success();
    let out = fs::read_to_string(tmp.path().join("docker-compose.yml")).unwrap();
    assert!(out.contains("services:"));
    assert!(out.contains("api:"));
    assert!(out.contains("example/api:dev"));
}

#[test]
fn env_check_missing_required_var_exits_nonzero() {
    let tmp = workspace_with(
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n",
    );
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    fs::write(
        tmp.path().join("api").join("metaphor.env.yaml"),
        "env:\n  - name: PLEASE_DO_NOT_SET_THIS_VAR_ANYWHERE\n    required: true\n",
    )
    .unwrap();
    metaphor()
        .current_dir(tmp.path())
        .args(["env", "check"])
        .env_remove("PLEASE_DO_NOT_SET_THIS_VAR_ANYWHERE")
        .assert()
        .failure()
        .stdout(predicate::str::contains("MISS"))
        .stderr(predicate::str::contains("missing"));
}

#[test]
fn env_check_workspace_dotenv_satisfies_required() {
    let tmp = workspace_with(
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n",
    );
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    fs::write(
        tmp.path().join("api").join("metaphor.env.yaml"),
        "env:\n  - name: FOO_FROM_DOTENV\n    required: true\n",
    )
    .unwrap();
    fs::write(tmp.path().join(".env"), "FOO_FROM_DOTENV=present\n").unwrap();
    metaphor()
        .current_dir(tmp.path())
        .args(["env", "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("OK"))
        .stdout(predicate::str::contains("workspace .env"));
}

// `deploy` is now a passthrough to metaphor-dev; the infra-script behavior
// lives in `metaphor-dev deploy exec` and is unit-tested in the plugin.

#[cfg(unix)]
#[test]
fn build_dry_run_shows_push_steps() {
    let tmp = workspace_with(MANIFEST);
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    fs::write(tmp.path().join("api").join("Dockerfile"), "FROM alpine\n").unwrap();
    metaphor()
        .current_dir(tmp.path())
        .args([
            "build",
            "--projects=api",
            "--dry-run",
            "--push",
            "--tag=registry.example.com/api:latest",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "would run: docker push registry.example.com/api:latest",
        ));
}

#[cfg(unix)]
#[test]
fn build_parallel_all_projects() {
    let tmp = workspace_with(MANIFEST);
    for p in ["domain", "api", "web"] {
        fs::create_dir_all(tmp.path().join(p)).unwrap();
        fs::write(tmp.path().join(p).join("Dockerfile"), "FROM alpine\n").unwrap();
    }
    let out = metaphor()
        .current_dir(tmp.path())
        .args(["build", "--all", "--dry-run", "--parallel=3"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    for name in ["domain", "api", "web"] {
        assert!(
            text.contains(&format!("== {name} ==")),
            "expected all three projects under --parallel:\n{text}"
        );
    }
}

#[cfg(unix)]
#[test]
fn build_honors_metaphor_build_yaml_override() {
    let tmp = workspace_with(MANIFEST);
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    fs::write(
        tmp.path().join("api").join("Dockerfile.api"),
        "FROM alpine\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("api").join("metaphor.build.yaml"),
        "dockerfile: Dockerfile.api\ntags:\n  - acme/{name}:latest\n",
    )
    .unwrap();
    let out = metaphor()
        .current_dir(tmp.path())
        .args(["build", "--projects=api", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains("Dockerfile.api"),
        "override not used:\n{text}"
    );
    assert!(text.contains("acme/api:latest"));
}

#[test]
fn env_check_json_envelope() {
    let tmp = workspace_with(
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n",
    );
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    fs::write(
        tmp.path().join("api").join("metaphor.env.yaml"),
        "env:\n  - name: PRESENT_VIA_DEFAULT\n    required: true\n    default: x\n  - name: A_SECRET\n    required: true\n    secret: true\n    default: hidden\n",
    )
    .unwrap();
    let out = metaphor()
        .current_dir(tmp.path())
        .args(["env", "check", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    let start = text.find('{').unwrap();
    let v: serde_json::Value = serde_json::from_str(&text[start..]).unwrap();
    assert_eq!(v["version"], 1);
    assert_eq!(v["data"]["missing_count"], 0);
    assert!(v["data"]["reports"][0]["present"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "A_SECRET" && p["secret"] == true));
}

#[test]
fn env_check_per_project_dotenv_wins_over_workspace() {
    let tmp = workspace_with(
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n",
    );
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    fs::write(
        tmp.path().join("api").join("metaphor.env.yaml"),
        "env:\n  - name: SHARED_VAR\n    required: true\n",
    )
    .unwrap();
    fs::write(tmp.path().join(".env"), "SHARED_VAR=from-workspace\n").unwrap();
    fs::write(
        tmp.path().join("api").join(".env"),
        "SHARED_VAR=from-project\n",
    )
    .unwrap();
    metaphor()
        .current_dir(tmp.path())
        .args(["env", "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("project .env"));
}

#[test]
fn compose_generate_preserves_fragment_volumes_and_env() {
    let tmp = workspace_with(
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n",
    );
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    fs::write(
        tmp.path().join("api").join("compose.fragment.yml"),
        r#"image: example/api:dev
volumes:
  - ./src:/app/src:ro
environment:
  DATABASE_URL: postgres://localhost/db
  LOG_LEVEL: debug
ports:
  - "8080:8080"
"#,
    )
    .unwrap();
    metaphor()
        .current_dir(tmp.path())
        .args(["compose", "generate", "--write"])
        .assert()
        .success();
    let body = fs::read_to_string(tmp.path().join("docker-compose.yml")).unwrap();
    assert!(body.contains("./src:/app/src:ro"));
    assert!(body.contains("DATABASE_URL"));
    assert!(body.contains("LOG_LEVEL"));
    assert!(body.contains("8080:8080"));
    // Must still round-trip valid YAML.
    let _: serde_yaml::Value = serde_yaml::from_str(&body).expect("output is valid YAML");
}

// Removed deploy_* tests: the native `metaphor deploy` infra-script wrapper
// is now `metaphor deploy exec` in metaphor-plugin-dev, where the picker
// logic is covered by unit tests on `pick_infra`.

// --------------------------------------------------------------------------
// Phase D+: cwd-aware current-project detection (metaphor info + show)
// --------------------------------------------------------------------------

#[test]
fn info_from_inside_a_project() {
    let tmp = workspace_with(MANIFEST);
    fs::create_dir_all(tmp.path().join("api/src/handlers")).unwrap();
    metaphor()
        .current_dir(tmp.path().join("api/src/handlers"))
        .arg("info")
        .assert()
        .success()
        .stdout(predicate::str::contains("current project: api"))
        .stdout(predicate::str::contains("depended-by: web"));
}

#[test]
fn info_outside_any_project_says_so() {
    let tmp = workspace_with(MANIFEST);
    // cwd is the workspace root — not inside any project.
    metaphor()
        .current_dir(tmp.path())
        .arg("info")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "current project: (not inside any registered project)",
        ))
        .stdout(predicate::str::contains("projects: 3 registered"));
}

#[test]
fn show_project_defaults_to_current_project() {
    let tmp = workspace_with(MANIFEST);
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    metaphor()
        .current_dir(tmp.path().join("api"))
        .args(["show", "project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("name:        api"));
}

#[test]
fn show_project_errors_when_not_inside_project() {
    let tmp = workspace_with(MANIFEST);
    // Workspace root isn't inside any project, and no name given.
    metaphor()
        .current_dir(tmp.path())
        .args(["show", "project"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not inside a registered project"));
}

#[test]
fn show_project_from_subdir_reports_absolute_path() {
    // Locks in the silent fix: cmd_show now uses find_and_load's workspace
    // root (not cwd) when computing resolved_path, so running show from a
    // subdirectory still yields the correct absolute path.
    let tmp = workspace_with(MANIFEST);
    fs::create_dir_all(tmp.path().join("api/src")).unwrap();
    let out = metaphor()
        .current_dir(tmp.path().join("api/src"))
        .args(["show", "project", "api"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    // The resolved path must start with the tmp workspace root and point at
    // the project's own dir — not at cwd (which is api/src/).
    let expected = tmp
        .path()
        .join("api")
        .canonicalize()
        .unwrap_or_else(|_| tmp.path().join("api"));
    assert!(
        text.contains(&format!("resolved:    {}", expected.display()))
            || text.contains(&format!(
                "resolved:    {}",
                tmp.path().join("api").display()
            )),
        "expected resolved to point at the project dir, got:\n{text}"
    );
}

#[test]
fn info_json_envelope() {
    let tmp = workspace_with(MANIFEST);
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    let out = metaphor()
        .current_dir(tmp.path().join("api"))
        .args(["info", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    let start = text.find('{').unwrap();
    let v: serde_json::Value = serde_json::from_str(&text[start..]).unwrap();
    assert_eq!(v["version"], 1);
    assert_eq!(v["data"]["current_project"]["name"], "api");
    assert_eq!(v["data"]["projects_registered"], 3);
    let depended_by = v["data"]["current_project"]["depended_by"]
        .as_array()
        .unwrap();
    assert!(depended_by.iter().any(|n| n == "web"));
}

// --------------------------------------------------------------------------
// metaphor repl
// --------------------------------------------------------------------------

#[test]
fn repl_executes_list_then_exits() {
    let tmp = workspace_with(MANIFEST);
    for p in ["domain", "api", "web"] {
        fs::create_dir_all(tmp.path().join(p)).unwrap();
    }
    metaphor()
        .current_dir(tmp.path())
        .arg("repl")
        .write_stdin("list\nexit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("interactive mode"))
        .stdout(predicate::str::contains("3 project(s):"))
        .stdout(predicate::str::contains("domain"))
        .stdout(predicate::str::contains("bye."));
}

#[test]
fn repl_help_lists_subcommands_and_builtins() {
    let tmp = workspace_with(MANIFEST);
    metaphor()
        .current_dir(tmp.path())
        .arg("repl")
        .write_stdin("help\nexit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Built-in:"))
        .stdout(predicate::str::contains("Subcommands:"))
        .stdout(predicate::str::contains("doctor"))
        .stdout(predicate::str::contains("info"));
}

#[test]
fn repl_recovers_from_bad_command_and_continues() {
    let tmp = workspace_with(MANIFEST);
    for p in ["domain", "api", "web"] {
        fs::create_dir_all(tmp.path().join(p)).unwrap();
    }
    // A bogus subcommand should NOT kill the loop — the next command must run.
    metaphor()
        .current_dir(tmp.path())
        .arg("repl")
        .write_stdin("this-is-not-a-command\nlist\nexit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("3 project(s):"));
}

#[test]
fn repl_respects_shell_quoting() {
    // `show project api` with api quoted should parse the same as unquoted.
    let tmp = workspace_with(MANIFEST);
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    metaphor()
        .current_dir(tmp.path())
        .arg("repl")
        .write_stdin("show project \"api\"\nexit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("name:        api"));
}

#[test]
fn repl_refuses_nested_repl() {
    let tmp = workspace_with(MANIFEST);
    metaphor()
        .current_dir(tmp.path())
        .arg("repl")
        .write_stdin("repl\nexit\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("already in a repl"));
}

// --------------------------------------------------------------------------
// metaphor doctor
// --------------------------------------------------------------------------

#[test]
fn doctor_clean_workspace_succeeds() {
    // A workspace with every project on disk and no Dockerfiles: all checks
    // should pass or warn (plugins are likely absent). Exit must be 0.
    let tmp = workspace_with(MANIFEST);
    for p in ["domain", "api", "web"] {
        fs::create_dir_all(tmp.path().join(p)).unwrap();
    }
    metaphor()
        .current_dir(tmp.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("manifest valid (3 project(s))"));
}

#[test]
fn doctor_fails_on_missing_project_dir() {
    let tmp = workspace_with(MANIFEST);
    // Intentionally do NOT create `web/` — it's in the manifest but missing.
    fs::create_dir_all(tmp.path().join("domain")).unwrap();
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    metaphor()
        .current_dir(tmp.path())
        .arg("doctor")
        .assert()
        .failure()
        .stdout(predicate::str::contains("web: directory missing"))
        .stderr(predicate::str::contains("check(s) failed"));
}

#[test]
fn doctor_warns_on_bad_env_yaml() {
    let tmp = workspace_with(
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n",
    );
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    fs::write(
        tmp.path().join("api/metaphor.env.yaml"),
        "this is: not: valid: yaml\n",
    )
    .unwrap();
    metaphor()
        .current_dir(tmp.path())
        .arg("doctor")
        .assert()
        .success() // WARN, not FAIL
        .stdout(predicate::str::contains("invalid YAML"));
}

#[test]
fn doctor_warns_on_missing_dockerignore() {
    let tmp = workspace_with(
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n",
    );
    fs::create_dir_all(tmp.path().join("api")).unwrap();
    fs::write(tmp.path().join("api/Dockerfile"), "FROM alpine\n").unwrap();
    metaphor()
        .current_dir(tmp.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("api: missing .dockerignore"));
}

#[test]
fn doctor_json_envelope() {
    let tmp = workspace_with(MANIFEST);
    for p in ["domain", "api", "web"] {
        fs::create_dir_all(tmp.path().join(p)).unwrap();
    }
    let out = metaphor()
        .current_dir(tmp.path())
        .args(["doctor", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    let start = text.find('{').unwrap();
    let v: serde_json::Value = serde_json::from_str(&text[start..]).unwrap();
    assert_eq!(v["version"], 1);
    assert!(v["data"]["checks"].is_array());
    assert!(v["data"]["summary"]["ok"].as_u64().unwrap() >= 1);
    // Every check has the expected keys.
    for c in v["data"]["checks"].as_array().unwrap() {
        assert!(c["category"].is_string());
        assert!(c["name"].is_string());
        assert!(matches!(
            c["status"].as_str(),
            Some("ok") | Some("warn") | Some("fail")
        ));
    }
}

// --------------------------------------------------------------------------
// Phase C+: metaphor clean (stale build-artifact pruning)
// --------------------------------------------------------------------------

#[cfg(unix)]
fn backdate(path: &std::path::Path, days_ago: u64) {
    use std::os::unix::fs::MetadataExt;
    use std::process::Command as StdCommand;
    let _ = std::fs::metadata(path).unwrap().mtime();
    // Portable enough: `touch -d @<unix_seconds>` works on GNU coreutils,
    // `touch -t YYYYMMDDHHMM` on BSD. Use -t with a computed stamp.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - days_ago * 86_400;
    // Format YYYYMMDDHHMM.ss in local time.
    let tm = chrono::DateTime::<chrono::Local>::from(
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs),
    )
    .format("%Y%m%d%H%M.%S")
    .to_string();
    let ok = StdCommand::new("touch")
        .args(["-t", &tm])
        .arg(path)
        .status()
        .unwrap()
        .success();
    assert!(ok, "failed to backdate {}", path.display());
}

#[cfg(unix)]
#[test]
fn clean_dry_run_reports_stale_dirs_and_preserves_recent() {
    let tmp = TempDir::new().unwrap();
    // Two projects: api (target stale), web (node_modules stale, dist recent).
    fs::create_dir_all(tmp.path().join("api/target/obj")).unwrap();
    fs::create_dir_all(tmp.path().join("web/node_modules/pkg")).unwrap();
    fs::create_dir_all(tmp.path().join("web/dist")).unwrap();
    fs::write(tmp.path().join("api/target/obj/a"), b"1234").unwrap();
    fs::write(tmp.path().join("web/node_modules/pkg/b"), b"5678").unwrap();
    fs::write(tmp.path().join("web/dist/bundle.js"), b"9").unwrap();

    fs::write(
        tmp.path().join("metaphor.yaml"),
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n  - name: web\n    type: webapp\n    path: ./web\n",
    )
    .unwrap();

    backdate(&tmp.path().join("api/target"), 60);
    backdate(&tmp.path().join("web/node_modules"), 60);
    // web/dist: leave mtime recent — must not appear.

    let out = metaphor()
        .current_dir(tmp.path())
        .arg("clean")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();

    assert!(text.contains("api/target"));
    assert!(text.contains("web/node_modules"));
    assert!(
        !text.contains("web/dist"),
        "dist is recent, should not be listed:\n{text}"
    );
    assert!(text.contains("Dry run"));
    // Nothing actually deleted.
    assert!(tmp.path().join("api/target/obj/a").exists());
    assert!(tmp.path().join("web/node_modules/pkg/b").exists());
}

#[cfg(unix)]
#[test]
fn clean_apply_removes_stale_dirs() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("api/target/obj")).unwrap();
    fs::write(tmp.path().join("api/target/obj/a"), b"bytes").unwrap();
    fs::write(
        tmp.path().join("metaphor.yaml"),
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n",
    )
    .unwrap();
    backdate(&tmp.path().join("api/target"), 60);

    metaphor()
        .current_dir(tmp.path())
        .args(["clean", "--apply"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Deleted 1 directory"));

    assert!(!tmp.path().join("api/target").exists());
}

#[cfg(unix)]
#[test]
fn clean_older_than_flag_filters_by_age() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("api/target/obj")).unwrap();
    fs::write(tmp.path().join("api/target/obj/a"), b"x").unwrap();
    fs::write(
        tmp.path().join("metaphor.yaml"),
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n",
    )
    .unwrap();
    // Backdate only 10 days — outside default 30d, inside --older-than=7d.
    backdate(&tmp.path().join("api/target"), 10);

    // Default cutoff 30d: should NOT be selected.
    metaphor()
        .current_dir(tmp.path())
        .arg("clean")
        .assert()
        .success()
        .stdout(predicate::str::contains("No stale build artifacts"));

    // --older-than=7d: IS selected.
    metaphor()
        .current_dir(tmp.path())
        .args(["clean", "--older-than=7d"])
        .assert()
        .success()
        .stdout(predicate::str::contains("api/target"));
}

#[cfg(unix)]
#[test]
fn clean_safelist_is_type_gated() {
    // A `crate` project with a `node_modules/` beside its `target/` must not
    // have node_modules deleted — it's not in the Crate safelist.
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("lib/target/obj")).unwrap();
    fs::create_dir_all(tmp.path().join("lib/node_modules/pkg")).unwrap();
    fs::write(tmp.path().join("lib/target/obj/a"), b"1").unwrap();
    fs::write(tmp.path().join("lib/node_modules/pkg/b"), b"2").unwrap();
    fs::write(
        tmp.path().join("metaphor.yaml"),
        "version: 1\nprojects:\n  - name: lib\n    type: crate\n    path: ./lib\n",
    )
    .unwrap();
    backdate(&tmp.path().join("lib/target"), 60);
    backdate(&tmp.path().join("lib/node_modules"), 60);

    metaphor()
        .current_dir(tmp.path())
        .args(["clean", "--apply"])
        .assert()
        .success();
    assert!(
        !tmp.path().join("lib/target").exists(),
        "target should have been removed"
    );
    assert!(
        tmp.path().join("lib/node_modules/pkg/b").exists(),
        "node_modules must NOT be touched on a crate project"
    );
}

#[cfg(unix)]
#[test]
fn clean_json_envelope_is_stable() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("api/target")).unwrap();
    fs::write(tmp.path().join("api/target/x"), b"z").unwrap();
    fs::write(
        tmp.path().join("metaphor.yaml"),
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n",
    )
    .unwrap();
    backdate(&tmp.path().join("api/target"), 60);

    let out = metaphor()
        .current_dir(tmp.path())
        .args(["clean", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    let start = text.find('{').expect("no json payload");
    let v: serde_json::Value = serde_json::from_str(&text[start..]).unwrap();
    assert_eq!(v["version"], 1);
    assert_eq!(v["data"]["dry_run"], true);
    assert!(v["data"]["candidates"].is_array());
    assert!(v["data"]["total_bytes"].as_u64().is_some());
    assert_eq!(
        v["data"]["candidates"][0]["project"], "api",
        "expected api in candidates:\n{text}"
    );
}

#[cfg(unix)]
#[test]
fn clean_confirm_over_blocks_large_deletes_without_yes() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("api/target")).unwrap();
    fs::write(tmp.path().join("api/target/x"), b"abcdefghij").unwrap(); // 10 bytes
    fs::write(
        tmp.path().join("metaphor.yaml"),
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n",
    )
    .unwrap();
    backdate(&tmp.path().join("api/target"), 60);

    // Threshold below actual size, no --yes → bail.
    metaphor()
        .current_dir(tmp.path())
        .args(["clean", "--apply", "--confirm-over=1B"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("refusing to delete"));

    // Same command with --yes → succeeds.
    metaphor()
        .current_dir(tmp.path())
        .args(["clean", "--apply", "--confirm-over=1B", "--yes"])
        .assert()
        .success();
    assert!(!tmp.path().join("api/target").exists());
}

#[cfg(unix)]
#[test]
fn clean_older_than_rejects_tiny_values() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("metaphor.yaml"),
        "version: 1\nprojects: []\n",
    )
    .unwrap();
    metaphor()
        .current_dir(tmp.path())
        .args(["clean", "--older-than=0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("too small"));
}

#[cfg(unix)]
#[test]
fn clean_apply_error_path_is_surfaced() {
    // Make a target dir unreadable/unwritable so remove_dir_all fails on it.
    // The command should still attempt every candidate and report the error.
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("api/target/locked")).unwrap();
    fs::write(tmp.path().join("api/target/locked/x"), b"n").unwrap();
    fs::write(
        tmp.path().join("metaphor.yaml"),
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n",
    )
    .unwrap();
    backdate(&tmp.path().join("api/target"), 60);

    // Remove write permission on the parent so the nested remove fails.
    let parent = tmp.path().join("api/target/locked");
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o500)).unwrap();

    let result = metaphor()
        .current_dir(tmp.path())
        .args(["clean", "--apply"])
        .assert()
        .failure();
    let out = result.get_output().clone();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("failed"),
        "expected failure report in stderr:\n{stderr}"
    );

    // Restore permissions so TempDir cleanup works.
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).ok();
}

#[cfg(unix)]
#[test]
fn clean_projects_filter_limits_scope() {
    let tmp = TempDir::new().unwrap();
    for p in ["api", "web"] {
        fs::create_dir_all(tmp.path().join(format!("{p}/target"))).unwrap();
        fs::write(tmp.path().join(format!("{p}/target/x")), b"y").unwrap();
        backdate(&tmp.path().join(format!("{p}/target")), 60);
    }
    // Both web/node_modules stale too.
    fs::create_dir_all(tmp.path().join("web/node_modules")).unwrap();
    backdate(&tmp.path().join("web/node_modules"), 60);

    fs::write(
        tmp.path().join("metaphor.yaml"),
        "version: 1\nprojects:\n  - name: api\n    type: backend-service\n    path: ./api\n  - name: web\n    type: webapp\n    path: ./web\n",
    )
    .unwrap();

    let out = metaphor()
        .current_dir(tmp.path())
        .args(["clean", "--projects=api"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("api/target"));
    assert!(
        !text.contains("web/"),
        "web should be filtered out:\n{text}"
    );
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

// --------------------------------------------------------------------------
// Sync & lock file
// --------------------------------------------------------------------------

#[test]
fn sync_no_remotes_reports_nothing() {
    let tmp = workspace_with(MANIFEST);
    metaphor()
        .current_dir(tmp.path())
        .arg("sync")
        .assert()
        .success()
        .stdout(predicate::str::contains("No projects with a remote to sync"));
}

#[test]
fn sync_clones_remote_and_writes_lock() {
    let (repo_tmp, remote_dir) = bare_repo_with_commit();
    let ws = workspace_with_remote(&remote_dir);

    // Run sync
    metaphor()
        .current_dir(ws.path())
        .arg("sync")
        .assert()
        .success()
        .stdout(predicate::str::contains("syncing"))
        .stdout(predicate::str::contains("ok"))
        .stdout(predicate::str::contains("Synced 1 project(s)"));

    // Module directory should exist with the cloned content
    assert!(ws.path().join("modules/mymod/README.md").exists());

    // Lock file should exist
    let lock_path = ws.path().join("metaphor.lock");
    assert!(lock_path.exists());
    let lock_content = fs::read_to_string(&lock_path).unwrap();
    assert!(lock_content.contains("mymod"));
    assert!(lock_content.contains("resolved:"));

    drop(repo_tmp);
}

#[test]
fn add_with_ref_persists_to_manifest() {
    let tmp = workspace_with("version: 1\nprojects: []\n");
    metaphor()
        .current_dir(tmp.path())
        .args([
            "add",
            "sapiens",
            "--project-type",
            "module",
            "--path",
            "./modules/sapiens",
            "--remote",
            "https://github.com/faridlab/backbone-sapiens",
            "--ref",
            "v1.0.0",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added project 'sapiens'"));

    // Verify the ref is persisted in the manifest
    let yaml = fs::read_to_string(tmp.path().join("metaphor.yaml")).unwrap();
    assert!(yaml.contains("ref: v1.0.0"), "ref not in manifest:\n{yaml}");
    assert!(yaml.contains("remote:"), "remote not in manifest:\n{yaml}");
}

#[test]
fn add_clone_without_remote_errors() {
    let tmp = workspace_with("version: 1\nprojects: []\n");
    metaphor()
        .current_dir(tmp.path())
        .args([
            "add",
            "noremote",
            "--project-type",
            "module",
            "--path",
            "./modules/noremote",
            "--clone",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--clone requires --remote"));
}

#[test]
fn show_project_displays_ref() {
    let manifest = "version: 1\nprojects:\n  - name: sapiens\n    type: module\n    path: ./sapiens\n    remote: https://github.com/faridlab/backbone-sapiens\n    ref: v2.0.0\n";
    let tmp = workspace_with(manifest);
    metaphor()
        .current_dir(tmp.path())
        .args(["show", "project", "sapiens"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ref:         v2.0.0"));
}

/// Create a bare git repo with a single commit and return (TempDir, remote_path).
fn bare_repo_with_commit() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let remote_dir = tmp.path().join("remote-repo");
    fs::create_dir_all(&remote_dir).unwrap();
    std::process::Command::new("git")
        .args(["init", "--bare"])
        .current_dir(&remote_dir)
        .output()
        .unwrap();

    let staging = tmp.path().join("staging");
    std::process::Command::new("git")
        .args([
            "clone",
            remote_dir.to_str().unwrap(),
            staging.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    fs::write(staging.join("README.md"), "hello").unwrap();
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(&staging)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@test.com",
            "commit",
            "-m",
            "init",
        ])
        .current_dir(&staging)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["push"])
        .current_dir(&staging)
        .output()
        .unwrap();

    (tmp, remote_dir)
}

/// Set up a workspace with one module pointing at a bare git remote.
fn workspace_with_remote(remote_dir: &std::path::Path) -> TempDir {
    let ws = TempDir::new().unwrap();
    let manifest = format!(
        "version: 1\nprojects:\n  - name: mymod\n    type: module\n    path: ./modules/mymod\n    remote: {}\n",
        remote_dir.display()
    );
    fs::write(ws.path().join("metaphor.yaml"), &manifest).unwrap();
    ws
}

#[test]
fn sync_filter_by_project_name() {
    let manifest = "version: 1\nprojects:\n  - name: no-remote\n    type: module\n    path: ./nr\n  - name: also-no-remote\n    type: module\n    path: ./anr\n";
    let tmp = workspace_with(manifest);
    // Filtering to a project that has no remote should report nothing.
    metaphor()
        .current_dir(tmp.path())
        .args(["sync", "--projects", "no-remote"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No projects with a remote to sync"));
}

#[test]
fn sync_idempotent_second_run_is_noop() {
    let (repo_tmp, remote_dir) = bare_repo_with_commit();
    let ws = workspace_with_remote(&remote_dir);

    // First sync — clones
    metaphor()
        .current_dir(ws.path())
        .arg("sync")
        .assert()
        .success()
        .stdout(predicate::str::contains("Synced 1 project(s)"));

    // Read lock after first sync
    let lock_1 = fs::read_to_string(ws.path().join("metaphor.lock")).unwrap();

    // Second sync — should be a no-op (no ref changed, lock exists)
    metaphor()
        .current_dir(ws.path())
        .arg("sync")
        .assert()
        .success()
        .stdout(predicate::str::contains("Synced 1 project(s)"));

    // Lock file should be identical
    let lock_2 = fs::read_to_string(ws.path().join("metaphor.lock")).unwrap();
    assert_eq!(lock_1, lock_2, "lock file changed on idempotent sync");

    drop(repo_tmp); // keep remote alive until end
}

#[test]
fn sync_update_flag_re_resolves() {
    let (repo_tmp, remote_dir) = bare_repo_with_commit();
    let ws = workspace_with_remote(&remote_dir);

    // First sync
    metaphor()
        .current_dir(ws.path())
        .arg("sync")
        .assert()
        .success();

    // Second sync with --update — should fetch + re-resolve even though
    // nothing changed, proving the flag forces re-evaluation.
    metaphor()
        .current_dir(ws.path())
        .args(["sync", "--update"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ok"));

    drop(repo_tmp);
}

#[test]
fn sync_non_git_target_dir_fails_gracefully() {
    let ws = TempDir::new().unwrap();

    // Create a non-git directory at the module path
    let mod_dir = ws.path().join("modules").join("mymod");
    fs::create_dir_all(&mod_dir).unwrap();
    fs::write(mod_dir.join("not-a-repo.txt"), "oops").unwrap();

    // Manifest points remote at a bogus URL — but the directory exists
    // so sync will try fetch inside a non-git dir.
    let manifest = format!(
        "version: 1\nprojects:\n  - name: mymod\n    type: module\n    path: ./modules/mymod\n    remote: file:///nonexistent\n",
    );
    fs::write(ws.path().join("metaphor.yaml"), &manifest).unwrap();

    metaphor()
        .current_dir(ws.path())
        .arg("sync")
        .assert()
        .failure()
        .stdout(predicate::str::contains("FAILED"));
}

#[test]
fn add_clone_writes_lock_entry() {
    let (repo_tmp, remote_dir) = bare_repo_with_commit();
    let ws = TempDir::new().unwrap();
    fs::write(ws.path().join("metaphor.yaml"), "version: 1\nprojects: []\n").unwrap();

    metaphor()
        .current_dir(ws.path())
        .args([
            "add",
            "mymod",
            "--project-type",
            "module",
            "--path",
            "./modules/mymod",
            "--remote",
            remote_dir.to_str().unwrap(),
            "--clone",
        ])
        .assert()
        .success();

    // Lock file should exist with a resolved commit
    let lock_path = ws.path().join("metaphor.lock");
    assert!(lock_path.exists(), "metaphor.lock not created by add --clone");
    let lock_content = fs::read_to_string(&lock_path).unwrap();
    assert!(lock_content.contains("mymod"), "lock missing project name");
    assert!(
        lock_content.contains("resolved:"),
        "lock missing resolved hash"
    );

    drop(repo_tmp);
}

#[test]
fn list_hides_ref_for_local_projects() {
    let tmp = workspace_with(MANIFEST);
    let out = metaphor()
        .current_dir(tmp.path())
        .arg("list")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(out).unwrap();
    // MANIFEST has no remotes, so ref= should not appear
    assert!(
        !stdout.contains("ref="),
        "ref= shown for local-only projects:\n{stdout}"
    );
}
