//! `gk hook` — the git hooks gk provides, and the command that installs them.
//! `install` writes a small shell shim into the repository's hooks directory
//! that calls back into `gk hook <name>`, so the rules live once, in Rust.
//! Ownership of a shim is a marker line, not a manifest. Repository and config
//! questions go to `git` itself, not libgit2, so `git -c`, `GIT_CONFIG_*` and
//! repository formats libgit2 cannot open behave as they do for git. See
//! docs/adr/0009-install-git-hooks-with-gk.md.

use clap::{Args as ClapArgs, Subcommand};
use gist_core::Human;
use regex::Regex;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::init::{write_atomic, Status, UninstallStatus};

/// The git hooks gk provides. Each is installed as a shim that runs
/// `gk hook <name>`, so each needs a `HookCommand` variant of the same name.
pub(crate) const HOOKS: &[&str] = &["commit-msg"];

const DEFAULT_LIMIT: usize = 72;
const SUBJECT_NOTE_ABOVE: usize = 50;
const EXEMPT_SUBJECTS: &[&str] = &["Merge ", "fixup! ", "squash! ", "Revert "];

/// Command line for `gk hook`.
#[derive(ClapArgs, Debug)]
pub struct Args {
    #[command(subcommand)]
    command: HookCommand,
}

#[derive(Subcommand, Debug)]
enum HookCommand {
    /// Install the git hooks gk provides into this repository, or remove them
    Install(InstallArgs),

    /// Check a commit message file; this is what the installed commit-msg hook runs
    CommitMsg {
        /// The commit message file git passes to the hook
        file: PathBuf,
    },
}

#[derive(ClapArgs, Debug)]
struct InstallArgs {
    /// Replace a hook that is not gk's, or a symlink, instead of reporting a conflict
    #[arg(long, conflicts_with = "uninstall")]
    force: bool,

    /// Remove the hooks a previous install wrote
    #[arg(long)]
    uninstall: bool,
}

#[derive(Debug, Serialize)]
struct InstallItem {
    name: String,
    path: String,
    status: Status,
}

#[derive(Debug, Serialize)]
struct UninstallItem {
    name: String,
    path: String,
    status: UninstallStatus,
}

/// What `hook install` did to each hook, and where.
#[derive(Debug, Serialize)]
pub struct InstallReport {
    directory: String,
    hooks: Vec<InstallItem>,
}

/// What `hook install --uninstall` did to each hook, and where.
#[derive(Debug, Serialize)]
pub struct UninstallReport {
    directory: String,
    hooks: Vec<UninstallItem>,
}

/// Advice on an accepted commit message; a rejected one is an `Err` instead.
#[derive(Debug, Serialize)]
pub struct CommitMsgReport {
    notes: Vec<String>,
}

/// Everything `gk hook` can report, whichever subcommand ran.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Report {
    Install(InstallReport),
    Uninstall(UninstallReport),
    CommitMsg(CommitMsgReport),
}

impl Report {
    /// Whether the caller should see a non-zero exit: a hook left in place
    /// that is not gk's, on either install or uninstall.
    pub(crate) fn is_failure(&self) -> bool {
        match self {
            Report::Install(report) => report.hooks.iter().any(|h| h.status == Status::Conflict),
            Report::Uninstall(report) => report
                .hooks
                .iter()
                .any(|h| h.status == UninstallStatus::Kept),
            Report::CommitMsg(_) => false,
        }
    }

    /// Advice from an accepted commit message is a diagnostic for whoever is
    /// committing, not a result: it goes to stderr, and to nowhere when empty.
    pub(crate) fn is_diagnostic(&self) -> bool {
        matches!(self, Report::CommitMsg(_))
    }
}

/// Run the `gk hook` subcommand `args` names.
pub fn run(args: Args) -> Result<Report, String> {
    match args.command {
        HookCommand::Install(args) if args.uninstall => uninstall().map(Report::Uninstall),
        HookCommand::Install(args) => install(args.force).map(Report::Install),
        HookCommand::CommitMsg { file } => commit_msg(&file).map(Report::CommitMsg),
    }
}

