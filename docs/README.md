# Metaphor docs

The user-facing manual for `metaphor-cli`.

## Getting started

- [Install](install.md) — shell script, npm, and `cargo install`. Env vars, upgrade, uninstall.
- [Quickstart](quickstart.md) — initialize a workspace, register a project, run a plugin command.

## Reference

- [CLI reference](cli-reference.md) — every subcommand and flag, with the plugin binary it forwards to.
- [Workspace manifest](workspace.md) — `metaphor.yaml` schema: fields, project types, path resolution, errors.
- [Plugins](plugins.md) — the three plugin binaries (`metaphor-schema`, `metaphor-codegen`, `metaphor-dev`), discovery rules, troubleshooting.

## Extending

- [Plugin API](plugin-api.md) — author guide for `GeneratorPlugin` and `ToolPlugin`. Context types, capabilities, the `dry_run` contract.
- [Contributing](contributing.md) — build, test, add a subcommand, point at locally-built plugins.

## Background

- [Architecture](architecture.md) — how the four crates fit together; subprocess delegation model.
- [Roadmap](roadmap.md) — phase status and what's planned.
- [Design plan](PLAN.md) — Nx-inspired orchestration features (graph, affected, run-many, caching) with phasing and rationale.
