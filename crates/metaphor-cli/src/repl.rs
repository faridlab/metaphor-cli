//! Interactive REPL.
//!
//! Entered two ways:
//! 1. Explicit `metaphor repl` subcommand (works from scripts too — useful
//!    for integration tests and for users who want the REPL on non-TTY
//!    setups like tmux panes).
//! 2. Bare `metaphor` invocation when stdin+stdout are both TTYs.
//!
//! Each line is shell-split (`shell_words`), prefixed with "metaphor" to
//! look like a real argv, reparsed through clap via `Cli::try_parse_from`,
//! and handed back to the main dispatcher. This means every subcommand —
//! existing or future — works inside the REPL with zero per-command glue.
//!
//! Built-in words that don't dispatch to a subcommand: `help`, `?`, `clear`,
//! `exit`, `quit`. An empty line is ignored. `Ctrl-C` interrupts the current
//! line; `Ctrl-D` (or `exit`/`quit`) leaves the loop.

use crate::{dispatch, Cli};
use anyhow::Result;
use clap::{CommandFactory, Parser};
use colored::*;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::path::PathBuf;

const PROMPT: &str = "metaphor> ";

pub fn run() -> Result<()> {
    let mut rl = DefaultEditor::new()?;
    let history = history_path();
    if let Some(path) = &history {
        let _ = rl.load_history(path);
    }

    print_welcome();

    loop {
        match rl.readline(PROMPT) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                // Persist history before handling the line, so history is
                // captured even if the command panics or exits the process.
                let _ = rl.add_history_entry(line);

                match handle_line(line) {
                    LineOutcome::Continue => {}
                    LineOutcome::Exit => break,
                    LineOutcome::Err(e) => {
                        eprintln!("{} {e}", "error:".red().bold());
                        // Show anyhow's context chain without the backtrace noise.
                        let mut src = e.source();
                        while let Some(cause) = src {
                            eprintln!("  caused by: {cause}");
                            src = cause.source();
                        }
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl-C — abandon this line, stay in the loop.
                println!("(interrupted — type `exit` to leave)");
            }
            Err(ReadlineError::Eof) => {
                // Ctrl-D — leave the loop.
                break;
            }
            Err(e) => {
                eprintln!("readline error: {e}");
                break;
            }
        }
    }

    if let Some(path) = &history {
        let _ = rl.save_history(path);
    }
    println!("bye.");
    Ok(())
}

enum LineOutcome {
    Continue,
    Exit,
    Err(anyhow::Error),
}

fn handle_line(line: &str) -> LineOutcome {
    // Built-ins first — these would otherwise fail clap parsing.
    match line {
        "exit" | "quit" | ":q" => return LineOutcome::Exit,
        "clear" => {
            print!("\x1b[2J\x1b[H");
            let _ = std::io::Write::flush(&mut std::io::stdout());
            return LineOutcome::Continue;
        }
        "help" | "?" => {
            print_help();
            return LineOutcome::Continue;
        }
        _ => {}
    }

    // shell_words handles quoted args, escapes, etc. — same as a real shell.
    let tokens = match shell_words::split(line) {
        Ok(t) => t,
        Err(e) => return LineOutcome::Err(anyhow::anyhow!("parse error: {e}")),
    };

    // Reparse through clap by prefixing with the binary name.
    let mut argv: Vec<String> = Vec::with_capacity(tokens.len() + 1);
    argv.push("metaphor".to_string());
    argv.extend(tokens);

    let cli = match Cli::try_parse_from(&argv) {
        Ok(c) => c,
        Err(e) => {
            // Clap prints help/version requests using its own machinery —
            // display the formatted message and keep the loop alive.
            e.print().ok();
            return LineOutcome::Continue;
        }
    };

    // Refuse re-entering repl from inside the repl — not useful, just nests.
    if matches!(cli.command, crate::Command::Repl) {
        return LineOutcome::Err(anyhow::anyhow!("already in a repl — use `exit` to leave"));
    }

    match dispatch(&cli) {
        Ok(()) => LineOutcome::Continue,
        Err(e) => LineOutcome::Err(e),
    }
}

fn print_welcome() {
    println!(
        "{} interactive mode. Type {} to list commands, {} to leave.",
        "metaphor".bright_green().bold(),
        "help".cyan(),
        "exit".cyan(),
    );
    println!();
}

/// Grouped help. Pulled from clap's own command tree so it stays in sync
/// automatically when new subcommands are added.
fn print_help() {
    println!("{}", "Built-in:".bold());
    println!("  help, ?          show this help");
    println!("  clear            clear the screen");
    println!("  exit, quit, :q   leave the REPL");
    println!();
    println!("{}", "Subcommands:".bold());

    let cmd = Cli::command();
    // Clap's subcommand list — name + short about — grouped alphabetically.
    let mut subs: Vec<_> = cmd.get_subcommands().filter(|s| !s.is_hide_set()).collect();
    subs.sort_by_key(|s| s.get_name());

    // Column-align: find the longest name, pad up to that width + 2.
    let width = subs.iter().map(|s| s.get_name().len()).max().unwrap_or(0);
    for s in subs {
        let name = s.get_name();
        let about = s.get_about().map(|t| t.to_string()).unwrap_or_default();
        // Strip trailing periods / newlines so the column stays tidy.
        let about = about.trim().trim_end_matches('.').to_string();
        println!("  {:<width$}  {about}", name, width = width);
    }
    println!();
    println!(
        "Type `{}` for full flag details on any subcommand.",
        "<cmd> --help".cyan()
    );
}

fn history_path() -> Option<PathBuf> {
    let base = dirs::data_local_dir().or_else(dirs::home_dir)?;
    let dir = base.join("metaphor");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("repl-history"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_includes_registered_subcommands() {
        // Not a test of actual output — we can't easily capture stdout —
        // but confirms clap's subcommand introspection is non-empty.
        let subs: Vec<_> = Cli::command()
            .get_subcommands()
            .map(|s| s.get_name().to_string())
            .collect();
        assert!(subs.contains(&"doctor".to_string()));
        assert!(subs.contains(&"info".to_string()));
        assert!(subs.contains(&"repl".to_string()));
    }
}
