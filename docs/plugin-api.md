# Plugin API

The `metaphor-plugin-api` crate defines the trait surface a plugin implements. This page is the author guide.

## Mental model

> Plugins are statically compiled into the `metaphor-cli` binary. Each plugin is a Rust struct that knows how to invoke an external tool (a metaphor CLI, for example) as a subprocess. There is no dynamic loading and no ABI: a plugin is just code that implements [`GeneratorPlugin`] and gets registered by the CLI dispatcher.

— from the crate-level rustdoc.

In other words: a "plugin" today is a small piece of Rust glue that owns the subprocess invocation for one external tool. The trait surface gives the dispatcher a uniform way to pick the right plugin for a given context and to call it.

The current `main.rs` doesn't go through these traits yet — it calls `plugin_env::passthrough` directly. The trait surface exists so that the upcoming in-process registry (see [roadmap.md](roadmap.md)) can dispatch through it without further breaking changes.

## Two traits

### `GeneratorPlugin` — producer → consumer code generation

```rust
pub trait GeneratorPlugin {
    fn name(&self) -> &'static str;
    fn handles(&self, producer: ProjectType, consumer: ProjectType) -> bool;
    fn generate(&self, ctx: &GenContext) -> anyhow::Result<()>;
}
```

| Method | Purpose |
| --- | --- |
| `name()` | Short label used in logs. Static string. |
| `handles(producer, consumer)` | Predicate the dispatcher uses to pick a plugin. Return `true` if you can generate from `producer` into `consumer`. |
| `generate(ctx)` | Do the work. Respect `ctx.dry_run`. Return `Err` on subprocess failure or invalid context. |

### `ToolPlugin` — single-project operations

```rust
pub trait ToolPlugin {
    fn name(&self) -> &'static str;
    fn handles_project(&self, project_type: ProjectType) -> bool;
    fn capabilities(&self) -> Vec<ToolCapability>;

    fn schema(&self,   _ctx: &ToolContext) -> anyhow::Result<()> { /* default: bail */ }
    fn migrate(&self,  _ctx: &ToolContext) -> anyhow::Result<()> { /* default: bail */ }
    fn seed(&self,     _ctx: &ToolContext) -> anyhow::Result<()> { /* default: bail */ }
    fn lint(&self,     _ctx: &ToolContext) -> anyhow::Result<()> { /* default: bail */ }
    fn test(&self,     _ctx: &ToolContext) -> anyhow::Result<()> { /* default: bail */ }
    fn dev(&self,      _ctx: &ToolContext) -> anyhow::Result<()> { /* default: bail */ }
    fn docs(&self,     _ctx: &ToolContext) -> anyhow::Result<()> { /* default: bail */ }
    fn proto(&self,    _ctx: &ToolContext) -> anyhow::Result<()> { /* default: bail */ }
    fn make(&self,     _ctx: &ToolContext) -> anyhow::Result<()> { /* default: bail */ }
    fn jobs(&self,     _ctx: &ToolContext) -> anyhow::Result<()> { /* default: bail */ }
    fn apps(&self,     _ctx: &ToolContext) -> anyhow::Result<()> { /* default: bail */ }
    fn config(&self,   _ctx: &ToolContext) -> anyhow::Result<()> { /* default: bail */ }
    fn webapp(&self,   _ctx: &ToolContext) -> anyhow::Result<()> { /* default: bail */ }
    fn module(&self,   _ctx: &ToolContext) -> anyhow::Result<()> { /* default: bail */ }
}
```

Every capability method has a default implementation that bails with `<name> not supported by <plugin>`. **Override only the ones you support**, and advertise them via `capabilities()`.

| Method | Capability | Maps to `metaphor` command |
| --- | --- | --- |
| `schema` | `Schema` | `metaphor schema` |
| `migrate` | `Migrate` | `metaphor migration` |
| `seed` | `Seed` | `metaphor seed` |
| `lint` | `Lint` | `metaphor lint` |
| `test` | `Test` | `metaphor test` |
| `dev` | `Dev` | `metaphor dev` |
| `docs` | `Docs` | `metaphor docs` |
| `proto` | `Proto` | `metaphor proto` |
| `make` | `Make` | `metaphor make` |
| `jobs` | `Jobs` | `metaphor jobs` |
| `apps` | `Apps` | `metaphor apps` |
| `config` | `Config` | `metaphor config` |
| `webapp` | `Webapp` | `metaphor webapp` |
| `module` | `Module` | `metaphor module` |

