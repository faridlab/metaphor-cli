# Plugins

Most `metaphor` subcommands don't do work themselves — they spawn a **plugin binary** as a subprocess and forward arguments to it. This page covers the contract: which binaries exist, how `metaphor` finds them, and how to debug missing or misbehaving plugins.

For each plugin's own command set, see that plugin's repository.

## The known plugins

| Binary | Provides | Driven by `metaphor` commands |
| --- | --- | --- |
| `metaphor-schema` | Schema parsing and webapp codegen | `schema`, `webapp` |
| `metaphor-codegen` | Laravel-style scaffolders, protos, migrations, seeds | `make`, `module`, `apps`, `proto`, `migration`, `seed` |
| `metaphor-dev` | Developer workflow commands | `dev`, `lint`, `test`, `docs`, `config`, `jobs` |
| `metaphor-agent` | Claude Code skills and subagents installer | `agent` |

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

Plugins are independent projects. Install paths, in rough order of convenience:

- **`metaphor plugin add <name>[@<version>]`** — downloads the matching release asset from the plugin's GitHub repo and drops it into `$METAPHOR_PLUGIN_BIN_DIR` (or `~/.metaphor/bin` if unset). Default version is `latest`. This is the recommended path; see [cli-reference.md § metaphor plugin](cli-reference.md#metaphor-plugin).
- Manual download of the release tarball into `~/.local/bin` (same idiom as the main installer).
- `cargo install --path <plugin-repo>/crates/<plugin-crate>` for source builds.
- Symlink from a debug build during development:
  ```bash
  mkdir -p ~/.metaphor/bin
  ln -sf $(realpath path/to/metaphor-plugin-schema/target/debug/metaphor-schema) ~/.metaphor/bin/
  ln -sf $(realpath path/to/metaphor-plugin-codegen/target/debug/metaphor-codegen) ~/.metaphor/bin/
  ln -sf $(realpath path/to/metaphor-plugin-dev/target/debug/metaphor-dev) ~/.metaphor/bin/
  ln -sf $(realpath path/to/metaphor-skill-agents/target/debug/metaphor-agent) ~/.metaphor/bin/
  export METAPHOR_PLUGIN_BIN_DIR=~/.metaphor/bin
  ```

This setup lets you `cargo build` any plugin and immediately have `metaphor` pick up the new binary on the next invocation.

### Release asset contract

`metaphor plugin add` expects each plugin repo to publish releases that follow this contract:

| Item | Value |
| --- | --- |
| Repo | One per plugin — see the table below for the canonical mapping |
| Tag | `v<semver>` (e.g. `v0.1.0`) |
| Asset name | `<binary-name>-<target>.tar.gz` (e.g. `metaphor-dev-aarch64-apple-darwin.tar.gz`) |
| Targets | `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` |
| Tarball layout | Single executable at the root, named `<binary-name>` — no nested directory |

| Binary | Repo |
| --- | --- |
| `metaphor-schema` | `faridlab/metaphor-plugin-schema` |
| `metaphor-codegen` | `faridlab/metaphor-plugin-codegen` |
| `metaphor-dev` | `faridlab/metaphor-plugin-dev` |
| `metaphor-agent` | `faridlab/metaphor-skill-agents` |

The binary name inside the tarball is the name `metaphor` dispatches to, which differs from the repo name (repo `metaphor-plugin-dev` → binary `metaphor-dev`; repo `metaphor-skill-agents` → binary `metaphor-agent`). The asset-name convention matches the main CLI's own releases for consistency — `taiki-e/upload-rust-binary-action` in `.github/workflows/release.yml` produces exactly this layout. A template workflow for plugin repos lives at [plugin-release-workflow.md](plugin-release-workflow.md).

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

Use `metaphor plugins` to see which plugins this install can find. Example output when some are installed and some are not:

```
Known plugins:
  ✓ metaphor-schema [schema, webapp]
      path:    /Users/you/.metaphor/bin/metaphor-schema
      version: metaphor-schema 0.3.1
  ✗ metaphor-codegen [make, module, apps, proto, migration, seed]  (not installed)
  ✓ metaphor-dev [dev, lint, test, docs, config, jobs]
      path:    /Users/you/.metaphor/bin/metaphor-dev
      version: metaphor-dev 0.2.0
  ✓ metaphor-agent [agent]
      path:    /Users/you/.metaphor/bin/metaphor-agent
      version: metaphor-agent 0.1.0
```

Add `--json` for a scriptable view — see [cli-reference.md § metaphor plugins](cli-reference.md#metaphor-plugins). The command is pure introspection: it doesn't run any plugin subcommand, only `<plugin> --version`.

Plugins without a `--version` flag show `version: (unknown)` but still appear as installed.

## Where plugin docs live

Per-plugin command reference is **not** in this repo. Look at:

- `metaphor-schema` — its own README/docs
- `metaphor-codegen` — its own README/docs
- `metaphor-dev` — its own README/docs
- `metaphor-agent` — its own README/docs

This repo only documents the **contract**: the discovery rules above, the trait surface in [plugin-api.md](plugin-api.md), and the command → binary mapping in [cli-reference.md](cli-reference.md).

## Working directory contract

When `metaphor` invokes a plugin as a **single-shot** command (no `--all` / `--projects` / `--affected`), the plugin inherits the user's current working directory — exactly as if the user ran the plugin binary directly.

When `metaphor` invokes a plugin as part of a **multi-project run** (`--all`, `--projects`, or `--affected`), it spawns the plugin once per project with:

- `current_dir = <absolute path to that project>` (`Project::resolved_path` applied against the workspace root).
- stdio captured and replayed under a `== <project-name> ==` header.

Plugins should therefore use `$PWD` / their process cwd to locate project files, and must **not** assume they were invoked from the workspace root. No extra flags or env vars are added by metaphor.

## Writing a new plugin

See [plugin-api.md](plugin-api.md). The short version: build a binary that accepts the subcommands `metaphor` will forward to it (e.g. `<your-plugin> dev <args…>` if you wire it into a hypothetical `metaphor` subcommand), put it on `$PATH` (or `METAPHOR_PLUGIN_BIN_DIR`), and add the subcommand arm in `crates/metaphor-cli/src/main.rs`. Until in-process registration lands (see [roadmap.md](roadmap.md)), every plugin requires a small change in `metaphor` itself to expose a top-level subcommand.
