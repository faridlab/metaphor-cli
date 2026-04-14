# Architecture

How the four crates fit together, and the design choices that shape the project.

## Crate graph

```
metaphor-cli (binary)
   ├── metaphor-workspace      ← reads/writes metaphor.yaml
   ├── metaphor-plugin-api     ← trait surface for future in-process plugins
   └── metaphor-scaffold       ← (Phase 2 — currently empty)
```

| Crate | Kind | Role |
| --- | --- | --- |
| `metaphor-cli` | binary | The user-facing `metaphor` command. Owns argument parsing (clap), the dispatcher, and subprocess invocation of plugin binaries. |
| `metaphor-workspace` | library | The `metaphor.yaml` schema, version checks, error types, and the `init` / `load` / `find_and_load` / `save` functions. |
| `metaphor-plugin-api` | library | The `GeneratorPlugin` and `ToolPlugin` traits plus their context types. Dependency-free (deliberately) so plugin authors can pull it in cheaply. |
| `metaphor-scaffold` | library | Will own "clone a starter repo, rename, re-init `.git`" once Phase 2 lands. Currently a placeholder. |

## Subprocess delegation: why?

Every subcommand except `init` and `list` shells out to a plugin binary. The reasons:

1. **Each plugin is its own repo.** Different release cadences, different maintainers, different language ecosystems where helpful.
2. **No ABI to worry about.** Subprocess is a stable contract — stdin/stdout/exit code. No worry about Rust ABI mismatches between dynamically-loaded plugins.
3. **Crash isolation.** A plugin segfaulting kills its subprocess, not `metaphor`.
4. **Easy to swap implementations.** Drop a different binary into `$METAPHOR_PLUGIN_BIN_DIR` and the same subcommand now does something different.

The cost: spawn overhead per command and a less ergonomic data interchange (string args, exit codes) than an in-process API. Acceptable for a developer tool that runs interactively.

## How a command flows

`metaphor schema build src/users.yaml` →

1. clap parses `Cli`, matching `Command::Schema { args: ["build", "src/users.yaml"] }`.
2. Banner is printed.
3. Dispatcher arm calls `plugin_env::passthrough_raw("metaphor-schema", &args)`.
4. `plugin_binary("metaphor-schema")` resolves to `$METAPHOR_PLUGIN_BIN_DIR/metaphor-schema` if set and present, else bare `metaphor-schema`.
5. `std::process::Command::new(<bin>).args(&args).status()` spawns the plugin with inherited stdio.
6. On exit: success → propagate `Ok(())`; non-zero → `bail!` with `<binary> exited with status: <code>`.

For `metaphor make user`, step 3 is `passthrough("metaphor-codegen", "make", &["user"])` — the helper inserts `make` between the binary and the user args. See [cli-reference.md](cli-reference.md) for the full mapping.

## Workspace data model

The manifest is intentionally tiny: a `version`, a flat list of `Project { name, type, path, remote? }`. No nested groups, no env-specific overrides, no derived state. The library owns:

- Round-trip serialization via `serde` + `serde_yaml`.
- Version gate (rejects anything that isn't `CURRENT_VERSION`).
- Path resolution (`Project::resolved_path`) so plugin code only ever sees absolute paths.
- Upward search (`find_and_load`) so commands can be run from anywhere inside a workspace.

See [workspace.md](workspace.md) for the schema and [crates/metaphor-workspace/src/lib.rs](../crates/metaphor-workspace/src/lib.rs) for the source.

## Plugin trait surface (today vs. tomorrow)

Today the dispatcher in `metaphor-cli/src/main.rs` doesn't go through `metaphor-plugin-api`. It calls the subprocess helpers directly. The trait surface (`GeneratorPlugin`, `ToolPlugin`) is staged ahead of an in-process registry that will:

- Look up the right plugin by `capabilities()` + `handles_project()` instead of by hard-coded binary name.
- Allow multiple plugins to coexist for the same subcommand and pick by project type.
- Make `--dry-run` a first-class concern handled uniformly.

Until that lands, the trait is the **forward-compatible contract** for plugin authors who want to be ready when the registry ships.

## Verbosity & logging

`-v / --verbose` sets `RUST_LOG=debug` and initializes `env_logger`. The flag is consumed by the parent `metaphor` process and **not** forwarded to plugin binaries — pass plugin-specific verbose flags via the plugin's own argument syntax.

## Testing strategy

- `metaphor-workspace` has unit tests for `init` / `load` covering happy path and `AlreadyInitialized` error.
- `metaphor-cli` is currently dispatch-only and exercised via integration with real plugin binaries.
- `metaphor-plugin-api` is a pure trait surface, tested by virtue of its consumers compiling.

## Source map (where things live)

| What | Where |
| --- | --- |
| Subcommand definitions | `crates/metaphor-cli/src/main.rs` (`Command` enum, ~lines 36–156) |
| Dispatch table | `crates/metaphor-cli/src/main.rs` (`match` in `main()`, ~lines 170–194) |
| Plugin binary lookup | `crates/metaphor-cli/src/plugin_env.rs` |
| `metaphor.yaml` schema | `crates/metaphor-workspace/src/lib.rs` |
| Plugin traits | `crates/metaphor-plugin-api/src/lib.rs` |
| Scaffold (placeholder) | `crates/metaphor-scaffold/src/lib.rs` |
