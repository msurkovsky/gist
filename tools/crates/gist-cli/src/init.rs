//! `gk init` — vendor this project's skills into a target repo, so an AI
//! coding agent's own skill discovery finds them. `--claude` targets Claude
//! Code's `.claude/skills/`, `--codex` targets Codex CLI's `.agents/skills/`
//! (see docs/adr/0006-codex-as-a-second-init-target.md) — either or both may
//! be passed in one call, and both receive byte-identical content.
//! `--experimental=<package>` adds one vendored tree under `experimental/` to
//! whichever target(s) were selected, prefixing each skill's name with the
//! package so it cannot silently shadow — or be shadowed by — a canonical
//! `gist-` skill.
//!
//! `gk` ships as a single binary with no source tree alongside it, so the
//! skills this repo produces are embedded into the binary at build time —
//! see docs/adr/0002-embed-skills-for-init.md.
//!
//! Every run of `init --claude` and/or `--codex` writes a manifest —
//! `<root>/.gist-manifest.json`, path + sha256 per file it placed — so
//! `--uninstall` can remove exactly what a prior `init` put there, whether or
//! not the binary running `--uninstall` still embeds the same skills. See
//! docs/adr/0007-manifest-driven-uninstall.md.

use clap::Args as ClapArgs;
use gist_core::Human;
use include_dir::{include_dir, Dir, File};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

static SKILLS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../../skills");
static EXPERIMENTAL: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../../experimental");

const CLAUDE_ROOT: &str = ".claude/skills";
const CODEX_ROOT: &str = ".agents/skills";
const MANIFEST_FILENAME: &str = ".gist-manifest.json";

/// Command line for `gk init`.
#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Vendor skills into ./.claude/skills/ for Claude Code
    #[arg(long)]
    claude: bool,

    /// Vendor skills into ./.agents/skills/ for Codex CLI
    #[arg(long)]
    codex: bool,

    /// Also vendor one experimental/<package> tree's skills, name-prefixed
    #[arg(long, num_args = 1.., value_name = "PACKAGE")]
    experimental: Vec<String>,

    /// Overwrite files with local changes instead of reporting a conflict
    #[arg(long)]
    force: bool,

    /// Remove what a previous init installed, using each target's manifest
    #[arg(long)]
    uninstall: bool,
}

/// What was done to one embedded file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Installed,
    Unchanged,
    Overwritten,
    Conflict,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Installed => "installed",
            Status::Unchanged => "unchanged",
            Status::Overwritten => "overwritten",
            Status::Conflict => "conflict",
        }
    }
}

#[derive(Debug, Serialize)]
struct FileReport {
    /// Path relative to the target root, e.g. `gist-outline/SKILL.md`.
    path: String,
    status: Status,
}

#[derive(Debug, Serialize)]
struct SkillReport {
    name: String,
    files: Vec<FileReport>,
}

#[derive(Debug, Default, Serialize)]
struct Totals {
    installed: usize,
    unchanged: usize,
    overwritten: usize,
    conflicts: usize,
}

impl Totals {
    fn record(&mut self, status: Status) {
        match status {
            Status::Installed => self.installed += 1,
            Status::Unchanged => self.unchanged += 1,
            Status::Overwritten => self.overwritten += 1,
            Status::Conflict => self.conflicts += 1,
        }
    }
}

/// Everything `init` did to one target root.
#[derive(Debug, Serialize)]
struct TargetReport {
    target: String,
    totals: Totals,
    skills: Vec<SkillReport>,
}

/// Everything an install run did, one `TargetReport` per selected target root.
#[derive(Debug, Serialize)]
pub struct InstallReport {
    totals: Totals,
    targets: Vec<TargetReport>,
}

/// What was done to one manifest-recorded file during `--uninstall`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UninstallStatus {
    Removed,
    Kept,
    Missing,
}

