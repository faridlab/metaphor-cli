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
//!   - `metaphor-agent`   — agent (Claude Code skills & subagents)

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::*;

mod affected;
mod cache;
mod cmd_add;
mod cmd_build;
mod cmd_clean;
mod cmd_compose;
mod cmd_doctor;
mod cmd_env;
mod cmd_info;
mod cmd_plugin_add;
mod cmd_plugins;
mod cmd_sync;
mod graph;
mod plugin_env;
mod repl;
mod run_many;

#[derive(Parser)]
#[command(
    name = "metaphor",
    version,
    about = "Orchestrate independent project repos",
    long_about = "Metaphor is a meta-CLI that manages a workspace of standalone project repos\n\
                  and helps them work together. Each project keeps its own git history;\n\
                  Metaphor coordinates scaffolding, code generation, and runtime wiring."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize a new metaphor workspace in the current directory
    Init,

    /// List projects registered in the current workspace
    List,

    /// Print the project dependency graph
    Graph {
        /// Emit machine-readable JSON instead of a text tree
        #[arg(long)]
        json: bool,

        /// Show only the subgraph reachable from this project
        #[arg(long, value_name = "NAME")]
        focus: Option<String>,
    },

    /// Inspect registered projects (JSON-friendly alternative to `list`)
    #[command(subcommand)]
    Show(ShowCommand),

    /// Summarize the workspace + which project cwd is inside
    Info {
        #[arg(long)]
        json: bool,
    },

    /// Run diagnostic checks against the workspace (paths, plugins, tools, conventions)
    Doctor {
        #[arg(long)]
        json: bool,
    },

    /// Enter an interactive REPL. Also the default when `metaphor` is run bare on a TTY.
    Repl,

    /// Register a new project in the current workspace
    Add {
        /// Project name (must be unique within the workspace)
        name: String,

        /// Project type
        #[arg(long, value_enum)]
        project_type: cmd_add::CliProjectType,

        /// Project path (absolute or relative to workspace root)
        #[arg(long)]
        path: String,

        /// Git remote URL
        #[arg(long)]
        remote: Option<String>,

        /// Git ref to pin (tag, branch, or commit hash)
        #[arg(long = "ref")]
        git_ref: Option<String>,

        /// Other project names this one depends on (repeatable or comma-separated)
        #[arg(long = "depends-on", value_delimiter = ',')]
        depends_on: Vec<String>,

        /// Clone the remote into the project path immediately
        #[arg(long)]
        clone: bool,
    },

    /// Clone or update remote projects to their pinned ref
    Sync {
        /// Re-resolve refs even if metaphor.lock already has an entry
        #[arg(long)]
        update: bool,

        /// Only sync these projects (comma-separated)
        #[arg(long, value_delimiter = ',')]
        projects: Vec<String>,
    },

    /// Manage plugin binaries (install, list)
    #[command(subcommand)]
    Plugin(PluginCommand),

    /// List plugin binaries visible to this metaphor install
    ///
    /// Retained for back-compat; equivalent to `metaphor plugin list`.
    #[command(hide = true)]
    Plugins {
        #[arg(long)]
        json: bool,
    },

    /// Manage the task result cache
    #[command(subcommand)]
    Cache(CacheCommand),

    /// Build Docker images for each project (docker build per project)
    Build {
        #[command(flatten)]
        flags: cmd_build::BuildFlags,
    },

    /// Generate workspace-level docker-compose.yml from per-project fragments
    #[command(subcommand)]
    Compose(ComposeCommand),

    /// Env-var schema validation (reads metaphor.env.yaml per project)
    #[command(subcommand)]
    Env(EnvCommand),

    /// Local docker compose lifecycle (up, down, logs, ps, restart, pull, build)
    ///
    /// Passthrough to metaphor-dev plugin. Reads `metaphor.deploy.yaml`.
    /// Run `metaphor docker --help` for full details.
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Docker {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Remote deployment (push, rollback, status, logs, migrate, exec)
    ///
    /// Passthrough to metaphor-dev plugin. Reads `metaphor.deploy.yaml`
    /// (or, for `deploy exec`, the workspace's `infra` project).
    /// Run `metaphor deploy --help` for full details.
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Deploy {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Remove stale build-artifact directories across projects
    Clean {
        /// Only consider directories older than this (e.g. 6h, 30d, 6w, 2m, 1y)
        #[arg(long, default_value = "30d")]
        older_than: String,

        /// Limit to the named projects (comma-separated)
        #[arg(long, value_delimiter = ',')]
        projects: Vec<String>,

        /// Actually delete. Without this, the command prints what would be freed.
        #[arg(long)]
        apply: bool,

        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,

        /// Skip per-directory sizing — faster on huge trees; reported sizes read as 0
        #[arg(long)]
        quick: bool,

        /// With --apply, refuse to delete more than this much without --yes (e.g. 10GB, 500MB)
        #[arg(long, value_name = "SIZE")]
        confirm_over: Option<String>,

        /// Bypass --confirm-over thresholds
        #[arg(long)]
        yes: bool,

        /// Also reclaim this workspace's Docker build-cache volumes (e.g. the dev
        /// stack's cargo_target). Scoped to the workspace compose project; data
        /// volumes (pgdata, miniodata, …) are never touched.
        #[arg(long)]
        docker: bool,

        /// With --docker, also empty volumes currently in use by a running
        /// container (forces a rebuild). Otherwise in-use volumes are only reported.
        #[arg(long)]
        include_running: bool,
    },

    // ====================================================================
    // metaphor-schema plugin
    // ====================================================================
    /// Schema parsing and code generation
    ///
    /// Passthrough to metaphor-schema plugin.
    /// Run `metaphor schema --help` for full details.
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Schema {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Webapp code generation commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Webapp {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    // ====================================================================
    // metaphor-codegen plugin
    // ====================================================================
    /// Laravel-style scaffolding commands (make:*)
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Make {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Module management commands (scaffolding)
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Module {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Application management commands (scaffolding)
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Apps {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Protocol buffer commands (buf/tonic operations)
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Proto {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Database migration commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Migration {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Database seeding commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Seed {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// List HTTP routes defined in the project
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Routes {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    // ====================================================================
    // metaphor-dev plugin
    // ====================================================================
    /// Development workflow commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Dev {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Code quality and linting commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Lint {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Test generation and management commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Test {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Documentation generation commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Docs {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Configuration validation and management commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Config {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Job scheduling commands
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Jobs {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    // ====================================================================
    // metaphor-agent plugin (Claude Code skills & subagents)
    // ====================================================================
    /// Install Claude Code skills and subagents into a project's .claude/
    ///
    /// Passthrough to metaphor-agent plugin.
    /// Run `metaphor agent --help` for full details.
    #[command(trailing_var_arg = true, allow_external_subcommands = true)]
    Agent {
        #[command(flatten)]
        run: run_many::RunFlags,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum PluginCommand {
    /// Install a known plugin binary from its GitHub release
    Add {
        /// Plugin spec: <name>[@<version>], e.g. metaphor-dev@latest or metaphor-dev@0.1.0
        spec: String,
    },
    /// List plugin binaries visible to this metaphor install
    List {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum ComposeCommand {
    /// Merge each project's compose.fragment.yml into a single docker-compose.yml
    Generate {
        /// Output path (default: <workspace_root>/docker-compose.yml)
        #[arg(long)]
        out: Option<String>,
        /// Write to disk. Without this, generated YAML prints to stdout.
        #[arg(long)]
        write: bool,
    },
}

#[derive(Subcommand)]
pub enum EnvCommand {
    /// Validate that every required env var is present for each project
    Check {
        /// Limit to these projects
        #[arg(long, value_delimiter = ',')]
        projects: Vec<String>,
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum CacheCommand {
    /// Remove all cached task entries
    Clear,
    /// Show cache location, entry count, and total size
    Stats {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum ShowCommand {
    /// List every project (respects --json)
    Projects {
        #[arg(long)]
        json: bool,
    },
    /// Show one project by name (respects --json).
    /// If <name> is omitted, uses the project detected from cwd.
    Project {
        name: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

pub fn print_banner() {
    println!("{}", "⚡ Metaphor CLI".bright_green().bold());
    println!("{}", "Orchestrate independent project repos".cyan());
    println!();
}

fn main() -> Result<()> {
    // Bare invocation on a TTY enters the interactive REPL. Anywhere
    // stdin/stdout is piped (CI, `metaphor | less`, ...) stays
    // script-friendly: clap prints standard help and exits.
    let raw: Vec<String> = std::env::args().collect();
    let bare = raw.len() == 1;
    if bare && is_interactive_tty() {
        print_banner();
        return repl::run();
    }

    let cli = Cli::parse();

    if cli.verbose {
        std::env::set_var("RUST_LOG", "debug");
        env_logger::init();
    }

    print_banner();
    dispatch(&cli)
}

fn is_interactive_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Dispatch a parsed `Cli` to its subcommand. Split out from `main` so the
/// REPL can re-enter dispatch after parsing each typed line.
pub fn dispatch(cli: &Cli) -> Result<()> {
    match &cli.command {
        // Core workspace commands
        Command::Init => cmd_init(),
        Command::List => cmd_list(),
        Command::Graph { json, focus } => cmd_graph(*json, focus.as_deref()),
        Command::Show(sub) => cmd_show(sub),
        Command::Info { json } => {
            let cwd = std::env::current_dir()?;
            let (manifest, root) = metaphor_workspace::find_and_load(&cwd)?;
            cmd_info::cmd_info(&manifest, &root, &cwd, *json)
        }
        Command::Doctor { json } => {
            let cwd = std::env::current_dir()?;
            let (manifest, root) = metaphor_workspace::find_and_load(&cwd)?;
            cmd_doctor::cmd_doctor(&manifest, &root, *json)
        }
        Command::Repl => repl::run(),
        Command::Add {
            name,
            project_type,
            path,
            remote,
            git_ref,
            depends_on,
            clone,
        } => cmd_add::cmd_add(cmd_add::AddArgs {
            name,
            project_type: *project_type,
            path,
            remote: remote.as_deref(),
            git_ref: git_ref.as_deref(),
            depends_on,
            clone: *clone,
        }),
        Command::Sync { update, projects } => {
            let cwd = std::env::current_dir()?;
            let (manifest, root) = metaphor_workspace::find_and_load(&cwd)?;
            cmd_sync::cmd_sync(
                &manifest,
                &root,
                &cmd_sync::SyncOptions {
                    update: *update,
                    projects: projects.clone(),
                },
            )
        }
        Command::Plugin(PluginCommand::Add { spec }) => cmd_plugin_add::cmd_plugin_add(spec),
        Command::Plugin(PluginCommand::List { json }) => cmd_plugins::cmd_plugins(*json),
        Command::Plugins { json } => cmd_plugins::cmd_plugins(*json),
        Command::Cache(sub) => cmd_cache(sub),
        Command::Build { flags } => {
            let cwd = std::env::current_dir()?;
            let (manifest, root) = metaphor_workspace::find_and_load(&cwd)?;
            cmd_build::cmd_build(&manifest, &root, flags)
        }
        Command::Compose(ComposeCommand::Generate { out, write }) => {
            let cwd = std::env::current_dir()?;
            let (manifest, root) = metaphor_workspace::find_and_load(&cwd)?;
            let output = match out {
                Some(p) => std::path::PathBuf::from(p),
                None => cmd_compose::default_output(&root),
            };
            cmd_compose::cmd_compose_generate(
                &manifest,
                &root,
                &cmd_compose::ComposeOptions {
                    output: &output,
                    dry_run: !*write,
                },
            )
        }
        Command::Env(EnvCommand::Check { projects, json }) => {
            let cwd = std::env::current_dir()?;
            let (manifest, root) = metaphor_workspace::find_and_load(&cwd)?;
            let filter = if projects.is_empty() {
                None
            } else {
                Some(projects.as_slice())
            };
            cmd_env::cmd_env_check(
                &manifest,
                &root,
                &cmd_env::EnvCheckOptions {
                    project_filter: filter,
                    json: *json,
                },
            )
        }
        Command::Clean {
            older_than,
            projects,
            apply,
            json,
            quick,
            confirm_over,
            yes,
            docker,
            include_running,
        } => {
            let cwd = std::env::current_dir()?;
            let (manifest, root) = metaphor_workspace::find_and_load(&cwd)?;
            let duration = cmd_clean::parse_older_than(older_than)?;
            let filter = if projects.is_empty() {
                None
            } else {
                Some(projects.as_slice())
            };
            let threshold = match confirm_over {
                Some(s) => Some(cmd_clean::parse_size(s)?),
                None => None,
            };
            cmd_clean::cmd_clean(
                &manifest,
                &root,
                &cmd_clean::CleanOptions {
                    older_than: duration,
                    project_filter: filter,
                    apply: *apply,
                    json: *json,
                    quick: *quick,
                    confirm_over: threshold,
                    yes: *yes,
                    docker: *docker,
                    include_running: *include_running,
                },
            )
        }

        // metaphor-schema plugin
        Command::Schema { run, args } => dispatch_plugin("metaphor-schema", None, args, run),
        Command::Webapp { run, args } => {
            dispatch_plugin("metaphor-schema", Some("generate:webapp"), args, run)
        }

        // metaphor-codegen plugin
        Command::Make { run, args } => dispatch_plugin("metaphor-codegen", Some("make"), args, run),
        Command::Module { run, args } => {
            dispatch_plugin("metaphor-codegen", Some("module"), args, run)
        }
        Command::Apps { run, args } => dispatch_plugin("metaphor-codegen", Some("apps"), args, run),
        Command::Proto { run, args } => {
            dispatch_plugin("metaphor-codegen", Some("proto"), args, run)
        }
        Command::Migration { run, args } => {
            dispatch_plugin("metaphor-codegen", Some("migration"), args, run)
        }
        Command::Seed { run, args } => dispatch_plugin("metaphor-codegen", Some("seed"), args, run),
        Command::Routes { run, args } => dispatch_plugin("metaphor-codegen", Some("routes"), args, run),

        // metaphor-dev plugin
        Command::Dev { run, args } => dispatch_plugin("metaphor-dev", Some("dev"), args, run),
        Command::Lint { run, args } => dispatch_plugin("metaphor-dev", Some("lint"), args, run),
        Command::Test { run, args } => dispatch_plugin("metaphor-dev", Some("test"), args, run),
        Command::Docs { run, args } => dispatch_plugin("metaphor-dev", Some("docs"), args, run),
        Command::Config { run, args } => dispatch_plugin("metaphor-dev", Some("config"), args, run),
        Command::Jobs { run, args } => dispatch_plugin("metaphor-dev", Some("jobs"), args, run),
        Command::Docker { run, args } => dispatch_plugin("metaphor-dev", Some("docker"), args, run),
        Command::Deploy { run, args } => dispatch_plugin("metaphor-dev", Some("deploy"), args, run),

        // metaphor-agent plugin
        Command::Agent { run, args } => dispatch_plugin("metaphor-agent", Some("agent"), args, run),
    }
}

/// Dispatch a passthrough command. If `run.is_multi()`, fan out across the
/// selected projects; otherwise preserve the original single-shot behavior
/// (spawn once with inherited stdio, no cwd change).
fn dispatch_plugin(
    binary: &str,
    subcommand: Option<&str>,
    args: &[String],
    run: &run_many::RunFlags,
) -> Result<()> {
    if !run.is_multi() {
        // Reject flags that only make sense alongside --all / --projects /
        // --affected, so a typo doesn't silently behave as single-shot.
        if run.parallel != 1 {
            anyhow::bail!("--parallel requires one of --all, --projects, or --affected");
        }
        if run.continue_on_error {
            anyhow::bail!("--continue-on-error requires one of --all, --projects, or --affected");
        }
        if run.no_cache {
            anyhow::bail!("--no-cache requires one of --all, --projects, or --affected");
        }
        return match subcommand {
            Some(sub) => plugin_env::passthrough(binary, sub, args),
            None => plugin_env::passthrough_raw(binary, args),
        };
    }
    let (manifest, root) = metaphor_workspace::find_and_load(&std::env::current_dir()?)?;
    let selected = run_many::select_projects(&manifest, &root, run)?;
    run_many::dispatch(binary, subcommand, args, &selected, &root, run)
}

fn cmd_cache(sub: &CacheCommand) -> Result<()> {
    let (_manifest, root) = metaphor_workspace::find_and_load(&std::env::current_dir()?)?;
    let cache = cache::Cache::open(&root)?;
    match sub {
        CacheCommand::Clear => {
            let stats = cache.clear()?;
            println!(
                "Cleared {} entries ({} bytes) from {}",
                stats.removed,
                stats.bytes,
                cache.root().display()
            );
        }
        CacheCommand::Stats { json } => {
            let stats = cache.stats()?;
            let newest_iso = stats.newest.map(|t| {
                chrono::DateTime::<chrono::Utc>::from(t)
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string()
            });
            if *json {
                let payload = json_envelope(serde_json::json!({
                    "root": stats.root.display().to_string(),
                    "entries": stats.entries,
                    "bytes": stats.bytes,
                    "newest": newest_iso,
                }));
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                println!("root:    {}", stats.root.display());
                println!("entries: {}", stats.entries);
                println!("bytes:   {}", stats.bytes);
                match newest_iso {
                    Some(iso) => println!("newest:  {iso}"),
                    None => println!("newest:  (empty)"),
                }
            }
        }
    }
    Ok(())
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
    print_projects_table(&manifest);
    Ok(())
}

/// Wrap a payload in the stable `{ "version": 1, "data": ... }` envelope.
/// Bumping the outer schema version becomes a one-line change here.
fn json_envelope(data: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "version": 1, "data": data })
}

fn print_projects_table(manifest: &metaphor_workspace::Manifest) {
    if manifest.projects.is_empty() {
        println!("No projects registered.");
        return;
    }
    println!("{} project(s):", manifest.projects.len());
    for p in &manifest.projects {
        if p.remote.is_some() {
            let remote = p.remote.as_deref().unwrap();
            let ref_info = p.git_ref.as_deref().unwrap_or("HEAD");
            println!(
                "  - {} [{:?}] path={} remote={} ref={}",
                p.name, p.project_type, p.path, remote, ref_info
            );
        } else {
            println!(
                "  - {} [{:?}] path={}",
                p.name, p.project_type, p.path
            );
        }
    }
}

fn cmd_graph(json: bool, focus: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let manifest = metaphor_workspace::load(&cwd)?;
    let mut g = graph::Graph::from_manifest(&manifest);
    if let Some(name) = focus {
        g = g.focus(name)?;
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json_envelope(g.to_json_data()))?
        );
    } else if manifest.projects.is_empty() {
        println!("No projects registered.");
    } else {
        print!("{}", g.render_text());
    }
    Ok(())
}

fn cmd_show(sub: &ShowCommand) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let (manifest, root) = metaphor_workspace::find_and_load(&cwd)?;
    match sub {
        ShowCommand::Projects { json } => {
            if *json {
                let payload = json_envelope(serde_json::json!({
                    "projects": &manifest.projects
                }));
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                print_projects_table(&manifest);
            }
        }
        ShowCommand::Project { name, json } => {
            let p = match name {
                Some(n) => manifest.find_project(n)?,
                None => manifest.current_project(&root, &cwd).ok_or_else(|| {
                    anyhow::anyhow!("not inside a registered project (cd into one or pass a name)")
                })?,
            };
            let absolute = p.resolved_path(&root);
            if *json {
                let payload = json_envelope(serde_json::json!({
                    "project": p,
                    "resolved_path": absolute,
                }));
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                println!("name:        {}", p.name);
                println!("type:        {:?}", p.project_type);
                println!("path:        {}", p.path);
                println!("resolved:    {}", absolute.display());
                if let Some(remote) = &p.remote {
                    println!("remote:      {}", remote);
                    println!(
                        "ref:         {}",
                        p.git_ref.as_deref().unwrap_or("HEAD")
                    );
                }
                if p.depends_on.is_empty() {
                    println!("depends_on:  (none)");
                } else {
                    println!("depends_on:  {}", p.depends_on.join(", "));
                }
            }
        }
    }
    Ok(())
}
