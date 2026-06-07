# Changelog

All notable changes to `metaphor-cli` are documented here.

The format is loosely based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
