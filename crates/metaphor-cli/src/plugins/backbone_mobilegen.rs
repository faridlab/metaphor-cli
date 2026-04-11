//! Plugin: invoke `backbone-mobilegen` for mobile (KMP) client codegen.
//!
//! `backbone-mobilegen` does not support a `--dry-run` flag. When the user
//! passes `--dry-run` to metaphor, this plugin prints the command that *would*
//! have been run instead of running it. This keeps metaphor's --dry-run
//! semantics consistent across plugins regardless of underlying tool support.

use anyhow::{bail, Context, Result};
use metaphor_plugin_api::{GenContext, GeneratorPlugin, ProjectType};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::backbone_env::backbone_binary;

pub struct BackboneMobilegenPlugin;

impl GeneratorPlugin for BackboneMobilegenPlugin {
    fn name(&self) -> &'static str {
        "backbone-mobilegen"
    }

    fn handles(&self, producer: ProjectType, consumer: ProjectType) -> bool {
        producer == ProjectType::Module && consumer == ProjectType::Mobileapp
    }

    fn generate(&self, ctx: &GenContext) -> Result<()> {
        let module_name = module_name_from_path(&ctx.producer.path)?;
        let modules_parent = modules_parent_from_module_path(&ctx.producer.path)?;
        let bin = backbone_binary("backbone-mobilegen")?;

        let consumer_path_str = ctx.consumer.path.to_string_lossy().to_string();
        let modules_parent_str = modules_parent.to_string_lossy().to_string();

        let args: Vec<String> = vec![
            "--module".into(),
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
                "[{}] dry-run: would invoke (no --dry-run support in tool):\n  {rendered}",
                self.name()
            );
            return Ok(());
        }

        eprintln!(
            "[{}] running: {} --module {} --module-path {} --output {}",
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
            bail!("backbone-mobilegen exited with {status}");
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
