//! The review store: where a review lives on disk, the lock that makes `serve`
//! its only writer, the append-only event log and its fold, and the snapshot
//! of the working file per round. docs/adr/0014-review-state-in-an-append-only-log.md;
//! cases in docs/cases/gist-md-review.md.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// The log format this `gk` reads and writes; `review_started` records it.
pub const FORMAT: u32 = 1;
/// Largest working file `serve` accepts, so rendering, diffing and `wait`
/// output stay bounded.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

const DIR: &str = ".md-review";
const REVIEWS: &str = "reviews";
const RECORDS: &str = "records";
const LOCK: &str = "lock";
const LOG: &str = "events.jsonl";
const SERVER: &str = "server.json";
const KEY_HEX_CHARS: usize = 16;

/// Where the review of one file lives: the root that holds `.md-review/`,
/// the file's path relative to it, and the store key derived from that path.
#[derive(Debug, Clone)]
pub struct Location {
    root: PathBuf,
    file: String,
    key: String,
    /// Absolute git common directory, whose `info/exclude` gains `.md-review/`.
    git_dir: Option<PathBuf>,
}

impl Location {
    /// Locate the review of `file`: inside a git work tree the root is its
    /// top level, otherwise the current directory. The file must exist and
    /// lie under the root.
    pub fn find(file: &Path) -> Result<Self, String> {
        let cwd = std::env::current_dir()
            .map_err(|err| format!("could not read the current directory: {err}"))?;
        Self::find_from(file, &cwd)
    }

    fn find_from(file: &Path, cwd: &Path) -> Result<Self, String> {
        let canonical =
            std::fs::canonicalize(file).map_err(|_| format!("no such file: {}", file.display()))?;
        if !canonical.is_file() {
            return Err(format!("not a file: {}", file.display()));
        }
        let parent = canonical.parent().unwrap_or(Path::new("/"));
        let (root, git_dir) = match git_dirs(parent)? {
            Some((top, common)) => (top, Some(common)),
            None => {
                let cwd = std::fs::canonicalize(cwd)
                    .map_err(|err| format!("could not resolve {}: {err}", cwd.display()))?;
                (cwd, None)
            }
        };
        let relative = canonical.strip_prefix(&root).map_err(|_| {
            format!(
                "{} is outside {}, where its review would be kept",
                file.display(),
                root.display()
            )
        })?;
        let mut parts = Vec::new();
        for part in relative.components() {
            parts.push(
                part.as_os_str()
                    .to_str()
                    .ok_or_else(|| format!("{} is not valid UTF-8", file.display()))?,
            );
        }
        let file = parts.join("/");
        Ok(Self {
            key: key_for(&file),
            root,
            file,
            git_dir,
        })
    }

    /// The reviewed file, relative to the root, `/`-separated.
    pub fn file(&self) -> &str {
        &self.file
    }

    /// The directory that holds `.md-review/`.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `.md-review/reviews/<key>` under the root.
    pub fn store_dir(&self) -> PathBuf {
        self.root.join(self.store_rel())
    }

    /// `.md-review/records` under the root.
    pub fn records_dir(&self) -> PathBuf {
        self.root.join(DIR).join(RECORDS)
    }

    fn store_rel(&self) -> PathBuf {
        Path::new(DIR).join(REVIEWS).join(&self.key)
    }

    /// Read the working file, refusing one over `MAX_FILE_BYTES`.
    pub fn read_working(&self) -> Result<Vec<u8>, String> {
        let path = self.root.join(&self.file);
        let file =
            File::open(&path).map_err(|err| format!("could not read {}: {err}", self.file))?;
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|err| format!("could not read {}: {err}", self.file))?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(format!(
                "{} is over the 1 MiB limit for a review",
                self.file
            ));
        }
        Ok(bytes)
    }

    /// The live server's `server.json`, if one was written.
    pub fn read_server(&self) -> Result<Option<ServerInfo>, String> {
        let rel = self.store_rel().join(SERVER);
        refuse_symlink(&self.root, &rel)?;
        match std::fs::read(self.root.join(&rel)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|err| format!("{} is not readable: {err}", rel.display())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(format!("could not read {}: {err}", rel.display())),
        }
    }
}

