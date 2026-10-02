//! Shared plumbing for gist tools.
//!
//! Every tool in this workspace speaks the same wire format: a JSON envelope
//! on `--json`, plain text otherwise, and a small fixed set of exit codes.
//! The contract is documented in `docs/tool-contract.md`.

use serde::Serialize;
use std::process::ExitCode;

/// Exit codes every gist tool honors.
pub mod exit {
    /// The tool did what was asked.
    pub const OK: u8 = 0;
    /// An expected failure: nothing found, path missing, work refused.
    pub const FAILURE: u8 = 1;
    /// The tool was called wrong: bad flags, unparseable input.
    pub const MISUSE: u8 = 2;
}

/// Human-readable rendering, required alongside the machine-readable one.
///
/// Implementing this on every output type is deliberate: a tool that can only
/// speak JSON is a tool nobody debugs by hand.
pub trait Human {
    fn human(&self) -> String;
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum Envelope<T> {
    Ok { data: T },
    Error { message: String },
}

/// Write a successful result to stdout and return the process exit code.
pub fn emit<T: Serialize + Human>(data: T, json: bool) -> ExitCode {
    emit_status(data, exit::OK, json)
}

/// Write a successful result to stdout, exiting with `code` instead of `OK`.
///
/// The envelope is unconditionally `status: "ok"` — nothing failed to run,
/// the report is complete. Some tools still need to stop a caller in its
/// tracks over part of that report, e.g. a file conflict refused rather
/// than resolved: `--json` gives the caller structured data either way, the
/// exit code is the one bit that says "look before you continue".
pub fn emit_status<T: Serialize + Human>(data: T, code: u8, json: bool) -> ExitCode {
    if json {
        match serde_json::to_string_pretty(&Envelope::Ok { data: &data }) {
            Ok(text) => println!("{text}"),
            Err(err) => {
                return fail(
                    format!("could not serialize output: {err}"),
                    exit::FAILURE,
                    json,
                )
            }
        }
    } else {
        println!("{}", data.human());
    }
    ExitCode::from(code)
}

/// Write a successful result as a single line and flush it, for a command
/// that keeps running after its result (docs/tool-contract.md, "Long-running
/// subcommands"): a caller reading the first line gets all of it.
pub fn emit_line<T: Serialize + Human>(data: &T, json: bool) {
    use std::io::Write;
    let line = if json {
        serde_json::to_string(&Envelope::Ok { data }).unwrap_or_else(|_| {
            r#"{"status":"error","message":"unserializable output"}"#.to_string()
        })
    } else {
        data.human().replace('\n', " ")
    };
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
}

/// Write an error to stderr and return the process exit code. A closed
/// stderr, such as a detached server's once its caller returned, loses the
/// message but not the exit code.
pub fn fail(message: impl Into<String>, code: u8, json: bool) -> ExitCode {
    use std::io::Write;
    let message = message.into();
    let text = if json {
        let envelope: Envelope<()> = Envelope::Error { message };
        // Hand-rolled fallback: a serializer failure here must not mask the
        // original error, and it must still be parseable by the caller.
        serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| {
            r#"{"status":"error","message":"unserializable error"}"#.to_string()
        })
    } else {
        format!("error: {message}")
    };
    let _ = writeln!(std::io::stderr(), "{text}");
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Sample {
        count: usize,
    }

    impl Human for Sample {
        fn human(&self) -> String {
            format!("{} items", self.count)
        }
    }

    #[test]
    fn ok_envelope_wraps_data_under_status_ok() {
        let json = serde_json::to_string(&Envelope::Ok {
            data: Sample { count: 3 },
        })
        .unwrap();
        assert_eq!(json, r#"{"status":"ok","data":{"count":3}}"#);
    }

    #[test]
    fn error_envelope_carries_the_message() {
        let envelope: Envelope<()> = Envelope::Error {
            message: "nope".to_string(),
        };
        let json = serde_json::to_string(&envelope).unwrap();
        assert_eq!(json, r#"{"status":"error","message":"nope"}"#);
    }

    #[test]
    fn human_rendering_is_separate_from_the_wire_format() {
        assert_eq!(Sample { count: 3 }.human(), "3 items");
    }
}
