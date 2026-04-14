//! Metaphor CLI — orchestrate independent project repos.
//!
//! Metaphor is a meta-CLI that manages a workspace of standalone project repos
//! and helps them work together. Each project keeps its own git history;
//! Metaphor coordinates scaffolding, code generation, and runtime wiring.
//!
//! All commands delegate to plugin binaries via subprocess:
//!   - `metaphor-schema`  — schema, webapp
//!   - `metaphor-codegen` — make, module, apps, proto, migration, seed
//!   - `metaphor-dev`     — dev, lint, test, docs, config, jobs

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::*;

mod plugin_env;

#[derive(Parser)]
#[command(
    name = "metaphor",
    version,
    about = "Orchestrate independent project repos",
    long_about = "Metaphor is a meta-CLI that manages a workspace of standalone project repos\n\
                  and helps them work together. Each project keeps its own git history;\n\
                  Metaphor coordinates scaffolding, code generation, and runtime wiring."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize a new metaphor workspace in the current directory
    Init,

    /// List projects registered in the current workspace
    List,

    // ====================================================================
    // metaphor-schema plugin
    // ====================================================================

    /// Schema parsing and code generation
    ///
    /// Passthrough to metaphor-schema plugin.
    /// Run `metaphor schema --help` for full details.
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Schema {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Webapp code generation commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Webapp {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    // ====================================================================
    // metaphor-codegen plugin
    // ====================================================================

    /// Laravel-style scaffolding commands (make:*)
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Make {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Module management commands (scaffolding)
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Module {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Application management commands (scaffolding)
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Apps {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Protocol buffer commands (buf/tonic operations)
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Proto {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Database migration commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Migration {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Database seeding commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Seed {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    // ====================================================================
    // metaphor-dev plugin
    // ====================================================================

    /// Development workflow commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Dev {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Code quality and linting commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Lint {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Test generation and management commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Test {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Documentation generation commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Docs {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Configuration validation and management commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Config {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Job scheduling commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Jobs {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.verbose {
        std::env::set_var("RUST_LOG", "debug");
        env_logger::init();
    }

    println!("{}", "⚡ Metaphor CLI".bright_green().bold());
    println!("{}", "Orchestrate independent project repos".cyan());
    println!();

    match &cli.command {
        // Core workspace commands
        Command::Init => cmd_init(),
        Command::List => cmd_list(),

        // metaphor-schema plugin
        Command::Schema { args } => plugin_env::passthrough_raw("metaphor-schema", args),
        Command::Webapp { args } => plugin_env::passthrough("metaphor-schema", "generate:webapp", args),

        // metaphor-codegen plugin
        Command::Make { args } => plugin_env::passthrough("metaphor-codegen", "make", args),
        Command::Module { args } => plugin_env::passthrough("metaphor-codegen", "module", args),
        Command::Apps { args } => plugin_env::passthrough("metaphor-codegen", "apps", args),
        Command::Proto { args } => plugin_env::passthrough("metaphor-codegen", "proto", args),
        Command::Migration { args } => plugin_env::passthrough("metaphor-codegen", "migration", args),
        Command::Seed { args } => plugin_env::passthrough("metaphor-codegen", "seed", args),

        // metaphor-dev plugin
        Command::Dev { args } => plugin_env::passthrough("metaphor-dev", "dev", args),
        Command::Lint { args } => plugin_env::passthrough("metaphor-dev", "lint", args),
        Command::Test { args } => plugin_env::passthrough("metaphor-dev", "test", args),
        Command::Docs { args } => plugin_env::passthrough("metaphor-dev", "docs", args),
        Command::Config { args } => plugin_env::passthrough("metaphor-dev", "config", args),
        Command::Jobs { args } => plugin_env::passthrough("metaphor-dev", "jobs", args),
    }
}

fn cmd_init() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let path = metaphor_workspace::init(&cwd)?;
    println!("Initialized empty metaphor workspace at {}", path.display());
    Ok(())
}

fn cmd_list() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let manifest = metaphor_workspace::load(&cwd)?;
    if manifest.projects.is_empty() {
        println!("No projects registered.");
        return Ok(());
    }
    println!("{} project(s):", manifest.projects.len());
    for p in &manifest.projects {
        let remote = p.remote.as_deref().unwrap_or("(no remote)");
        println!(
            "  - {} [{:?}] path={} remote={}",
            p.name, p.project_type, p.path, remote
        );
    }
    Ok(())
}