/// The top level and the common git directory of the work tree holding
/// `dir`, or `None` outside one. Asked of git itself, as `gk hook` does.
fn git_dirs(dir: &Path) -> Result<Option<(PathBuf, PathBuf)>, String> {
    let output = std::process::Command::new("git")
        .args([
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-common-dir",
        ])
        .current_dir(dir)
        .output()
        .map_err(|err| format!("could not run git: {err}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    match (lines.next(), lines.next()) {
        (Some(top), Some(common)) => {
            let top = std::fs::canonicalize(top)
                .map_err(|err| format!("could not resolve {top}: {err}"))?;
            Ok(Some((top, PathBuf::from(common))))
        }
        _ => Ok(None),
    }
}

/// Hex, one path segment, derived from the path alone: an existing review
/// is found again, and no two paths share a store the way a flattened slug
/// (`docs/foo.md`, `docs-foo.md`) would.
fn key_for(file: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(file.as_bytes()));
    digest[..KEY_HEX_CHARS].to_string()
}

/// SHA-256 of a version's bytes, as recorded in the log.
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Refuse a symlink at any component of `rel` under `root`, since writing
/// or deleting through one lands outside the store.
fn refuse_symlink(root: &Path, rel: &Path) -> Result<(), String> {
    let mut current = root.to_path_buf();
    for component in rel.components() {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "{} is a symlink, and gk will not keep a review through one",
                    current.display()
                ))
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(format!("could not inspect {}: {err}", current.display())),
        }
    }
    Ok(())
}

/// Address and credentials of a running `serve`, read by its clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub pid: u32,
    pub port: u16,
    pub token: String,
    /// The `gk` version that started the server; a client of another
    /// version refuses to talk to it.
    pub version: String,
}

/// Who wrote a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Author {
    Human,
    Agent,
}

/// What a message asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Comment,
    Question,
}

/// The agent's answer to a thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Applied,
    Declined,
    Answered,
}

/// Where a thread points in the rendered document; built by the page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub quote: String,
    pub prefix: String,
    pub suffix: String,
    pub blocks: Vec<u32>,
    pub lines: [u32; 2],
    pub headings: Vec<String>,
    pub version: u32,
}

/// One entry of `events.jsonl`. Field names are a persisted format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    ReviewStarted {
        format: u32,
        file: String,
        version: u32,
        hash: String,
    },
    MessagePosted {
        thread: String,
        message: String,
        author: Author,
        kind: Kind,
        body: String,
        /// Only on the message that opens a thread.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anchor: Option<Anchor>,
        /// Only on an agent's message.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        outcome: Option<Outcome>,
    },
    MessageEdited {
        message: String,
        body: String,
    },
    MessageDeleted {
        message: String,
    },
    ReviewSubmitted {
        round: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    /// A client received the round's submit, or the approval.
    ReviewDelivered {
        round: u32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        approved: bool,
    },
    RoundStarted {
        round: u32,
        version: u32,
        hash: String,
    },
    ThreadResolved {
        thread: String,
    },
    Approved {
        round: u32,
        version: u32,
        hash: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        /// Threads still pending when the reviewer approved.
        discarded: Vec<String>,
    },
}

/// An event with the time it was appended, in seconds since the Unix epoch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub at: u64,
    #[serde(flatten)]
    pub event: Event,
}

/// The review as the fold of its log.
#[derive(Debug, Clone, Default)]
pub struct Review {
    pub file: String,
    /// Current round, from 1.
    pub round: u32,
    /// Hash of each version; version `n` is at index `n - 1`.
    pub versions: Vec<String>,
    /// In the order they were opened.
    pub threads: Vec<Thread>,
    /// The current round's submit, pending until the next round starts.
    pub submit: Option<Submit>,
    pub approval: Option<Approval>,
    messages_seen: HashSet<String>,
}

/// A submitted round, waiting for the agent.
#[derive(Debug, Clone, PartialEq)]
pub struct Submit {
    pub summary: Option<String>,
    pub at: u64,
    /// When a client first received it; a later delivery is a repeat.
    pub first_delivery: Option<u64>,
}

/// The reviewer's approval of the version on screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Approval {
    pub round: u32,
    pub version: u32,
    pub hash: String,
    pub note: Option<String>,
    pub discarded: Vec<String>,
    pub at: u64,
    pub delivered: bool,
}

/// A comment thread on one anchor: the reviewer's messages and the agent's replies.
#[derive(Debug, Clone, PartialEq)]
pub struct Thread {
    pub id: String,
    pub anchor: Anchor,
    pub messages: Vec<Message>,
    pub state: ThreadState,
}

/// Whether a thread still needs attention; derived on fold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Open,
    Resolved,
    /// The agent applied it in this round.
    Applied(u32),
}

/// One message in a thread.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub id: String,
    pub author: Author,
    pub kind: Kind,
    pub body: String,
    pub outcome: Option<Outcome>,
    /// Round it was posted in.
    pub round: u32,
    pub at: u64,
}