struct GitOutput {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Run `git` in the current directory. Only failing to start it is an error
/// here; the caller decides what an exit status means.
fn git(args: &[&str]) -> Result<GitOutput, String> {
    let output = std::process::Command::new("git")
        .args(args)
        .output()
        .map_err(|err| format!("could not run git: {err}"))?;
    Ok(GitOutput {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout)
            .trim_end_matches('\n')
            .to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

/// A lookup where exit status 1 means "not set", as for `git config --get`.
/// Any other failure is an error.
fn lookup(args: &[&str]) -> Result<Option<String>, String> {
    let output = git(args)?;
    match output.code {
        Some(0) => Ok(Some(output.stdout)),
        Some(1) => Ok(None),
        _ => Err(output.stderr),
    }
}

fn marker(name: &str) -> String {
    format!("# gk-hook: {name}")
}

fn shim(name: &str) -> String {
    format!(
        "#!/bin/sh\n\
         {marker}\n\
         command -v gk >/dev/null 2>&1 || {{ echo \"{name}: gk is not on PATH; install gk, or delete this hook ($0)\" >&2; exit 1; }}\n\
         exec gk hook {name} \"$@\"\n",
        marker = marker(name),
    )
}

/// A file is gk's when its second line is the marker for `name`.
fn is_ours(bytes: &[u8], name: &str) -> bool {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| text.lines().nth(1))
        == Some(marker(name).as_str())
}

enum Existing {
    Absent,
    Symlink,
    File { bytes: Vec<u8>, executable: bool },
}

#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &std::fs::Metadata) -> bool {
    true
}

fn inspect(path: &Path) -> Result<Existing, String> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Ok(Existing::Symlink),
        Ok(meta) => std::fs::read(path)
            .map(|bytes| Existing::File {
                bytes,
                executable: is_executable(&meta),
            })
            .map_err(|err| format!("could not read {}: {err}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Existing::Absent),
        Err(err) => Err(format!("could not inspect {}: {err}", path.display())),
    }
}

fn decide_install(existing: &Existing, name: &str, incoming: &str, force: bool) -> Status {
    match existing {
        Existing::Absent => Status::Installed,
        // Git skips a hook that is not executable, so identical bytes are
        // not enough to call it installed.
        Existing::File { bytes, executable } if bytes == incoming.as_bytes() => {
            if *executable {
                Status::Unchanged
            } else {
                Status::Overwritten
            }
        }
        Existing::File { bytes, .. } if is_ours(bytes, name) => Status::Overwritten,
        _ if force => Status::Overwritten,
        _ => Status::Conflict,
    }
}

fn decide_uninstall(existing: &Existing, name: &str) -> UninstallStatus {
    match existing {
        Existing::Absent => UninstallStatus::Missing,
        Existing::File { bytes, .. } if is_ours(bytes, name) => UninstallStatus::Removed,
        _ => UninstallStatus::Kept,
    }
}

/// Where git looks for hooks by default. Linked worktrees share the main
/// repository's directory, as they do for git.
fn hooks_dir() -> Result<PathBuf, String> {
    let output = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    if output.code != Some(0) {
        return Err(format!("not inside a git repository: {}", output.stderr));
    }
    Ok(PathBuf::from(output.stdout).join("hooks"))
}

fn refuse_configured_hooks_path() -> Result<(), String> {
    match lookup(&["config", "--get", "core.hooksPath"])
        .map_err(|err| format!("could not read core.hooksPath: {err}"))?
    {
        Some(value) => Err(format!(
            "core.hooksPath is set to '{value}'; gk will not install a hook there, since that \
             directory is shared or managed by another tool. Have that tool run \
             `gk hook commit-msg \"$1\"` instead"
        )),
        None => Ok(()),
    }
}

fn install(force: bool) -> Result<InstallReport, String> {
    let dir = hooks_dir()?;
    refuse_configured_hooks_path()?;
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("could not create {}: {err}", dir.display()))?;

    let mut hooks = Vec::new();
    for name in HOOKS {
        let path = dir.join(name);
        let incoming = shim(name);
        let status = decide_install(&inspect(&path)?, name, &incoming, force);
        if matches!(status, Status::Installed | Status::Overwritten) {
            let tmp = dir.join(format!("{name}.gk-tmp"));
            write_atomic(&path, &tmp, incoming.as_bytes(), true)?;
        }
        hooks.push(InstallItem {
            name: name.to_string(),
            path: path.display().to_string(),
            status,
        });
    }
    Ok(InstallReport {
        directory: dir.display().to_string(),
        hooks,
    })
}

/// Unlike install, this ignores `core.hooksPath`: it removes only a file that
/// carries gk's marker, so cleaning up after an install made before the setting
/// existed is safe.
fn uninstall() -> Result<UninstallReport, String> {
    let dir = hooks_dir()?;

    let mut hooks = Vec::new();
    for name in HOOKS {
        let path = dir.join(name);
        let status = decide_uninstall(&inspect(&path)?, name);
        if status == UninstallStatus::Removed {
            std::fs::remove_file(&path)
                .map_err(|err| format!("could not remove {}: {err}", path.display()))?;
        }
        hooks.push(UninstallItem {
            name: name.to_string(),
            path: path.display().to_string(),
            status,
        });
    }
    Ok(UninstallReport {
        directory: dir.display().to_string(),
        hooks,
    })
}

/// A project's own subject convention, opted into with `mr.commitPattern`.
struct Project {
    regex: Regex,
    /// The pattern as written, for the error message.
    source: String,
}

struct Rules {
    subject_max: usize,
    body_max: usize,
    project: Option<Project>,
}

fn limit(key: &str) -> Result<usize, String> {
    let value = lookup(&["config", "--type=int", "--get", key])
        .map_err(|err| format!("could not read {key}: {err}"))?;
    match value {
        Some(value) => value
            .parse()
            .map_err(|_| format!("{key} must not be negative: {value}")),
        None => Ok(DEFAULT_LIMIT),
    }
}

