# Roadmap

The project is in its **Foundation** phase. This page tracks what's done, what's next, and what's on the horizon.

## Phase 1 — Foundation (done)

- Workspace skeleton, four crates wired up under one Cargo workspace.
- `metaphor init` — write a fresh `metaphor.yaml`.
- `metaphor list` — read `metaphor.yaml` and print registered projects.
- `metaphor.yaml` schema v1: 10 project types, version-gated loader, absolute/relative path resolution, upward `find_and_load`.
- Plugin passthrough wiring for the four known plugin binaries (`metaphor-schema`, `metaphor-codegen`, `metaphor-dev`, `metaphor-agent`) covering 15 subcommands.
- Plugin discovery via `METAPHOR_PLUGIN_BIN_DIR` + `$PATH` fallback.
- Two install paths (curl|bash, npm) plus `cargo install`.

## Phase 1b — Module sync & version pinning (done)

- `ref` field on `Project` — pin a remote project to a tag, branch, or commit hash.
- `metaphor sync` — clone or update all remote projects to their pinned ref, in topological order.
- `metaphor.lock` — records the resolved commit hash for each synced project; enables reproducible builds across machines.
- `--clone` flag on `metaphor add` — register and clone in one step.
- `--ref` flag on `metaphor add` — set the pinned ref at registration time.

## Phase 2 — Scaffolding (planned)

`metaphor-scaffold` lands. Provides a `metaphor new` (or similar) command that:

- Clones a starter repo for a chosen project type.
- Renames the package / module to the new project name.
- Sets the git remote (or strips it).
- Re-initializes `.git` so the new project has clean history.

Crate is currently a placeholder — see [crates/metaphor-scaffold/src/lib.rs](../crates/metaphor-scaffold/src/lib.rs).

## Phase 3 — Orchestration

The orchestration design space — project graph, `--affected`, `run-many`,
project registration, task caching — is owned by [PLAN.md](PLAN.md). That
document is Nx-inspired and lays out eight adaptations across three sub-phases
(A foundation, B orchestration, C performance). Read it before starting any
work in this area.

## Phase 4 — In-process plugin registry (planned)

Replace the hard-coded subprocess dispatch in `main.rs` with a registry that goes through the `GeneratorPlugin` / `ToolPlugin` traits in `metaphor-plugin-api`. Outcomes:

- Multiple plugins can coexist for the same subcommand and be picked by project type.
- `--dry-run` becomes a first-class flag enforced uniformly.
- Plugin authors can ship a Rust crate (still wrapping a subprocess if they like) instead of needing a code change in `metaphor` to add a top-level subcommand.

The trait surface is already in place so plugin authors can target it now — see [plugin-api.md](plugin-api.md).

## Open questions

- **In-process vs. always-subprocess.** Even with the registry, we may keep subprocess as the only plugin contract. The trait surface allows either.
- **Schema versioning.** `CURRENT_VERSION` is `1`; once we have to bump it, we need a migration story (auto-migrate vs. fail with instructions).
- **Cross-workspace operations.** Today everything is single-workspace. Whether to support "use this project from that workspace" is open.
