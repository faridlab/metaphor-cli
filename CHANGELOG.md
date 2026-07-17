# Changelog

All notable changes to `metaphor-cli` are documented here.

The format is loosely based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.1] — 2026-07-17

### Added

- **`metaphor init <name>` now scaffolds a real workspace from the
  `metaphor-workspace` template** instead of only writing an empty manifest —
  the workspace analogue of how `module create` clones `backbone-module`. It
  shallow-clones the template into `./<name>`, drops the template's `.git` (a
  fresh workspace, not a fork), replaces the `__project__` / `__PROJECT__`
  placeholders throughout every text file with the workspace name, writes a
  product README in place of the template's usage guide, and re-inits git.
  - `--template <URL>` overrides the template repo. The default is SSH
    (`git@github.com:faridlab/metaphor-workspace.git`) because the template is
    private; pass an HTTPS URL for a public fork.
  - `--bare`, or `metaphor init` with no name, keeps the original behavior:
    write an empty `metaphor.yaml` in the current directory.
  - Refuses to clone over an existing `./<name>`.

  See [docs/cli-reference.md § `metaphor init`](docs/cli-reference.md#metaphor-init).

### Fixed

- **`metaphor sync --update` now actually advances a branch-pinned project.**
  `git fetch` moves the remote-tracking ref (`origin/<branch>`) but leaves the
  local branch behind, so the subsequent `git checkout <branch>` landed on the
  *stale* local branch and sync re-recorded the old SHA in `metaphor.lock` —
  fetching the new tip and then ignoring it. Sync now fast-forwards the local
  branch to `origin/<branch>` after checkout. Tag- and SHA-pinned projects have
  no `origin/<ref>`, so they are untouched and immutable pins stay put; a local
  branch that has diverged from the remote fails loudly rather than being
  clobbered. See [`cmd_sync`](crates/metaphor-cli/src/cmd_sync.rs).

## [0.2.0] — 2026-06-07

### Added

- **`metaphor clean --docker` — reclaim the dev stack's Docker build-cache
  volumes.** The dev stack keeps Rust/Node build artifacts in *named Docker
  volumes* (e.g. `<project>_cargo_target`) that the host-side `clean` sweep
  can't see — a runaway one is the usual cause of a `No space left on device`
  that takes Postgres down with it. `--docker` extends `clean` to those volumes
  with the same safety posture as the host path: dry-run by default, `--apply`
  to delete, and `--confirm-over`/`--yes` thresholds honoured. Scope and safety:
  - **Workspace-scoped.** Only volumes labelled with this workspace's own
    Compose project name(s) are considered — read from the top-level `name:`
    field of `deployment/compose*.y{a,}ml`. Other Compose projects on the same
    daemon are never touched.
  - **Build caches only — never data.** Within those projects, only volumes
    whose short name is on the safelist are eligible (`cargo_target`,
    `cargo_registry`, `cargo_git`, `target`, `node_modules`, `gradle_cache`,
    `build_cache`). Data volumes — `pgdata`, `miniodata`, `redisdata`, etc. —
    are never removed or emptied.
  - **Idle vs. in-use.** Idle volumes are removed outright; a volume mounted by
    a running container is reported but skipped unless `--include-running` is
    passed, which empties it in place (the container and volume stay, the cache
    is rebuilt on the next build).
  - **No daemon, no-op.** If Docker isn't running, or the workspace declares no
    Compose project name, the Docker pass prints a skip notice and the host
    sweep still completes.

  New `--docker` and `--include-running` flags on `metaphor clean`. See
  [`cmd_clean`](crates/metaphor-cli/src/cmd_clean.rs) and
  [docs/cli-reference.md § `metaphor clean`](docs/cli-reference.md#metaphor-clean).

## [0.1.9] — 2026-04-25

### Changed

- **`metaphor deploy` now delegates to the `metaphor-dev` `docker`/`deploy`
  passthrough** instead of carrying a native deploy implementation. See
  [docs/cli-reference.md](docs/cli-reference.md).

---

Older versions are not retroactively chronicled — see `git log` for
pre-0.1.9 history.
