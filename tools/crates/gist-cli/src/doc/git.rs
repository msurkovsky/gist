//! Getting added lines out of git.
//!
//! The diff is read with `-U0`, so it carries no context — which means block
//! comment state cannot be recovered from the diff alone. Wherever the
//! post-image is readable we classify the whole file and select the added
//! lines from it; the diff text is only a fallback.

use std::path::{Path, PathBuf};
use std::process::Command;

/// What to compare.
#[derive(Clone, Debug)]
pub enum Source {
    /// Everything not yet committed.
    Worktree,
    /// The index only.
    Staged,
    /// Any range git understands, e.g. `main..HEAD`.
    Range(String),
    /// Merge base with a branch, i.e. "what this branch added".
    Base(String),
}

impl Source {
    /// How this source is named in output.
    pub fn label(&self) -> String {
        match self {
            Source::Worktree => "worktree".to_string(),
            Source::Staged => "staged".to_string(),
            Source::Range(spec) => spec.clone(),
            Source::Base(rev) => format!("merge-base with {rev}"),
        }
    }

    fn diff_args(&self) -> Vec<String> {
        let mut args: Vec<String> = ["diff", "--unified=0", "--no-color", "--no-ext-diff"]
            .iter()
            .map(|arg| arg.to_string())
            .collect();
        match self {
            Source::Worktree => args.push("HEAD".to_string()),
            Source::Staged => args.push("--cached".to_string()),
            Source::Range(spec) => args.push(spec.clone()),
            Source::Base(rev) => {
                args.push("--merge-base".to_string());
                args.push(rev.clone());
            }
        }
        args
    }

    /// Where to read the post-image of a file from.
    fn post_image_rev(&self) -> Option<String> {
        match self {
            // These all compare against the working tree.
            Source::Worktree | Source::Base(_) => None,
            Source::Staged => Some(String::new()),
            Source::Range(spec) => spec
                .split_once("...")
                .or_else(|| spec.split_once(".."))
                .map(|(_, right)| right.to_string())
                .filter(|right| !right.is_empty()),
        }
    }
}

/// One file's added lines, as (1-based line number in the post-image, text).
#[derive(Debug)]
pub struct FileDiff {
    pub path: String,
    pub added: Vec<(usize, String)>,
}

/// Absolute path of the enclosing checkout.
pub fn repo_root() -> Result<PathBuf, String> {
    let output = run(&["rev-parse".to_string(), "--show-toplevel".to_string()])?;
    Ok(PathBuf::from(output.trim()))
}

/// Added lines per file. Untracked files count in full for a worktree source.
pub fn diff(source: &Source) -> Result<Vec<FileDiff>, String> {
    let mut files = parse_diff(&run(&source.diff_args())?);

    // `git diff` never mentions untracked files, but a new file is the most
    // documentation-heavy thing a change can contain. Every line of one is an
    // added line.
    if matches!(source, Source::Worktree) {
        let root = repo_root()?;
        for path in untracked()? {
            let Ok(content) = std::fs::read_to_string(root.join(&path)) else {
                continue;
            };
            let added = content
                .lines()
                .enumerate()
                .map(|(index, text)| (index + 1, text.to_string()))
                .collect();
            files.push(FileDiff { path, added });
        }
    }

    Ok(files)
}

fn untracked() -> Result<Vec<String>, String> {
    let output = run(&[
        "ls-files".to_string(),
        "--others".to_string(),
        "--exclude-standard".to_string(),
    ])?;
    Ok(output.lines().map(|line| line.to_string()).collect())
}

/// Full content of a file as it looks after the change, or `None` when it
/// cannot be read — deleted, binary, or outside the checkout.
pub fn post_image(source: &Source, root: &Path, path: &str) -> Option<String> {
    match source.post_image_rev() {
        None => std::fs::read_to_string(root.join(path)).ok(),
        Some(rev) => run(&["show".to_string(), format!("{rev}:{path}")]).ok(),
    }
}

fn run(args: &[String]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|err| format!("could not run git: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {}: {}", args.join(" "), stderr.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_diff(text: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut line_number = 0usize;

    for line in text.lines() {
        if let Some(target) = line.strip_prefix("+++ ") {
            match parse_target(target) {
                // `+++ /dev/null` means the file was deleted; nothing was added.
                Some(path) => files.push(FileDiff {
                    path,
                    added: Vec::new(),
                }),
                None => continue,
            }
            continue;
        }

        if line.starts_with("@@") {
            if let Some(start) = parse_hunk_start(line) {
                line_number = start;
            }
            continue;
        }

        if let Some(added) = line.strip_prefix('+') {
            if line.starts_with("+++") {
                continue;
            }
            if let Some(file) = files.last_mut() {
                file.added.push((line_number, added.to_string()));
                line_number += 1;
            }
        }
    }

    files
}

fn parse_target(target: &str) -> Option<String> {
    if target == "/dev/null" {
        return None;
    }
    let path = target.strip_prefix("b/").unwrap_or(target);
    // git quotes paths containing unusual bytes.
    let path = path
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(path);
    Some(path.replace("\\\"", "\"").replace("\\\\", "\\"))
}

/// `@@ -12,0 +13,4 @@` → 13.
fn parse_hunk_start(line: &str) -> Option<usize> {
    let plus = line.split('+').nth(1)?;
    let digits: String = plus.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "diff --git a/src/a.ts b/src/a.ts\n\
index 1111111..2222222 100644\n\
--- a/src/a.ts\n\
+++ b/src/a.ts\n\
@@ -3,0 +4,2 @@ function f() {\n\
+// one\n\
+// two\n\
@@ -20,0 +23 @@\n\
+const x = 1;\n\
diff --git a/src/gone.ts b/src/gone.ts\n\
--- a/src/gone.ts\n\
+++ /dev/null\n";

    #[test]
    fn added_lines_carry_their_post_image_line_numbers() {
        let files = parse_diff(SAMPLE);
        assert_eq!(files.len(), 1, "deleted files contribute nothing");
        assert_eq!(files[0].path, "src/a.ts");
        assert_eq!(
            files[0].added,
            vec![
                (4, "// one".to_string()),
                (5, "// two".to_string()),
                (23, "const x = 1;".to_string()),
            ]
        );
    }

    #[test]
    fn hunk_headers_without_a_count_mean_one_line() {
        assert_eq!(parse_hunk_start("@@ -20,0 +23 @@"), Some(23));
        assert_eq!(parse_hunk_start("@@ -1,2 +1,5 @@ fn main()"), Some(1));
    }

    #[test]
    fn ranges_resolve_their_right_hand_side_for_the_post_image() {
        assert_eq!(
            Source::Range("main..feature".to_string()).post_image_rev(),
            Some("feature".to_string())
        );
        assert_eq!(Source::Range("main".to_string()).post_image_rev(), None);
        assert_eq!(Source::Worktree.post_image_rev(), None);
        assert_eq!(Source::Staged.post_image_rev(), Some(String::new()));
    }
}
