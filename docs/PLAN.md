# Design plan: workspace orchestration

This is a **design document**, not a changelog. It captures the next wave of
features that turn `metaphor` from "list of projects + command forwarder" into
a real workspace orchestrator. Inspired heavily by [Nx](https://nx.dev), pared
down to what fits a meta-CLI of independent repos.

For the high-level phase tracker (Foundation / Scaffolding / Plugin Registry)
see [roadmap.md](roadmap.md). This document owns the **orchestration** design
space.

## Why this exists

The README promises that Metaphor "manages a workspace of standalone project
repos and helps them work together." Today it stores a project list
([workspace.md](workspace.md)) and forwards single commands to plugin binaries
([plugins.md](plugins.md)) — but there is no notion of project relationships,
no way to run a command across many projects, no caching, no inspection
beyond `metaphor list`. This plan describes how we close that gap.

## Inspirations (and what we are NOT building)

| Nx feature | Decision | Reason |
| --- | --- | --- |
| `nx graph` | **Adapt** | A meta-CLI of repos *needs* a dependency graph to be useful. |
| `nx run-many` | **Adapt** | Natural fit for the existing passthrough model. |
| `nx affected` | **Adapt** | Pays for itself the first time CI runs. |
| `nx show projects` / `show project` | **Adapt** | Cheap extension of `metaphor list`. |
| `nx list` (plugins) | **Adapt** | Pairs with the Phase 4 in-process plugin registry. |
| `nx generate` (`nx g`) | **Adapt** | The `make`/`module`/`apps` passthroughs are already this; formalize via `ToolPlugin`. |
| `nx init` / `nx add` | **Adapt** | `metaphor add <project>` replaces hand-editing YAML. |
| Task result caching | **Adapt (deferred)** | High value but only worth it once real plugins exist. |
| Nx Cloud / DTE | **Skip** | Overkill for per-developer workspaces. |
| `nx release` | **Skip** | Each project owns its own release flow — that's the whole point of independent repos. |
| `nx daemon` | **Skip** | Subprocess spawn isn't the bottleneck. |
| `nx migrate` | **Defer** | Only matters once we bump `CURRENT_VERSION` past 1. |

## Adaptations

Eight items, ordered by dependency. Each follows the same template.

---

### 1. `depends_on` field on `Project`

**Inspired by:** Nx project `dependsOn` / implicit graph.

**Problem it solves:** Without explicit relationships, `metaphor` can't
do any cross-project orchestration — no graph, no affected, no ordered
execution. Today the manifest is a flat list with zero edges.

**CLI surface:** None. This is a manifest schema change only.

**Manifest impact:**

```yaml
projects:
  - name: billing-web
    type: webapp
    path: ./apps/billing-web
    depends_on: [billing-api, billing-domain]   # NEW, optional, defaults to []
```

Additive only — existing manifests parse unchanged. Names must resolve to
projects in the same manifest; loader rejects unknown names with a clear
error.

**Implementation notes:**
- Extend `Project` in [crates/metaphor-workspace/src/lib.rs](../crates/metaphor-workspace/src/lib.rs)
  with `#[serde(default)] pub depends_on: Vec<String>`.
- Add `Manifest::validate()` that checks every `depends_on` entry resolves
  via the existing `find_project` helper.
- Update [docs/workspace.md](workspace.md) with the new field.

**Phase:** A. **Depends on:** nothing.

**Out of scope:**
- Capability-based dependencies (see [Open questions](#open-questions)).
- External (cross-workspace) dependencies.
- Cycle detection beyond a clear error message — fancy cycle reporting
  comes with `metaphor graph`.

---

### 2. `metaphor graph`

**Inspired by:** `nx graph`.

**Problem it solves:** Once `depends_on` exists, users need to *see* the
graph to trust it. Also the prerequisite for graph-aware execution
(topological order, affected).

**CLI surface:**

```
metaphor graph                # text/ASCII rendering, default
metaphor graph --json         # machine-readable; node/edge arrays
metaphor graph --focus <name> # subgraph reachable from one project
```

**Manifest impact:** None (consumes #1).

**Implementation notes:**
- New module `crates/metaphor-cli/src/graph.rs` owns the DAG type, topo
  sort, and renderers.
- Reuse `metaphor_workspace::find_and_load` to locate the manifest from
  any cwd.
- Detect cycles and report them with the offending edges.
- Do **not** pull in a graph-rendering crate for the text view — a small
  hand-rolled indented-tree printer is enough at this scale.

**Phase:** A. **Depends on:** #1.

**Out of scope:**
- Browser-served interactive graph (Nx's selling point — but heavy and
  not a fit for a CLI tool).
- Visual diff between two refs.

---

### 3. `metaphor show projects` / `metaphor show project <name>`

**Inspired by:** `nx show projects`, `nx show project <name>`.

**Problem it solves:** `metaphor list` is great for humans, useless for
scripts. Once the manifest grows fields (`depends_on`, future
capabilities), a JSON inspection command becomes essential for tooling.

**CLI surface:**

```
metaphor show projects                        # like list, but --json supported
metaphor show projects --json
metaphor show project billing-web             # full detail for one project
metaphor show project billing-web --json
```

`metaphor list` stays as the friendly default; `show` is the structured
view.

**Manifest impact:** None.

**Implementation notes:**
- New `Command::Show` variant in
  [crates/metaphor-cli/src/main.rs](../crates/metaphor-cli/src/main.rs).
- Reuses `metaphor_workspace::load` and `Manifest::find_project`.
- JSON output: `serde_json::to_string_pretty(&manifest)` for the projects
  variant; the per-project variant adds the resolved absolute path.

**Phase:** A. **Depends on:** nothing (parallelizable with #1, #2).

**Out of scope:**
- Filtering by tag / type — wait until we have tags.

---

### 4. `metaphor plugins`

**Inspired by:** `nx list` / `nx list <plugin>`.

**Problem it solves:** Today there is no way to ask "what plugins does
this metaphor install actually see?" Discovery is implicit
(`METAPHOR_PLUGIN_BIN_DIR` + `$PATH`) and silent failures are easy to miss.

**CLI surface:**

```
metaphor plugins                  # list discovered plugin binaries + their path
metaphor plugins --json
metaphor plugins <name>           # show one plugin's advertised capabilities
```

**Manifest impact:** None.

**Implementation notes:**
- Walks the same lookup order as
  [crates/metaphor-cli/src/plugin_env.rs](../crates/metaphor-cli/src/plugin_env.rs):
  `$METAPHOR_PLUGIN_BIN_DIR` first, then `$PATH`.
- For "advertised capabilities" we need the in-process plugin registry
  (existing roadmap Phase 4) so plugins implement
  [`ToolPlugin::capabilities()`](plugin-api.md#tool-plugin) rather than
  shelling out. Until that lands, `metaphor plugins` can only show paths
  + version output (`<plugin> --version`).

**Phase:** C (its useful form depends on the existing roadmap Phase 4).

**Depends on:** in-process plugin registry (roadmap Phase 4) for the
capabilities view; the binary-listing variant could ship in Phase A but
isn't worth the surface-area churn alone.

**Out of scope:**
- Installing plugins (`metaphor plugin install`) — separate concern,
  separate plan when needed.

---

### 5. `--all` and `--projects=<a,b>` on every passthrough

**Inspired by:** `nx run-many`.

**Problem it solves:** Today every plugin command operates on whatever
the plugin decides (usually cwd or a flag). We need first-class
"run this command across these projects" semantics.

**CLI surface:** Adds two flags to every passthrough command:

```
metaphor lint --all
metaphor test --projects=billing-api,billing-web
metaphor build --all --parallel=4
metaphor lint --all --continue-on-error
```

When `--all` or `--projects` is present, `metaphor` filters the manifest's
projects and invokes the plugin once per project. Without those flags,
behavior is unchanged (plugin gets called once with the user's args).

**Manifest impact:** None directly; benefits from #1 (graph) for ordering.

**Implementation notes:**
- A small helper in `crates/metaphor-cli/src/run_many.rs` that builds the
  project list from flags and loops `plugin_env::passthrough` /
  `passthrough_raw`.
- Pass the project's absolute path (`Project::resolved_path`) to the
  plugin via `--cwd <path>` or by setting `current_dir` on the spawned
  command — pick one convention and document it.
- `--parallel=N` uses a simple bounded worker pool. No need for `tokio`;
  `std::thread` + a channel is enough.
- `--continue-on-error` switches from fail-fast to "report at the end."

**Phase:** B. **Depends on:** nothing strictly, but better with #1 (topo
order).

**Out of scope:**
- Per-project argument overrides (e.g. "lint with --strict for project X
  only"). Out of scope until users ask.

---

### 6. `--affected --base=<ref>`

**Inspired by:** `nx affected`.

**Problem it solves:** CI shouldn't lint every project on every push.
"Run this only on what changed" is the single highest-leverage feature
Nx has.

**CLI surface:** Another flag on every passthrough:

```
metaphor lint --affected
metaphor test --affected --base=main
metaphor test --affected --base=origin/main --head=HEAD
```

`--affected` implies `--all` filtered to the affected set. Combinable
with `--parallel`.

**Manifest impact:** None (uses #1's `depends_on`).

**Implementation notes:**
- "Affected" = projects whose `path` contains a changed file *plus* their
  reverse dependencies (transitively).
- Changed files come from `git diff --name-only <base>...<head>`; shell
  out via `std::process::Command`.
- New module `crates/metaphor-cli/src/affected.rs`. Reuses the graph
  built for #2.

**Phase:** B. **Depends on:** #1, #2, #5.

**Out of scope:**
- Non-git VCS support.
- "What if there's no `<base>`" beyond a clear error suggesting a default
  (likely `main` or `HEAD~1`).

---

### 7. `metaphor add <name>`

**Inspired by:** `nx add` / `nx generate` for project registration.

**Problem it solves:** Hand-editing `metaphor.yaml` is a friction point
documented in [workspace.md](workspace.md). For a tool whose value is
"managing many projects," requiring manual YAML for every project is
backwards.

**CLI surface:**

```
metaphor add billing-api \
  --type backend-service \
  --path ./services/billing-api \
  --remote git@github.com:acme/billing-api.git \
  --depends-on billing-domain
```

Optional flags: `--remote`, `--depends-on` (repeatable or comma-list).
Required: `--type`, `--path`. Errors on duplicate names, on `--type`
values not in the `ProjectType` enum, and on `--depends-on` names not in
the manifest.

**Manifest impact:** None (writes the existing schema, plus #1's
`depends_on`).

**Implementation notes:**
- Reuses `metaphor_workspace::find_and_load` + `save` (already in the
  workspace crate).
- Round-trips through `serde_yaml`; existing comments in the file are
  **not** preserved (already documented in
  [workspace.md](workspace.md#editing-by-hand)).

**Phase:** B. **Depends on:** #1 only for `--depends-on`; the rest works
today.

**Out of scope:**
- `metaphor remove`, `metaphor rename` — straightforward follow-ups, not
  on this plan.
- Auto-detecting project type from disk.

---

### 8. Task result caching

**Inspired by:** Nx local task cache.

**Problem it solves:** Re-running `metaphor lint --all` on an unchanged
tree should be near-instant.

**CLI surface:** Transparent by default; one opt-out flag:

```
metaphor lint --all                # uses cache
metaphor lint --all --no-cache     # bypasses + does not write
metaphor cache clear               # nukes .metaphor/cache/
metaphor cache stats               # hit rate, size on disk
```

**Manifest impact:** None for the manifest itself. A workspace-level
`.metaphor/cache/` directory is created lazily; should be added to
`.gitignore` (document this).

**Implementation notes:**
- Cache key inputs: plugin binary version (or hash — see Open questions),
  plugin command + subcommand, the project's file tree hash (limited to
  files declared in a future `inputs` field, or the whole project dir as
  v1), and `extra_args`.
- Cache value: stdout, stderr, exit code.
- A cache hit replays stdout/stderr to the user and exits with the
  recorded code.
- New crate or module `metaphor-cache` (separate crate keeps the cache
  dependencies — likely `blake3`, `serde_cbor` — out of the core
  workspace crate).

**Phase:** C. **Depends on:** #5, #6 (no value without run-many), and
ideally the in-process registry (roadmap Phase 4) so cache hashing has a
stable handle on the plugin.

**Out of scope:**
- Remote cache (Nx Cloud equivalent).
- Distributed task execution.
- Caching commands with side effects outside the project tree (e.g.
  `metaphor migration up` — plugins should opt out via a
  `cacheable: false` declaration).

## Phasing summary

| Phase | Items | Goal |
| --- | --- | --- |
| **A — Foundation** | #1, #2, #3 | Project relationships + introspection. Unblocks everything else. |
| **B — Orchestration** | #5, #6, #7 | Run things across many projects, register projects without YAML. The user-visible payoff. |
| **C — Performance & polish** | #4, #8 | Plugin discovery view + caching. Both gated on the existing roadmap Phase 4 (in-process registry) for full value. |

## Cross-cutting concerns

- **Exit semantics for run-many.** Default to fail-fast; `--continue-on-error`
  collects failures and exits non-zero with a summary at the end.
- **Execution order.** `--all` runs in topological order by default. A
  `--parallel=N` flag relaxes this within layers of the DAG.
- **JSON output.** Every introspection command (`list`, `show`, `graph`,
  `plugins`) accepts `--json` with a stable, documented schema. Wrap in
  `{"version": 1, "data": ...}` so we can evolve.
- **What "affected" means without git.** If the workspace isn't a git repo or
  the base ref is missing, `--affected` errors with a clear message —
  no silent fallback to "all" (footgun).

## Open questions

- **`depends_on` by name only, or by capability?** "I depend on whatever
  project produces schemas" is more flexible but harder to debug. Lean
  toward names-only for v1.
- **Cache key: binary hash or version string?** Hash is correct but slow
  on every invocation; version string is fast but trusts plugins. Likely
  default to version string with a `--cache-strict` opt-in.
- **Adopt Nx's "target" concept?** Nx lets a project declare named tasks
  (`build`, `test`, `serve`) with per-project commands. We've been
  one-command-one-plugin so far. Targets would let projects say "my
  `lint` is `cargo clippy`, my `test` is `pytest`" — appealing but a
  significant model shift. Defer to a separate plan.

## See also

- [roadmap.md](roadmap.md) — phase tracker for the project as a whole.
- [workspace.md](workspace.md) — current `metaphor.yaml` schema.
- [plugin-api.md](plugin-api.md) — `GeneratorPlugin` and `ToolPlugin`
  trait surface.
- [architecture.md](architecture.md) — crate layout + dispatch model.
