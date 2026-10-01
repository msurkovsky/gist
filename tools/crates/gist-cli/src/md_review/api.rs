//! What `serve` answers the agent's clients. The server builds these and a
//! client prints them as received, so `wait`, `reply`, `next`, `status` and
//! `stop` render the same data either way. docs/design/md-review-hld.md#what-wait-returns.

use gist_core::Human;
use serde::{Deserialize, Serialize};

use super::store::{Anchor, Author, Kind, Message, Outcome};

/// Longest quote the human rendering prints before cutting it; the JSON
/// rendering keeps the whole anchor.
const QUOTE_CHARS: usize = 400;

fn is_false(value: &bool) -> bool {
    !value
}

/// One thread as the agent sees it, anchored to the current version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadView {
    pub id: String,
    pub state: State,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_in: Option<u32>,
    /// Where the thread points in the current version; when orphaned, the
    /// last place it was found.
    pub anchor: Anchor,
    #[serde(default, skip_serializing_if = "is_false")]
    pub orphaned: bool,
    pub messages: Vec<Message>,
}

/// Whether a thread still needs attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Open,
    Resolved,
    Applied,
}

/// What `wait` returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Delivery {
    ReviewSubmitted(Submitted),
    Approved(Approved),
    Timeout(Timeout),
}

/// A submitted round, for the agent to act on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Submitted {
    pub file: String,
    pub round: u32,
    /// The version the reviewer read, which the line numbers refer to.
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub threads: Vec<ThreadView>,
    /// Threads in the submit, of which `threads` may be a slice.
    pub total: usize,
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redelivered: Option<Redelivered>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub file_differs: bool,
}

/// Marks a delivery after the first: the agent may have acted on part of it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Redelivered {
    /// When it was first delivered, in seconds since the Unix epoch.
    pub first: u64,
    /// Threads of the submit the agent already replied to.
    pub replied: Vec<String>,
}

/// The reviewer approved; the review is over.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Approved {
    pub file: String,
    pub round: u32,
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The review record, relative to the root.
    pub record: String,
    /// The working file is not the approved version.
    pub file_differs: bool,
    pub threads: Tally,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discarded: Vec<String>,
}

/// Threads by state, the bounded summary of a record.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Tally {
    pub total: usize,
    pub applied: usize,
    pub resolved: usize,
    pub open: usize,
}

/// `wait` ran out of time with nothing pending.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeout {
    #[serde(skip)]
    pub file: String,
    /// The timeout as given, for example `110m`.
    pub after: String,
}

/// What `reply` recorded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replied {
    pub file: String,
    pub thread: String,
    pub outcome: Outcome,
}

/// The round `next` started.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Started {
    pub file: String,
    pub round: u32,
    pub version: u32,
    /// Blocks changed, added or deleted since the previous version.
    pub changed: usize,
    /// Open threads whose quote was not found in the new version.
    pub orphaned: Vec<String>,
}

/// Where the review stands, for `status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub file: String,
    pub url: String,
    pub round: u32,
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit: Option<SubmitStatus>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub approved: bool,
    /// `wait` calls blocked on the server now.
    pub waiters: usize,
    /// When a `wait` last polled, in seconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_wait: Option<u64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub file_differs: bool,
    /// The round whose threads are listed; open threads when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listed_round: Option<u32>,
    pub threads: Vec<ThreadView>,
    pub total: usize,
    pub offset: usize,
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
}

/// The current round's submit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitStatus {
    pub at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_delivery: Option<u64>,
}

/// What `stop` did.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stopped {
    pub file: String,
}

/// What `serve` prints first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Served {
    pub url: String,
    pub file: String,
    /// A live server already held the review.
    pub reused: bool,
}

impl Human for Delivery {
    fn human(&self) -> String {
        match self {
            Delivery::ReviewSubmitted(submitted) => submitted.human(),
            Delivery::Approved(approved) => approved.human(),
            Delivery::Timeout(timeout) => timeout.human(),
        }
    }
}

impl Human for Submitted {
    fn human(&self) -> String {
        let mut out = format!(
            "review submitted · {} · round {} · {}\n",
            self.file,
            self.round,
            count(self.total, "thread")
        );
        if let Some(again) = &self.redelivered {
            out.push_str(&format!("redelivered · first {}", clock(again.first)));
            if !again.replied.is_empty() {
                out.push_str(&format!(" · replied {}", again.replied.join(" ")));
            }
            out.push('\n');
        }
        if self.file_differs {
            out.push_str(&format!(
                "warning: {} changed since round {} was read; line numbers refer to v{}\n",
                self.file, self.round, self.version
            ));
        }
        if let Some(summary) = &self.summary {
            out.push_str("\nsummary\n");
            out.push_str(&indent(summary, "  "));
        }
        for thread in &self.threads {
            out.push('\n');
            out.push_str(&thread.human());
        }
        if self.truncated {
            out.push_str(&format!(
                "\nshowing {} of {} threads; see the rest with gk md-review status {} --round {} --offset {}\n",
                self.threads.len(),
                self.total,
                self.file,
                self.round,
                self.threads.len()
            ));
        }
        out.push_str(&format!(
            "\nnext: gk md-review reply {file} <id> --outcome applied|declined --note \"…\", then gk md-review next {file}",
            file = self.file
        ));
        out
    }
}

