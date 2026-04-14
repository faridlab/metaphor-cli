# Quickstart

A guided tour from zero to a working Metaphor workspace with one project registered.

## 1. Install

Pick one method from [install.md](install.md). The rest of this guide assumes `metaphor` is on your `$PATH`:

```bash
metaphor --version
```

## 2. Create a workspace

```bash
mkdir my-workspace
cd my-workspace
metaphor init
```

Output:

```
⚡ Metaphor CLI
Orchestrate independent project repos

Initialized empty metaphor workspace at /…/my-workspace/metaphor.yaml
```

`metaphor init` writes a `metaphor.yaml` containing only the schema version. It refuses to overwrite an existing file:

```
Error: metaphor.yaml already exists at /…/my-workspace/metaphor.yaml
```

## 3. Inspect the manifest

```bash
cat metaphor.yaml
```

```yaml
version: 1
projects: []
```

## 4. Register a project

Clone (or move) one of your real project repos under the workspace:

```bash
git clone git@github.com:you/billing-api.git
```

Then edit `metaphor.yaml`:

```yaml
version: 1
projects:
  - name: billing-api
    type: backend-service
    path: ./billing-api
    remote: git@github.com:you/billing-api.git
```

The full schema — every project type, path resolution rules, and field semantics — is documented in [workspace.md](workspace.md).

## 5. List projects

```bash
metaphor list
```

```
1 project(s):
  - billing-api [BackendService] path=./billing-api remote=git@github.com:you/billing-api.git
```

`metaphor list` reads `metaphor.yaml` from the **current working directory**. (Walking up to find a parent workspace happens inside library code via `find_and_load`, but the `list` command itself uses CWD only.)

## 6. Run a plugin command

The remaining `metaphor <command>` subcommands are passthroughs to external plugin binaries. For example, once `metaphor-schema` is installed:

```bash
metaphor schema --help          # forwarded to metaphor-schema --help
metaphor schema build           # forwarded to metaphor-schema build
metaphor webapp generate users  # forwarded to metaphor-schema generate:webapp users
```

If the plugin binary isn't installed, you'll see:

```
Error: failed to spawn metaphor-schema — is it installed?
```

See [plugins.md](plugins.md) for the full plugin → binary mapping and how to install plugins, and [cli-reference.md](cli-reference.md) for every subcommand.

## 7. Verbose output

Add `-v` / `--verbose` (a global flag) to any command to enable debug logging:

```bash
metaphor -v list
```

This sets `RUST_LOG=debug` and initializes `env_logger` before dispatching.

## Where to next

- [cli-reference.md](cli-reference.md) — full command reference.
- [workspace.md](workspace.md) — every field of `metaphor.yaml`.
- [plugins.md](plugins.md) — install plugins, control discovery via `METAPHOR_PLUGIN_BIN_DIR`.
- [plugin-api.md](plugin-api.md) — write your own plugin.
