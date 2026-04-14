//! Plugin API surface for metaphor-cli generators.
//!
//! Plugins are statically compiled into the `metaphor-cli` binary. Each plugin
//! is a Rust struct that knows how to invoke an external tool (a metaphor CLI,
//! for example) as a subprocess. There is no dynamic loading and no ABI: a
//! plugin is just code that implements [`GeneratorPlugin`] and gets registered
//! by the CLI dispatcher.

use std::path::PathBuf;

/// Context passed to a generator plugin invocation.
///
/// Carries the resolved producer and consumer project info plus a `dry_run`
/// flag. Plugins should respect `dry_run` either by passing it through to the
/// underlying tool (when supported) or by printing the command they *would*
/// have run instead of running it.
#[derive(Debug, Clone)]
pub struct GenContext {
    /// The producer project (where the schema lives). Always a module today.
    pub producer: ResolvedProject,
    /// The consumer project (where generated files land).
    pub consumer: ResolvedProject,
    /// Workspace root directory containing `metaphor.yaml`.
    pub workspace_root: PathBuf,
    /// When true, plugins must not write any files.
    pub dry_run: bool,
}

/// A workspace project after path resolution. The `path` is always absolute.
#[derive(Debug, Clone)]
pub struct ResolvedProject {
    pub name: String,
    pub project_type: ProjectType,
    pub path: PathBuf,
}

/// Project type. Mirrors `metaphor_workspace::ProjectType` so plugin-api stays
/// dependency-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectType {
    BackendService,
    Webservice,
    Webapp,
    Mobileapp,
    Desktopapp,
    Module,
    Crate,
    CliTool,
    Infra,
    DocsSite,
}

/// A plugin that can run a code generator from a producer into a consumer.
pub trait GeneratorPlugin {
    /// Short name used in logs.
    fn name(&self) -> &'static str;

    /// True if this plugin can handle generating from `producer` into
    /// `consumer`. Used by the dispatcher to pick a plugin.
    fn handles(&self, producer: ProjectType, consumer: ProjectType) -> bool;

    /// Run the generator. Returns Ok(()) on success, Err on subprocess
    /// failure or invalid context.
    fn generate(&self, ctx: &GenContext) -> anyhow::Result<()>;
}

// ============================================================================
// ToolPlugin — extended plugin API for non-generation commands
// ============================================================================

/// Context passed to a tool plugin invocation.
///
/// Unlike [`GenContext`] (which models producer→consumer generation), this
/// models a single-project operation (migrate, seed, lint, etc.).
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// The target project.
    pub project: ResolvedProject,
    /// Workspace root directory (or project root in standalone mode).
    pub workspace_root: PathBuf,
    /// Optional module name within the project.
    pub module: Option<String>,
    /// Extra CLI arguments forwarded verbatim to the underlying tool.
    pub extra_args: Vec<String>,
    /// When true, the plugin must not write any files.
    pub dry_run: bool,
}

/// Capabilities a [`ToolPlugin`] can advertise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCapability {
    Schema,
    Migrate,
    Seed,
    Lint,
    Test,
    Dev,
    Docs,
    Proto,
    Make,
    Jobs,
    Apps,
    Config,
    Webapp,
    Module,
}

/// A plugin that supports project-level tool operations beyond code generation.
///
/// Default implementations bail with "not supported" so plugins can opt in to
/// only the capabilities they handle.
pub trait ToolPlugin {
    /// Short name used in logs.
    fn name(&self) -> &'static str;

    /// True if this plugin can handle the given project type.
    fn handles_project(&self, project_type: ProjectType) -> bool;

    /// Declared capabilities.
    fn capabilities(&self) -> Vec<ToolCapability>;

    fn schema(&self, _ctx: &ToolContext) -> anyhow::Result<()> {
        anyhow::bail!("schema not supported by {}", self.name())
    }
    fn migrate(&self, _ctx: &ToolContext) -> anyhow::Result<()> {
        anyhow::bail!("migrate not supported by {}", self.name())
    }
    fn seed(&self, _ctx: &ToolContext) -> anyhow::Result<()> {
        anyhow::bail!("seed not supported by {}", self.name())
    }
    fn lint(&self, _ctx: &ToolContext) -> anyhow::Result<()> {
        anyhow::bail!("lint not supported by {}", self.name())
    }
    fn test(&self, _ctx: &ToolContext) -> anyhow::Result<()> {
        anyhow::bail!("test not supported by {}", self.name())
    }
    fn dev(&self, _ctx: &ToolContext) -> anyhow::Result<()> {
        anyhow::bail!("dev not supported by {}", self.name())
    }
    fn docs(&self, _ctx: &ToolContext) -> anyhow::Result<()> {
        anyhow::bail!("docs not supported by {}", self.name())
    }
    fn proto(&self, _ctx: &ToolContext) -> anyhow::Result<()> {
        anyhow::bail!("proto not supported by {}", self.name())
    }
    fn make(&self, _ctx: &ToolContext) -> anyhow::Result<()> {
        anyhow::bail!("make not supported by {}", self.name())
    }
    fn jobs(&self, _ctx: &ToolContext) -> anyhow::Result<()> {
        anyhow::bail!("jobs not supported by {}", self.name())
    }
    fn apps(&self, _ctx: &ToolContext) -> anyhow::Result<()> {
        anyhow::bail!("apps not supported by {}", self.name())
    }
    fn config(&self, _ctx: &ToolContext) -> anyhow::Result<()> {
        anyhow::bail!("config not supported by {}", self.name())
    }
    fn webapp(&self, _ctx: &ToolContext) -> anyhow::Result<()> {
        anyhow::bail!("webapp not supported by {}", self.name())
    }
    fn module(&self, _ctx: &ToolContext) -> anyhow::Result<()> {
        anyhow::bail!("module not supported by {}", self.name())
    }
}