fn read_project() -> Result<Option<Project>, String> {
    let source = lookup(&["config", "--get", "mr.commitPattern"])
        .map_err(|err| format!("could not read mr.commitPattern: {err}"))?
        .unwrap_or_default();
    if source.is_empty() {
        return Ok(None);
    }
    let regex = Regex::new(&source)
        .map_err(|err| format!("mr.commitPattern is not a valid pattern: {err}"))?;
    Ok(Some(Project { regex, source }))
}

fn read_rules() -> Result<Rules, String> {
    Ok(Rules {
        subject_max: limit("commit.subjectMax")?,
        body_max: limit("commit.bodyMax")?,
        project: read_project()?,
    })
}

fn message_lines(text: &str) -> Vec<&str> {
    text.lines().filter(|line| !line.starts_with('#')).collect()
}

fn is_exempt(subject: &str) -> bool {
    EXEMPT_SUBJECTS
        .iter()
        .any(|prefix| subject.starts_with(prefix))
}

/// URLs, trailers like `Co-Authored-By: x`, and indented lines may run long.
fn body_line_may_run_long(line: &str) -> bool {
    let trailer = line.split_once(": ").is_some_and(|(key, _)| {
        !key.is_empty() && key.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
    });
    line.contains("http://")
        || line.contains("https://")
        || trailer
        || line.starts_with(char::is_whitespace)
}

/// Apply the rules to the comment-stripped lines of a message. Returns the
/// advice to print on accept, or the rule that rejects it.
fn check(lines: &[&str], rules: &Rules) -> Result<Vec<String>, String> {
    let subject = lines.first().copied().unwrap_or("");
    let mut notes = Vec::new();

    if subject.is_empty() {
        return Err("empty subject".to_string());
    }
    if rules.project.is_none() && !subject.chars().next().is_some_and(char::is_uppercase) {
        return Err(format!(
            "subject must start with an uppercase letter: {subject}"
        ));
    }
    if subject.ends_with('.') {
        return Err(format!("subject must not end with a period: {subject}"));
    }
    let length = subject.chars().count();
    if length > rules.subject_max {
        return Err(format!(
            "subject is {length} chars, max {}: {subject}",
            rules.subject_max
        ));
    }
    if length > SUBJECT_NOTE_ABOVE {
        notes.push(format!(
            "note: subject is {length} chars; under {SUBJECT_NOTE_ABOVE} reads better"
        ));
    }

    if lines.len() > 1 {
        if !lines[1].is_empty() {
            return Err("line 2 must be blank (subject, blank line, body)".to_string());
        }
        for (index, line) in lines.iter().enumerate().skip(2) {
            let length = line.chars().count();
            if length > rules.body_max && !body_line_may_run_long(line) {
                return Err(format!(
                    "body line {} is {length} chars, max {}",
                    index + 1,
                    rules.body_max
                ));
            }
        }
    }

    if let Some(project) = &rules.project {
        if !project.regex.is_match(subject) {
            return Err(format!(
                "project requires subject matching {}",
                project.source
            ));
        }
    }
    Ok(notes)
}

fn commit_msg(file: &Path) -> Result<CommitMsgReport, String> {
    let bytes =
        std::fs::read(file).map_err(|err| format!("could not read {}: {err}", file.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    let lines = message_lines(&text);

    if is_exempt(lines.first().copied().unwrap_or("")) {
        return Ok(CommitMsgReport { notes: Vec::new() });
    }
    let notes = check(&lines, &read_rules()?)?;
    Ok(CommitMsgReport { notes })
}

impl Human for Report {
    fn human(&self) -> String {
        match self {
            Report::Install(report) => report.human(),
            Report::Uninstall(report) => report.human(),
            Report::CommitMsg(report) => report.human(),
        }
    }
}

impl Human for CommitMsgReport {
    fn human(&self) -> String {
        self.notes.join("\n")
    }
}

impl Human for InstallReport {
    fn human(&self) -> String {
        let count = |status| self.hooks.iter().filter(|h| h.status == status).count();
        let mut out = format!(
            "{} · {} installed, {} unchanged, {} overwritten, {} conflict(s)\n",
            self.directory,
            count(Status::Installed),
            count(Status::Unchanged),
            count(Status::Overwritten),
            count(Status::Conflict),
        );
        for hook in &self.hooks {
            out.push_str(&format!("  {:<11} {}\n", hook.status.label(), hook.name));
        }
        if count(Status::Conflict) > 0 {
            out.push_str(
                "\n  conflict: a hook there is not gk's — rerun with --force to replace it\n",
            );
        }
        out.trim_end().to_string()
    }
}

impl Human for UninstallReport {
    fn human(&self) -> String {
        let count = |status| self.hooks.iter().filter(|h| h.status == status).count();
        let mut out = format!(
            "{} · {} removed, {} kept, {} missing\n",
            self.directory,
            count(UninstallStatus::Removed),
            count(UninstallStatus::Kept),
            count(UninstallStatus::Missing),
        );
        for hook in &self.hooks {
            out.push_str(&format!("  {:<8} {}\n", hook.status.label(), hook.name));
        }
        if count(UninstallStatus::Kept) > 0 {
            out.push_str("\n  kept: not a gk hook — left alone\n");
        }
        out.trim_end().to_string()
    }
}