impl UninstallStatus {
    fn label(self) -> &'static str {
        match self {
            UninstallStatus::Removed => "removed",
            UninstallStatus::Kept => "kept",
            UninstallStatus::Missing => "missing",
        }
    }
}

#[derive(Debug, Serialize)]
struct UninstallFileReport {
    path: String,
    status: UninstallStatus,
}

#[derive(Debug, Default, Serialize)]
struct UninstallTotals {
    removed: usize,
    kept: usize,
    missing: usize,
}

impl UninstallTotals {
    fn record(&mut self, status: UninstallStatus) {
        match status {
            UninstallStatus::Removed => self.removed += 1,
            UninstallStatus::Kept => self.kept += 1,
            UninstallStatus::Missing => self.missing += 1,
        }
    }
}

#[derive(Debug, Serialize)]
struct UninstallTargetReport {
    target: String,
    totals: UninstallTotals,
    files: Vec<UninstallFileReport>,
}

/// Everything an uninstall run did, one `UninstallTargetReport` per selected
/// target root.
#[derive(Debug, Serialize)]
pub struct UninstallReport {
    totals: UninstallTotals,
    targets: Vec<UninstallTargetReport>,
}

/// Everything `gk init` did, in whichever direction it ran.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Report {
    Install(InstallReport),
    Uninstall(UninstallReport),
}

impl Report {
    /// Whether the caller should see a non-zero exit: an install conflict
    /// left unresolved, or an uninstall that left a locally modified file in
    /// place. Mirrors `--force` being the documented way past either.
    pub(crate) fn is_failure(&self) -> bool {
        match self {
            Report::Install(report) => report.totals.conflicts > 0,
            Report::Uninstall(report) => report.totals.kept > 0,
        }
    }
}

/// One file a skill places, path relative to a target root.
struct PlacedFile {
    path: PathBuf,
    contents: Cow<'static, [u8]>,
}

/// One skill's files, computed once and placed as-is into every selected
/// target root — content never varies by target, only the root does.
struct SkillItem {
    name: String,
    files: Vec<PlacedFile>,
}

/// A path under a target root: relative and made of plain components only,
/// so joining it to the root can never leave the root. A manifest is data a
/// repo can carry, so its paths are checked when parsed, not trusted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
struct ManifestPath(String);

impl TryFrom<String> for ManifestPath {
    type Error = String;

    fn try_from(path: String) -> Result<Self, String> {
        let plain = !path.is_empty()
            && Path::new(&path)
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
        if plain {
            Ok(Self(path))
        } else {
            Err(format!(
                "manifest path {path:?} is not a plain relative path"
            ))
        }
    }
}

impl ManifestPath {
    fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

/// A record of exactly what one `init` run placed at a target root — the
/// source of truth `--uninstall` acts on, independent of whatever the
/// currently running binary's embedded skills happen to be.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestEntry {
    path: ManifestPath,
    sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    /// For debugging, and later for flagging outdated skills; nothing reads it yet.
    gk_version: String,
    files: Vec<ManifestEntry>,
}

