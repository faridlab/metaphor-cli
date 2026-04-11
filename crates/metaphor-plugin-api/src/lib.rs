//! Plugin API surface for metaphor-cli generators.
//!
//! Plugins are statically compiled into the `metaphor-cli` binary. Each plugin
//! is a Rust struct that knows how to invoke an external tool (a backbone CLI,
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