impl Human for ThreadView {
    fn human(&self) -> String {
        let kind = match self.messages.first().map(|m| m.kind) {
            Some(Kind::Question) => "question",
            _ => "comment",
        };
        let lines = if self.anchor.lines[0] == self.anchor.lines[1] {
            format!("line {}", self.anchor.lines[0])
        } else {
            format!("lines {}–{}", self.anchor.lines[0], self.anchor.lines[1])
        };
        let mut header = format!("[{}] {kind}", self.id);
        if self.orphaned {
            header.push_str(&format!(
                " · orphaned, was {lines} in v{}",
                self.anchor.version
            ));
        } else {
            header.push_str(&format!(" · {lines}"));
        }
        if !self.anchor.headings.is_empty() {
            header.push_str(&format!(" · {}", self.anchor.headings.join(" › ")));
        }
        match (self.state, self.applied_in) {
            (State::Applied, Some(round)) => {
                header.push_str(&format!(" · applied in round {round}"))
            }
            (State::Resolved, _) => header.push_str(" · resolved"),
            _ => {}
        }
        let mut out = format!("{header}\n");
        out.push_str(&indent(&cut(&self.anchor.quote), "  > "));
        for message in &self.messages {
            match (message.author, message.outcome) {
                (Author::Human, _) => out.push_str(&indent(&message.body, "  ")),
                (Author::Agent, outcome) => {
                    let label = match outcome {
                        Some(Outcome::Applied) => "applied: ",
                        Some(Outcome::Declined) => "declined: ",
                        Some(Outcome::Answered) => "answered: ",
                        None => "",
                    };
                    out.push_str(&indent(&format!("{label}{}", message.body), "  ↳ "));
                }
            }
        }
        out
    }
}

impl Human for Approved {
    fn human(&self) -> String {
        let mut out = format!(
            "review approved · {} · round {} · v{}\n",
            self.file, self.round, self.version
        );
        if self.file_differs {
            out.push_str(&format!(
                "warning: {} changed after v{} was approved; the record holds the approved text and the changes since\n",
                self.file, self.version
            ));
        }
        if let Some(note) = &self.note {
            out.push_str("\nnote\n");
            out.push_str(&indent(note, "  "));
            out.push('\n');
        }
        out.push_str(&format!("record {}\n", self.record));
        let tally = &self.threads;
        out.push_str(&format!(
            "{}: {} applied, {} resolved, {} open",
            count(tally.total, "thread"),
            tally.applied,
            tally.resolved,
            tally.open
        ));
        if !self.discarded.is_empty() {
            out.push_str(&format!(
                "\ndiscarded at approve: {}",
                self.discarded.join(" ")
            ));
        }
        out
    }
}

impl Human for Timeout {
    fn human(&self) -> String {
        format!(
            "timeout after {}; nothing submitted\nnext: gk md-review wait {} --timeout {}",
            self.after, self.file, self.after
        )
    }
}

impl Human for Replied {
    fn human(&self) -> String {
        let outcome = match self.outcome {
            Outcome::Applied => "applied",
            Outcome::Declined => "declined",
            Outcome::Answered => "answered",
        };
        format!("{} {outcome} · {}", self.thread, self.file)
    }
}

impl Human for Started {
    fn human(&self) -> String {
        let mut out = format!(
            "round {} · {} · v{} · {} changed",
            self.round,
            self.file,
            self.version,
            count(self.changed, "block")
        );
        if !self.orphaned.is_empty() {
            out.push_str(&format!(
                "\norphaned: {} (quote not found in v{})",
                self.orphaned.join(" "),
                self.version
            ));
        }
        out.push_str(&format!(
            "\nnext: gk md-review wait {} --timeout <dur>, in the background",
            self.file
        ));
        out
    }
}

impl Human for Status {
    fn human(&self) -> String {
        let mut out = format!("{} · round {} · v{}", self.file, self.round, self.version);
        if self.approved {
            out.push_str(" · approved, not yet delivered");
        } else if let Some(submit) = &self.submit {
            out.push_str(&format!(" · submitted {}", clock(submit.at)));
            match submit.first_delivery {
                Some(at) => out.push_str(&format!(", delivered {}", clock(at))),
                None => out.push_str(", not yet delivered"),
            }
        }
        out.push_str(&match (self.waiters, self.last_wait) {
            (0, Some(at)) => format!(" · no wait running, last {}", clock(at)),
            (0, None) => " · no wait running".to_string(),
            (n, _) => format!(" · {} running", count(n, "wait")),
        });
        out.push_str(&format!("\nurl {}", self.url));
        if self.file_differs {
            out.push_str(&format!(
                "\nwarning: {} differs from v{}",
                self.file, self.version
            ));
        }
        let what = match self.listed_round {
            Some(round) => format!("threads of round {round}"),
            None => "open threads".to_string(),
        };
        out.push_str(&format!("\n{what}: {}", self.total));
        if self.truncated || self.offset > 0 {
            out.push_str(&format!(
                ", showing {}–{}",
                self.offset + 1,
                self.offset + self.threads.len()
            ));
        }
        for thread in &self.threads {
            out.push_str("\n\n");
            out.push_str(thread.human().trim_end());
        }
        out
    }
}

