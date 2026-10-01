//! `gk md-review` — a browser review loop for one markdown file.
//! Design: docs/design/md-review-hld.md; cases: docs/cases/gist-md-review.md.

pub mod anchor;
pub mod api;
pub mod client;
pub mod diff;
pub mod record;
pub mod render;
pub mod serve;
pub mod store;

use clap::{Args as ClapArgs, Subcommand, ValueEnum};
use gist_core::{emit, emit_line, exit, fail};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use client::Client;
use store::Outcome;

const DEFAULT_LIMIT: usize = 25;

/// Command line for `gk md-review`.
#[derive(ClapArgs, Debug)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Serve the review page for a markdown file and print its URL; runs
    /// until the approval is delivered or `stop`
    Serve {
        /// The markdown file to review
        file: PathBuf,
    },

    /// Return the reviewer's submit or approval, waiting for one until the timeout
    Wait {
        /// The reviewed file; may be left out while only one review is live
        file: Option<PathBuf>,

        /// Give up after this long, e.g. 110m; keep it below the host's
        /// background-task limit
        #[arg(long, value_parser = parse_span)]
        timeout: Span,

        /// Maximum number of threads to return
        #[arg(long, default_value_t = DEFAULT_LIMIT)]
        limit: usize,

        /// Longest single request to the server
        #[arg(long, value_parser = parse_span, default_value = "60s", hide = true)]
        poll: Span,
    },

    /// Record the agent's answer to a thread: [FILE] THREAD
    Reply {
        /// The reviewed file, optional, then the thread id
        #[arg(num_args = 1..=2, required = true, value_name = "FILE THREAD")]
        targets: Vec<String>,

        #[arg(long)]
        outcome: ReplyOutcome,

        /// What was done, or why not
        #[arg(long)]
        note: String,
    },

    /// Close the submitted round and snapshot the file as the next version
    Next {
        /// The reviewed file; may be left out while only one review is live
        file: Option<PathBuf>,
    },

    /// Show the round, open threads, waiting clients and any undelivered submit
    Status {
        /// The reviewed file; may be left out while only one review is live
        file: Option<PathBuf>,

        /// List the threads of this round's submit instead of the open ones
        #[arg(long)]
        round: Option<u32>,

        /// Maximum number of threads to list
        #[arg(long, default_value_t = DEFAULT_LIMIT)]
        limit: usize,

        /// Threads to skip, for paging
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },

    /// Stop the review server; the review is kept and `serve` resumes it
    Stop {
        /// The reviewed file; may be left out while only one review is live
        file: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ReplyOutcome {
    Applied,
    Declined,
}

/// A duration as written on the command line, kept for echoing back.
#[derive(Clone, Debug)]
struct Span {
    text: String,
    duration: Duration,
}

fn parse_span(text: &str) -> Result<Span, String> {
    let (number, unit) = text.split_at(
        text.find(|c: char| !c.is_ascii_digit())
            .unwrap_or(text.len()),
    );
    let number: u64 = number
        .parse()
        .map_err(|_| format!("{text:?} is not a duration like 90s, 110m or 2h"))?;
    let seconds = match unit {
        "s" => number,
        "m" => number * 60,
        "h" => number * 3600,
        _ => return Err(format!("{text:?} is not a duration like 90s, 110m or 2h")),
    };
    Ok(Span {
        text: text.to_string(),
        duration: Duration::from_secs(seconds),
    })
}

/// Run `gk md-review`; every outcome is printed here, since `serve` prints
/// its result before it is done.
pub fn run(args: Args, json: bool) -> ExitCode {
    let done = match args.command {
        Command::Serve { file } => {
            serve::run(&file, |served| emit_line(served, json)).map(|()| ExitCode::from(exit::OK))
        }
        Command::Wait {
            file,
            timeout,
            limit,
            poll,
        } => Client::find(file.as_deref(), true)
            .and_then(|client| client.wait(timeout.duration, &timeout.text, limit, poll.duration))
            .map(|delivery| emit(delivery, json)),
        Command::Reply {
            targets,
            outcome,
            note,
        } => {
            let (file, thread) = match targets.as_slice() {
                [thread] => (None, thread),
                [file, thread] => (Some(PathBuf::from(file)), thread),
                _ => unreachable!("clap takes one or two"),
            };
            let outcome = match outcome {
                ReplyOutcome::Applied => Outcome::Applied,
                ReplyOutcome::Declined => Outcome::Declined,
            };
            Client::find(file.as_deref(), true)
                .and_then(|client| client.reply(thread, outcome, &note))
                .map(|replied| emit(replied, json))
        }
        Command::Next { file } => Client::find(file.as_deref(), true)
            .and_then(|client| client.next())
            .map(|started| emit(started, json)),
        Command::Status {
            file,
            round,
            limit,
            offset,
        } => Client::find(file.as_deref(), true)
            .and_then(|client| client.status(round, limit, offset))
            .map(|status| emit(status, json)),
        Command::Stop { file } => Client::find(file.as_deref(), false)
            .and_then(|client| client.stop())
            .map(|stopped| emit(stopped, json)),
    };
    done.unwrap_or_else(|message| fail(message, exit::FAILURE, json))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_take_seconds_minutes_and_hours() {
        assert_eq!(parse_span("2s").unwrap().duration, Duration::from_secs(2));
        assert_eq!(
            parse_span("110m").unwrap().duration,
            Duration::from_secs(6600)
        );
        assert_eq!(
            parse_span("2h").unwrap().duration,
            Duration::from_secs(7200)
        );
        assert_eq!(parse_span("110m").unwrap().text, "110m");
        for bad in ["", "90", "m", "1.5h", "10d", "-1s"] {
            assert!(parse_span(bad).is_err(), "{bad}");
        }
    }
}
