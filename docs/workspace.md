# Workspace manifest (`metaphor.yaml`)

The manifest is the single source of truth for "what projects make up this workspace." It lives at the workspace root and is created by `metaphor init`.

## Filename and version

- Filename: `metaphor.yaml` (constant `MANIFEST_FILE`)
- Current schema version: `1` (constant `CURRENT_VERSION`)
- A loader will reject any other version with `unsupported metaphor.yaml version: <found> (expected 1)`.

## Top-level schema

```yaml
version: <u32>          # required, must equal CURRENT_VERSION (1)
projects: [Project]     # optional, defaults to []
```

## Project schema

```yaml
name: <string>          # required, used by metaphor list and find_project
type: <ProjectType>     # required, see enum below (kebab-case in YAML)
path: <string>          # required, absolute or relative to the workspace root
remote: <string>        # optional, git remote URL; omitted when not set
depends_on: [<string>]  # optional, list of project names this one depends on
```

YAML field name is `type` (the Rust field is `project_type` with `#[serde(rename = "type")]`).

## depends_on

Declare cross-project edges. Each entry must be the `name` of another project
in the same manifest. The loader validates these and rejects:

- unknown dependency names (`project '<name>' depends_on unknown project '<missing>'`),
- a project listing itself (`project '<name>' lists itself in depends_on`),
- duplicate project names (`duplicate project name '<name>'`).

`depends_on` edges drive `metaphor graph` (see [cli-reference.md](cli-reference.md#metaphor-graph))
and, in later phases, run-many ordering and affected-set computation.

## ProjectType enum

Project types are written in **kebab-case** in YAML (`#[serde(rename_all = "kebab-case")]`).

| YAML value | Rust variant | Intended use |
| --- | --- | --- |
| `backend-service` | `BackendService` | A backend HTTP/gRPC service |
| `webservice` | `Webservice` | A general-purpose web service |
| `webapp` | `Webapp` | A web frontend application |
| `mobileapp` | `Mobileapp` | A mobile application (iOS/Android) |
| `desktopapp` | `Desktopapp` | A desktop application |
| `module` | `Module` | A reusable feature module / domain package |
| `crate` | `Crate` | A Rust library crate |
| `cli-tool` | `CliTool` | A command-line tool |
| `infra` | `Infra` | Infrastructure-as-code (Terraform, Pulumi, etc.) |
| `docs-site` | `DocsSite` | A documentation site |

A loader rejects any other value with a YAML parse error.

## Path resolution

`Project::resolved_path(workspace_root)` returns:

- the literal `path` if it is absolute,
- otherwise `workspace_root.join(path)`.

Plugin code receives an already-resolved (absolute) path on `ResolvedProject.path`. See [plugin-api.md](plugin-api.md).

## Locating the manifest

Two strategies exist in the workspace library:

| Function | Behavior |
| --- | --- |
| `load(dir)` | Look for `<dir>/metaphor.yaml`. Errors with `NotFound(<dir>)` if absent. Used by `metaphor list`. |
| `find_and_load(start)` | Walk up from `start`, returning the first directory that contains `metaphor.yaml`. Errors with `NotFound(<start>)` if no parent has one. |

`find_and_load` is the right choice for plugin commands that should "just work" anywhere inside the tree; `load` is right for commands that should only operate on the cwd.

## Errors (`WorkspaceError`)

| Variant | Message |
| --- | --- |
| `AlreadyInitialized(path)` | `metaphor.yaml already exists at <path>` |
| `NotFound(dir)` | `metaphor.yaml not found in <dir> or any parent directory` |
| `UnsupportedVersion { found, expected }` | `unsupported metaphor.yaml version: <found> (expected <expected>)` |
| `ProjectNotFound(name)` | `project '<name>' not found in workspace` |
| `DuplicateProject(name)` | `duplicate project name '<name>'` |
| `UnknownDependency { project, missing }` | `project '<project>' depends_on unknown project '<missing>'` |
| `SelfDependency(name)` | `project '<name>' lists itself in depends_on` |

All other I/O / parse errors surface as `anyhow` chains with context like `reading <path>` or `parsing metaphor.yaml`.

## Example

```yaml
version: 1
projects:
  - name: billing-api
    type: backend-service
    path: ./services/billing-api
    remote: git@github.com:acme/billing-api.git

  - name: billing-web
    type: webapp
    path: ./apps/billing-web
    remote: git@github.com:acme/billing-web.git
    depends_on: [billing-api, billing-domain]

  - name: billing-mobile
    type: mobileapp
    path: ./apps/billing-mobile
    remote: git@github.com:acme/billing-mobile.git

  - name: billing-domain
    type: module
    path: ./modules/billing-domain
    # no remote: lives only inside this workspace

  - name: shared-protos
    type: crate
    path: /Users/me/work/shared-protos
    remote: git@github.com:acme/shared-protos.git

  - name: terraform
    type: infra
    path: ./infra
    remote: git@github.com:acme/terraform.git
```

After saving this file, `metaphor list` prints one line per project with the resolved path and remote (or `(no remote)`).

## Deployment conventions (Phase D)

Metaphor assumes, but does not enforce, a small set of per-project files to
make dev-loop and build commands work out of the box. Full rationale lives
in [DEPLOYMENT.md](DEPLOYMENT.md); quick reference:

| File (per project) | Purpose | Consumed by |
| --- | --- | --- |
| `Dockerfile` | Production image. Multi-stage, minimal. | [`metaphor build`](cli-reference.md#metaphor-build), the project's own CI. |
| `Dockerfile.dev` | Live-reload image. Mounts source as a volume. | Workspace-level `docker-compose.yml`, `metaphor dev`. |
| `.dockerignore` | Excludes build artifacts (`target/`, `node_modules/`, `build/`, `.gradle/`, `.git/`, `.metaphor/`) from the build context. Mirrors the [`metaphor clean`](cli-reference.md#metaphor-clean) safelist. | `docker build`. |
| `compose.fragment.yml` | Partial Compose service definition for this project. | [`metaphor compose generate`](cli-reference.md#metaphor-compose-generate). |
| `metaphor.env.yaml` | Declares required/optional environment variables (name, required, default, secret). | [`metaphor env check`](cli-reference.md#metaphor-env-check). |

These are **conventions, not hard requirements.** A project without any of
them still participates in `metaphor list` / `graph` / `show`; it just opts
out of the corresponding workflow. Conventions make the workspace-level
orchestrator do useful work without project-by-project configuration.

## Editing by hand vs. `metaphor add`

Two ways to register a project:

- **Hand-edit `metaphor.yaml`.** Good for bulk changes and when you want to keep comments — the file is yours.
- **`metaphor add <name> --project-type <type> --path <path> [--remote ...] [--depends-on ...]`.** Validates names, rejects duplicates, resolves deps. Round-trips through `serde_yaml` so **hand-written comments are lost**. See [cli-reference.md § metaphor add](cli-reference.md#metaphor-add-name).

Both paths run `Manifest::validate` on write, so they reject the same set of bad states (duplicates, unknown deps, self-deps).