impl Review {
    /// Fold a whole log; the first line must be `review_started`.
    pub fn fold(lines: &[Line]) -> Self {
        let mut review = Self::default();
        for line in lines {
            review.apply(line);
        }
        review
    }

    /// Advance the state by one event. Validity of a write is checked by
    /// `serve` before it appends, so the fold only applies; a duplicated
    /// message id, from a client retrying after a crash, appears once.
    pub fn apply(&mut self, line: &Line) {
        match &line.event {
            Event::ReviewStarted { file, hash, .. } => {
                self.file = file.clone();
                self.round = 1;
                self.versions = vec![hash.clone()];
            }
            Event::MessagePosted {
                thread,
                message,
                author,
                kind,
                body,
                anchor,
                outcome,
            } => {
                if !self.messages_seen.insert(message.clone()) {
                    return;
                }
                let posted = Message {
                    id: message.clone(),
                    author: *author,
                    kind: *kind,
                    body: body.clone(),
                    outcome: *outcome,
                    round: self.round,
                    at: line.at,
                };
                let index = match self.threads.iter().position(|t| &t.id == thread) {
                    Some(index) => index,
                    None => match anchor {
                        Some(anchor) => {
                            self.threads.push(Thread {
                                id: thread.clone(),
                                anchor: anchor.clone(),
                                messages: Vec::new(),
                                state: ThreadState::Open,
                            });
                            self.threads.len() - 1
                        }
                        None => return,
                    },
                };
                let target = &mut self.threads[index];
                target.state = match (author, outcome) {
                    (Author::Human, _) => ThreadState::Open,
                    (Author::Agent, Some(Outcome::Applied)) => ThreadState::Applied(self.round),
                    (Author::Agent, _) => target.state,
                };
                target.messages.push(posted);
            }
            Event::MessageEdited { message, body } => {
                if let Some(found) = self
                    .threads
                    .iter_mut()
                    .flat_map(|t| t.messages.iter_mut())
                    .find(|m| &m.id == message)
                {
                    found.body = body.clone();
                }
            }
            Event::MessageDeleted { message } => {
                for thread in &mut self.threads {
                    thread.messages.retain(|m| &m.id != message);
                }
                self.threads.retain(|t| !t.messages.is_empty());
            }
            Event::ReviewSubmitted { round, summary } => {
                if *round == self.round && self.submit.is_none() {
                    self.submit = Some(Submit {
                        summary: summary.clone(),
                        at: line.at,
                        first_delivery: None,
                    });
                }
            }
            Event::ReviewDelivered { round, approved } => {
                if *approved {
                    if let Some(approval) = &mut self.approval {
                        approval.delivered = true;
                    }
                } else if *round == self.round {
                    if let Some(submit) = &mut self.submit {
                        submit.first_delivery.get_or_insert(line.at);
                    }
                }
            }
            Event::RoundStarted { round, hash, .. } => {
                self.round = *round;
                self.versions.push(hash.clone());
                self.submit = None;
            }
            Event::ThreadResolved { thread } => {
                if let Some(found) = self.threads.iter_mut().find(|t| &t.id == thread) {
                    found.state = ThreadState::Resolved;
                }
            }
            Event::Approved {
                round,
                version,
                hash,
                note,
                discarded,
            } => {
                self.approval = Some(Approval {
                    round: *round,
                    version: *version,
                    hash: hash.clone(),
                    note: note.clone(),
                    discarded: discarded.clone(),
                    at: line.at,
                    delivered: false,
                });
            }
        }
    }

    /// Whether a message with this id is already in the log.
    pub fn has_message(&self, id: &str) -> bool {
        self.messages_seen.contains(id)
    }
}

/// Result of trying to become the writer of a review.
#[derive(Debug)]
pub enum Opened {
    Writer(Box<Store>),
    /// Another process holds the lock: a live server owns the review.
    Busy,
}

/// The review store, held by its only writer for as long as it lives.
#[derive(Debug)]
pub struct Store {
    location: Location,
    /// Holds the OS lock; the OS releases it when the process dies.
    _lock: File,
    log: File,
    review: Review,
}

impl Store {
    /// Take the store's lock and load the review, starting one from
    /// `working` when the store has no log yet.
    ///
    /// A log whose format this `gk` cannot read is refused and left as it
    /// is. A trailing line cut short by a crash is dropped: it was never
    /// acknowledged to a client.
    pub fn open(location: &Location, working: &[u8]) -> Result<Opened, String> {
        let store_rel = location.store_rel();
        refuse_symlink(&location.root, &store_rel)?;
        let dir = location.store_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("could not create {}: {err}", dir.display()))?;
        if let Some(git_dir) = &location.git_dir {
            exclude(git_dir)?;
        }

