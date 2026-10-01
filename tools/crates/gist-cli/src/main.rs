//! `gk` — the Gist toolkit.
//!
//! Tools here are called by an agent far more often than by a person, so the
//! defaults lean that way: no prompts, no spinners, bounded output.

mod check;
mod doc;
mod hook;
mod init;
mod md_review;
mod outline;
mod skill;

use clap::{Parser, Subcommand};
use gist_core::{emit, emit_status, exit, fail, Human};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "gk",
    version,
    about = "Gist toolkit — understand enough to steer"
)]
struct Cli {
    /// Emit a JSON envelope instead of human-readable text
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate a Gist source checkout without installing anything
    Check(check::Args),
    /// Summarize the shape of a tree: how many files, which languages, how big
    Outline {
        /// Directory to summarize
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Maximum number of file types to report
        #[arg(long, default_value_t = 20)]
        limit: usize,

        /// Include files that .gitignore excludes
        #[arg(long)]
        all: bool,
    },

    /// Measure how much of a change is documentation
    Doc(doc::Args),

    /// Vendor this project's skills for Claude Code and/or Codex CLI, or
    /// remove them with --uninstall
    Init(init::Args),

    /// Install the git hooks gk provides, or run one
    Hook(hook::Args),

    /// Review a markdown file in the browser, round by round, with the agent
    MdReview(md_review::Args),
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Check(args) => check::run(args)
            .map(|report| {
                let code = if report.is_failure() {
                    exit::FAILURE
                } else {
                    exit::OK
                };
                emit_status(report, code, cli.json)
            })
            .map_err(|message| fail(message, exit::FAILURE, cli.json)),
        Command::Outline { path, limit, all } => outline::outline(&path, limit, all)
            .map(|report| emit(report, cli.json))
            .map_err(|message| fail(message, exit::FAILURE, cli.json)),
        Command::Doc(args) => doc::run(args)
            .map(|report| emit(report, cli.json))
            .map_err(|message| fail(message, exit::FAILURE, cli.json)),
        // A conflict, or a kept file on uninstall, is a successful
        // measurement, not a run failure — `emit` always signals success, so
        // exit code comes from the report itself.
        Command::Init(args) => init::run(args)
            .map(|report| {
                let code = if report.is_failure() {
                    exit::FAILURE
                } else {
                    exit::OK
                };
                emit_status(report, code, cli.json)
            })
            .map_err(|message| fail(message, exit::FAILURE, cli.json)),
        // A hook's advice goes to stderr and is empty on most commits: git
        // shows a hook's output to the person committing, every time.
        Command::Hook(args) => hook::run(args)
            .map(|report| {
                if report.is_diagnostic() && !cli.json {
                    let text = report.human();
                    if !text.is_empty() {
                        eprintln!("{text}");
                    }
                    return ExitCode::from(exit::OK);
                }
                let code = if report.is_failure() {
                    exit::FAILURE
                } else {
                    exit::OK
                };
                emit_status(report, code, cli.json)
            })
            .map_err(|message| fail(message, exit::FAILURE, cli.json)),
        // `serve` prints its result line before it is done, so md-review
        // emits its own output.
        Command::MdReview(args) => Ok(md_review::run(args, cli.json)),
    };

    match result {
        Ok(code) | Err(code) => code,
    }
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn every_installable_hook_has_a_subcommand_to_run() {
        let cli = Cli::command();
        let hook = cli.find_subcommand("hook").expect("gk hook");
        for name in crate::hook::HOOKS {
            assert!(
                hook.find_subcommand(name).is_some(),
                "`gk hook install` writes a shim for {name}, but `gk hook {name}` does not exist"
            );
        }
    }

    #[test]
    fn the_command_line_definition_is_internally_consistent() {
        // Catches conflicting flag names and bad arg groups at test time
        // rather than when a caller trips over them.
        Cli::command().debug_assert();
    }
}
