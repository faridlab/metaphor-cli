//! Plugin: invoke `backbone-schema` for server-side regeneration.
//!
//! When the consumer is the same module as the producer (or another module),
//! we run `backbone-schema schema generate <module> [--dry-run]` against the
//! repository root that contains the producer module's `libs/modules/`.

use anyhow::{bail, Context, Result};
use metaphor_plugin_api::{GenContext, GeneratorPlugin, ProjectType};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::backbone_env::backbone_binary;

pub struct BackboneSchemaPlugin;

impl GeneratorPlugin for BackboneSchemaPlugin {
    fn name(&self) -> &'static str {
        "backbone-schema"
    }

    fn handles(&self, producer: ProjectType, consumer: ProjectType) -> bool {
        producer == ProjectType::Module && consumer == ProjectType::Module
    }

    fn generate(&self, ctx: &GenContext) -> Result<()> {
        let module_name = module_name_from_path(&ctx.producer.path)?;
        let cwd = repo_root_from_module_path(&ctx.producer.path)?;
        let bin = backbone_binary("backbone-schema")?;

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
            bail!("backbone-schema exited with {status}");
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