impl Human for Stopped {
    fn human(&self) -> String {
        format!(
            "stopped the review server for {}; the review is kept, and gk md-review serve {} resumes it",
            self.file, self.file
        )
    }
}

impl Human for Served {
    fn human(&self) -> String {
        self.url.clone()
    }
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// Every line of `text` behind `prefix`, ending in a newline.
fn indent(text: &str, prefix: &str) -> String {
    let mut out = String::new();
    for line in text.trim_end().lines() {
        out.push_str(prefix);
        out.push_str(line);
        out.push('\n');
    }
    if out.is_empty() {
        out.push_str(prefix.trim_end());
        out.push('\n');
    }
    out
}

fn cut(text: &str) -> String {
    match text.char_indices().nth(QUOTE_CHARS) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_string(),
    }
}

/// `HH:MM UTC` of a Unix time.
pub fn clock(secs: u64) -> String {
    let [_, _, _, hour, minute, _] = utc(secs);
    format!("{hour:02}:{minute:02} UTC")
}

/// Year, month, day, hour, minute and second of a Unix time, in UTC.
pub fn utc(secs: u64) -> [u64; 6] {
    let days = secs / 86_400;
    let rest = secs % 86_400;
    // Days to a civil date, after Howard Hinnant's `civil_from_days`.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    [year, month, day, rest / 3600, rest % 3600 / 60, rest % 60]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(author: Author, body: &str, outcome: Option<Outcome>) -> Message {
        Message {
            id: "m".to_string(),
            author,
            kind: Kind::Comment,
            body: body.to_string(),
            outcome,
            round: 1,
            at: 0,
        }
    }

    fn thread() -> ThreadView {
        ThreadView {
            id: "t7".to_string(),
            state: State::Open,
            applied_in: None,
            anchor: Anchor {
                quote: "A comment stores the quoted text".to_string(),
                prefix: String::new(),
                suffix: String::new(),
                blocks: vec![3],
                lines: [41, 44],
                headings: vec!["Loop".to_string(), "Anchors".to_string()],
                version: 2,
            },
            orphaned: false,
            messages: vec![
                message(Author::Human, "Too long; one sentence.", None),
                message(Author::Agent, "Cut to one.", Some(Outcome::Applied)),
            ],
        }
    }

    #[test]
    fn utc_dates_count_leap_years() {
        assert_eq!(utc(0), [1970, 1, 1, 0, 0, 0]);
        assert_eq!(utc(951_782_400), [2000, 2, 29, 0, 0, 0]);
        assert_eq!(utc(1_790_883_360), [2026, 10, 1, 19, 36, 0]);
    }

    // Case: docs/cases/gist-md-review.md#wait-blocks
    #[test]
    fn reviewer_text_is_quoted_under_its_thread_id() {
        let text = thread().human();
        assert_eq!(
            text,
            "[t7] comment · lines 41–44 · Loop › Anchors\n  > A comment stores the quoted text\n  Too long; one sentence.\n  ↳ applied: Cut to one.\n"
        );
    }

    #[test]
    fn an_orphaned_thread_names_where_it_was() {
        let mut orphaned = thread();
        orphaned.orphaned = true;
        assert!(orphaned
            .human()
            .starts_with("[t7] comment · orphaned, was lines 41–44 in v2 · Loop › Anchors\n"));
    }

    #[test]
    fn a_long_quote_is_cut_in_the_human_rendering_only() {
        let mut long = thread();
        long.anchor.quote = "x".repeat(QUOTE_CHARS + 5);
        let text = long.human();
        assert!(text.contains(&format!("{}…", "x".repeat(QUOTE_CHARS))));
        let json = serde_json::to_string(&long).unwrap();
        assert!(json.contains(&"x".repeat(QUOTE_CHARS + 5)));
    }

    // Case: docs/cases/gist-md-review.md#wait-redelivered
    #[test]
    fn a_redelivery_names_its_first_time_and_replied_threads() {
        let submitted = Submitted {
            file: "docs/foo.md".to_string(),
            round: 2,
            version: 2,
            summary: None,
            threads: vec![thread()],
            total: 1,
            truncated: false,
            redelivered: Some(Redelivered {
                first: 1_790_883_720,
                replied: vec!["t7".to_string()],
            }),
            file_differs: true,
        };
        let text = submitted.human();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            "review submitted · docs/foo.md · round 2 · 1 thread"
        );
        assert_eq!(lines[1], "redelivered · first 19:42 UTC · replied t7");
        assert_eq!(
            lines[2],
            "warning: docs/foo.md changed since round 2 was read; line numbers refer to v2"
        );
    }
}