/// Vendor the embedded skills into every target requested by `args`, or (with
/// `--uninstall`) remove what a prior run of this placed there.
///
/// A refusal (no target flag) and a real I/O error both come back as `Err`;
/// a file conflict, or a kept file on uninstall, does not — either is a
/// normal, reportable outcome, not a failure to run. The caller picks the
/// exit code from `Report::is_failure`.
pub fn run(args: Args) -> Result<Report, String> {
    if !args.claude && !args.codex {
        return Err(
            "nothing to do — pass --claude and/or --codex (optionally with --experimental=<package>)"
                .to_string(),
        );
    }

    let mut roots: Vec<&str> = Vec::new();
    if args.claude {
        roots.push(CLAUDE_ROOT);
    }
    if args.codex {
        roots.push(CODEX_ROOT);
    }

    if args.uninstall {
        if !args.experimental.is_empty() {
            return Err(
                "--uninstall does not take --experimental — it removes whatever each \
                 target's manifest recorded, not whatever the running binary currently \
                 embeds"
                    .to_string(),
            );
        }
        return run_uninstall(&roots, args.force).map(Report::Uninstall);
    }

    let items = collect_items(&args.experimental)?;

    let mut totals = Totals::default();
    let mut targets = Vec::with_capacity(roots.len());

    for root in roots {
        let root_path = Path::new(root);
        let mut target_totals = Totals::default();
        let mut skills = Vec::with_capacity(items.len());
        let manifest_path = root_path.join(MANIFEST_FILENAME);
        refuse_symlink(root_path, Path::new(MANIFEST_FILENAME))?;
        let mut manifest_entries: BTreeMap<ManifestPath, String> =
            match read_manifest(&manifest_path)? {
                ManifestRead::Absent => BTreeMap::new(),
                ManifestRead::Found(manifest) => manifest
                    .files
                    .into_iter()
                    .map(|entry| (entry.path, entry.sha256))
                    .collect(),
                // Rebuilding over it would drop tracking of every file outside
                // this run's selection without a word.
                ManifestRead::Corrupt(reason) | ManifestRead::Rejected(reason) => {
                    return Err(format!(
                        "{} is not a valid manifest ({reason}) — delete it to start \
                     tracking afresh; files an earlier run placed will no longer \
                     be tracked",
                        manifest_path.display()
                    ))
                }
            };

        let placing = (|| -> Result<(), String> {
            for item in &items {
                let mut file_reports = Vec::with_capacity(item.files.len());
                for file in &item.files {
                    let target = root_path.join(&file.path);
                    refuse_symlink(root_path, &file.path)?;
                    let status = place(&target, &file.contents, args.force)?;
                    totals.record(status);
                    target_totals.record(status);

                    // A conflict means `place` left the existing file
                    // untouched, so this run placed nothing there: keep
                    // whatever an earlier run recorded (nothing, if the file
                    // was never gk's) rather than recording foreign content,
                    // or `--uninstall` would delete a file gk doesn't own.
                    let path = file.path.display().to_string();
                    if status != Status::Conflict {
                        manifest_entries
                            .insert(ManifestPath::try_from(path.clone())?, hash(&file.contents));
                    }

                    file_reports.push(FileReport { path, status });
                }
                skills.push(SkillReport {
                    name: item.name.clone(),
                    files: file_reports,
                });
            }
            Ok(())
        })();

        // Written even when placing failed partway: the files already on
        // disk are gk's, and an untracked file is one `--uninstall` can
        // never remove. The placing error is the one worth reporting.
        let entries = manifest_entries
            .into_iter()
            .map(|(path, sha256)| ManifestEntry { path, sha256 })
            .collect();
        let written = write_manifest(root_path, entries);
        placing?;
        written?;

        targets.push(TargetReport {
            target: root.to_string(),
            totals: target_totals,
            skills,
        });
    }

    Ok(Report::Install(InstallReport { totals, targets }))
}