        refuse_symlink(&location.root, &store_rel.join(LOCK))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join(LOCK))
            .map_err(|err| format!("could not open {}: {err}", dir.join(LOCK).display()))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Ok(Opened::Busy),
            Err(TryLockError::Error(err)) => {
                return Err(format!(
                    "could not lock {}: {err}",
                    dir.join(LOCK).display()
                ))
            }
        }

        refuse_symlink(&location.root, &store_rel.join(LOG))?;
        let log_path = dir.join(LOG);
        let mut log = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&log_path)
            .map_err(|err| format!("could not open {}: {err}", log_path.display()))?;
        let mut bytes = Vec::new();
        log.read_to_end(&mut bytes)
            .map_err(|err| format!("could not read {}: {err}", log_path.display()))?;
        let complete = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        let lines = parse_log(&bytes[..complete], location.file())?;
        if complete < bytes.len() {
            log.set_len(complete as u64)
                .map_err(|err| format!("could not repair {}: {err}", log_path.display()))?;
        }

        let mut store = Self {
            location: location.clone(),
            _lock: lock,
            log,
            review: Review::fold(&lines),
        };
        if lines.is_empty() {
            let hash = store.write_version(1, working)?;
            store.append(Event::ReviewStarted {
                format: FORMAT,
                file: location.file().to_string(),
                version: 1,
                hash,
            })?;
        }
        Ok(Opened::Writer(Box::new(store)))
    }

    /// The review as folded from the log so far.
    pub fn review(&self) -> &Review {
        &self.review
    }

    /// Where this store lives.
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// Append one event, flushed to disk before it is folded in, so state a
    /// client was told about survives a crash.
    pub fn append(&mut self, event: Event) -> Result<&Review, String> {
        let line = Line { at: now(), event };
        let mut text = serde_json::to_string(&line)
            .map_err(|err| format!("could not encode an event: {err}"))?;
        text.push('\n');
        self.log
            .write_all(text.as_bytes())
            .and_then(|()| self.log.sync_data())
            .map_err(|err| format!("could not append to {LOG}: {err}"))?;
        self.review.apply(&line);
        Ok(&self.review)
    }

    /// Snapshot version `number` as `v<number>.md` and return its hash.
    pub fn write_version(&self, number: u32, bytes: &[u8]) -> Result<String, String> {
        let name = format!("v{number}.md");
        self.write_file(&name, bytes, false)?;
        Ok(hash(bytes))
    }

    /// Read back the snapshot of version `number`.
    pub fn read_version(&self, number: u32) -> Result<Vec<u8>, String> {
        let rel = self.location.store_rel().join(format!("v{number}.md"));
        refuse_symlink(&self.location.root, &rel)?;
        std::fs::read(self.location.root.join(&rel))
            .map_err(|err| format!("could not read {}: {err}", rel.display()))
    }

    /// Write `server.json`, readable by its owner only: it holds the token.
    pub fn write_server(&self, info: &ServerInfo) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(info)
            .map_err(|err| format!("could not encode {SERVER}: {err}"))?;
        self.write_file(SERVER, &bytes, true)
    }

    /// Delete the store, confined to `.md-review/reviews/<key>`: refused,
    /// leaving everything in place, when any part of that path is a
    /// symlink. The key is hex, so the path has no `..` to escape by.
    pub fn remove(self) -> Result<(), String> {
        refuse_symlink(&self.location.root, &self.location.store_rel())?;
        let dir = self.location.store_dir();
        std::fs::remove_dir_all(&dir)
            .map_err(|err| format!("could not delete {}: {err}", dir.display()))
    }

    /// Replace `name` in the store through a temporary file and a rename,
    /// so a reader never sees half of it and a symlink there is replaced,
    /// not followed.
    fn write_file(&self, name: &str, bytes: &[u8], private: bool) -> Result<(), String> {
        let dir = self.location.store_dir();
        let path = dir.join(name);
        let tmp = dir.join(format!(".{name}.tmp"));
        let _ = std::fs::remove_file(&tmp);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        if private {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(not(unix))]
        let _ = private;
        let written = options
            .open(&tmp)
            .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_data()))
            .and_then(|()| std::fs::rename(&tmp, &path));
        if written.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        written.map_err(|err| format!("could not write {}: {err}", path.display()))
    }
}

/// Parse complete log lines. The format is checked on the first line before
/// anything else, so a log from a newer `gk` is refused rather than misread.
fn parse_log(bytes: &[u8], file: &str) -> Result<Vec<Line>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| format!("{LOG} is not UTF-8"))?;
    let mut lines = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        if index == 0 {
            check_first_line(raw, file)?;
        }
        let line: Line =
            serde_json::from_str(raw).map_err(|err| format!("{LOG} line {}: {err}", index + 1))?;
        lines.push(line);
    }
    Ok(lines)
}