## Context types

### `GenContext`

```rust
pub struct GenContext {
    pub producer: ResolvedProject,
    pub consumer: ResolvedProject,
    pub workspace_root: PathBuf,
    pub dry_run: bool,
}
```

Used for cross-project generation. The producer is "where the schema lives" (today, always a module). The consumer is "where the generated files land."

### `ToolContext`

```rust
pub struct ToolContext {
    pub project: ResolvedProject,
    pub workspace_root: PathBuf,
    pub module: Option<String>,
    pub extra_args: Vec<String>,
    pub dry_run: bool,
}
```

Used for single-project operations. `extra_args` is forwarded verbatim to the underlying tool — preserve hyphen-prefixed values.

### `ResolvedProject`

```rust
pub struct ResolvedProject {
    pub name: String,
    pub project_type: ProjectType,
    pub path: PathBuf,    // always absolute
}
```

The `path` here has already been resolved against the workspace root. You don't need to re-resolve.

### `ProjectType`

Mirror of `metaphor_workspace::ProjectType`, duplicated so the plugin-api crate stays dependency-free:

```rust
pub enum ProjectType {
    BackendService, Webservice, Webapp, Mobileapp, Desktopapp,
    Module, Crate, CliTool, Infra, DocsSite,
}
```

The workspace crate provides `to_plugin_api()` to convert.

## The `dry_run` contract

> When true, plugins must not write any files.

Two acceptable strategies:

1. **Pass it through.** If your underlying tool has its own `--dry-run`, forward it.
2. **Print and skip.** If not, print the command you *would* have run and return `Ok(())` without spawning.

Either way: never write files when `dry_run` is `true`. Reads (loading config, parsing schema for validation) are fine.

## Worked example: a minimal `ToolPlugin`

```rust
use metaphor_plugin_api::{ProjectType, ToolCapability, ToolContext, ToolPlugin};
use std::process::Command;

pub struct CargoTestPlugin;

impl ToolPlugin for CargoTestPlugin {
    fn name(&self) -> &'static str { "cargo-test" }

    fn handles_project(&self, t: ProjectType) -> bool {
        matches!(t, ProjectType::Crate | ProjectType::CliTool | ProjectType::BackendService)
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Test]
    }

    fn test(&self, ctx: &ToolContext) -> anyhow::Result<()> {
        if ctx.dry_run {
            println!("would run: cargo test (in {})", ctx.project.path.display());
            return Ok(());
        }
        let status = Command::new("cargo")
            .arg("test")
            .args(&ctx.extra_args)
            .current_dir(&ctx.project.path)
            .status()?;
        if !status.success() {
            anyhow::bail!("cargo test failed: {status}");
        }
        Ok(())
    }
}
```

A minimal `GeneratorPlugin` looks similar — implement `handles` to gate on `(producer, consumer)` pairs and `generate` to do the work, respecting `ctx.dry_run`.

## Today vs. tomorrow

Today, `crates/metaphor-cli/src/main.rs` doesn't dispatch through these traits. It calls `plugin_env::passthrough` directly, with the plugin binary name and forwarded arguments. The traits exist for the upcoming in-process plugin registry (Phase 4 — see [roadmap.md](roadmap.md)), at which point each subcommand arm will resolve a plugin via `capabilities()` and `handles_project()` instead of hard-coding the binary name.

If you're writing a plugin **right now**, the path of least resistance is:

1. Build it as an external binary that follows the existing argument convention (subcommand prefix, then args).
2. Drop it on `$PATH` or in `$METAPHOR_PLUGIN_BIN_DIR`.
3. If your plugin needs a new top-level subcommand on `metaphor`, add an arm in `Command` and the dispatch `match` in `main.rs`.
4. When the in-process registry lands, you'll re-implement steps 1–3 as a `ToolPlugin` impl with no user-visible change.
