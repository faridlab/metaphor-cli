# Design plan: deployment & development environments

This is a **design document**, not a how-to guide. It stakes out the pattern
Metaphor workspaces follow for local development and production deployment,
so that future tooling has a contract to target.

The guiding principle: deployment should mirror Metaphor's "independent repos
loosely coordinated" posture. Local dev gets a single command that brings the
whole workspace up with live reload. Production ships each project
independently. There is **no shared monolithic deploy pipeline** — that would
fight the model.

Sibling to [PLAN.md](PLAN.md) (orchestration) and [roadmap.md](roadmap.md)
(phase tracker). This doc owns the **deployment** design space.

## Why this exists

The [README](../README.md) promises that Metaphor "manages a workspace of
standalone project repos and helps them work together," but today it's silent
on what happens when you *run* them. Teams adopting Metaphor inevitably ask:
"how do I bring the whole workspace up locally?" and "how do I ship a
release?" This doc answers both, and names the small set of Metaphor features
we'd build to make the pattern ergonomic.

## The two layers

Dev and prod deserve different answers, rooted in the same posture:

- **Local development.** One command — `metaphor dev --all` — brings every
  registered project up with live reload, regardless of language. Must be
  fast, reproducible, and "works on a fresh clone in under five minutes."
- **Production.** Each project deploys independently on its own cadence,
  from its own CI, to its own image tag. An `infra` project in the workspace
  knows the wiring (what image runs where) and is the source of truth for
  releases. Metaphor's job ends at helping with *builds*, not *releases*.

## Decisions (and what we are NOT doing)

