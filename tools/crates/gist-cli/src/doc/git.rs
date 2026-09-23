//! Getting added lines out of git.
//!
//! libgit2 does the diffing, asked for zero context lines — which means block
//! comment state cannot be recovered from the diff alone. Wherever the
//! post-image is readable we classify the whole file and select the added
//! lines from it; the diff text is only a fallback.
//!
//! Renames count as renames, so a moved file contributes only the lines that
//! changed in it. libgit2 scores similarity its own way and will pair a file
//! or two that `git diff` leaves alone; on a 600-commit range that moved the
//! ratio by 0.0001.

use std::path::{Path, PathBuf};

use git2::{Blob, DiffFindOptions, DiffOptions, ErrorCode, Oid, Repository, RevparseMode, Tree};

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
}

/// One file's added lines, as (1-based line number in the post-image, text).
#[derive(Debug)]
pub struct FileDiff {
    pub path: String,
    pub added: Vec<(usize, String)>,
}

/// An open checkout.
pub struct Repo {
    inner: Repository,
}

impl Repo {
    /// Open the checkout containing the current directory.
    pub fn discover() -> Result<Repo, String> {
        Repo::discover_from(Path::new("."))
    }

    fn discover_from(path: &Path) -> Result<Repo, String> {
        let inner = Repository::discover(path)
            .map_err(|err| format!("not a git repository: {}", err.message()))?;
        if inner.workdir().is_none() {
            return Err("bare repository: nothing to measure".to_string());
        }
        Ok(Repo { inner })
    }

    /// Absolute path of the checkout.
    pub fn root(&self) -> PathBuf {
        self.inner
            .workdir()
            .expect("checked at open time")
            .to_path_buf()
    }

    /// Added lines per file. Untracked files count in full for a worktree source.
    pub fn diff(&self, source: &Source) -> Result<Vec<FileDiff>, String> {
        let mut opts = DiffOptions::new();
        opts.context_lines(0)
            // git has had this on by default since 2.14. Without it a hunk can
            // be anchored a line off, which moves comment lines in and out of
            // the change.
            .indent_heuristic(true)
            // `git diff` never mentions untracked files, but a new file is the
            // most documentation-heavy thing a change can contain.
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .show_untracked_content(true);

        let mut diff = match source {
            Source::Worktree => {
                let head = self.head_tree()?;
                self.inner
                    .diff_tree_to_workdir_with_index(head.as_ref(), Some(&mut opts))
            }
            Source::Staged => {
                let head = self.head_tree()?;
                self.inner
                    .diff_tree_to_index(head.as_ref(), None, Some(&mut opts))
            }
            Source::Base(rev) => {
                let base = self.merge_base_tree(rev)?;
                self.inner
                    .diff_tree_to_workdir_with_index(Some(&base), Some(&mut opts))
            }
            Source::Range(spec) => match self.range_trees(spec)? {
                (left, Some(right)) => {
                    self.inner
                        .diff_tree_to_tree(Some(&left), Some(&right), Some(&mut opts))
                }
                // A bare revision compares that revision against the worktree.
                (left, None) => self
                    .inner
                    .diff_tree_to_workdir_with_index(Some(&left), Some(&mut opts)),
            },
        }
        .map_err(|err| format!("could not diff: {}", err.message()))?;

        // git detects renames for `git diff` by default; matching it keeps a
        // moved file from counting as wholly added. libgit2 would pair files
        // git leaves alone, so the threshold is pinned to git's 50%.
        let mut renames = DiffFindOptions::new();
        renames
            .renames(true)
            .rename_threshold(50)
            // We count untracked files, so they have to be rename targets too.
            .for_untracked(true);
        diff.find_similar(Some(&mut renames))
            .map_err(|err| format!("could not detect renames: {}", err.message()))?;

        let mut files: Vec<FileDiff> = Vec::new();
        diff.foreach(
            &mut |_, _| true,
            None,
            None,
            Some(&mut |delta, _, line| {
                if line.origin() != '+' {
                    return true;
                }
                let (Some(number), Some(path), Ok(text)) = (
                    line.new_lineno(),
                    delta.new_file().path().and_then(Path::to_str),
                    std::str::from_utf8(line.content()),
                ) else {
                    return true;
                };
                if files.last().map(|file| file.path.as_str()) != Some(path) {
                    files.push(FileDiff {
                        path: path.to_string(),
                        added: Vec::new(),
                    });
                }
                let text = text.trim_end_matches('\n').trim_end_matches('\r');
                if let Some(file) = files.last_mut() {
                    file.added.push((number as usize, text.to_string()));
                }
                true
            }),
        )
        .map_err(|err| format!("could not read the diff: {}", err.message()))?;

        Ok(files)
    }

    /// Full content of a file as it looks after the change, or `None` when it
    /// cannot be read — deleted, binary, or outside the checkout.
    pub fn post_image(&self, source: &Source, path: &str) -> Option<String> {
        let blob = match source {
            Source::Worktree | Source::Base(_) => return self.read_worktree(path),
            Source::Staged => {
                let entry = self.inner.index().ok()?.get_path(Path::new(path), 0)?;
                self.inner.find_blob(entry.id).ok()?
            }
            Source::Range(spec) => match self.range_trees(spec).ok()? {
                (_, Some(right)) => self.blob_in(&right, path)?,
                (_, None) => return self.read_worktree(path),
            },
        };
        decode(&blob)
    }

