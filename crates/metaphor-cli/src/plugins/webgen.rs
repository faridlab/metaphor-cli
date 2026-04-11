//! Plugin: invoke `metaphor-plugin-webgen` for webapp (TS+React) codegen.
//!
//! `metaphor-plugin-webgen` supports a native `--dry-run` flag, so we pass
//! it through directly when requested.

use anyhow::{bail, Context, Result};
use metaphor_plugin_api::{GenContext, GeneratorPlugin, ProjectType};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::plugin_env::plugin_binary;

pub const BINARY_NAME: &str = "metaphor-plugin-webgen";

pub struct WebgenPlugin;

impl GeneratorPlugin for WebgenPlugin {
    fn name(&self) -> &'static str {
        BINARY_NAME
    }

    fn handles(&self, producer: ProjectType, consumer: ProjectType) -> bool {
        producer == ProjectType::Module && consumer == ProjectType::Webapp
    }

    fn generate(&self, ctx: &GenContext) -> Result<()> {
        let module_name = module_name_from_path(&ctx.producer.path)?;
        let modules_dir = modules_parent_from_module_path(&ctx.producer.path)?;
        let bin = plugin_binary(BINARY_NAME)?;

        let consumer_path_str = ctx.consumer.path.to_string_lossy().to_string();
        let modules_dir_str = modules_dir.to_string_lossy().to_string();

        // Always pass --enhanced: schemas in this ecosystem are YAML, not
        // compiled .proto files. Without this flag, webgen looks for a
        // <module>/proto/ directory and errors out.
        let mut args: Vec<String> = vec![
            "generate".into(),
            module_name.clone(),
            "--modules-dir".into(),
            modules_dir_str.clone(),
            "--output".into(),
            consumer_path_str.clone(),
            "--enhanced".into(),
        ];
        if ctx.dry_run {
            args.push("--dry-run".into());
        }

        eprintln!(
            "[{}] running: {} generate {} --modules-dir {} --output {} --enhanced{}",
            self.name(),
            bin.display(),
            module_name,
            modules_dir_str,
            consumer_path_str,
            if ctx.dry_run { " --dry-run" } else { "" },
        );

        let status = Command::new(&bin)
            .args(&args)
            .status()
            .with_context(|| format!("failed to spawn {}", bin.display()))?;
        if !status.success() {
            bail!("{BINARY_NAME} exited with {status}");
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

fn modules_parent_from_module_path(path: &Path) -> Result<PathBuf> {
    path.parent()
        .map(|p| p.to_path_buf())
        .with_context(|| format!("no parent for module path {}", path.display()))
}