| Approach | Decision | Reason |
| --- | --- | --- |
| `docker-compose` at workspace level for dev | **Adopt** | Language-agnostic, fast, matches the independent-repo model without forcing a runtime. |
| Per-project `Dockerfile` | **Adopt** | Ships with the project, not centralized. Changes to a service's runtime live in the repo that owns the service. |
| `infra` project type = deployment source-of-truth | **Adopt** | [`ProjectType::Infra`](workspace.md#projecttype-enum) already exists; give it a concrete purpose. |
| Per-project CI pushing images to a registry | **Adopt** | Each repo's concern. Metaphor orchestrates *builds*, not *pushes*, not *releases*. |
| Monolithic deploy pipeline | **Skip** | Fights the model. "One green-button deploy of the whole workspace" is not a goal. |
| Nx Cloud / remote build cache | **Skip** (again) | Covered in [PLAN.md § Inspirations](PLAN.md#inspirations-and-what-we-are-not-building). Not re-litigated here. |
| Kubernetes from day one | **Defer** | Default to Compose-on-VM. Migrate when you hit ~10 services, need multi-region, or require zero-downtime. |
| `nx release` equivalent | **Skip** | Each project owns its own release tooling (semver, changelog, tag). Metaphor doesn't cut releases. |
| Metaphor-managed secrets | **Skip** | Metaphor declares *which* env vars are secret (if using the env schema below); storage is the platform's job. |

## Local development pattern

### Convention

Every project in the workspace carries **two Dockerfiles** at its repo root:

- `Dockerfile` — production image. Multi-stage, minimal, no dev tooling.
- `Dockerfile.dev` — live-reload image. Mounts source as a volume, runs
  the language-native watcher (`cargo watch`, `./gradlew --continuous`,
  `next dev`, `pytest --watch`, etc.).

A **workspace-level `docker-compose.yml`** at the root wires services
together. Service names match `Project.name` from `metaphor.yaml` so that
DNS-inside-the-compose-network resolves to the project.

### The dev loop

```bash
git clone <workspace-repo> && cd <workspace-repo>
# each project is either already in-tree or fetched via metaphor-scaffold
metaphor dev --all
```

`metaphor dev --all` fans out (see [PLAN.md item #5](PLAN.md#5---all-and---projects-on-every-passthrough)):
for each project, the `metaphor-dev` plugin runs `docker compose up <service>`
in that project's working directory. Projects the user names via
`--projects=api,web` run in that subset. Everything on one compose network.

### Worked example — 3-project billing workspace

`metaphor.yaml`:

```yaml
version: 1
projects:
  - name: billing-api
    type: backend-service
    path: ./services/billing-api
    depends_on: [billing-domain]

  - name: billing-web
    type: webapp
    path: ./apps/billing-web
    depends_on: [billing-api]

  - name: billing-domain
    type: module
    path: ./modules/billing-domain
```

`docker-compose.yml` at the workspace root (generated or hand-written):

```yaml
services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: billing
      POSTGRES_PASSWORD: dev

  billing-api:
    build:
      context: ./services/billing-api
      dockerfile: Dockerfile.dev
    volumes:
      - ./services/billing-api:/app
      - ./modules/billing-domain:/shared/billing-domain:ro
    environment:
      DATABASE_URL: postgres://postgres:dev@postgres/billing
    depends_on: [postgres]
    ports: ["8080:8080"]

  billing-web:
    build:
      context: ./apps/billing-web
      dockerfile: Dockerfile.dev
    volumes:
      - ./apps/billing-web:/app
      - /app/node_modules
    environment:
      API_URL: http://billing-api:8080
    depends_on: [billing-api]
    ports: ["3000:3000"]
```

`services/billing-api/Dockerfile.dev` (Rust example):

```dockerfile
FROM rust:1-slim
RUN cargo install cargo-watch
WORKDIR /app
CMD ["cargo", "watch", "-x", "run"]
```

`apps/billing-web/Dockerfile.dev` (Next.js example):

```dockerfile
FROM node:20-alpine
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
CMD ["npm", "run", "dev"]
```

Fresh clone → `metaphor dev --all` → every service up with live reload,
connected over one network. Works the same whether the project is Rust,
Kotlin, Node, Python, or Go.

## Production deployment pattern

### Per-project builds

Each project's own CI (GitHub Actions, GitLab, whatever) does:

1. `docker build -t <registry>/<project>:<git-sha> -t <registry>/<project>:<semver> .`
2. `docker push` both tags.

Metaphor does **not** orchestrate the release. Its contribution is helping
with the *build* — `metaphor build --affected` (Phase D-2, below) iterates
over only the projects that changed vs. a base ref, tags them consistently,
and optionally pushes. For CI that runs outside Metaphor, the per-project
Dockerfiles stand alone.

### The `infra` project as release source-of-truth

One project in the workspace has `type: infra`. It holds the
Terraform modules / Kubernetes manifests / Compose file / Helm charts /
Pulumi program — whatever the platform needs — that reference **specific
image tags** for the services:

```yaml
# infra/compose.prod.yml
services:
  billing-api:
    image: registry.example.com/billing-api:1.2.3
  billing-web:
    image: registry.example.com/billing-web:4.5.6
```

**Cutting a release** = bump the tag in `infra/`, commit, apply
(`terraform apply`, `kubectl apply`, whatever). That's it. No parallel
deployment pipeline, no shared orchestration.

**Rolling back** = revert the tag bump in `infra/` and re-apply.
Mechanical, because every project is image-first.

### Multiple environments

Default convention: **one `infra` project per environment** (`infra-staging`,
`infra-prod`). Clearer blast radius, easier to grant scoped access, easier
to diff. Alternative: one `infra` repo with env-specific subdirs. Pick one
per workspace; the doc recommends the former.

## Metaphor features that support this

Six items, phased. Template matches [PLAN.md](PLAN.md) so they read the same
shape.

---

### D-1. `Dockerfile` / `Dockerfile.dev` convention (docs only)

**Problem it solves:** No agreed place for per-project container definitions.

**CLI surface:** None. Pure convention.

**Manifest impact:** None.

**Implementation notes:** Document in [workspace.md](workspace.md) and
[contributing.md](contributing.md). Link examples from here.

**Phase:** D-1. **Depends on:** nothing.

**Out of scope:** Auto-generating Dockerfiles. That's scaffolding-plugin
territory; see [roadmap.md](roadmap.md#phase-2--scaffolding-planned).

---

### D-2. `metaphor build --all --affected`

**Problem it solves:** Rebuilding N projects' Docker images by hand is
tedious and misses the Phase B `--affected` win.

**CLI surface:** new `ToolCapability::Build` on
[`ToolPlugin`](plugin-api.md#tool-plugin), surfaced as `metaphor build`.
Accepts the full [RunFlags](cli-reference.md#running-across-many-projects)
set plus `--push` (push after build) and `--tag=<template>` (default
`{name}:{git_sha}`).

**Manifest impact:** None. Plugins opt in via capability.

**Implementation notes:**

- Runs `docker build` in each project's `resolved_path`.
- Tags with both `{name}:{git_sha}` and any additional `--tag` templates.
- With `--push`, pushes after a successful build. Separate flag from
  `--apply` intentionally — matches Nx's split of build vs. release.
- Reuses the Phase C task cache automatically. Cache key already covers the
  project tree + argv, so a `Dockerfile` change busts the entry; a no-op
  second run replays the build metadata for free.

**Phase:** D-2. **Depends on:** Phase B (run_many), Phase C (cache).

**Out of scope:** BuildKit frontends, SBOM/attestation generation, multi-arch
builds. All pass through whatever the user's BuildKit setup already does.

---

### D-3. `metaphor compose generate`

**Problem it solves:** Workspace-level `docker-compose.yml` must be
hand-edited every time a project is added, removed, or renamed — easy to
forget, easy to drift.

**CLI surface:**

```
metaphor compose generate [--out docker-compose.yml] [--dry-run]
```

**Manifest impact:** Each project optionally ships a `compose.fragment.yml`
at its repo root — a partial Compose service definition. `compose generate`
reads `metaphor.yaml`, collects each project's fragment, and emits the
merged workspace-level file.

**Implementation notes:**

- Dry-run default (prints to stdout); `--out` writes.
- Fragments are plain Compose YAML with the service body; the tool fills
  in `name:` and global wiring (networks, volumes).
- Round-tripped through `serde_yaml`. Comments in an existing hand-written
  `docker-compose.yml` are lost — same caveat as `metaphor add`.

**Phase:** D-3. **Depends on:** nothing strictly; plays best with D-1.

**Out of scope:** Generating the per-project fragments themselves. Authors
write those.

---

### D-4. Env schema per project

**Problem it solves:** "Works locally, breaks in prod because the new env
var wasn't configured" is the single most common release bug in
multi-repo workspaces.

**CLI surface:**

```
metaphor env check [--all | --projects=a,b] [--json]
```

**Manifest impact:** New optional file at each project's repo root:
`metaphor.env.yaml`:

```yaml
env:
  - name: DATABASE_URL
    required: true
    secret: true
    example: postgres://user:pass@host/db
  - name: LOG_LEVEL
    required: false
    default: info
```

**Implementation notes:**

- `metaphor env check` walks each selected project, parses its schema,
  and validates every *required* var has a value — consulting, in order:
  the current shell environment, a workspace-root `.env` file, and the
  workspace-level `docker-compose.yml` for that service.
- Exits non-zero on missing required vars. Reports what's missing and
  which source was consulted.
- `secret: true` is a *declaration*, not a store. It flags the var so
  the infra project knows to read from the platform's secret manager
  rather than a plaintext file.

**Phase:** D-4. **Depends on:** nothing; orthogonal.

**Out of scope:** Secret storage. Typed validation (ints, URLs, enums) —
v1 is just "present or absent."

---

### D-5. `metaphor deploy`

**Problem it solves:** Users want a consistent command to trigger a
deploy, regardless of whether the infra repo uses Terraform, `kubectl`,
or a hand-rolled script.

**CLI surface:**

```
metaphor deploy <push|rollback|status|logs|migrate|exec> [args...]
metaphor docker <up|down|logs|ps|restart|pull|build> [args...]
```

Both are **passthroughs** to the `metaphor-dev` plugin (`metaphor-dev
deploy …` / `metaphor-dev docker …`). The CLI itself owns no deploy
logic anymore — it just forwards arguments and inherits stdio so
interactive prompts from Terraform / `kubectl` / `gcloud` work as
expected.

**Manifest impact:** Plugin reads `metaphor.deploy.yaml` at the
workspace root for the registry-driven subcommands (`push`,
`rollback`, `status`, etc.). `metaphor deploy exec` is the exception:
it shells out to the workspace's `infra` project.

**Implementation notes:**

- `metaphor deploy exec` is the successor to the previous native
  `metaphor deploy`. It finds the project with `type: infra` and
  runs, in order, the first thing it finds: `./deploy.sh` (if
  executable), then `make deploy`. The picker logic (including
  `--infra=<name>` when multiple infra projects exist) lives in the
  plugin and is unit-tested there.
- Metaphor's responsibility ends at delegating. The infra project owns
  what "deploy" means.
- No `--dry-run` on Metaphor's side — that's for the infra tool to
  implement (`terraform plan`, `kubectl diff`, etc.) and users to invoke
  via `-- --dry-run` / `-- plan`.

**Phase:** D-5 (shipped — now lives in `metaphor-plugin-dev`).
**Depends on:** D-2 (you want images built before you deploy).

**Out of scope:** Rollback orchestration, blue-green, canary. Platform
concerns.

---

### D-6. `.dockerignore` guidance (docs only)

**Problem it solves:** A stray 4 GB `target/` inside a Docker build
context makes every build slow and inflates the cache key for D-2.

**CLI surface:** None.

**Manifest impact:** None.

**Implementation notes:** Document in [workspace.md](workspace.md) that
each project must ship a `.dockerignore` excluding at minimum: `target/`,
`node_modules/`, `build/`, `.gradle/`, `.git/`, `.metaphor/`. (Matches
the [`metaphor clean`](cli-reference.md#metaphor-clean) safelist, not
by coincidence.)

**Phase:** D-1 (alongside the Dockerfile convention).

## Phasing summary

| Phase | Items | Goal |
| --- | --- | --- |
| **D-1 — Conventions** | D-1, D-3 fragment shape, D-6 | Documentation only. Workspaces can adopt today without waiting on Metaphor features. |
| **D-2 — Builds** | D-2 `metaphor build`, D-3 `compose generate` | First Metaphor code. Turns the conventions from D-1 into a one-command loop. |
| **D-3 — Hygiene** | D-4 `metaphor env check` | Catch the "missing env var" class of bugs before they hit prod. |
| **D-4 — Delegated deploy** | D-5 `metaphor deploy` | Thin glue. Doesn't ship without real users asking. |

## Platform guidance

When do you graduate off `docker compose`?

- **Stay on Compose-on-VM** while: fewer than ~10 services, single region,
  team ≤ 10 people, no hard zero-downtime requirement. This covers a
  surprising number of real deployments. `docker compose up -d` on a
  systemd-managed VM is a legitimate production posture.
- **Move to Kubernetes** when: multi-region, zero-downtime required,
  multiple teams operationally own different services, per-service
  auto-scaling matters.
- **PaaS per project** (Fly.io / Railway / Render / Vercel for web): strong
  fit when projects are mostly stateless services. Each project owns its
  own deploy; Metaphor just coordinates builds. Infra project shrinks to
  a thin config repo.

No deadlines on the graduation. The `infra` project type is the abstraction
boundary — **switching platforms means editing one repo**, not every service.

## Cross-cutting concerns

- **Secrets.** Metaphor never touches secret values directly. The env schema
  (D-4) declares *which* vars are secret; the platform (Compose `.env`,
  Kubernetes `Secret`, a cloud secret manager) stores them. `metaphor env
  check` validates presence; it does not read or print values.
- **Migrations.** Each project owns its migrations — no central migration
  pipeline. The `infra` project coordinates *ordering* in prod (run
  migrations before rolling the app). The existing project graph
  (`depends_on`) already expresses this order; `metaphor deploy` could
  honor it.
- **Build provenance.** SBOM, attestation, signing — pass through whatever
  the Dockerfile BuildKit frontend produces. Not Metaphor's job.
- **Monorepo-style atomic commits across services.** Explicitly unsupported.
  If a change spans two services, that's two PRs in two repos, coordinated
  by whoever's driving. Metaphor's graph helps you see the ordering;
  executing the change is human work.

## Open questions

- **`metaphor build --push` semantics.** Separate from `--apply`, per Nx?
  Or merge into one flag? Lean toward **separate** so a local dev can build
  without pushing.
- **One `infra` per env, or one repo with multiple targets?** Doc recommends
  one-per-env; open to feedback from real adoption.
- **`metaphor status`.** Does Metaphor need a "what's deployed vs. what's
  built" command that queries the infra project? Probably Phase E; deferred
  until a user asks.
- **Dev `Dockerfile.dev` literal filename.** Keeping it as a *convention*
  for now (overridable by a future `metaphor.build.yaml` per project).

## See also

- [PLAN.md](PLAN.md) — orchestration roadmap. Phase D (this doc) layers on
  top of Phase B (run-many, affected) and Phase C (cache).
- [workspace.md](workspace.md) — `infra` project type, `depends_on`.
- [plugin-api.md](plugin-api.md) — where `build` / `deploy` capabilities
  would land on `ToolPlugin`.
- [cli-reference.md § metaphor clean](cli-reference.md#metaphor-clean) —
  the safelist from `clean` intentionally matches the `.dockerignore`
  guidance in D-6.
