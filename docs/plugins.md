# Plugins

Most `metaphor` subcommands don't do work themselves — they spawn a **plugin binary** as a subprocess and forward arguments to it. This page covers the contract: which binaries exist, how `metaphor` finds them, and how to debug missing or misbehaving plugins.

For each plugin's own command set, see that plugin's repository.

## The three known plugins

| Binary | Provides | Driven by `metaphor` commands |
| --- | --- | --- |
| `metaphor-schema` | Schema parsing and webapp codegen | `schema`, `webapp` |
| `metaphor-codegen` | Laravel-style scaffolders, protos, migrations, seeds | `make`, `module`, `apps`, `proto`, `migration`, `seed` |
| `metaphor-dev` | Developer workflow commands | `dev`, `lint`, `test`, `docs`, `config`, `jobs` |

The mapping is implemented in the dispatch table inside `crates/metaphor-cli/src/main.rs`. The full table — including the implicit subcommand prefix added for each `metaphor` command — is in [cli-reference.md](cli-reference.md).

## Two passthrough flavors

| Helper | Behavior |
| --- | --- |
| `passthrough(binary, subcommand, args)` | Runs `<binary> <subcommand> <args…>`. Used for everything except `metaphor schema`. |
| `passthrough_raw(binary, args)` | Runs `<binary> <args…>` — no subcommand prefix. Used for `metaphor schema` so the plugin owns its full command surface. |

Both:

- Spawn the binary with the user's stdio inherited (you see the plugin's output live).
- Wait for it to exit and propagate its status code.
- Surface a clear error if spawn fails or if the plugin returns non-zero.

## Discovery: how `metaphor` finds a plugin

Lookup order at invocation time:

1. **`$METAPHOR_PLUGIN_BIN_DIR`** — if set, look for `<dir>/<binary-name>`. If that file exists, use it (without checking `$PATH`).
2. **Bare name** — fall back to `<binary-name>`, which `std::process::Command` resolves via `$PATH`.

This deliberately keeps `metaphor` decoupled from where plugins live. There is no plugin manifest, no registry file, no init step. Drop the binary somewhere on `$PATH` (or set the env var) and it works.

## Installing plugins

Plugins are independent projects. Typical install paths:

- Pre-built release tarball into `~/.local/bin` (same idiom as the main installer).
- `cargo install --path <plugin-repo>/crates/<plugin-crate>` for source builds.
- Symlink from a debug build during development:
  ```bash
  mkdir -p ~/.metaphor/bin
  ln -sf $(realpath path/to/metaphor-schema/target/debug/metaphor-schema) ~/.metaphor/bin/
  ln -sf $(realpath path/to/metaphor-codegen/target/debug/metaphor-codegen) ~/.metaphor/bin/
  ln -sf $(realpath path/to/metaphor-dev/target/debug/metaphor-dev) ~/.metaphor/bin/
  export METAPHOR_PLUGIN_BIN_DIR=~/.metaphor/bin
  ```

This setup lets you `cargo build` any plugin and immediately have `metaphor` pick up the new binary on the next invocation.

## Errors and troubleshooting

### `failed to spawn <binary> — is it installed?`

The binary couldn't be executed. Check:

1. Is it installed? (`which metaphor-schema`)
2. If you set `METAPHOR_PLUGIN_BIN_DIR`, does the file exist there? (`ls "$METAPHOR_PLUGIN_BIN_DIR"`) — note that when the env var points at a directory but the binary is missing **inside it**, `metaphor` falls back to a bare-name lookup against `$PATH`. So a misconfigured env var may cause silent fallback rather than a useful error.
3. Is the file executable? (`chmod +x`)
4. On macOS, is it quarantined? (`xattr -d com.apple.quarantine <path>`)

### `<binary> exited with status: <code>`

The plugin ran but failed. Re-run with the plugin's own verbose flag — `metaphor`'s `--verbose` does **not** propagate to the plugin:

```bash
metaphor schema -- -v build       # passes -v to metaphor-schema
```

### `--help` doesn't show plugin subcommands

`metaphor --help` only shows the top-level commands defined in this repo. To see what a plugin offers:

```bash
metaphor schema --help
metaphor codegen-style-command --help   # e.g. metaphor make --help
metaphor dev --help
```

Each forwards to the plugin's own help.

## Inspecting plugin installation

Use `metaphor plugins` to see which plugins this install can find. Example output when only two of three are installed:

```
Known plugins:
  ✓ metaphor-schema [schema, webapp]
      path:    /Users/you/.metaphor/bin/metaphor-schema
      version: metaphor-schema 0.3.1
  ✗ metaphor-codegen [make, module, apps, proto, migration, seed]  (not installed)
  ✓ metaphor-dev [dev, lint, test, docs, config, jobs]
      path:    /Users/you/.metaphor/bin/metaphor-dev
      version: metaphor-dev 0.2.0
```

Add `--json` for a scriptable view — see [cli-reference.md § metaphor plugins](cli-reference.md#metaphor-plugins). The command is pure introspection: it doesn't run any plugin subcommand, only `<plugin> --version`.

Plugins without a `--version` flag show `version: (unknown)` but still appear as installed.

## Where plugin docs live

Per-plugin command reference is **not** in this repo. Look at:

- `metaphor-schema` — its own README/docs
- `metaphor-codegen` — its own README/docs
- `metaphor-dev` — its own README/docs

This repo only documents the **contract**: the discovery rules above, the trait surface in [plugin-api.md](plugin-api.md), and the command → binary mapping in [cli-reference.md](cli-reference.md).

## Working directory contract

When `metaphor` invokes a plugin as a **single-shot** command (no `--all` / `--projects` / `--affected`), the plugin inherits the user's current working directory — exactly as if the user ran the plugin binary directly.

When `metaphor` invokes a plugin as part of a **multi-project run** (`--all`, `--projects`, or `--affected`), it spawns the plugin once per project with:

- `current_dir = <absolute path to that project>` (`Project::resolved_path` applied against the workspace root).
- stdio captured and replayed under a `== <project-name> ==` header.

Plugins should therefore use `$PWD` / their process cwd to locate project files, and must **not** assume they were invoked from the workspace root. No extra flags or env vars are added by metaphor.

## Writing a new plugin

See [plugin-api.md](plugin-api.md). The short version: build a binary that accepts the subcommands `metaphor` will forward to it (e.g. `<your-plugin> dev <args…>` if you wire it into a hypothetical `metaphor` subcommand), put it on `$PATH` (or `METAPHOR_PLUGIN_BIN_DIR`), and add the subcommand arm in `crates/metaphor-cli/src/main.rs`. Until in-process registration lands (see [roadmap.md](roadmap.md)), every plugin requires a small change in `metaphor` itself to expose a top-level subcommand.