fn check_first_line(raw: &str, file: &str) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|err| format!("{LOG} line 1: {err}"))?;
    if value["event"] != "review_started" {
        return Err(format!("{LOG} does not start with review_started"));
    }
    match value["format"].as_u64() {
        Some(format) if format == u64::from(FORMAT) => {}
        Some(format) => {
            return Err(format!(
                "{LOG} has format {format}, and this gk reads format {FORMAT}; \
                 continue the review with the gk that started it"
            ))
        }
        None => return Err(format!("{LOG} has no format")),
    }
    if value["file"] != file {
        return Err(format!("{LOG} belongs to {}, not {file}", value["file"]));
    }
    Ok(())
}

/// Add `.md-review/` to the repository's `info/exclude` once; the user's
/// `.gitignore` is never touched.
fn exclude(git_dir: &Path) -> Result<(), String> {
    let path = git_dir.join("info").join("exclude");
    let entry = format!("{DIR}/");
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(format!("could not read {}: {err}", path.display())),
    };
    if existing
        .lines()
        .any(|line| line.trim().trim_start_matches('/') == entry)
    {
        return Ok(());
    }
    let mut addition = String::new();
    if !existing.is_empty() && !existing.ends_with('\n') {
        addition.push('\n');
    }
    addition.push_str(&entry);
    addition.push('\n');
    std::fs::create_dir_all(git_dir.join("info"))
        .and_then(|()| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)?
                .write_all(addition.as_bytes())
        })
        .map_err(|err| format!("could not update {}: {err}", path.display()))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct Fixture {
        dir: TempDir,
    }

    impl Fixture {
        fn git() -> Self {
            let fixture = Self::plain();
            let status = std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(fixture.dir.path())
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .status()
                .expect("run git");
            assert!(status.success());
            fixture
        }

        fn plain() -> Self {
            Self {
                dir: TempDir::new().expect("temp dir"),
            }
        }

        fn path(&self, rel: &str) -> PathBuf {
            self.dir.path().join(rel)
        }

        fn write(&self, rel: &str, text: &str) -> Location {
            let path = self.path(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            Location::find(&path).expect("locate")
        }

        fn exclude(&self) -> String {
            std::fs::read_to_string(self.path(".git/info/exclude")).unwrap_or_default()
        }
    }

    /// `Store::open`, waiting out a busy lock for up to a second. A process
    /// another test thread is spawning holds a copy of every open
    /// descriptor, the lock's included, until its exec closes them, so a
    /// released lock can read as busy for a moment.
    fn open(location: &Location, working: &[u8]) -> Result<Opened, String> {
        for _ in 0..100 {
            match Store::open(location, working)? {
                Opened::Busy => std::thread::sleep(std::time::Duration::from_millis(10)),
                writer => return Ok(writer),
            }
        }
        Ok(Opened::Busy)
    }

    fn writer(location: &Location) -> Store {
        let working = location.read_working().expect("read");
        match open(location, &working).expect("open") {
            Opened::Writer(store) => *store,
            Opened::Busy => panic!("store still locked after a second"),
        }
    }

    fn anchor(quote: &str) -> Anchor {
        Anchor {
            quote: quote.to_string(),
            prefix: String::new(),
            suffix: String::new(),
            blocks: vec![0],
            lines: [1, 1],
            headings: vec![],
            version: 1,
        }
    }

    fn post(thread: &str, message: &str, author: Author, outcome: Option<Outcome>) -> Event {
        Event::MessagePosted {
            thread: thread.to_string(),
            message: message.to_string(),
            author,
            kind: Kind::Comment,
            body: format!("body of {message}"),
            anchor: Some(anchor("Hello")),
            outcome,
        }
    }

    // Case: docs/cases/gist-md-review.md#serve-start
    #[test]
    fn a_new_review_starts_its_log_and_first_version_and_excludes_the_store() {
        let fixture = Fixture::git();
        let location = fixture.write("docs/foo.md", "# Hello\n");
        let store = writer(&location);

        let dir = location.store_dir();
        assert!(dir.starts_with(
            fixture
                .dir
                .path()
                .canonicalize()
                .unwrap()
                .join(".md-review/reviews")
        ));
        assert!(dir.join("lock").is_file());
        assert_eq!(std::fs::read(dir.join("v1.md")).unwrap(), b"# Hello\n");
        let log = std::fs::read_to_string(dir.join("events.jsonl")).unwrap();
        let first: Line = serde_json::from_str(log.lines().next().unwrap()).unwrap();
        assert_eq!(
            first.event,
            Event::ReviewStarted {
                format: 1,
                file: "docs/foo.md".to_string(),
                version: 1,
                hash: hash(b"# Hello\n"),
            }
        );
        assert_eq!(store.review().round, 1);
        assert_eq!(fixture.exclude().matches(".md-review/").count(), 1);
        assert!(!fixture.path(".gitignore").exists());
    }

    // Case: docs/cases/gist-md-review.md#serve-start
    #[test]
    fn the_exclude_entry_is_added_once() {
        let fixture = Fixture::git();
        let foo = fixture.write("docs/foo.md", "a\n");
        let bar = fixture.write("docs/bar.md", "b\n");
        drop(writer(&foo));
        drop(writer(&foo));
        drop(writer(&bar));
        assert_eq!(fixture.exclude().matches(".md-review/").count(), 1);
    }

    // Case: docs/cases/gist-md-review.md#serve-start
    #[cfg(unix)]
    #[test]
    fn server_json_is_private_to_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::git();
        let location = fixture.write("docs/foo.md", "a\n");
        let store = writer(&location);
        let info = ServerInfo {
            pid: 1,
            port: 4000,
            token: "secret".to_string(),
            version: "0.1.0".to_string(),
        };
        store.write_server(&info).unwrap();
        let mode = std::fs::metadata(location.store_dir().join("server.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(location.read_server().unwrap(), Some(info));
    }

    // Case: docs/cases/gist-md-review.md#serve-reuse
    #[test]
    fn a_second_open_while_the_writer_lives_is_busy_and_appends_nothing() {
        let fixture = Fixture::git();
        let location = fixture.write("docs/foo.md", "a\n");
        let first = writer(&location);
        let log = location.store_dir().join("events.jsonl");
        let before = std::fs::read(&log).unwrap();

        assert!(matches!(
            Store::open(&location, b"a\n").unwrap(),
            Opened::Busy
        ));
        assert_eq!(std::fs::read(&log).unwrap(), before);

        drop(first);
        writer(&location);
    }

    // Case: docs/cases/gist-md-review.md#serve-resume
    #[test]
    fn a_reopened_store_resumes_threads_rounds_and_versions() {
        let fixture = Fixture::git();
        let location = fixture.write("docs/foo.md", "v1\n");
        let mut store = writer(&location);
        store.append(post("t1", "m1", Author::Human, None)).unwrap();
        store.append(post("t2", "m2", Author::Human, None)).unwrap();
        store
            .append(Event::MessageEdited {
                message: "m1".to_string(),
                body: "edited".to_string(),
            })
            .unwrap();
        store
            .append(Event::MessageDeleted {
                message: "m2".to_string(),
            })
            .unwrap();
        store
            .append(Event::ReviewSubmitted {
                round: 1,
                summary: None,
            })
            .unwrap();
        let hash2 = store.write_version(2, b"v2\n").unwrap();
        store
            .append(Event::RoundStarted {
                round: 2,
                version: 2,
                hash: hash2.clone(),
            })
            .unwrap();
        drop(store);

        let resumed = writer(&location);
        let review = resumed.review();
        assert_eq!(review.round, 2);
        assert_eq!(review.versions, vec![hash(b"v1\n"), hash2]);
        assert_eq!(
            review.threads.len(),
            1,
            "the deleted message's thread is gone"
        );
        assert_eq!(review.threads[0].messages[0].body, "edited");
        assert_eq!(review.submit, None, "the round's submit closed with it");
        assert_eq!(resumed.read_version(2).unwrap(), b"v2\n");
    }

    // Case: docs/cases/gist-md-review.md#serve-refuse
    #[test]
    fn a_log_of_an_unknown_format_is_refused_and_left_untouched() {
        let fixture = Fixture::git();
        let location = fixture.write("docs/foo.md", "a\n");
        drop(writer(&location));
        let log = location.store_dir().join("events.jsonl");
        let newer = "{\"at\":1,\"event\":\"review_started\",\"format\":2,\"file\":\"docs/foo.md\"}\n{\"at\":2,\"event\":\"something_new\"}\npartial";
        std::fs::write(&log, newer).unwrap();

        let err = open(&location, b"a\n").unwrap_err();
        assert!(err.contains("format 2"), "{err}");
        assert_eq!(std::fs::read_to_string(&log).unwrap(), newer);
    }

    // Case: docs/cases/gist-md-review.md#serve-refuse
    #[test]
    fn a_file_over_one_mebibyte_is_refused_naming_the_limit() {
        let fixture = Fixture::git();
        let big = "a".repeat(MAX_FILE_BYTES as usize + 1);
        let location = fixture.write("docs/big.md", &big);
        let err = location.read_working().unwrap_err();
        assert!(err.contains("1 MiB"), "{err}");

        let exact = "a".repeat(MAX_FILE_BYTES as usize);
        let location = fixture.write("docs/exact.md", &exact);
        assert_eq!(location.read_working().unwrap().len(), exact.len());
    }

    // Case: docs/cases/gist-md-review.md#serve-refuse
    #[test]
    fn a_missing_file_is_refused() {
        let fixture = Fixture::git();
        let err = Location::find(&fixture.path("docs/none.md")).unwrap_err();
        assert!(err.contains("no such file"), "{err}");
    }

    #[test]
    fn a_line_cut_short_by_a_crash_is_dropped_and_the_log_stays_appendable() {
        let fixture = Fixture::git();
        let location = fixture.write("docs/foo.md", "a\n");
        let mut store = writer(&location);
        store.append(post("t1", "m1", Author::Human, None)).unwrap();
        drop(store);
        let log = location.store_dir().join("events.jsonl");
        let mut file = OpenOptions::new().append(true).open(&log).unwrap();
        file.write_all(b"{\"at\":3,\"event\":\"message_po").unwrap();
        drop(file);

        let mut store = writer(&location);
        store.append(post("t2", "m2", Author::Human, None)).unwrap();
        drop(store);
        let resumed = writer(&location);
        let ids: Vec<_> = resumed
            .review()
            .threads
            .iter()
            .map(|t| t.id.as_str())
            .collect();
        assert_eq!(ids, ["t1", "t2"]);
    }

    // Case: docs/cases/gist-md-review.md#store-key
    #[test]
    fn a_log_that_belongs_to_another_file_is_refused() {
        let fixture = Fixture::git();
        let foo = fixture.write("docs/foo.md", "a\n");
        let bar = fixture.write("docs/bar.md", "b\n");
        drop(writer(&foo));
        drop(writer(&bar));
        let copied = std::fs::read(foo.store_dir().join("events.jsonl")).unwrap();
        std::fs::write(bar.store_dir().join("events.jsonl"), &copied).unwrap();

        let err = open(&bar, b"b\n").unwrap_err();
        assert!(err.contains("belongs to"), "{err}");
    }

    // Case: docs/cases/gist-md-review.md#store-key
    #[test]
    fn paths_that_flatten_alike_get_separate_stores() {
        let fixture = Fixture::git();
        let nested = fixture.write("docs/foo.md", "nested\n");
        let flat = fixture.write("docs-foo.md", "flat\n");
        let records = fixture.write("records", "named records\n");
        assert_ne!(nested.store_dir(), flat.store_dir());
        assert_ne!(records.store_dir(), records.records_dir());
        assert!(records
            .store_dir()
            .starts_with(records.root().join(".md-review/reviews")));

        drop(writer(&nested));
        drop(writer(&flat));
        assert_eq!(writer(&nested).review().file, "docs/foo.md");
        assert_eq!(writer(&flat).review().file, "docs-foo.md");
    }

    #[test]
    fn outside_a_git_repository_the_store_goes_under_the_current_directory() {
        let fixture = Fixture::plain();
        let path = fixture.path("notes/a.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "a\n").unwrap();

        let location = Location::find_from(&path, fixture.dir.path()).unwrap();
        assert_eq!(location.file(), "notes/a.md");
        assert_eq!(location.root(), fixture.dir.path().canonicalize().unwrap());

        let elsewhere = TempDir::new().unwrap();
        let err = Location::find_from(&path, elsewhere.path()).unwrap_err();
        assert!(err.contains("outside"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_store_directory_is_refused_on_open() {
        let fixture = Fixture::git();
        let location = fixture.write("docs/foo.md", "a\n");
        let elsewhere = TempDir::new().unwrap();
        std::fs::create_dir_all(fixture.path(".md-review")).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), fixture.path(".md-review/reviews")).unwrap();

        let err = open(&location, b"a\n").unwrap_err();
        assert!(err.contains("symlink"), "{err}");
        assert_eq!(std::fs::read_dir(elsewhere.path()).unwrap().count(), 0);
    }

    // Case: docs/cases/gist-md-review.md#approve-confined
    #[cfg(unix)]
    #[test]
    fn removal_is_refused_when_the_store_became_a_symlink() {
        let fixture = Fixture::git();
        let location = fixture.write("docs/foo.md", "a\n");
        let store = writer(&location);
        let dir = location.store_dir();
        let moved = fixture.path("moved");
        std::fs::rename(&dir, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &dir).unwrap();

        let err = store.remove().unwrap_err();
        assert!(err.contains("symlink"), "{err}");
        assert!(moved.join("events.jsonl").is_file(), "the target is kept");
    }

    // Case: docs/cases/gist-md-review.md#approve
    #[test]
    fn removal_deletes_the_store_and_nothing_beside_it() {
        let fixture = Fixture::git();
        let other = fixture.write("docs/bar.md", "b\n");
        let location = fixture.write("docs/foo.md", "a\n");
        drop(writer(&other));
        let store = writer(&location);
        std::fs::create_dir_all(location.records_dir()).unwrap();
        std::fs::write(location.records_dir().join("kept.md"), "x").unwrap();

        store.remove().unwrap();
        assert!(!location.store_dir().exists());
        assert!(other.store_dir().join("events.jsonl").is_file());
        assert!(location.records_dir().join("kept.md").is_file());
        assert!(fixture.path("docs/foo.md").is_file());
    }

    #[test]
    fn a_duplicated_message_id_appears_once() {
        let fixture = Fixture::git();
        let location = fixture.write("docs/foo.md", "a\n");
        let mut store = writer(&location);
        store.append(post("t1", "m1", Author::Human, None)).unwrap();
        store.append(post("t1", "m1", Author::Human, None)).unwrap();
        assert!(store.review().has_message("m1"));
        drop(store);
        let review = writer(&location).review().clone();
        assert_eq!(review.threads.len(), 1);
        assert_eq!(review.threads[0].messages.len(), 1);
    }

    // Case: docs/cases/gist-md-review.md#reopen
    #[test]
    fn a_reviewer_message_reopens_a_resolved_or_applied_thread() {
        let mut review = Review::fold(&[Line {
            at: 0,
            event: Event::ReviewStarted {
                format: 1,
                file: "f.md".to_string(),
                version: 1,
                hash: "h".to_string(),
            },
        }]);
        let mut apply = |event| review.apply(&Line { at: 0, event });
        apply(post("t1", "m1", Author::Human, None));
        apply(post("t2", "m2", Author::Human, None));
        apply(post("t1", "m3", Author::Agent, Some(Outcome::Applied)));
        apply(Event::ThreadResolved {
            thread: "t2".to_string(),
        });
        apply(post("t3", "m4", Author::Human, None));
        apply(post("t3", "m5", Author::Agent, Some(Outcome::Declined)));
        assert_eq!(
            review.threads.iter().map(|t| t.state).collect::<Vec<_>>(),
            [
                ThreadState::Applied(1),
                ThreadState::Resolved,
                ThreadState::Open
            ]
        );

        let mut apply = |event| review.apply(&Line { at: 0, event });
        apply(post("t1", "m6", Author::Human, None));
        apply(post("t2", "m7", Author::Human, None));
        assert_eq!(review.threads[0].state, ThreadState::Open);
        assert_eq!(review.threads[1].state, ThreadState::Open);
    }

    // Case: docs/cases/gist-md-review.md#wait-redelivered
    #[test]
    fn a_submit_remembers_its_first_delivery() {
        let fixture = Fixture::git();
        let location = fixture.write("docs/foo.md", "a\n");
        let mut store = writer(&location);
        store
            .append(Event::ReviewSubmitted {
                round: 1,
                summary: Some("ok".to_string()),
            })
            .unwrap();
        assert_eq!(store.review().submit.as_ref().unwrap().first_delivery, None);
        store
            .append(Event::ReviewDelivered {
                round: 1,
                approved: false,
            })
            .unwrap();
        let first = store.review().submit.as_ref().unwrap().first_delivery;
        assert!(first.is_some());
        store
            .append(Event::ReviewDelivered {
                round: 1,
                approved: false,
            })
            .unwrap();
        assert_eq!(
            store.review().submit.as_ref().unwrap().first_delivery,
            first
        );
    }

    // Case: docs/cases/gist-md-review.md#wait-approved
    #[test]
    fn an_approval_is_pending_until_delivered() {
        let fixture = Fixture::git();
        let location = fixture.write("docs/foo.md", "a\n");
        let mut store = writer(&location);
        store
            .append(Event::Approved {
                round: 1,
                version: 1,
                hash: hash(b"a\n"),
                note: None,
                discarded: vec![],
            })
            .unwrap();
        assert!(!store.review().approval.as_ref().unwrap().delivered);
        store
            .append(Event::ReviewDelivered {
                round: 1,
                approved: true,
            })
            .unwrap();
        drop(store);
        assert!(
            writer(&location)
                .review()
                .approval
                .as_ref()
                .unwrap()
                .delivered
        );
    }
}
