use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use metaphor_plugin_api::{GenContext, ResolvedProject};

mod plugin_env;
mod plugins;

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
}

#[derive(Subcommand)]
enum Command {
    /// Initialize a new metaphor workspace in the current directory
    Init,
    /// List projects registered in the current workspace
    List,
    /// Run a generator across project boundaries
    #[command(subcommand)]
    Generate(GenerateCmd),
}

#[derive(Subcommand)]
enum GenerateCmd {
    /// Generate code from a producer module into a consumer project
    Schema {
        /// Producer project name (must be a module that owns the schema)
        #[arg(long)]
        from: String,
        /// Consumer project name (where generated files land)
        #[arg(long)]
        to: String,
        /// Print what would be generated without writing files
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Init => cmd_init(),
        Command::List => cmd_list(),
        Command::Generate(GenerateCmd::Schema { from, to, dry_run }) => {
            cmd_generate_schema(from, to, dry_run)
        }
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

fn cmd_generate_schema(from: String, to: String, dry_run: bool) -> Result<()> {
    let workspace_root = std::env::current_dir()?;
    let manifest = metaphor_workspace::load(&workspace_root)?;

    let producer = manifest.find_project(&from)?;
    let consumer = manifest.find_project(&to)?;

    let producer_resolved = ResolvedProject {
        name: producer.name.clone(),
        project_type: producer.project_type.to_plugin_api(),
        path: producer.resolved_path(&workspace_root),
    };
    let consumer_resolved = ResolvedProject {
        name: consumer.name.clone(),
        project_type: consumer.project_type.to_plugin_api(),
        path: consumer.resolved_path(&workspace_root),
    };

    let ctx = GenContext {
        producer: producer_resolved.clone(),
        consumer: consumer_resolved.clone(),
        workspace_root: workspace_root.clone(),
        dry_run,
    };

    let plugins = plugins::all_plugins();
    let plugin = plugins
        .iter()
        .find(|p| p.handles(producer_resolved.project_type, consumer_resolved.project_type))
        .ok_or_else(|| {
            anyhow!(
                "no plugin handles producer={:?} → consumer={:?}",
                producer_resolved.project_type,
                consumer_resolved.project_type
            )
        })?;

    eprintln!(
        "metaphor generate schema --from {} --to {}{}",
        from,
        to,
        if dry_run { " --dry-run" } else { "" }
    );
    eprintln!("  producer: {} ({})", producer_resolved.name, producer_resolved.path.display());
    eprintln!("  consumer: {} ({})", consumer_resolved.name, consumer_resolved.path.display());
    eprintln!("  plugin:   {}", plugin.name());

    plugin.generate(&ctx)?;
    Ok(())
}
