//! `gk init --claude` — vendor this project's skills into a target repo's
//! `.claude/skills/`, so Claude Code's own Skill tool can find them.
//! `--experimental=<package>` does the same for one vendored tree under
//! `experimental/`, prefixing each skill's name with the package so it
//! cannot silently shadow — or be shadowed by — a canonical `gist-` skill.
//!
//! `gk` ships as a single binary with no source tree alongside it, so the
//! skills this repo produces are embedded into the binary at build time —
//! see docs/adr/0002-embed-skills-for-init.md.

use clap::Args as ClapArgs;
use gist_core::Human;
use include_dir::{include_dir, Dir, File};
use serde::Serialize;
use std::borrow::Cow;
use std::path::Path;

static SKILLS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../../skills");
static EXPERIMENTAL: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../../experimental");

/// Command line for `gk init`.
#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Vendor this project's skills into ./.claude/skills/ for Claude Code
    #[arg(long)]
    claude: bool,

    /// Vendor one experimental/<package> tree's skills too, name-prefixed
    #[arg(long, num_args = 1.., value_name = "PACKAGE")]
    experimental: Vec<String>,

    /// Overwrite files with local changes instead of reporting a conflict
    #[arg(long)]
    force: bool,
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
    /// Path relative to `.claude/skills/`, e.g. `gist-outline/SKILL.md`.
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

/// Everything `gk init --claude` did, one entry per embedded file.
#[derive(Debug, Serialize)]
pub struct Report {
    target: String,
    totals: Totals,
    skills: Vec<SkillReport>,
}

impl Report {
    pub(crate) fn has_conflicts(&self) -> bool {
        self.totals.conflicts > 0
    }
}

/// Vendor the embedded skills into the target requested by `args`.
///
/// A refusal (no target flag) and a real I/O error both come back as `Err`;
/// a file conflict does not — it is a normal, reportable outcome, not a
/// failure to run. The caller picks the exit code from `Report::has_conflicts`.
pub fn run(args: Args) -> Result<Report, String> {
    if !args.claude && args.experimental.is_empty() {
        return Err("nothing to do — pass --claude or --experimental=<package>".to_string());
    }

    let root = Path::new(".claude/skills");
    let mut totals = Totals::default();
    let mut skills = Vec::new();

    if args.claude {
        let mut skill_dirs: Vec<&Dir> = SKILLS.dirs().collect();
        skill_dirs.sort_by_key(|dir| dir.path());

        for skill_dir in skill_dirs {
            let name = skill_dir
                .path()
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("not a valid skill name: {}", skill_dir.path().display()))?
                .to_string();

            let mut files: Vec<&File> = Vec::new();
            collect_files(skill_dir, &mut files);
            files.sort_by_key(|file| file.path());

            let mut file_reports = Vec::with_capacity(files.len());
            for file in files {
                let target = root.join(file.path());
                let status = place(&target, file.contents(), args.force)?;
                totals.record(status);
                file_reports.push(FileReport {
                    path: file.path().display().to_string(),
                    status,
                });
            }

            skills.push(SkillReport {
                name,
                files: file_reports,
            });
        }
    }

    let mut packages = args.experimental.clone();
    packages.sort();
    packages.dedup();

    for package in packages {
        let skills_root = find_package_skills(&package)?;
        let mut skill_dirs: Vec<&Dir> = Vec::new();
        find_skill_dirs(skills_root, &mut skill_dirs);
        skill_dirs.sort_by_key(|dir| dir.path());

        for skill_dir in skill_dirs {
            let skill_name = skill_dir
                .path()
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("not a valid skill name: {}", skill_dir.path().display()))?;
            let new_name = format!("{package}-{skill_name}");

            let mut files: Vec<&File> = Vec::new();
            collect_files(skill_dir, &mut files);
            files.sort_by_key(|file| file.path());

            let mut file_reports = Vec::with_capacity(files.len());
            for file in files {
                let rel = file
                    .path()
                    .strip_prefix(skill_dir.path())
                    .expect("file is under its own skill dir");

                let contents: Cow<[u8]> = if rel == Path::new("SKILL.md") {
                    Cow::Owned(
                        rewrite_skill_name(file.contents(), skill_name, &new_name)
                            .map_err(|err| format!("{}: {err}", file.path().display()))?,
                    )
                } else {
                    Cow::Borrowed(file.contents())
                };

                let target = root.join(&new_name).join(rel);
                let status = place(&target, &contents, args.force)?;
                totals.record(status);
                file_reports.push(FileReport {
                    path: format!("{new_name}/{}", rel.display()),
                    status,
                });
            }

            skills.push(SkillReport {
                name: new_name,
                files: file_reports,
            });
        }
    }

    Ok(Report {
        target: root.display().to_string(),
        totals,
        skills,
    })
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

impl Human for Report {
    fn human(&self) -> String {
        let mut out = format!(
            "{} · {} installed, {} unchanged, {} overwritten, {} conflict(s)\n",
            self.target,
            self.totals.installed,
            self.totals.unchanged,
            self.totals.overwritten,
            self.totals.conflicts,
        );

        for skill in &self.skills {
            out.push_str(&format!("\n  {}\n", skill.name));
            for file in &skill.files {
                out.push_str(&format!("    {:<11} {}\n", file.status.label(), file.path));
            }
        }

        if self.totals.conflicts > 0 {
            out.push_str("\n  conflict: local changes would be overwritten — rerun with --force\n");
        }

        out.trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
