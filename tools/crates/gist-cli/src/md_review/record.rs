//! The review record: a markdown file under `.md-review/records/` written on
//! approve, which outlives the store. Complete, not bounded: every thread with
//! its messages and outcomes, the note, and the approved version; when the
//! working file has moved on, the approved text and the block diff to it.
//! docs/design/md-review-hld.md#approve.

use super::api::{utc, ThreadView};
use super::diff::{diff_blocks, Change};
use super::render::render;
use super::store::{Approval, Author, ThreadState};

/// The record's file name: the reviewed path flattened for reading, and the
/// approval time, which keeps names apart and finds the record again after
/// a restart.
pub fn name(file: &str, approved_at: u64) -> String {
    let flat: String = file
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let [year, month, day, hour, minute, second] = utc(approved_at);
    format!("{flat}-{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z.md")
}

/// What the record is written from.
pub struct Input<'a> {
    pub file: &'a str,
    pub approval: &'a Approval,
    pub threads: &'a [ThreadView],
    /// The approved version's text.
    pub approved: &'a [u8],
    /// The working file at approve, or why it could not be read.
    pub working: &'a Result<Vec<u8>, String>,
}

/// The record as markdown.
pub fn compose(input: &Input) -> String {
    let approval = input.approval;
    let [year, month, day, hour, minute, _] = utc(approval.at);
    let mut out = format!("# Review of {}\n\n", input.file);
    out.push_str(&format!(
        "Approved at round {}, version {}, on {year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC.\n\
         Approved text SHA-256: `{}`.\n",
        approval.round, approval.version, approval.hash
    ));
    let differs = match input.working {
        Ok(bytes) => bytes.as_slice() != input.approved,
        Err(_) => true,
    };
    if differs {
        out.push_str(
            "\n**The working file differs from the approved version.** Only the approved \
             text below was approved; the changes to the working file after it were not.\n",
        );
    }
    if let Some(note) = &approval.note {
        out.push_str("\n## Note\n\n");
        out.push_str(&quote(note));
    }
    if !approval.discarded.is_empty() {
        out.push_str(&format!(
            "\nPending at approve and never sent to the agent: {}.\n",
            approval.discarded.join(", ")
        ));
    }

    out.push_str("\n## Threads\n");
    if input.threads.is_empty() {
        out.push_str("\nNone.\n");
    }
    for thread in input.threads {
        out.push_str(&section(thread));
    }

    if differs {
        out.push_str("\n## Approved text\n\n");
        out.push_str(&fenced(
            &String::from_utf8_lossy(input.approved),
            "markdown",
        ));
        out.push_str("\n## Changes after approval\n\n");
        match input.working {
            Ok(working) => out.push_str(&changes(input.approved, working)),
            Err(reason) => {
                out.push_str(&format!("The working file could not be read: {reason}.\n"))
            }
        }
    }
    out
}

fn section(thread: &ThreadView) -> String {
    let anchor = &thread.anchor;
    let state = match thread.state {
        ThreadState::Applied(round) => format!("applied in round {round}"),
        ThreadState::Resolved => "resolved".to_string(),
        ThreadState::Open => "open".to_string(),
    };
    let mut place = format!(
        "lines {}–{} of v{}",
        anchor.lines[0], anchor.lines[1], anchor.version
    );
    if thread.orphaned {
        place = format!("orphaned, was {place}");
    }
    if !anchor.headings.is_empty() {
        place.push_str(&format!(", {}", anchor.headings.join(" › ")));
    }
    let mut out = format!("\n### {} · {state}\n\n{place}\n\n", thread.id);
    out.push_str(&quote(&anchor.quote));
    for message in &thread.messages {
        let who = match message.author {
            Author::Human => "Reviewer".to_string(),
            Author::Agent => match message.outcome {
                Some(outcome) => format!("Agent, {}", outcome.as_str()),
                None => "Agent".to_string(),
            },
        };
        out.push_str(&format!("\n**{who}** (round {}):\n\n", message.round));
        out.push_str(&quote(&message.body));
    }
    out
}

/// The block diff from the approved text to the working file, each changed
/// block's sources shown before and after.
fn changes(approved: &[u8], working: &[u8]) -> String {
    let (old, new) = match (render(approved), render(working)) {
        (Ok(old), Ok(new)) => (old, new),
        (_, Err(reason)) | (Err(reason), _) => {
            return format!("The working file could not be compared: {reason}.\n")
        }
    };
    let result = diff_blocks(&old, &new);
    let mut out = String::new();
    for deleted in &result.deleted {
        let block = &old[deleted.old];
        out.push_str(&format!(
            "Deleted, approved lines {}–{}:\n\n{}\n",
            block.lines[0],
            block.lines[1],
            fenced(&block.source, "markdown")
        ));
    }
    for (index, change) in result.blocks.iter().enumerate() {
        let block = &new[index];
        match change {
            Change::Unchanged { .. } => {}
            Change::Changed { old: was } => out.push_str(&format!(
                "Changed, approved lines {}–{}, now lines {}–{}:\n\n{}\n{}\n",
                old[*was].lines[0],
                old[*was].lines[1],
                block.lines[0],
                block.lines[1],
                fenced(&old[*was].source, "markdown"),
                fenced(&block.source, "markdown")
            )),
            Change::Added => out.push_str(&format!(
                "Added, now lines {}–{}:\n\n{}\n",
                block.lines[0],
                block.lines[1],
                fenced(&block.source, "markdown")
            )),
        }
    }
    if out.is_empty() {
        out.push_str("The blocks are the same; only whitespace between them differs.\n");
    }
    out
}

