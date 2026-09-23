//! `gk` — the Gist toolkit.
//!
//! Tools here are called by an agent far more often than by a person, so the
//! defaults lean that way: no prompts, no spinners, bounded output.

mod doc;
mod outline;

use clap::{Parser, Subcommand};
use gist_core::{emit, exit, fail};
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
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Outline { path, limit, all } => outline::outline(&path, limit, all)
            .map(|report| emit(report, cli.json))
            .map_err(|message| fail(message, exit::FAILURE, cli.json)),
        Command::Doc(args) => doc::run(args)
            .map(|report| emit(report, cli.json))
            .map_err(|message| fail(message, exit::FAILURE, cli.json)),
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
    fn the_command_line_definition_is_internally_consistent() {
        // Catches conflicting flag names and bad arg groups at test time
        // rather than when a caller trips over them.
        Cli::command().debug_assert();
    }
}
