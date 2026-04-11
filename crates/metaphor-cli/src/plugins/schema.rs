//! Plugin: invoke `metaphor-schema` for both server-side regeneration and
//! Kotlin Multiplatform mobile codegen.
//!
//! After the mobilegen → schema merge, a single binary handles both flows
//! through two subcommands:
//!
//!   - `metaphor-schema schema generate <module>` for `Module → Module`
//!     (Rust, SQL, handlers, dto, etc. — the 31 server-side targets)
//!   - `metaphor-schema kotlin generate <module>` for `Module → Mobileapp`
//!     (KMP entities, repositories, view-models, etc.)
//!
//! Metaphor's user-facing CLI is unchanged. The plugin picks the right
//! subcommand based on the consumer project type.

use anyhow::{bail, Context, Result};
use metaphor_plugin_api::{GenContext, GeneratorPlugin, ProjectType};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::plugin_env::plugin_binary;

pub const BINARY_NAME: &str = "metaphor-schema";

pub struct SchemaPlugin;

impl GeneratorPlugin for SchemaPlugin {
    fn name(&self) -> &'static str {
        BINARY_NAME
    }

    fn handles(&self, producer: ProjectType, consumer: ProjectType) -> bool {
        producer == ProjectType::Module
            && (consumer == ProjectType::Module || consumer == ProjectType::Mobileapp)
    }

    fn generate(&self, ctx: &GenContext) -> Result<()> {
        match ctx.consumer.project_type {
            ProjectType::Module => self.generate_server(ctx),
            ProjectType::Mobileapp => self.generate_kotlin(ctx),
            other => bail!("schema plugin does not handle consumer type {other:?}"),
        }
    }
}

impl SchemaPlugin {
    fn generate_server(&self, ctx: &GenContext) -> Result<()> {
        let module_name = module_name_from_path(&ctx.producer.path)?;
        let cwd = repo_root_from_module_path(&ctx.producer.path)?;
        let bin = plugin_binary(BINARY_NAME)?;

        let mut cmd = Command::new(&bin);
        cmd.current_dir(&cwd)
            .args(["schema", "generate", &module_name]);
        if ctx.dry_run {
            cmd.arg("--dry-run");
        }

        eprintln!(
            "[{}] running: {} schema generate {}{} (cwd: {})",
            self.name(),
            bin.display(),
            module_name,
            if ctx.dry_run { " --dry-run" } else { "" },
            cwd.display()
        );

        let status = cmd
            .status()
            .with_context(|| format!("failed to spawn {}", bin.display()))?;
        if !status.success() {
            bail!("{BINARY_NAME} schema exited with {status}");
        }
        Ok(())
    }

    fn generate_kotlin(&self, ctx: &GenContext) -> Result<()> {
        let module_name = module_name_from_path(&ctx.producer.path)?;
        let modules_parent = modules_parent_from_module_path(&ctx.producer.path)?;
        let bin = plugin_binary(BINARY_NAME)?;

        // The kotlin subcommand has no native --dry-run. When metaphor's
        // --dry-run is requested, print the command instead of executing.
        let consumer_path_str = ctx.consumer.path.to_string_lossy().to_string();
        let modules_parent_str = modules_parent.to_string_lossy().to_string();

        let args: Vec<String> = vec![
            "kotlin".into(),
            "generate".into(),
            module_name.clone(),
            "--module-path".into(),
            modules_parent_str.clone(),
            "--output".into(),
            consumer_path_str.clone(),
        ];

        if ctx.dry_run {
            let rendered = format!(
                "{} {}",
                bin.display(),
                args.iter()
                    .map(|a| shell_quote(a))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            eprintln!(
                "[{}] dry-run: would invoke (kotlin subcommand has no --dry-run):\n  {rendered}",
                self.name()
            );
            return Ok(());
        }

        eprintln!(
            "[{}] running: {} kotlin generate {} --module-path {} --output {}",
            self.name(),
            bin.display(),
            module_name,
            modules_parent_str,
            consumer_path_str,
        );

        let status = Command::new(&bin)
            .args(&args)
            .status()
            .with_context(|| format!("failed to spawn {}", bin.display()))?;
        if !status.success() {
            bail!("{BINARY_NAME} kotlin exited with {status}");
        }
        Ok(())
    }
}

fn module_name_from_path(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .with_context(|| format!("could not derive module name from {}", path.display()))
}

/// Given `<root>/libs/modules/<module>`, return `<root>`.
fn repo_root_from_module_path(path: &Path) -> Result<PathBuf> {
    let modules = path
        .parent()
        .with_context(|| format!("no parent for module path {}", path.display()))?;
    let libs = modules
        .parent()
        .with_context(|| format!("no parent for modules dir {}", modules.display()))?;
    let root = libs
        .parent()
        .with_context(|| format!("no parent for libs dir {}", libs.display()))?;
    Ok(root.to_path_buf())
}

/// Given `<root>/libs/modules/<module>`, return `<root>/libs/modules`.
fn modules_parent_from_module_path(path: &Path) -> Result<PathBuf> {
    path.parent()
        .map(|p| p.to_path_buf())
        .with_context(|| format!("no parent for module path {}", path.display()))
}

fn shell_quote(s: &str) -> String {
    if s.chars().all(|c| c.is_alphanumeric() || "/._-".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}
