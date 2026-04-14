# Roadmap

The project is in its **Foundation** phase. This page tracks what's done, what's next, and what's on the horizon.

## Phase 1 — Foundation (done)

- Workspace skeleton, four crates wired up under one Cargo workspace.
- `metaphor init` — write a fresh `metaphor.yaml`.
- `metaphor list` — read `metaphor.yaml` and print registered projects.
- `metaphor.yaml` schema v1: 10 project types, version-gated loader, absolute/relative path resolution, upward `find_and_load`.
- Plugin passthrough wiring for the three known plugin binaries (`metaphor-schema`, `metaphor-codegen`, `metaphor-dev`) covering 14 subcommands.
- Plugin discovery via `METAPHOR_PLUGIN_BIN_DIR` + `$PATH` fallback.
- Two install paths (curl|bash, npm) plus `cargo install`.

## Phase 2 — Scaffolding (planned)

`metaphor-scaffold` lands. Provides a `metaphor new` (or similar) command that:

- Clones a starter repo for a chosen project type.
- Renames the package / module to the new project name.
- Sets the git remote (or strips it).
- Re-initializes `.git` so the new project has clean history.

Crate is currently a placeholder — see [crates/metaphor-scaffold/src/lib.rs](../crates/metaphor-scaffold/src/lib.rs).

## Phase 3 — Workspace operations (sketch)

Likely candidates:

- `metaphor add <name> --type <type> --path <path>` to register projects without hand-editing YAML.
- `metaphor remove <name>` and `metaphor rename`.
- `metaphor doctor` to verify path/remote consistency.

Not committed; subject to user feedback.

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
