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
metaphor init my-workspace   # clone the workspace template, stamped with the name
cd my-workspace
metaphor sync                # pull the pinned modules into modules/
metaphor doctor              # tooling + plugin health
```

Prefer to start from nothing? `metaphor init --bare` writes an empty `metaphor.yaml` in the current directory instead.

Then register projects by editing [metaphor.yaml](docs/workspace.md) and run plugin commands like `metaphor schema build` or `metaphor dev start`. See [docs/quickstart.md](docs/quickstart.md) for the full walkthrough.

## Interactive mode

Run `metaphor` with no arguments on a TTY, or `metaphor repl`, to enter the interactive REPL. Every subcommand below works inside the loop exactly as it does on the command line, with persistent history and shell-style quoting. In CI or under a pipe, bare invocation stays non-interactive — scripts and automation are unaffected.

## Commands at a glance

| Category | Command | What it does |
| --- | --- | --- |
| **Workspace** | `metaphor init [<name>]` | Scaffold `./<name>` from the workspace template (`--template <URL>` to override). No name or `--bare` writes an empty `metaphor.yaml` in the current directory. |
|  | `metaphor add <name>` | Register a project (type, path, remote, depends_on) without hand-editing YAML. |
|  | `metaphor list` | List registered projects. |
|  | `metaphor show projects` / `show project [<name>]` | JSON-friendly inspection (add `--json`). `<name>` optional — auto-detects from cwd. |
|  | `metaphor info` | Summarize the workspace and which project cwd is currently inside. |
|  | `metaphor doctor [--json]` | Run diagnostic checks (paths exist, plugins installed, YAML parses, tools available). Exits non-zero on failures. |
|  | `metaphor repl` or bare `metaphor` on a TTY | Enter the interactive REPL — persistent history, shell-style quoting, same subcommands. |
|  | `metaphor graph` | Print the project dependency graph (tree or `--json`, optional `--focus <name>`). |
| **Orchestration** | `metaphor <cmd> --all` | Run a plugin command across every project. |
|  | `metaphor <cmd> --projects=a,b` | Run across a chosen subset (topologically ordered). |
|  | `metaphor <cmd> --affected --base=main` | Run only on projects whose files changed in git + their dependents. |
|  | `metaphor <cmd> --parallel=N` | Fan out with N concurrent workers. |
|  | `metaphor <cmd> --continue-on-error` | Keep going on failures; exit non-zero at the end. |
|  | `metaphor <cmd> --no-cache` | Bypass the task result cache for this run. |
| **Plugin passthrough** | `metaphor schema …` / `webapp …` | Forward to `metaphor-schema` (schema parsing, webapp codegen). |
|  | `metaphor make / module / apps / proto / migration / seed …` | Forward to `metaphor-codegen`. |
|  | `metaphor dev / lint / test / docs / config / jobs / docker / deploy …` | Forward to `metaphor-dev`. |
|  | `metaphor agent …` | Forward to `metaphor-agent` (install Claude Code skills and subagents into a project's `.claude/`). |
| **Tooling** | `metaphor plugins [--json]` | Show which plugin binaries this install can find + their versions. |
|  | `metaphor cache stats` / `cache clear` | Inspect or clear the per-workspace task result cache. |
|  | `metaphor clean [--older-than=30d] [--apply] [--docker]` | Remove stale build-artifact directories (`target/`, `node_modules/`, `build/`, etc.) per project type, and with `--docker` the dev stack's Docker build-cache volumes (data volumes untouched). Dry-run by default. |
| **Deployment** | `metaphor build --all / --affected [--push]` | Run `docker build` per project with consistent tagging. |
|  | `metaphor compose generate` | Merge per-project `compose.fragment.yml` files into a workspace `docker-compose.yml`. |
|  | `metaphor env check` | Validate every project's `metaphor.env.yaml` against the environment + workspace `.env`. |
|  | `metaphor docker <up\|down\|logs\|ps\|restart\|pull\|build>` | Local docker-compose lifecycle. Passthrough to `metaphor-dev`; reads `metaphor.deploy.yaml`. |
|  | `metaphor deploy <push\|rollback\|status\|logs\|migrate\|exec>` | Remote deployment passthrough to `metaphor-dev`. `deploy exec` shells out to the workspace's `infra` project (`deploy.sh` / `make deploy`). |

Every passthrough command accepts the orchestration flags above — without any of them, it behaves exactly like running the plugin binary directly. See [docs/cli-reference.md](docs/cli-reference.md) for the full surface.

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
