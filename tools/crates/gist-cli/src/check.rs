//! Repository checks: docs/adr/0012-validate-repository-content.md and
//! docs/cases/repository-checks.md. Inspect source and local Git history only.

use clap::Args as ClapArgs;
use gist_core::Human;
use git2::{Repository, StatusOptions};
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Command line for read-only repository validation.
#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Gist source checkout to check
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Check only registered vendor content against its import history
    #[arg(long)]
    vendors_only: bool,
    /// Maximum failure details to report (the exit code includes all failures)
    #[arg(long, default_value_t = 20)]
    limit: usize,
}

/// Mechanical check result; a complete report may still describe failed checks.
#[derive(Debug, Serialize)]
pub struct Report {
    failures: usize,
    errors: Vec<String>,
    truncated: bool,
    vendors_only: bool,
}

impl Report {
    pub(crate) fn is_failure(&self) -> bool {
        self.failures != 0
    }
}

impl Human for Report {
    fn human(&self) -> String {
        if self.failures == 0 {
            return if self.vendors_only {
                "clean: no local edits under experimental/"
            } else {
                "all checks passed"
            }
            .into();
        }
        let mut out = format!("{} repository check(s) failed", self.failures);
        for error in &self.errors {
            out.push_str(&format!("\nFAIL  {error}"));
        }
        if self.truncated {
            out.push_str("\n… more failures hidden by --limit");
        }
        out
    }
}