    fn read_worktree(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.root().join(path)).ok()
    }

    fn blob_in<'repo>(&'repo self, tree: &Tree<'repo>, path: &str) -> Option<Blob<'repo>> {
        tree.get_path(Path::new(path))
            .ok()?
            .to_object(&self.inner)
            .ok()?
            .peel_to_blob()
            .ok()
    }

    /// `None` when HEAD is unborn — a repository with no commits, where every
    /// line of every file is new.
    fn head_tree(&self) -> Result<Option<Tree<'_>>, String> {
        match self.inner.head() {
            Ok(head) => head
                .peel_to_tree()
                .map(Some)
                .map_err(|err| format!("could not read HEAD: {}", err.message())),
            Err(err) if err.code() == ErrorCode::UnbornBranch => Ok(None),
            Err(err) => Err(format!("could not read HEAD: {}", err.message())),
        }
    }

    fn merge_base_tree(&self, rev: &str) -> Result<Tree<'_>, String> {
        let head = self
            .inner
            .head()
            .and_then(|head| head.peel_to_commit())
            .map_err(|err| format!("could not read HEAD: {}", err.message()))?;
        let other = self
            .inner
            .revparse_single(rev)
            .and_then(|object| object.peel_to_commit())
            .map_err(|err| format!("bad revision {rev}: {}", err.message()))?;
        self.tree_at_merge_base(head.id(), other.id())
    }

    fn tree_at_merge_base(&self, left: Oid, right: Oid) -> Result<Tree<'_>, String> {
        let base = self
            .inner
            .merge_base(left, right)
            .map_err(|err| format!("no merge base: {}", err.message()))?;
        self.inner
            .find_commit(base)
            .and_then(|commit| commit.tree())
            .map_err(|err| format!("could not read the merge base: {}", err.message()))
    }

    /// Both sides of a revision spec. The right side is `None` for a bare
    /// revision, which means "compare against the worktree".
    fn range_trees(&self, spec: &str) -> Result<(Tree<'_>, Option<Tree<'_>>), String> {
        let bad = |err: git2::Error| format!("bad revision {spec}: {}", err.message());
        let parsed = self.inner.revparse(spec).map_err(bad)?;
        let from = parsed
            .from()
            .ok_or_else(|| format!("bad revision {spec}"))?;
        let Some(to) = parsed.to() else {
            return Ok((from.peel_to_tree().map_err(bad)?, None));
        };

        // `a...b` asks what b added, so the left side is the two commits' merge
        // base rather than a itself.
        let left = if parsed.mode().contains(RevparseMode::MERGE_BASE) {
            let ends = (
                from.peel_to_commit().map_err(bad)?,
                to.peel_to_commit().map_err(bad)?,
            );
            self.tree_at_merge_base(ends.0.id(), ends.1.id())?
        } else {
            from.peel_to_tree().map_err(bad)?
        };
        Ok((left, Some(to.peel_to_tree().map_err(bad)?)))
    }
}

fn decode(blob: &Blob<'_>) -> Option<String> {
    String::from_utf8(blob.content().to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// A repository with one commit adding `src/a.ts`.
    fn fixture() -> (TempDir, Repo) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/a.ts"), "const x = 1;\nconst y = 2;\n").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("src/a.ts")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let who = git2::Signature::now("t", "t@example.com").unwrap();
        repo.commit(Some("HEAD"), &who, &who, "first", &tree, &[])
            .unwrap();
        drop(tree);
        drop(repo);

        let opened = Repo::discover_from(dir.path()).unwrap();
        (dir, opened)
    }

    #[test]
    fn added_lines_carry_their_post_image_line_numbers() {
        let (dir, repo) = fixture();
        fs::write(
            dir.path().join("src/a.ts"),
            "const x = 1;\n// one\n// two\nconst y = 2;\n",
        )
        .unwrap();

        let files = repo.diff(&Source::Worktree).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/a.ts");
        assert_eq!(
            files[0].added,
            vec![(2, "// one".to_string()), (3, "// two".to_string())]
        );
    }

    #[test]
    fn untracked_files_count_in_full() {
        let (dir, repo) = fixture();
        fs::write(dir.path().join("src/new.ts"), "// a\n// b\nlet z = 3;\n").unwrap();

        let files = repo.diff(&Source::Worktree).unwrap();
        let new = files.iter().find(|f| f.path == "src/new.ts").unwrap();
        assert_eq!(new.added.len(), 3);
        assert_eq!(new.added[0], (1, "// a".to_string()));
    }

    #[test]
    fn deleted_files_contribute_nothing() {
        let (dir, repo) = fixture();
        fs::remove_file(dir.path().join("src/a.ts")).unwrap();

        let files = repo.diff(&Source::Worktree).unwrap();
        assert!(files.iter().all(|file| file.added.is_empty()));
    }

    #[test]
    fn the_post_image_of_a_worktree_source_is_the_file_on_disk() {
        let (dir, repo) = fixture();
        fs::write(dir.path().join("src/a.ts"), "changed\n").unwrap();
        assert_eq!(
            repo.post_image(&Source::Worktree, "src/a.ts"),
            Some("changed\n".to_string())
        );
        assert_eq!(repo.post_image(&Source::Worktree, "src/gone.ts"), None);
    }
}
