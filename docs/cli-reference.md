# CLI reference

Every `metaphor` subcommand. Two are implemented in-process (`init`, `list`); the rest forward to external plugin binaries via subprocess.

## Synopsis

```
metaphor [-v|--verbose] <command> [args...]
```

## Global flags

| Flag | Effect |
| --- | --- |
| `-v`, `--verbose` | Sets `RUST_LOG=debug` and initializes `env_logger` before dispatching. Available on every subcommand. |
| `--help`, `-h` | Print help. Works at every level. |
| `--version`, `-V` | Print the binary version. |

Every invocation prints a two-line banner before running the command:

```
⚡ Metaphor CLI
Orchestrate independent project repos
```

## Exit codes

- `0` — success.
- non-zero — propagated from the plugin binary (`<plugin> exited with status: <code>`), or from a workspace error (e.g. `metaphor.yaml already exists`, `metaphor.yaml not found`).

If a plugin binary cannot be spawned at all (not installed, not executable), the error is `failed to spawn <plugin> — is it installed?`.

---

## Core commands

### `metaphor init`

Initialize a new workspace in the current directory.

- Writes `metaphor.yaml` with `version: 1` and an empty `projects` list.
- Refuses to overwrite an existing manifest (`metaphor.yaml already exists at …`).
- No flags.

```bash
mkdir my-workspace && cd my-workspace
metaphor init
# → Initialized empty metaphor workspace at …/metaphor.yaml
```

### `metaphor list`

List projects registered in the workspace.

- Reads `metaphor.yaml` from the **current working directory**.
- Errors if no manifest is found (`metaphor.yaml not found in <cwd> or any parent directory`).
- Prints `No projects registered.` when the list is empty.
- Otherwise prints `<n> project(s):` followed by one line per project: `  - <name> [<ProjectType>] path=<path> remote=<remote-or-(no remote)>`.

---

## Plugin passthrough commands

Each subcommand forwards its arguments verbatim to a plugin binary. Pass `--help` through to see the plugin's own help, e.g. `metaphor schema --help`.

### Mapping table

| `metaphor` command | Plugin binary | Forwarded as |
| --- | --- | --- |
| `metaphor schema <args…>` | `metaphor-schema` | `metaphor-schema <args…>` (raw passthrough — no subcommand prefix) |
| `metaphor webapp <args…>` | `metaphor-schema` | `metaphor-schema generate:webapp <args…>` |
| `metaphor make <args…>` | `metaphor-codegen` | `metaphor-codegen make <args…>` |
| `metaphor module <args…>` | `metaphor-codegen` | `metaphor-codegen module <args…>` |
| `metaphor apps <args…>` | `metaphor-codegen` | `metaphor-codegen apps <args…>` |
| `metaphor proto <args…>` | `metaphor-codegen` | `metaphor-codegen proto <args…>` |
| `metaphor migration <args…>` | `metaphor-codegen` | `metaphor-codegen migration <args…>` |
| `metaphor seed <args…>` | `metaphor-codegen` | `metaphor-codegen seed <args…>` |
| `metaphor dev <args…>` | `metaphor-dev` | `metaphor-dev dev <args…>` |
| `metaphor lint <args…>` | `metaphor-dev` | `metaphor-dev lint <args…>` |
| `metaphor test <args…>` | `metaphor-dev` | `metaphor-dev test <args…>` |
| `metaphor docs <args…>` | `metaphor-dev` | `metaphor-dev docs <args…>` |
| `metaphor config <args…>` | `metaphor-dev` | `metaphor-dev config <args…>` |
| `metaphor jobs <args…>` | `metaphor-dev` | `metaphor-dev jobs <args…>` |

All passthrough commands accept `--` and hyphen-prefixed arguments without `metaphor` itself trying to interpret them (`trailing_var_arg = true`, `allow_hyphen_values = true`).

### `metaphor schema <args…>`

Schema parsing and code generation. **Raw passthrough** — what you type after `schema` is what `metaphor-schema` receives. Run `metaphor schema --help` for the plugin's own command list.

### `metaphor webapp <args…>`

Webapp code generation. Forwards to `metaphor-schema generate:webapp <args…>`. The `generate:webapp` prefix is added automatically; you only supply the rest.

### `metaphor make <args…>`

Laravel-style scaffolding (`make:*`). Forwards to `metaphor-codegen make <args…>`.

### `metaphor module <args…>`

Module-level scaffolding inside a project. Forwards to `metaphor-codegen module <args…>`.

### `metaphor apps <args…>`

Application-level scaffolding. Forwards to `metaphor-codegen apps <args…>`.

### `metaphor proto <args…>`

Protocol buffer operations (buf / tonic). Forwards to `metaphor-codegen proto <args…>`.

### `metaphor migration <args…>`

Database migrations. Forwards to `metaphor-codegen migration <args…>`.

### `metaphor seed <args…>`

Database seeding. Forwards to `metaphor-codegen seed <args…>`.

### `metaphor dev <args…>`

Development workflow (run, watch, hot reload). Forwards to `metaphor-dev dev <args…>`.

### `metaphor lint <args…>`

Code quality and linting. Forwards to `metaphor-dev lint <args…>`.

### `metaphor test <args…>`

Test generation and execution. Forwards to `metaphor-dev test <args…>`.

### `metaphor docs <args…>`

Documentation generation. Forwards to `metaphor-dev docs <args…>`.

### `metaphor config <args…>`

Configuration validation and management. Forwards to `metaphor-dev config <args…>`.

### `metaphor jobs <args…>`

Job scheduling. Forwards to `metaphor-dev jobs <args…>`.

---

## Plugin discovery

Plugin binaries are resolved at invocation time:

1. If `METAPHOR_PLUGIN_BIN_DIR` is set, look for `<dir>/<binary-name>` and use it if it exists.
2. Otherwise invoke the bare name and rely on `$PATH`.

See [plugins.md](plugins.md) for setup, troubleshooting, and the contract each plugin must implement.

## Notes on argument parsing

- Plugin commands use clap's `trailing_var_arg = true` and `allow_external_subcommands = true`. The CLI does no validation on the forwarded args — the plugin owns its own argument schema.
- The `--verbose` flag is global and is consumed before the plugin args are forwarded. It does **not** propagate to the plugin process. If you need the plugin to be verbose, pass its own verbose flag (e.g. `metaphor schema -- -v build`).