/// Remove, from each target root, whatever that root's manifest says a prior
/// install placed there. Never consults the embedded `SKILLS`/`EXPERIMENTAL`
/// trees — the manifest is the only source of truth for what to remove.
fn run_uninstall(roots: &[&str], force: bool) -> Result<UninstallReport, String> {
    let mut totals = UninstallTotals::default();
    let mut targets = Vec::with_capacity(roots.len());

    for root in roots {
        let root_path = Path::new(root);
        let manifest_path = root_path.join(MANIFEST_FILENAME);
        refuse_symlink(root_path, Path::new(MANIFEST_FILENAME))?;

        // A missing or corrupt manifest means nothing to do, not an error —
        // running `--uninstall` twice (or on a target `init` never touched)
        // is not a failure, matching install's own idempotency. One that
        // parses but is invalid is an error: silence would read as success.
        let manifest = match read_manifest(&manifest_path)? {
            ManifestRead::Found(manifest) => manifest,
            ManifestRead::Rejected(reason) => {
                return Err(format!(
                    "{} is not a valid manifest ({reason}) — nothing was removed",
                    manifest_path.display()
                ))
            }
            ManifestRead::Absent | ManifestRead::Corrupt(_) => {
                targets.push(UninstallTargetReport {
                    target: root.to_string(),
                    totals: UninstallTotals::default(),
                    files: Vec::new(),
                });
                continue;
            }
        };

        let mut entries = manifest.files;
        entries.sort_by(|a, b| a.path.cmp(&b.path));

        // Checked up front so a refusal removes nothing at all.
        for entry in &entries {
            refuse_symlink(root_path, entry.path.as_path())?;
        }

        let mut target_totals = UninstallTotals::default();
        let mut file_reports = Vec::with_capacity(entries.len());
        let mut kept_entries = Vec::new();
        let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();

        for entry in entries {
            let target = root_path.join(entry.path.as_path());

            let status = match std::fs::read(&target) {
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => UninstallStatus::Missing,
                Err(err) => return Err(format!("could not read {}: {err}", target.display())),
                Ok(bytes) => {
                    if force || hash(&bytes) == entry.sha256 {
                        std::fs::remove_file(&target).map_err(|err| {
                            format!("could not remove {}: {err}", target.display())
                        })?;
                        UninstallStatus::Removed
                    } else {
                        UninstallStatus::Kept
                    }
                }
            };

            if let Some(parent) = entry.path.as_path().parent() {
                for ancestor in parent.ancestors() {
                    if ancestor.as_os_str().is_empty() {
                        continue;
                    }
                    dirs.insert(root_path.join(ancestor));
                }
            }

            totals.record(status);
            target_totals.record(status);
            file_reports.push(UninstallFileReport {
                path: entry.path.as_str().to_string(),
                status,
            });
            if status == UninstallStatus::Kept {
                kept_entries.push(entry);
            }
        }

        // Bottom-up: `remove_dir` refuses a non-empty directory on its own,
        // so it is safe to attempt every touched ancestor and ignore the
        // ones that still hold a kept file or something not gk's.
        let mut dirs: Vec<PathBuf> = dirs.into_iter().collect();
        dirs.sort_by_key(|dir| std::cmp::Reverse(dir.components().count()));
        for dir in dirs {
            let _ = std::fs::remove_dir(&dir);
        }

        if kept_entries.is_empty() {
            match std::fs::remove_file(&manifest_path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(format!(
                        "could not remove {}: {err}",
                        manifest_path.display()
                    ))
                }
            }
        } else {
            write_manifest(root_path, kept_entries)?;
        }

        targets.push(UninstallTargetReport {
            target: root.to_string(),
            totals: target_totals,
            files: file_reports,
        });
    }

    Ok(UninstallReport { totals, targets })
}

/// Every skill this run will place, content computed once regardless of how
/// many target roots it ends up written to.
fn collect_items(experimental: &[String]) -> Result<Vec<SkillItem>, String> {
    let mut items = Vec::new();

    let mut skill_dirs: Vec<&Dir> = SKILLS.dirs().collect();
    skill_dirs.sort_by_key(|dir| dir.path());

    for skill_dir in skill_dirs {
        let name = skill_name_of(skill_dir)?.to_string();
        let files = sorted_files(skill_dir)
            .into_iter()
            .map(|file| PlacedFile {
                path: file.path().to_path_buf(),
                contents: Cow::Borrowed(file.contents()),
            })
            .collect();

        items.push(SkillItem { name, files });
    }

    let mut packages = experimental.to_vec();
    packages.sort();
    packages.dedup();

    for package in packages {
        let skills_root = find_package_skills(&package)?;
        let mut skill_dirs: Vec<&Dir> = Vec::new();
        find_skill_dirs(skills_root, &mut skill_dirs);
        skill_dirs.sort_by_key(|dir| dir.path());

        for skill_dir in skill_dirs {
            let skill_name = skill_name_of(skill_dir)?;
            let new_name = format!("{package}-{skill_name}");

            let mut files = Vec::new();
            for file in sorted_files(skill_dir) {
                let rel = file
                    .path()
                    .strip_prefix(skill_dir.path())
                    .expect("file is under its own skill dir");

                let contents: Cow<'static, [u8]> = if rel == Path::new("SKILL.md") {
                    Cow::Owned(
                        rewrite_skill_name(file.contents(), skill_name, &new_name)
                            .map_err(|err| format!("{}: {err}", file.path().display()))?,
                    )
                } else {
                    Cow::Borrowed(file.contents())
                };

                files.push(PlacedFile {
                    path: Path::new(&new_name).join(rel),
                    contents,
                });
            }

            items.push(SkillItem {
                name: new_name,
                files,
            });
        }
    }

    Ok(items)
}

