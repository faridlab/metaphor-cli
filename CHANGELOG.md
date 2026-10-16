# Changelog

All notable changes to `metaphor-cli` are documented here.

The format is loosely based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`metaphor doctor` checks container images.** It reads every compose file at the workspace
  root, in `deployment/`, and in each project and its `deployment/`, and fails on an image that
  should not come back: a floating tag (`:latest`, or no tag at all), `redis:7` (Redis 7.4+ is
  not open source), any MinIO server or `mc` image (no longer published or maintained),
  MailHog, Promtail (end of life), and `node:20` (end of life). Each failure names the file, the
  service and the replacement. An interpolated tag such as `:${SERVICE_TAG}` counts as pinned.

## [0.6.1] — 2026-10-13

### Changed

- The npm packages moved from the `@metaphor` scope to **`@metaphora`**, because the `@metaphor`
  name on npm is registered to an unrelated account and could never be published under.
  `install.sh` and the `metaphor ui` install hint now install `@metaphora/metaphor-ui`
  (first published as 0.2.0), so a fresh machine gets the interactive UI again. The binary
  name stays `metaphor-ui`; only the npm package name changed. The npm wrapper's version now
  tracks the CLI's so its installer downloads the matching GitHub release.

## [0.6.0] — 2026-10-12

### Added

- **`metaphor pull`** brings every working clone of a workspace up to date in one command, so a
  new machine needs only the workspace repository and `metaphor pull` instead of cloning each app
  and module by hand. A working clone is a project with a `remote` and `pin: false`: one whose
  path is missing is cloned (on its `ref` when the manifest names one); a clean clone is fetched
  and fast-forwarded to its upstream. A clone with uncommitted changes to tracked files, a branch
  that has diverged from its upstream, a detached HEAD or a branch without an upstream is left as
  it is and listed. Four repositories at a time (`--parallel N`); `--projects a,b` narrows it;
  `--dry-run` reports from local state with nothing fetched or written; `--json` prints the
  outcome. It never writes `metaphor.lock`, and never prompts for credentials (a repository that
  needs them fails and says so).
- **`pin` on a project** (default `true`). `pin: false` marks a working clone: `metaphor sync`
  leaves it to `metaphor pull` instead of pinning it to the lock, where checking out the recorded
  commit would rewind a clone being worked in onto a detached HEAD. Pinned projects sync exactly
  as before.

### Fixed

- `--json` output no longer starts with the banner, so it parses as one JSON document
  (`graph`, `info`, `show`, `plugin list`, `pull` and the rest).

## [0.5.0] — 2026-10-02

### Changed

- **Modules are filled from the crate, not cloned.** When the `refs_from` Cargo.lock pins a
  project's crate from a registry, `metaphor sync` fills the project's path from that exact
  `.crate` instead of cloning the repository: taken from Cargo's download cache (`cargo fetch
  --locked` runs once in the consumer when it is missing), checked against the lock's SHA-256,
  and unpacked with every file read-only. `metaphor.lock` records the commit the crate was
  published from, which is the release tag's commit, so switching a workspace leaves the lock
  unchanged. The `modules/<name>/` layout stays, so migration and seed runners and
  `schema generate:webapp --schema-dir modules/<name>/schema` read the same files, now
  byte-for-byte the release that is compiled.
- A later sync verifies the directory against the `.crate` and fails, naming the files, when one
  was edited, removed or added; `metaphor sync --update` restores the release. A git clone is
  replaced only when it has no uncommitted changes. Apps and modules under a local path override
  are still git clones.

## [0.4.0] — 2026-10-02

### Added

- **`refs_from` in `metaphor.yaml`** (manifest `version: 2`). When the modules are compiled from a
  registry, the consumer's `Cargo.lock` already says which version of each module is built.
  `refs_from: { cargo_lock: <path> }` makes it the only place that version is written: a project
  without `ref:` syncs to the release tag (`tag`, default `v{version}`) of the version the lock
  carries for its crate, so the clone's migrations and schema match the compiled code. A project
  names its package with `crate:` when it differs from the project name. Upgrading a module is
  `cargo update` followed by `metaphor sync`. `metaphor list` / `metaphor show` mark derived refs.
- Resolution refuses rather than guesses: a `ref:` that disagrees with the locked version, a crate
  locked at more than one version, and a `module` / `crate` project the lock does not carry.
- Manifest version 2 is required for `refs_from`, so a CLI older than 0.4.0 rejects the manifest
  (`unsupported metaphor.yaml version: 2`) instead of ignoring the key and syncing every ref-less
  module to its remote HEAD. Version 1 manifests load unchanged.

### Fixed

- **`metaphor sync` honours a pulled lock.** When `metaphor.lock` already records a project at the
  manifest's ref, sync now moves the checkout to the recorded commit, fetching it first if the
  clone lacks it. Before, pulling a manifest and lock that moved projects to new tags and running
  plain `metaphor sync` left every existing clone on its old tag, reported `ok`, and rewrote the
  lock with the stale commits. A fresh clone of a branch-pinned project likewise landed on the
  branch tip instead of the locked commit. A locked commit the remote no longer has now fails and
  points at `metaphor sync --update`.

## [0.2.2] — 2026-09-10

### Added

- **`metaphor chaos`** — a passthrough to the metaphor-dev plugin that runs the
  workspace's fault-injection kit (`deployment/chaos/run.sh`) as a first-class
  command, so the post-change chaos gate has a single obvious entry point:
  `metaphor chaos list`, `metaphor chaos --dry-run all`, `metaphor chaos
  <experiment>`, `metaphor chaos --full all`. Every argument is forwarded
  verbatim; the bash driver remains the single source of truth for the
  catalog, flags, and exit-code semantics (`0` pass / `1` fail / `2` skip).

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
