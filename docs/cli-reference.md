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

### `metaphor graph`

Print the project dependency graph derived from each project's
[`depends_on`](workspace.md#depends_on).

| Flag | Effect |
| --- | --- |
| `--json` | Emit structured JSON: `{ "version": 1, "data": { "nodes": [...], "edges": [{"from","to"}, ...] } }`. |
| `--focus <name>` | Show only the subgraph reachable from `<name>` via `depends_on` edges. Errors with `unknown project '<name>'` if the project isn't in the manifest. |

Text output is an indented listing: each project on its own line with its
direct dependencies listed beneath it as `  └─ <dep>`. Projects are printed
in sorted order.

Errors:
- `cycle detected among projects: <a, b, ...>` — unreachable today because the
  loader rejects cycles via `Manifest::validate`, but surfaces if a cycle is
  ever introduced at runtime.

### `metaphor show projects`

JSON-friendly inspection of the full project list.

| Flag | Effect |
| --- | --- |
| `--json` | Emit `{ "version": 1, "data": { "projects": [...] } }`. Each project is serialized with every manifest field (`name`, `type`, `path`, `remote?`, `depends_on`). |

Without `--json`, this is identical to `metaphor list`.

### `metaphor show project <name>`

JSON-friendly detail view for a single project.

| Flag | Effect |
| --- | --- |
| `--json` | Emit `{ "version": 1, "data": { "project": {...}, "resolved_path": "<absolute path>" } }`. |

Without `--json`, prints a labeled block: `name`, `type`, `path`, `resolved`,
`remote`, `depends_on`.

Errors with `project '<name>' not found in workspace` if the name is not in
the manifest.

### `metaphor plugins`

List the plugin binaries this `metaphor` install can see.

| Flag | Effect |
| --- | --- |
| `--json` | Emit the list under the stable `{ "version": 1, "data": { "plugins": [...] } }` envelope. |

Text output shows each known plugin (`metaphor-schema`, `metaphor-codegen`, `metaphor-dev`) with `✓`/`✗` installed status, the subcommands it backs, the resolved path (if installed), and the output of `<plugin> --version`.

Discovery uses the same rules as command dispatch: `$METAPHOR_PLUGIN_BIN_DIR` first, then `$PATH`. See [plugins.md](plugins.md).

### `metaphor cache`

Manage the task-result cache. See the "Running across many projects" section below for when entries are written and what invalidates them.

Subcommands:

| Command | Effect |
| --- | --- |
| `metaphor cache clear` | Remove every cache entry. Prints the count and byte total removed. |
| `metaphor cache stats [--json]` | Show the cache root directory, the number of entries, total bytes on disk, and the newest entry's timestamp. |

Both require a workspace — they walk up from cwd to find `metaphor.yaml` (the cache lives at `<workspace_root>/.metaphor/cache/`). Add `.metaphor/` to your `.gitignore`.

### `metaphor add <name>`

Register a new project in the workspace manifest without hand-editing YAML.

| Flag | Required | Effect |
| --- | --- | --- |
| `--project-type <type>` | yes | One of the kebab-case [project types](workspace.md#projecttype-enum): `backend-service`, `webservice`, `webapp`, `mobileapp`, `desktopapp`, `module`, `crate`, `cli-tool`, `infra`, `docs-site`. |
| `--path <path>` | yes | Absolute or relative to workspace root. |
| `--remote <url>` | no | Git remote URL. |
| `--depends-on <names>` | no | Comma-separated or repeatable list of project names this one depends on. Every name must already exist in the manifest. |

Validation uses the same rules as manifest loading ([workspace.md § depends_on](workspace.md#depends_on)) — duplicate names, unknown deps, and self-deps are rejected.

```bash
metaphor add billing-api --project-type backend-service --path ./services/billing-api
metaphor add billing-web --project-type webapp --path ./apps/billing-web \
  --depends-on billing-api,billing-domain \
  --remote git@github.com:acme/billing-web.git
```

Comments in a hand-edited `metaphor.yaml` are **not** preserved — `add` round-trips the full manifest through `serde_yaml`.

---

## Running across many projects

Every passthrough command accepts the same `RunFlags` block. Without any of these flags, the command behaves as before (single invocation, inherited stdio, no cwd change).

| Flag | Effect |
| --- | --- |
| `--all` | Run the plugin once per registered project. Topological order (deps first). |
| `--projects <a,b,c>` | Run only across the named projects (comma-separated). Topological order within the selection. Mutually exclusive with `--all` and `--affected`. |
| `--affected` | Run only on projects whose files changed (via `git diff`) plus their transitive dependents. See below. |
| `--base <ref>` | Base ref for `--affected`. Default: `main`. |
| `--head <ref>` | Head ref for `--affected`. Default: `HEAD`. |
| `--parallel <N>` | Max concurrent plugin invocations. Default: `1` (sequential). |
| `--continue-on-error` | Keep going on project failures; exit non-zero at the end with a summary. Without this, execution stops at the first failure. |
| `--no-cache` | Bypass the task result cache (neither read nor write). |

### Examples

```bash
metaphor lint --all
metaphor test --projects=billing-api,billing-web
metaphor lint --affected --base=origin/main
metaphor lint --all --parallel=4 --continue-on-error

# Plugin-specific args go after --
metaphor lint --all -- --strict
metaphor test --projects=api -- --filter unit
```

### How it works

- Each project is spawned in its own working directory (`current_dir = Project::resolved_path`). Plugins don't need any new flags to learn which project they're running for — they just use `$PWD`.
- Output is buffered per project and printed under a `== <project> ==` header so parallel runs stay readable.
- Sequential (default) mode stops at the first failure unless `--continue-on-error` is set. Parallel mode always runs every scheduled job to completion and reports failures at the end.

### `--affected`: git semantics

`--affected` requires a git workspace. It runs:

```
git diff --name-only <base>..<head>
```

**Two-dot range (`..`)**, not three-dot. `base..head` lists commits reachable from `head` but not from `base`, which matches Nx's `nx affected` convention. Using two-dot means the affected set doesn't shift when `base` moves (e.g. after a `git fetch`).

Metaphor then maps each changed file to the project with the longest matching `path` prefix and closes that set under **reverse-dependency edges** — so touching a shared module also selects everything that depends on it.

**Only tracked files count.** A newly-created, never-`git add`ed file does not mark its project as affected. This matches Nx's behavior.

Failure modes:
- `failed to invoke \`git diff\`` — git not installed, or not a git repo.
- `\`git diff <base>..<head>\` failed` — ref doesn't exist, or worktree is corrupt.

There is no silent fallback to `--all` — a missing base ref is always an error.

### Interaction with the manifest

- **Single-shot** plugin commands (no `--all` / `--projects` / `--affected`) do not require `metaphor.yaml`. They just spawn the plugin from whatever cwd you're in.
- **Multi-project** commands require a manifest — `find_and_load` walks up from cwd looking for `metaphor.yaml`. No manifest → clear error.
- `--parallel`, `--continue-on-error`, and `--no-cache` are all rejected without a selector flag (`--all` / `--projects` / `--affected`) so typos don't silently behave as single-shot.

### Task-result cache

Multi-project runs cache successful results at `<workspace_root>/.metaphor/cache/`. On a cache hit, the stored stdout/stderr is replayed and the recorded exit code is returned — the plugin is not spawned at all. The `== <project> == (cached)` header tells you a replay happened.

**Cache key** = `blake3(plugin-binary-path || plugin --version || argv || project-name || project-tree-hash)`. Any change to the plugin binary it points to, the plugin's self-reported version, the forwarded args, or any file under the project (except `.git/`, `target/`, `node_modules/`, `dist/`, `build/`, `.next/`, `.venv/`, `__pycache__/`, `.metaphor/`) invalidates the entry.

**What's cached:** only runs that exited 0. Failures always re-run so a flaky test doesn't get stuck "red".

**What isn't cached:** single-shot plugin invocations (they inherit stdio, which the cache can't replay losslessly). Cross-project side effects (writing to shared infra) — if a plugin mutates state outside its project tree, a cache hit won't re-apply the mutation.

**Security note.** Cache entries store the recorded stdout and stderr of successful runs *verbatim* — including any tokens, passwords, or URLs a plugin may have printed. Treat `.metaphor/cache/` like CI log artifacts: don't commit it, don't share it, scrub it if a plugin leaks secrets. `.metaphor/` should be in your `.gitignore`.

**Known limitation — ignore list is fixed.** The tree-hash skips a hard-coded set of directories (`.git/`, `target/`, `node_modules/`, `dist/`, `build/`, `.next/`, `.venv/`, `__pycache__/`, `.metaphor/`). There is no `.metaphorignore` or `.gitignore` integration yet. Files outside those dirs contribute to the hash even if your VCS would ignore them. If this bites you, `metaphor cache clear` + restructuring the project is the workaround until richer ignore rules land.

Bypass or manage the cache with `--no-cache`, [`metaphor cache clear`](#metaphor-cache), or [`metaphor cache stats`](#metaphor-cache).

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