/// A skill directory's name, taken from its own path.
fn skill_name_of<'a>(dir: &'a Dir<'a>) -> Result<&'a str, String> {
    dir.path()
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("not a valid skill name: {}", dir.path().display()))
}

/// Every file under a skill directory, sorted for deterministic output.
fn sorted_files<'a>(dir: &'a Dir<'a>) -> Vec<&'a File<'a>> {
    let mut files = Vec::new();
    collect_files(dir, &mut files);
    files.sort_by_key(|file| file.path());
    files
}

/// The `skills/` dir inside `experimental/<package>`, or an error listing
/// every package that actually has one.
fn find_package_skills(package: &str) -> Result<&'static Dir<'static>, String> {
    EXPERIMENTAL
        .get_dir(package)
        .and_then(|dir| dir.get_dir(dir.path().join("skills")))
        .ok_or_else(|| {
            let mut available: Vec<&str> = EXPERIMENTAL
                .dirs()
                .filter(|dir| dir.get_dir(dir.path().join("skills")).is_some())
                .filter_map(|dir| dir.path().file_name().and_then(|n| n.to_str()))
                .collect();
            available.sort_unstable();
            let available = if available.is_empty() {
                "none vendored".to_string()
            } else {
                available.join(", ")
            };
            format!("unknown experimental package: {package} (available: {available})")
        })
}

/// Find every skill directory under `dir` — one containing `SKILL.md`
/// directly — without descending into a skill's own subdirectories, which
/// hold reference material, not more skills.
fn find_skill_dirs<'a>(dir: &'a Dir<'a>, out: &mut Vec<&'a Dir<'a>>) {
    if dir.get_file(dir.path().join("SKILL.md")).is_some() {
        out.push(dir);
        return;
    }
    for sub in dir.dirs() {
        find_skill_dirs(sub, out);
    }
}

/// Replace the frontmatter `name: <old_name>` line with `name: <new_name>`.
/// Errors rather than guessing when the line isn't there verbatim — an
/// upstream skill whose frontmatter doesn't match its directory name needs a
/// human to look, not a silent mismatch between the file and the path it's
/// installed at.
fn rewrite_skill_name(contents: &[u8], old_name: &str, new_name: &str) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(contents).map_err(|_| "not valid utf-8".to_string())?;
    let needle = format!("name: {old_name}");
    let replacement = format!("name: {new_name}");

    let mut found = false;
    let lines: Vec<&str> = text
        .lines()
        .map(|line| {
            if !found && line.trim() == needle {
                found = true;
                replacement.as_str()
            } else {
                line
            }
        })
        .collect();

    if !found {
        return Err(format!("frontmatter has no `{needle}` line to rewrite"));
    }

    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    Ok(out.into_bytes())
}

fn collect_files<'a>(dir: &'a Dir<'a>, out: &mut Vec<&'a File<'a>>) {
    out.extend(dir.files());
    for sub in dir.dirs() {
        collect_files(sub, out);
    }
}

