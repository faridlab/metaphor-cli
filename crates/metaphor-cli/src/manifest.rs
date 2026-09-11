//! `metaphor manifest` — emit the clap command tree as a machine-readable manifest.
//!
//! Walks `Cli::command()` so the interactive UI (and any script) can render
//! menus and guided forms that cannot drift from the real command surface.
//! Serialized through the stable `json_envelope()` like every other `--json`
//! command. Works from any directory — no workspace needed.

use anyhow::Result;
use clap::{ArgAction, CommandFactory};
use serde::Serialize;

#[derive(Serialize)]
struct CommandNode {
    name: String,
    about: Option<String>,
    long_about: Option<String>,
    aliases: Vec<String>,
    /// Emitted so the UI can filter meta commands; never filtered here.
    hidden: bool,
    /// True for plugin passthrough commands — the UI renders a raw
    /// "extra arguments" field instead of a form for the trailing args.
    trailing_var_arg: bool,
    allow_external_subcommands: bool,
    flags: Vec<FlagNode>,
    subcommands: Vec<CommandNode>,
}

#[derive(Serialize)]
struct FlagNode {
    id: String,
    long: Option<String>,
    short: Option<char>,
    value_name: Option<String>,
    help: Option<String>,
    /// No long and no short — supplied positionally.
    positional: bool,
    takes_value: bool,
    required: bool,
    num_args: Option<NumArgs>,
    default_values: Vec<String>,
    possible_values: Vec<PossibleValue>,
    global: bool,
    hidden: bool,
}

#[derive(Serialize)]
struct NumArgs {
    min: usize,
    /// None = unbounded.
    max: Option<usize>,
}

#[derive(Serialize)]
struct PossibleValue {
    name: String,
    help: Option<String>,
}

/// Emit the manifest. `--json` prints the stable envelope; the default is a
/// compact text tree for humans.
pub fn cmd_manifest(json: bool) -> Result<()> {
    let root = command_tree();
    if json {
        let payload = crate::json_envelope(serde_json::to_value(&root)?);
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        text_tree(&root, 0);
    }
    Ok(())
}

/// Build the manifest from the derive-generated clap definitions.
fn command_tree() -> CommandNode {
    let mut cmd = crate::Cli::command();
    // REQUIRED: propagates global args (--verbose) into every subcommand and
    // inlines #[command(flatten)] structs (RunFlags/BuildFlags) into each
    // subcommand's argument list. Without it the tree would miss them.
    cmd.build();
    node_from(&mut cmd)
}

fn node_from(cmd: &mut clap::Command) -> CommandNode {
    let trailing_var_arg = cmd.is_trailing_var_arg_set();

    // Snapshot args; find the raw trailing positional so it can be dropped
    // for passthrough commands (the UI renders its own raw-args field).
    let args: Vec<clap::Arg> = cmd.get_arguments().cloned().collect();
    let last_positional = args
        .iter()
        .rposition(|a| a.get_long().is_none() && a.get_short().is_none());

    let flags = args
        .iter()
        .enumerate()
        .filter(|(i, a)| {
            // clap auto-adds these during build(); they are noise for the UI.
            if a.get_id() == "help" || a.get_id() == "version" {
                return false;
            }
            // Passthrough commands carry a raw `args: Vec<String>` positional
            // whose help text belongs to the plugin's CLI, not ours.
            !(trailing_var_arg && last_positional == Some(*i))
        })
        .map(|(_, a)| flag_from(a))
        .collect();

    let subcommands = cmd
        .get_subcommands()
        .filter(|c| c.get_name() != "help")
        .map(|c| node_from(&mut c.clone()))
        .collect();

    CommandNode {
        name: cmd.get_name().to_string(),
        about: cmd.get_about().map(|s| s.to_string()),
        long_about: cmd.get_long_about().map(|s| s.to_string()),
        aliases: cmd.get_visible_aliases().map(|s| s.to_string()).collect(),
        hidden: cmd.is_hide_set(),
        trailing_var_arg,
        allow_external_subcommands: cmd.is_allow_external_subcommands_set(),
        flags,
        subcommands,
    }
}

fn flag_from(a: &clap::Arg) -> FlagNode {
    let positional = a.get_long().is_none() && a.get_short().is_none();
    FlagNode {
        id: a.get_id().to_string(),
        long: a.get_long().map(str::to_string),
        short: a.get_short(),
        value_name: a
            .get_value_names()
            .and_then(|names| names.first())
            .map(|id| id.to_string()),
        help: a.get_help().map(|s| s.to_string()),
        positional,
        // Set/Append consume values; SetTrue/SetFalse/Help/Version do not.
        takes_value: matches!(a.get_action(), ArgAction::Set | ArgAction::Append),
        required: a.is_required_set(),
        num_args: a.get_num_args().map(|r| NumArgs {
            min: r.min_values(),
            // clap encodes "unbounded" as usize::MAX.
            max: match r.max_values() {
                usize::MAX => None,
                n => Some(n),
            },
        }),
        default_values: a
            .get_default_values()
            .iter()
            .map(|v| v.to_string_lossy().to_string())
            .collect(),
        possible_values: a
            .get_possible_values()
            .into_iter()
            .map(|pv| PossibleValue {
                name: pv.get_name().to_string(),
                help: pv.get_help().map(|s| s.to_string()),
            })
            .collect(),
        global: a.is_global_set(),
        hidden: a.is_hide_set(),
    }
}

/// Indented text tree fallback for humans (`metaphor manifest` without `--json`).
fn text_tree(node: &CommandNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let about = node
        .about
        .as_deref()
        .and_then(|s| s.lines().next())
        .unwrap_or("");
    println!("{}{} — {}", indent, node.name, about);
    for f in node.flags.iter().filter(|f| !f.hidden && !f.positional) {
        let flag = match (&f.long, f.short) {
            (Some(l), _) => format!("--{}", l),
            (None, Some(s)) => format!("-{}", s),
            (None, None) => f.id.clone(),
        };
        let value = if f.takes_value {
            format!(
                " <{}>",
                f.value_name.clone().unwrap_or_else(|| f.id.clone())
            )
        } else {
            String::new()
        };
        println!(
            "{}  {:<26} {}{}",
            indent,
            flag + &value,
            "",
            f.help.clone().unwrap_or_default()
        );
    }
    for sub in &node.subcommands {
        text_tree(sub, depth + 1);
    }
}
