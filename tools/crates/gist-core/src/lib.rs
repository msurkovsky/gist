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
    ExitCode::from(exit::OK)
}

/// Write an error to stderr and return the process exit code.
pub fn fail(message: impl Into<String>, code: u8, json: bool) -> ExitCode {
    let message = message.into();
    if json {
        let envelope: Envelope<()> = Envelope::Error { message };
        // Hand-rolled fallback: a serializer failure here must not mask the
        // original error, and it must still be parseable by the caller.
        let text = serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| {
            r#"{"status":"error","message":"unserializable error"}"#.to_string()
        });
        eprintln!("{text}");
    } else {
        eprintln!("error: {message}");
    }
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
