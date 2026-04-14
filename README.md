# metaphor-cli

> Orchestrate independent project repos.

Metaphor is a meta-CLI that manages a workspace of standalone project repos and helps them work together. Each project keeps its own git history; Metaphor coordinates scaffolding, code generation, and runtime wiring across them.

## Status

**Foundation.** The workspace skeleton, `metaphor init`, `metaphor list`, and plugin passthrough wiring are in place. Scaffolding (`metaphor new`) and a formal in-process plugin registry are on the roadmap — see [docs/roadmap.md](docs/roadmap.md).

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/faridlab/metaphor-cli/main/install.sh | bash
```

Other installers (npm, `cargo install`) are documented in [docs/install.md](docs/install.md).

## 30-second quickstart

```bash
mkdir my-workspace && cd my-workspace
metaphor init
cat metaphor.yaml          # empty manifest, version: 1
metaphor list              # "No projects registered."
```

Then register projects by editing [metaphor.yaml](docs/workspace.md) and run plugin commands like `metaphor schema build` or `metaphor dev start`. See [docs/quickstart.md](docs/quickstart.md) for the full walkthrough.

## Documentation

- [Install](docs/install.md) — every install method, env vars, upgrade, uninstall.
- [Quickstart](docs/quickstart.md) — first workspace, end to end.
- [CLI reference](docs/cli-reference.md) — every subcommand, flag, and exit behavior.
- [Workspace manifest](docs/workspace.md) — `metaphor.yaml` schema with examples.
- [Plugins](docs/plugins.md) — the three plugin binaries and how discovery works.
- [Plugin API](docs/plugin-api.md) — author guide for `GeneratorPlugin` and `ToolPlugin`.
- [Architecture](docs/architecture.md) — how the four crates fit together.
- [Roadmap](docs/roadmap.md) — what's done, what's next.
- [Contributing](docs/contributing.md) — build, test, add a subcommand.

## Workspace layout

```
metaphor-cli/
├── Cargo.toml                       workspace root
└── crates/
    ├── metaphor-cli/                the binary + dispatcher
    ├── metaphor-workspace/          metaphor.yaml schema + I/O
    ├── metaphor-scaffold/           clones starter repos (planned)
    └── metaphor-plugin-api/         plugin trait surface
```

## License

MIT.
