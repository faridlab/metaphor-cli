# Metaphor CLI Tool

> Type: **`cli-tool`** — a Rust binary that dispatches subcommands, often with plugin discovery.
> This file orients Claude. Skills carry depth; load them on demand.

## What this is

A user-facing binary. Parses args with `clap`, dispatches to subcommand handlers, and for orchestrator-style CLIs discovers and invokes external plugin binaries via subprocess. Stable contract is: stdin/stdout/exit code — no shared-library ABI.

## Golden path

```bash
metaphor dev build                # build the binary
metaphor dev test                 # unit + integration tests
./target/debug/<bin> --help       # smoke-test the surface
metaphor lint check
```

## Rules

- **MUST** have `[[bin]]` in `Cargo.toml`.
- **MUST** use `clap` with `derive` macros for arg parsing. Match the existing `Parser` / `Subcommand` pattern in sibling CLIs.
- **MUST** keep command modules small — one file per subcommand, each exposing `pub fn run(Options) -> Result<()>`.
- **MUST** exit with a non-zero code on error. Use `anyhow::Result<()>` from `main()` and let `?` bubble.
- **MUST** print errors to stderr, not stdout (stdout is for tool-consumable output).
- **SHOULD** offer a `--json` / machine-readable output mode on any command likely to be scripted.
- **SHOULD** support both top-level and namespaced invocation if this plugin is dispatched by `metaphor` (see pattern in `metaphor-skill-agents/src/main.rs:27-56` — accept a leading subcommand namespace).
- **NEVER** make network / fs writes in dry-run code paths. Guard behind an explicit `--write` / `--apply` flag.
- **NEVER** link plugins as dylibs — plugins are always separate binaries found on `$PATH` / `$METAPHOR_PLUGIN_BIN_DIR` / `~/.metaphor/bin/`.

## Folder cheatsheet

```
src/
├── main.rs                     # clap Parser + dispatch
├── lib.rs                      # optional: re-export commands for integration tests
├── commands/
│   ├── mod.rs
│   ├── <subcmd>.rs             # one file per subcommand
│   └── ...
├── target.rs / config.rs       # cross-cutting helpers
└── catalog.rs                  # if embedding assets via include_dir!
assets/                         # embedded via include_dir!() (optional)
tests/                          # integration: assert_cmd + predicates
Cargo.toml
```

## Plugin dispatcher pattern (if this CLI dispatches plugins)

- Discover plugins by looking for `<prefix>-<name>` binaries on `$PATH`.
- Forward all args unchanged; plugin owns its own `--help`.
- Inherit stdin/stdout/stderr (don't buffer — subcommands may be interactive).
- Surface the plugin's exit code unchanged.

Reference: `metaphor-cli/docs/architecture.md` and `metaphor-cli/crates/metaphor-plugin-api/src/lib.rs` for the current plugin traits.

## Common tasks

- "Add a subcommand" → new file in `src/commands/`, add variant to `Subcommand` enum in `main.rs`, dispatch in `match`.
- "Embed a static asset" → put it in `assets/` and expose via `include_dir!("$CARGO_MANIFEST_DIR/assets")` in a `catalog.rs`.
- "Add a `--json` mode" → add `--json` flag; branch on it in the print path; serialize with serde.
- "Write integration tests" → use `assert_cmd` + `predicates` to invoke the built binary; cover happy path + at least one error path.

## Key files to read before editing

- `src/main.rs` — argument surface + dispatch.
- `Cargo.toml` — clap features, bin target.
- `src/commands/mod.rs` — what subcommands exist.
- `README.md` — stated command surface (don't let it drift).

## Deeper knowledge (load on demand)

- Skill: `api-and-interface-design` — CLI surface design.
- Skill: `documentation-and-adrs` — keeping `--help` and README in sync.
- Skill: `test-driven-development` — CLIs are easy to TDD via `assert_cmd`.
- Skill: `shipping-and-launch` — release checks before cutting a version.

## Anti-patterns

- Putting business logic in `main.rs` (keep `main` thin: parse → dispatch → exit).
- Mixing stdout (data) and stderr (logs) — scripts will break.
- Interactive prompts in a path that may be called from CI.
- Linking plugins dynamically instead of dispatching as subprocess.
- Silent failures (every error path should print something actionable to stderr).

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