/// Decide what to do with one file, given what (if anything) is already at
/// the target — no I/O, so every branch is cheap to test.
fn decide(existing: Option<&[u8]>, incoming: &[u8], force: bool) -> Status {
    match existing {
        None => Status::Installed,
        Some(bytes) if bytes == incoming => Status::Unchanged,
        Some(_) if force => Status::Overwritten,
        Some(_) => Status::Conflict,
    }
}

fn place(target: &Path, contents: &[u8], force: bool) -> Result<Status, String> {
    let existing = match std::fs::read(target) {
        Ok(bytes) => Some(bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(format!("could not read {}: {err}", target.display())),
    };

    let status = decide(existing.as_deref(), contents, force);
    if matches!(status, Status::Installed | Status::Overwritten) {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("could not create {}: {err}", parent.display()))?;
        }
        std::fs::write(target, contents)
            .map_err(|err| format!("could not write {}: {err}", target.display()))?;
    }
    Ok(status)
}

/// Refuse a path with a symlink anywhere under `root`, the file itself
/// included: a read, write, or removal through it would land wherever the
/// link points, outside the root. The root itself may be a symlink, since
/// people link `~/.claude/skills` into a dotfiles repo.
fn refuse_symlink(root: &Path, rel: &Path) -> Result<(), String> {
    let mut current = root.to_path_buf();
    for component in rel.components() {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "{} is a symlink, and gk will not read or write through one — \
                     remove the link (scripts/link.sh makes them) and rerun",
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

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// What sits at a target root's manifest path.
enum ManifestRead {
    Absent,
    /// Not JSON at all, such as a truncated write.
    Corrupt(String),
    /// JSON, but not a valid manifest: a path leaving the root, a missing
    /// field. Someone wrote it on purpose, or a newer gk did.
    Rejected(String),
    Found(Manifest),
}

/// Read the manifest at `path`. Only an I/O failure is an `Err`; a file that
/// does not parse is `Corrupt` or `Rejected`, so each caller decides what
/// that means.
fn read_manifest(path: &Path) -> Result<ManifestRead, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(match serde_json::from_slice(&bytes) {
            Ok(manifest) => ManifestRead::Found(manifest),
            Err(err) if err.classify() == serde_json::error::Category::Data => {
                ManifestRead::Rejected(err.to_string())
            }
            Err(err) => ManifestRead::Corrupt(err.to_string()),
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(ManifestRead::Absent),
        Err(err) => Err(format!("could not read {}: {err}", path.display())),
    }
}

/// Write the manifest for one target root, sorted by path — deterministic
/// ordering, per `docs/tool-contract.md`.
fn write_manifest(root: &Path, mut entries: Vec<ManifestEntry>) -> Result<(), String> {
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = Manifest {
        gk_version: env!("CARGO_PKG_VERSION").to_string(),
        files: entries,
    };
    let path = root.join(MANIFEST_FILENAME);
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("could not serialize manifest: {err}"))?;
    std::fs::write(&path, json).map_err(|err| format!("could not write {}: {err}", path.display()))
}

impl Human for Report {
    fn human(&self) -> String {
        match self {
            Report::Install(report) => report.human(),
            Report::Uninstall(report) => report.human(),
        }
    }
}

impl Human for InstallReport {
    fn human(&self) -> String {
        let mut out = String::new();

        for (i, target) in self.targets.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&format!(
                "{} · {} installed, {} unchanged, {} overwritten, {} conflict(s)\n",
                target.target,
                target.totals.installed,
                target.totals.unchanged,
                target.totals.overwritten,
                target.totals.conflicts,
            ));

            for skill in &target.skills {
                out.push_str(&format!("\n  {}\n", skill.name));
                for file in &skill.files {
                    out.push_str(&format!("    {:<11} {}\n", file.status.label(), file.path));
                }
            }
        }

        if self.totals.conflicts > 0 {
            out.push_str("\n  conflict: local changes would be overwritten — rerun with --force\n");
        }

        out.trim_end().to_string()
    }
}

