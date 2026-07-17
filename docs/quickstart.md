# Quickstart

A guided tour from zero to a working Metaphor workspace with one project registered.

## 1. Install

Pick one method from [install.md](install.md). The rest of this guide assumes `metaphor` is on your `$PATH`:

```bash
metaphor --version
```

## 2. Create a workspace

This guide builds a workspace up by hand, so start from an empty manifest:

```bash
mkdir my-workspace
cd my-workspace
metaphor init --bare
```

Output:

```
⚡ Metaphor CLI
Orchestrate independent project repos

Initialized empty metaphor workspace at /…/my-workspace/metaphor.yaml
```

`metaphor init --bare` writes a `metaphor.yaml` containing only the schema version. It refuses to overwrite an existing file:

```
Error: metaphor.yaml already exists at /…/my-workspace/metaphor.yaml
```

> **Starting a real product?** Skip the hand-assembly: `metaphor init <name>` clones the
> `metaphor-workspace` template into `./<name>` — modules, `deployment/`, and docs already wired,
> with the workspace name stamped throughout. See
> [cli-reference.md § `metaphor init`](cli-reference.md#metaphor-init).

## 3. Inspect the manifest

```bash
cat metaphor.yaml
```

```yaml
version: 1
projects: []
```

## 4. Register a project

### Option A: Hand-edit + manual clone

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

### Option B: `metaphor add` with `--clone`

Register and clone in one step:

```bash
metaphor add billing-api \
  --project-type backend-service \
  --path ./billing-api \
  --remote git@github.com:you/billing-api.git \
  --clone
```

To pin to a specific version (tag, branch, or commit):

```bash
metaphor add backbone-sapiens \
  --project-type module \
  --path ./modules/backbone-sapiens \
  --remote https://github.com/faridlab/backbone-sapiens \
  --ref v1.0.0 --clone
```

The full schema — every project type, path resolution rules, and field semantics — is documented in [workspace.md](workspace.md).

## 5. List projects

```bash
metaphor list
```

```
1 project(s):
  - billing-api [BackendService] path=./billing-api remote=git@github.com:you/billing-api.git ref=HEAD
```

`metaphor list` reads `metaphor.yaml` from the **current working directory**. (Walking up to find a parent workspace happens inside library code via `find_and_load`, but the `list` command itself uses CWD only.)

## 5b. Sync remote projects

If your workspace has remote projects (modules from GitHub, shared libraries, etc.), use `metaphor sync` to clone or update them all:

```bash
metaphor sync
```

Sync clones missing projects, fetches updates for existing ones, checks out each project's pinned `ref`, and writes `metaphor.lock` with the resolved commit hashes.

```
syncing backbone-sapiens ... ok (a1b2c3d4e5f6)
syncing backbone-bucket ... ok (deadbeef0123)

Synced 2 project(s), 0 failed. Lock written to /…/my-workspace/metaphor.lock
```

After syncing, commit `metaphor.lock` so team members get the same versions.

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
