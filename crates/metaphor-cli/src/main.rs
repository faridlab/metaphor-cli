use anyhow::Result;
use clap::{Parser, Subcommand};

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
    }
}

fn cmd_init() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let path = metaphor_workspace::init(&cwd)?;
    println!("Initialized empty metaphor workspace at {}", path.display());
    Ok(())
}