fn quote(text: &str) -> String {
    let mut out = String::new();
    for line in text.trim_end().lines() {
        out.push_str(if line.is_empty() { ">" } else { "> " });
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// `text` in a code fence longer than any backtick run inside it.
fn fenced(text: &str, info: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}{info}\n{}\n{fence}\n", text.trim_end_matches('\n'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::md_review::store::{Anchor, Kind, Message};

    fn approval(note: Option<&str>, discarded: &[&str]) -> Approval {
        Approval {
            round: 2,
            version: 2,
            hash: "abc".to_string(),
            note: note.map(str::to_string),
            discarded: discarded.iter().map(|d| d.to_string()).collect(),
            at: 1_790_883_360,
            delivered: false,
        }
    }

    fn thread() -> ThreadView {
        ThreadView {
            id: "t1".to_string(),
            state: ThreadState::Applied(1),
            anchor: Anchor {
                quote: "the quote".to_string(),
                prefix: String::new(),
                suffix: String::new(),
                blocks: vec![1],
                lines: [3, 3],
                headings: vec!["Top".to_string()],
                version: 2,
            },
            orphaned: false,
            messages: vec![Message {
                id: "m1".to_string(),
                author: Author::Human,
                kind: Kind::Comment,
                body: "Shorter.".to_string(),
                outcome: None,
                round: 1,
                at: 0,
            }],
        }
    }

    #[test]
    fn the_name_flattens_the_path_and_carries_the_approval_time() {
        assert_eq!(
            name("docs/foo.md", 1_790_883_360),
            "docs-foo-md-20261001T193600Z.md"
        );
    }

    // Case: docs/cases/gist-md-review.md#approve
    #[test]
    fn a_record_of_an_unchanged_file_holds_threads_note_and_discards_but_not_the_text() {
        let text = b"# Top\n\nthe quote\n".to_vec();
        let record = compose(&Input {
            file: "docs/foo.md",
            approval: &approval(Some("Ship it."), &["t4"]),
            threads: &[thread()],
            approved: &text,
            working: &Ok(text.clone()),
        });
        assert!(record.starts_with("# Review of docs/foo.md\n"), "{record}");
        assert!(record.contains("Approved at round 2, version 2, on 2026-10-01 19:36 UTC."));
        assert!(record.contains("## Note\n\n> Ship it.\n"));
        assert!(record.contains("never sent to the agent: t4."));
        assert!(
            record.contains("### t1 · applied in round 1\n\nlines 3–3 of v2, Top\n\n> the quote\n")
        );
        assert!(record.contains("**Reviewer** (round 1):\n\n> Shorter.\n"));
        assert!(!record.contains("Approved text\n"), "{record}");
        assert!(!record.contains("differs"), "{record}");
    }

    // Case: docs/cases/gist-md-review.md#approve-drifted
    #[test]
    fn a_record_of_a_drifted_file_holds_the_approved_text_and_the_changes() {
        let approved = b"# Top\n\nkept\n\nold words\n\ngone\n".to_vec();
        let working = b"# Top\n\nkept\n\nnew words\n\nadded\n".to_vec();
        let record = compose(&Input {
            file: "docs/foo.md",
            approval: &approval(None, &[]),
            threads: &[],
            approved: &approved,
            working: &Ok(working),
        });
        assert!(record.contains("working file differs from the approved version"));
        assert!(record.contains(
            "## Approved text\n\n```markdown\n# Top\n\nkept\n\nold words\n\ngone\n```\n"
        ));
        assert!(record.contains("Changed, approved lines 5–5, now lines 5–5:\n\n```markdown\nold words\n```\n\n```markdown\nnew words\n```"), "{record}");
        assert!(
            record.contains("Changed, approved lines 7–7, now lines 7–7"),
            "{record}"
        );
    }

    #[test]
    fn a_fence_outgrows_the_backticks_inside() {
        assert_eq!(fenced("a ```` b", ""), "`````\na ```` b\n`````\n");
        assert_eq!(fenced("plain\n", "md"), "```md\nplain\n```\n");
    }
}