impl Human for UninstallReport {
    fn human(&self) -> String {
        let mut out = String::new();

        for (i, target) in self.targets.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&format!(
                "{} · {} removed, {} kept, {} missing\n",
                target.target, target.totals.removed, target.totals.kept, target.totals.missing,
            ));

            for file in &target.files {
                out.push_str(&format!("  {:<8} {}\n", file.status.label(), file.path));
            }
        }

        if self.totals.kept > 0 {
            out.push_str("\n  kept: local changes would be lost — rerun with --force\n");
        }

        out.trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_paths_must_be_plain_and_relative() {
        for good in ["a", "gist-outline/SKILL.md"] {
            assert!(ManifestPath::try_from(good.to_string()).is_ok(), "{good}");
        }
        for bad in ["", "/", "/etc/passwd", "..", "../x", "a/../../x", "./a"] {
            assert!(ManifestPath::try_from(bad.to_string()).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_missing_target_is_installed() {
        assert_eq!(decide(None, b"new", false), Status::Installed);
    }

    #[test]
    fn identical_content_is_unchanged() {
        assert_eq!(decide(Some(b"same"), b"same", false), Status::Unchanged);
    }

    #[test]
    fn different_content_without_force_is_a_conflict() {
        assert_eq!(decide(Some(b"old"), b"new", false), Status::Conflict);
    }

    #[test]
    fn different_content_with_force_is_overwritten() {
        assert_eq!(decide(Some(b"old"), b"new", true), Status::Overwritten);
    }

    #[test]
    fn the_embedded_skills_are_present_and_named_with_the_gist_prefix() {
        let names: Vec<String> = SKILLS
            .dirs()
            .map(|dir| dir.path().display().to_string())
            .collect();
        assert!(names.contains(&"gist-outline".to_string()));
        assert!(names.contains(&"gist-doc-review".to_string()));
    }

    #[test]
    fn every_embedded_skill_declares_a_frontmatter_name_matching_its_directory() {
        for dir in SKILLS.dirs() {
            let path = dir.path().join("SKILL.md");
            let skill_md = SKILLS.get_file(&path).expect("SKILL.md present");
            let text = std::str::from_utf8(skill_md.contents()).expect("utf8");
            let expected = format!("name: {}", dir.path().display());
            assert!(
                text.lines().any(|line| line.trim() == expected),
                "expected `{expected}` in {}",
                path.display()
            );
        }
    }

    fn decide_uninstall(
        disk: Option<&[u8]>,
        recorded_sha256: &str,
        force: bool,
    ) -> UninstallStatus {
        match disk {
            None => UninstallStatus::Missing,
            Some(bytes) => {
                if force || hash(bytes) == recorded_sha256 {
                    UninstallStatus::Removed
                } else {
                    UninstallStatus::Kept
                }
            }
        }
    }

    #[test]
    fn a_file_matching_its_recorded_hash_is_removed() {
        let recorded = hash(b"content");
        assert_eq!(
            decide_uninstall(Some(b"content"), &recorded, false),
            UninstallStatus::Removed
        );
    }

    #[test]
    fn a_file_whose_hash_differs_is_kept_without_force() {
        let recorded = hash(b"original");
        assert_eq!(
            decide_uninstall(Some(b"edited"), &recorded, false),
            UninstallStatus::Kept
        );
    }

    #[test]
    fn a_file_whose_hash_differs_is_removed_with_force() {
        let recorded = hash(b"original");
        assert_eq!(
            decide_uninstall(Some(b"edited"), &recorded, true),
            UninstallStatus::Removed
        );
    }

    #[test]
    fn a_manifest_entry_with_no_file_on_disk_is_missing() {
        let recorded = hash(b"content");
        assert_eq!(
            decide_uninstall(None, &recorded, false),
            UninstallStatus::Missing
        );
    }
}