/// Check metadata, resources, repository conventions, and vendor content.
pub fn run(args: Args) -> Result<Report, String> {
    let root = args
        .path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", args.path.display()))?;
    let mut errors = Vec::new();
    if !args.vendors_only {
        check_owned(&root, &mut errors)?;
    }
    check_vendors(&root, &mut errors)?;
    errors.sort();
    errors.dedup();
    let failures = errors.len();
    errors.truncate(args.limit);
    Ok(Report {
        failures,
        truncated: failures > args.limit,
        errors,
        vendors_only: args.vendors_only,
    })
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn entries(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = std::fs::read_dir(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .map(|entry| entry.map(|e| e.path()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    Ok(paths)
}

fn check_owned(root: &Path, errors: &mut Vec<String>) -> Result<(), String> {
    for dir in ["scripts", "hooks"] {
        for path in entries(&root.join(dir))? {
            if path.extension().is_some_and(|ext| ext == "sh") {
                let output = std::process::Command::new("bash")
                    .arg("-n")
                    .arg(&path)
                    .output()
                    .map_err(|e| format!("could not check shell syntax: {e}"))?;
                if !output.status.success() {
                    errors.push(format!(
                        "{}: shell syntax error: {}",
                        path.display(),
                        String::from_utf8_lossy(&output.stderr).trim()
                    ));
                }
            }
        }
    }
    let readme = read(&root.join("README.md"))?;
    let dirs = entries(&root.join("skills"))?;
    if dirs.is_empty() {
        errors.push("skills/: no owned skills".into());
    }
    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        let name = dir.file_name().unwrap().to_string_lossy();
        if !readme.contains(&format!("`{name}`")) {
            errors.push(format!("README.md does not list {name}"));
        }
        if !root.join("docs/cases").join(format!("{name}.md")).is_file() {
            errors.push(format!("{name}: missing docs/cases/{name}.md"));
        }
        if let Err(e) = check_skill(&dir, &name) {
            errors.push(format!("{}: {e}", dir.display()));
        }
    }
    let row = Regex::new(r"(?m)^\| [0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2} \|").unwrap();
    for path in entries(&root.join("docs/adr"))? {
        if !path.file_name().unwrap().to_string_lossy().starts_with('0')
            || path.extension().is_none_or(|e| e != "md")
        {
            continue;
        }
        let text = read(&path)?;
        let last_section = text.rsplit_once("\n## ").map(|(_, s)| s);
        if !last_section.is_some_and(|s| s.starts_with("Changelog\n") && row.is_match(s)) {
            errors.push(format!(
                "{}: needs a final Changelog with a dated row",
                path.display()
            ));
        }
    }
    Ok(())
}

fn check_skill(dir: &Path, name: &str) -> Result<(), String> {
    let text = read(&dir.join("SKILL.md"))?;
    let metadata = crate::skill::parse(&text)?;
    let valid_name = Regex::new(r"^gist-[a-z0-9]+(?:-[a-z0-9]+)*$").unwrap();
    if !valid_name.is_match(name) || name.len() > 64 || metadata.fields["name"] != name {
        return Err("name must match its gist-prefixed directory (lowercase letters, digits, hyphens; at most 64 characters)".into());
    }
    let explicit = metadata.fields["disable-model-invocation"]
        .as_bool()
        .unwrap_or(false);
    let sidecar = dir.join("agents/openai.yaml");
    let implicit = if sidecar.exists() {
        let fields: serde_json::Value = serde_saphyr::from_str(&read(&sidecar)?)
            .map_err(|e| format!("invalid agents/openai.yaml: {e}"))?;
        if !fields.is_object() || fields.get("policy").is_some_and(|p| !p.is_object()) {
            return Err("agents/openai.yaml and its policy must be mappings".into());
        }
        match fields["policy"].get("allow_implicit_invocation") {
            Some(v) => v
                .as_bool()
                .ok_or("allow_implicit_invocation must be a boolean")?,
            None => true,
        }
    } else {
        true
    };
    if explicit == implicit {
        return Err("Claude and Codex invocation policies disagree".into());
    }
    check_resources(dir, dir)
}

fn check_resources(root: &Path, dir: &Path) -> Result<(), String> {
    let link =
        Regex::new(r#"!?\[[^\]\n]*\]\((<[^>]+>|[^\s)]+)(?:[ \t]+["'][^)]*["'])?\)"#).unwrap();
    for path in entries(dir)? {
        let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "{}: skill resources must not be symlinks",
                path.display()
            ));
        }
        if path.is_dir() {
            check_resources(root, &path)?;
            continue;
        }
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let text = read(&path)?;
        let mut fence: Option<char> = None;
        let mut prose = String::new();
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let marker = trimmed.chars().next().unwrap();
                if fence == Some(marker) {
                    fence = None;
                } else if fence.is_none() {
                    fence = Some(marker);
                }
                prose.push_str("\n\n");
                continue;
            }
            if fence.is_some() || line.starts_with("    ") {
                prose.push('\n');
                continue;
            }
            prose.push_str(line);
            prose.push('\n');
        }
        for caps in link.captures_iter(&mask_inline_code(&prose)) {
            let target = caps[1].trim_start_matches('<').trim_end_matches('>');
            if target.contains(':') || target.starts_with('#') {
                continue;
            }
            let target = decode_link_path(target.split('#').next().unwrap())?;
            let resolved = path
                .parent()
                .unwrap()
                .join(&target)
                .canonicalize()
                .map_err(|_| format!("{}: missing resource {target}", path.display()))?;
            if !resolved.starts_with(root) {
                return Err(format!(
                    "{}: resource escapes skill: {target}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

fn mask_inline_code(text: &str) -> String {
    let ticks = Regex::new(r"`+").unwrap();
    let paragraphs = Regex::new(r"\n[ \t]*\n").unwrap();
    let runs: Vec<_> = ticks.find_iter(text).collect();
    let mut output = text.as_bytes().to_vec();
    let mut index = 0;
    while index < runs.len() {
        let open = runs[index];
        let escaped = text[..open.start()]
            .bytes()
            .rev()
            .take_while(|b| *b == b'\\')
            .count()
            % 2
            != 0;
        if escaped {
            index += 1;
            continue;
        }
        let close = (index + 1..runs.len())
            .take_while(|&next| !paragraphs.is_match(&text[open.end()..runs[next].start()]))
            .find(|&next| runs[next].len() == open.len());
        if let Some(close) = close {
            for byte in &mut output[open.start()..runs[close].end()] {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
            index = close + 1;
        } else {
            index += 1;
        }
    }
    String::from_utf8(output).expect("code spans replaced at UTF-8 boundaries")
}

fn decode_link_path(path: &str) -> Result<String, String> {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (
                (bytes[index + 1] as char).to_digit(16),
                (bytes[index + 2] as char).to_digit(16),
            ) {
                decoded.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(decoded)
        .map_err(|_| format!("resource path is not UTF-8 after percent decoding: {path}"))
}

fn check_vendors(root: &Path, errors: &mut Vec<String>) -> Result<(), String> {
    let repo =
        Repository::open(root).map_err(|e| format!("cannot read repository history: {e}"))?;
    let head = repo
        .head()
        .and_then(|h| h.peel_to_commit())
        .map_err(|e| format!("cannot read HEAD: {e}"))?;
    let head_tree = head.tree().map_err(|e| e.to_string())?;
    let mut vendors = BTreeMap::new();
    for line in read(&root.join("scripts/vendors.conf"))?.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        if !(2..=3).contains(&fields.len())
            || fields[0] == "."
            || fields[0] == ".."
            || !fields[0]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        {
            return Err(format!("invalid vendor registry entry: {line}"));
        }
        if vendors
            .insert(fields[0].to_string(), fields[1].to_string())
            .is_some()
        {
            return Err(format!("duplicate vendor: {}", fields[0]));
        }
    }
    let known: BTreeSet<_> = vendors.keys().cloned().collect();
    if let Ok(entry) = head_tree.get_path(Path::new("experimental")) {
        let tree = repo
            .find_tree(entry.id())
            .map_err(|e| format!("invalid experimental tree: {e}"))?;
        for entry in &tree {
            let name = entry
                .name()
                .map_err(|e| format!("invalid vendor name: {e}"))?;
            if !known.contains(name) {
                errors.push(format!("experimental/{name}: unregistered vendor content"));
            }
        }
    }
    let mut walk = repo.revwalk().map_err(|e| e.to_string())?;
    walk.push_head().map_err(|e| e.to_string())?;
    walk.set_sorting(git2::Sort::TOPOLOGICAL)
        .map_err(|e| e.to_string())?;
    let history = walk
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for (name, url) in vendors {
        let prefix = format!("experimental/{name}");
        let filter = format!("Filter: :prefix={prefix}");
        let upstream = format!("Upstream: {url}@");
        let mut latest = None;
        for oid in &history {
            let commit = repo.find_commit(*oid).map_err(|e| e.to_string())?;
            let message = commit.message().unwrap_or("");
            if !message.lines().any(|l| l == filter) {
                continue;
            }
            if commit.parent_count() != 2
                || !message.lines().any(|l| {
                    l.strip_prefix(&upstream).is_some_and(|sha| {
                        (sha.len() == 40 || sha.len() == 64)
                            && sha.bytes().all(|b| b.is_ascii_hexdigit())
                    })
                })
            {
                return Err(format!("{prefix}: invalid import record {oid}"));
            }
            if let Some(newer) = latest {
                if !repo
                    .graph_descendant_of(newer, *oid)
                    .map_err(|e| e.to_string())?
                {
                    return Err(format!(
                        "{prefix}: ambiguous import history; reconcile competing imports"
                    ));
                }
            } else {
                latest = Some(*oid);
            }
        }
        let oid = latest.ok_or_else(|| {
            format!("{prefix}: no reachable import record; full history is required")
        })?;
        let imported = repo
            .find_commit(oid)
            .and_then(|c| c.parent(1))
            .and_then(|c| c.tree())
            .map_err(|e| e.to_string())?;
        let expected = imported
            .get_path(Path::new(&prefix))
            .map_err(|e| format!("{prefix}: invalid imported subtree: {e}"))?;
        let matches = head_tree.get_path(Path::new(&prefix)).is_ok_and(|entry| {
            entry.id() == expected.id() && entry.filemode() == expected.filemode()
        });
        if !matches {
            errors.push(format!(
                "{prefix}: HEAD differs from imported content at {oid}"
            ));
        }
    }
    let mut options = StatusOptions::new();
    options
        .pathspec("experimental/")
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(|e| format!("cannot inspect vendor worktree: {e}"))?;
    for entry in statuses.iter() {
        errors.push(format!(
            "{}: staged, unstaged, or untracked vendor change",
            entry.path().unwrap_or("experimental/<non-UTF8 path>")
        ));
    }
    Ok(())
}
