//! End-to-end tests: a real git repository, the real binary, real exit codes.
//!
//! The unit tests cover classification logic on synthetic input. These cover
//! everything between the command line and that logic — argument handling,
//! git invocation, post-image lookup, the JSON envelope, and exit codes.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

/// A throwaway git repository with a deterministic identity.
struct Repo {
    dir: TempDir,
}

impl Repo {
    fn new() -> Self {
        let dir = TempDir::new().expect("temp dir");
        let repo = Self { dir };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "test@example.com"]);
        repo.git(&["config", "user.name", "Test"]);
        repo
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn git(&self, args: &[&str]) -> Output {
        let output = Command::new("git")
            .args(args)
            .current_dir(self.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn write(&self, name: &str, contents: &str) {
        let path = self.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, contents).expect("write file");
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    /// Run `gk` inside the repo and return (exit code, stdout, stderr).
    fn gk(&self, args: &[&str]) -> (i32, String, String) {
        gk_isolated(self.path(), args)
    }

    fn gk_json(&self, args: &[&str]) -> (i32, String, String) {
        let mut with_json = args.to_vec();
        with_json.push("--json");
        self.gk(&with_json)
    }

    /// Run `gk --json` and return the exit code with the success envelope's
    /// data, for commands that report a result yet exit non-zero.
    fn data_with_code(&self, args: &[&str]) -> (i32, Value) {
        let (code, stdout, stderr) = self.gk_json(args);
        let value: Value = serde_json::from_str(&stdout).unwrap_or_else(|err| {
            panic!("gk {args:?} printed no json on stdout ({err}); stderr: {stderr}")
        });
        assert_eq!(value["status"], "ok");
        (code, value["data"].clone())
    }

    /// Run `gk --json` and unwrap the success envelope.
    fn data(&self, args: &[&str]) -> Value {
        let (code, data) = self.data_with_code(args);
        assert_eq!(code, 0, "gk {args:?} exited {code}");
        data
    }

    /// Run `gk --json` expecting a failure: the exit code and the message
    /// from the error envelope on stderr.
    fn error(&self, args: &[&str]) -> (i32, String) {
        let (code, _, stderr) = self.gk_json(args);
        let value: Value = serde_json::from_str(&stderr).expect("error envelope on stderr");
        assert_eq!(value["status"], "error");
        let message = value["message"].as_str().expect("message");
        (code, message.to_string())
    }
}

const BASE_TS: &str = "export const a = 1;\nexport const b = 2;\n";

#[test]
fn counts_a_documented_addition_in_the_working_tree() {
    let repo = Repo::new();
    repo.write("src/a.ts", BASE_TS);
    repo.commit("base");

    repo.write(
        "src/a.ts",
        "export const a = 1;\n\
         export const b = 2;\n\
         /**\n\
         \x20* Two lines of prose,\n\
         \x20* so this is documentation.\n\
         \x20*/\n\
         export const c = 3;\n\
         // a lone aside\n\
         export const d = 4;\n",
    );

    let data = repo.data(&["doc"]);
    assert_eq!(data["totals"]["code"], 2, "two added code lines");
    assert_eq!(
        data["totals"]["comment"], 5,
        "four block lines plus the aside"
    );
    assert_eq!(
        data["totals"]["doc"], 4,
        "the lone aside is not documentation"
    );
    assert_eq!(data["source"], "worktree");
    assert_eq!(data["runs"][0]["lines"], 4);
    assert_eq!(data["runs"][0]["line"], 3);
}

#[test]
fn min_run_one_counts_every_comment_line() {
    let repo = Repo::new();
    repo.write("src/a.ts", BASE_TS);
    repo.commit("base");
    repo.write(
        "src/a.ts",
        "export const a = 1;\nexport const b = 2;\n// lone\nexport const c = 3;\n",
    );

    assert_eq!(repo.data(&["doc"])["totals"]["doc"], 0);
    assert_eq!(repo.data(&["doc", "--min-run", "1"])["totals"]["doc"], 1);
}

#[test]
fn block_state_does_not_leak_between_hunks() {
    // The bug the awk original had: an unclosed `/**` in one hunk marked
    // unrelated added lines in a later hunk as comments.
    let repo = Repo::new();
    let mut base = String::new();
    for i in 0..40 {
        base.push_str(&format!("export const v{i} = {i};\n"));
    }
    repo.write("src/a.ts", &base);
    repo.commit("base");

    let mut changed = String::new();
    for (i, line) in base.lines().enumerate() {
        if i == 0 {
            changed.push_str("/**\n * doc for the first one\n */\n");
        }
        changed.push_str(line);
        changed.push('\n');
        if i == 30 {
            changed.push_str("export const extra = 99;\n");
        }
    }
    repo.write("src/a.ts", &changed);

    let data = repo.data(&["doc"]);
    assert_eq!(data["totals"]["comment"], 3);
    assert_eq!(
        data["totals"]["code"], 1,
        "the line added 30 lines below is code, not a comment continuation"
    );
}

#[test]
fn html_comments_close_on_their_own_delimiter() {
    // The other awk bug: only the opening line counted as a comment.
    let repo = Repo::new();
    repo.write("src/a.vue", "<template>\n  <div/>\n</template>\n");
    repo.commit("base");
    repo.write(
        "src/a.vue",
        "<template>\n  <!-- one\n       two\n       three -->\n  <div/>\n</template>\n",
    );

    let data = repo.data(&["doc"]);
    assert_eq!(data["totals"]["comment"], 3);
    assert_eq!(data["totals"]["code"], 0);
}

#[test]
fn files_without_comment_syntax_are_skipped_not_counted_as_code() {
    let repo = Repo::new();
    repo.write("src/a.ts", BASE_TS);
    repo.commit("base");
    repo.write("data.json", "{\n  \"a\": 1\n}\n");
    repo.write("NOTES.md", "# heading\n\nprose\n");

    let data = repo.data(&["doc"]);
    assert_eq!(data["totals"]["code"], 0);
    assert_eq!(data["skipped_files"], 2);
}

#[test]
fn staged_and_worktree_are_different_sources() {
    let repo = Repo::new();
    repo.write("src/a.ts", BASE_TS);
    repo.commit("base");

    repo.write("src/a.ts", &format!("{BASE_TS}// staged\n// staged two\n"));
    repo.git(&["add", "src/a.ts"]);
    repo.write(
        "src/a.ts",
        &format!("{BASE_TS}// staged\n// staged two\nexport const unstaged = 1;\n"),
    );

    let staged = repo.data(&["doc", "--staged"]);
    assert_eq!(staged["source"], "staged");
    assert_eq!(staged["totals"]["doc"], 2);
    assert_eq!(
        staged["totals"]["code"], 0,
        "the unstaged line is not in the index"
    );

    let worktree = repo.data(&["doc"]);
    assert_eq!(worktree["totals"]["code"], 1, "worktree sees both");
}

#[test]
fn a_range_reads_its_post_image_from_the_revision_not_the_worktree() {
    let repo = Repo::new();
    repo.write("src/a.ts", BASE_TS);
    repo.commit("base");
    repo.write(
        "src/a.ts",
        &format!("{BASE_TS}/**\n * doc\n */\nexport const c = 3;\n"),
    );
    repo.commit("documented");

    // Leave the worktree in a state that would give a different answer if the
    // post-image were read from disk instead of from the commit.
    repo.write("src/a.ts", "export const only = 1;\n");

    let data = repo.data(&["doc", "--range", "HEAD~1..HEAD"]);
    assert_eq!(data["source"], "HEAD~1..HEAD");
    assert_eq!(data["totals"]["doc"], 3);
    assert_eq!(data["totals"]["code"], 1);
    assert_eq!(data["approximate_files"], 0);
}

#[test]
fn base_measures_what_a_branch_added() {
    let repo = Repo::new();
    repo.write("src/a.ts", BASE_TS);
    repo.commit("base");
    repo.git(&["checkout", "-q", "-b", "feature"]);
    repo.write(
        "src/a.ts",
        &format!("{BASE_TS}// one\n// two\nexport const c = 3;\n"),
    );
    repo.commit("feature work");

    let data = repo.data(&["doc", "--base", "main"]);
    assert_eq!(data["source"], "merge-base with main");
    assert_eq!(data["totals"]["doc"], 2);
    assert_eq!(data["totals"]["code"], 1);
}

#[test]
fn deleted_files_contribute_nothing() {
    let repo = Repo::new();
    repo.write("src/a.ts", BASE_TS);
    repo.write("src/b.ts", "export const gone = 1;\n");
    repo.commit("base");
    std::fs::remove_file(repo.path().join("src/b.ts")).expect("remove");

    let data = repo.data(&["doc"]);
    assert_eq!(data["totals"]["code"], 0);
    assert_eq!(data["files"].as_array().expect("files").len(), 0);
}

#[test]
fn whole_file_mode_walks_directories() {
    let repo = Repo::new();
    repo.write("src/a.ts", "/**\n * doc\n */\nexport const a = 1;\n");
    repo.write("src/nested/b.ts", "// lone\nexport const b = 2;\n");

    let data = repo.data(&["doc", "--files", "src"]);
    assert_eq!(data["source"], "files");
    assert_eq!(data["totals"]["code"], 2);
    assert_eq!(data["totals"]["comment"], 4);
    assert_eq!(data["totals"]["doc"], 3, "the lone comment is not a run");
    assert_eq!(data["files"].as_array().expect("files").len(), 2);
}

#[test]
fn whole_file_mode_needs_no_git_repository() {
    let dir = TempDir::new().expect("temp dir");
    std::fs::write(dir.path().join("a.ts"), "/**\n * doc\n */\nconst a = 1;\n").expect("write");

    let output = Command::new(env!("CARGO_BIN_EXE_gk"))
        .args(["doc", "--files", "a.ts", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("run gk");

    assert!(output.status.success(), "should not touch git at all");
}

#[test]
fn a_missing_path_is_an_expected_failure() {
    let repo = Repo::new();
    let (code, message) = repo.error(&["doc", "--files", "nope"]);

    assert_eq!(code, 1, "expected failure, not misuse");
    assert!(message.contains("no such path"), "got: {message}");
}

#[test]
fn running_outside_a_repository_reports_git_failing_not_a_panic() {
    let dir = TempDir::new().expect("temp dir");
    let output = Command::new(env!("CARGO_BIN_EXE_gk"))
        .args(["doc", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("run gk");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let value: Value = serde_json::from_str(&stderr).expect("error envelope");
    assert!(value["message"]
        .as_str()
        .expect("message")
        .contains("not a git repository"));
}

#[test]
fn conflicting_sources_are_misuse() {
    let repo = Repo::new();
    let (code, _, stderr) = repo.gk(&["doc", "--staged", "--range", "main..HEAD"]);
    assert_eq!(code, 2, "clap rejects conflicting flags with exit 2");
    assert!(stderr.contains("cannot be used with"), "got: {stderr}");
}

#[test]
fn human_output_is_not_json_and_still_exits_zero() {
    let repo = Repo::new();
    repo.write("src/a.ts", BASE_TS);
    repo.commit("base");
    repo.write("src/a.ts", &format!("{BASE_TS}// one\n// two\n"));

    let (code, stdout, _) = repo.gk(&["doc"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("documentation"), "got: {stdout}");
    assert!(!stdout.starts_with('{'));
}

#[test]
fn outline_reports_the_shape_of_a_tree() {
    let repo = Repo::new();
    repo.write("src/a.ts", BASE_TS);
    repo.write("src/b.rs", "fn main() {}\n");

    let data = repo.data(&["outline", "."]);
    assert_eq!(data["files"], 2, "the .git directory is not walked");
    let extensions: Vec<&str> = data["file_types"]
        .as_array()
        .expect("file_types")
        .iter()
        .map(|entry| entry["extension"].as_str().expect("extension"))
        .collect();
    assert!(extensions.contains(&"ts") && extensions.contains(&"rs"));
}

#[test]
fn limit_truncates_and_says_so() {
    let repo = Repo::new();
    repo.write("a.ts", "const a = 1;\n");
    repo.write("b.rs", "fn b() {}\n");
    repo.write("c.py", "c = 1\n");

    let data = repo.data(&["outline", ".", "--limit", "2"]);
    assert_eq!(data["file_types"].as_array().expect("types").len(), 2);
    assert_eq!(data["truncated"], true);
}

#[test]
fn a_new_untracked_file_counts_as_entirely_added() {
    let repo = Repo::new();
    repo.write("src/a.ts", BASE_TS);
    repo.commit("base");
    repo.write(
        "src/new.ts",
        "/**\n * A new module.\n */\nexport const fresh = 1;\n",
    );

    let data = repo.data(&["doc"]);
    assert_eq!(data["totals"]["doc"], 3);
    assert_eq!(data["totals"]["code"], 1);
    assert_eq!(data["files"][0]["path"], "src/new.ts");
}

#[test]
fn gitignored_files_are_not_mistaken_for_new_work() {
    let repo = Repo::new();
    repo.write(".gitignore", "build/\n");
    repo.commit("base");
    repo.write(
        "build/generated.ts",
        "// generated\n// do not count\nconst x = 1;\n",
    );

    let data = repo.data(&["doc"]);
    assert_eq!(data["totals"]["doc"], 0);
    assert_eq!(data["totals"]["code"], 0);
}

/// Number of files under this repo's `skills/`, the tree `init --claude` embeds.
/// Counted from the source tree so adding a skill does not mean editing a literal here.
fn embedded_skill_file_count() -> u64 {
    fn walk(dir: &Path, n: &mut u64) {
        for entry in std::fs::read_dir(dir).expect("read skills dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                walk(&path, n);
            } else {
                *n += 1;
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../skills");
    let mut n = 0;
    walk(&root, &mut n);
    n
}

#[test]
fn init_claude_installs_the_embedded_skills_into_dot_claude() {
    let repo = Repo::new();
    let data = repo.data(&["init", "--claude"]);

    assert_eq!(data["totals"]["installed"], embedded_skill_file_count());
    assert_eq!(data["totals"]["conflicts"], 0);

    let outline = repo.path().join(".claude/skills/gist-outline/SKILL.md");
    assert!(outline.exists());
    assert!(std::fs::read_to_string(&outline)
        .expect("read")
        .contains("name: gist-outline"));

    let reference = repo
        .path()
        .join(".claude/skills/gist-doc-review/references/public-surface.md");
    assert!(reference.exists());
}

#[test]
fn init_claude_is_idempotent_on_a_second_run() {
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);
    let data = repo.data(&["init", "--claude"]);

    assert_eq!(data["totals"]["installed"], 0);
    assert_eq!(data["totals"]["unchanged"], embedded_skill_file_count());
    assert_eq!(data["totals"]["conflicts"], 0);
}

#[test]
fn init_without_a_target_is_a_refusal_not_misuse() {
    let repo = Repo::new();
    let (code, message) = repo.error(&["init"]);

    assert_eq!(code, 1, "expected refusal, not misuse");
    assert!(message.contains("--claude"), "got: {message}");
    assert!(message.contains("--codex"), "got: {message}");
}

#[test]
fn init_experimental_alone_is_still_a_refusal_no_implicit_target() {
    let repo = Repo::new();
    let (code, _) = repo.error(&["init", "--experimental=mattpocock"]);

    assert_eq!(code, 1, "expected refusal, not a guess at which target");
}

#[test]
fn init_claude_reports_a_conflict_and_leaves_the_file_untouched() {
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);
    repo.write(".claude/skills/gist-outline/SKILL.md", "locally modified\n");

    let (code, data) = repo.data_with_code(&["init", "--claude"]);
    assert_eq!(code, 1);
    assert_eq!(data["totals"]["conflicts"], 1);

    let contents =
        std::fs::read_to_string(repo.path().join(".claude/skills/gist-outline/SKILL.md"))
            .expect("read");
    assert_eq!(contents, "locally modified\n");
}

#[test]
fn init_claude_force_overwrites_a_conflicting_file() {
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);
    repo.write(".claude/skills/gist-outline/SKILL.md", "locally modified\n");

    let data = repo.data(&["init", "--claude", "--force"]);
    assert_eq!(data["totals"]["overwritten"], 1);
    assert_eq!(data["totals"]["conflicts"], 0);

    let contents =
        std::fs::read_to_string(repo.path().join(".claude/skills/gist-outline/SKILL.md"))
            .expect("read");
    assert!(contents.contains("name: gist-outline"));
}

#[test]
fn init_claude_needs_no_git_repository() {
    let dir = TempDir::new().expect("temp dir");
    let output = Command::new(env!("CARGO_BIN_EXE_gk"))
        .args(["init", "--claude", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("run gk");

    assert!(output.status.success(), "should not touch git at all");
    assert!(dir
        .path()
        .join(".claude/skills/gist-outline/SKILL.md")
        .exists());
}

#[test]
fn init_claude_human_output_lists_each_skill_and_file() {
    let repo = Repo::new();
    let (code, stdout, _) = repo.gk(&["init", "--claude"]);

    assert_eq!(code, 0);
    assert!(stdout.contains("gist-outline"), "got: {stdout}");
    assert!(stdout.contains("installed"), "got: {stdout}");
    assert!(!stdout.starts_with('{'));
}

/// One skill name under `experimental/<package>/skills/`, so a test does not
/// hard-code an upstream name that `just vendor update` may rename.
fn an_experimental_skill(package: &str) -> String {
    fn find(dir: &Path) -> Option<String> {
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .expect("read dir")
            .map(|entry| entry.expect("dir entry").path())
            .collect();
        paths.sort();
        for path in paths.into_iter().filter(|path| path.is_dir()) {
            if path.join("SKILL.md").is_file() {
                return path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned());
            }
            if let Some(found) = find(&path) {
                return Some(found);
            }
        }
        None
    }
    let skills = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../experimental")
        .join(package)
        .join("skills");
    find(&skills).expect("package vendors at least one skill")
}

/// Number of files under every skill directory (one containing `SKILL.md`
/// directly) inside `experimental/<package>/skills/`, recursively — mirrors
/// what `init --experimental` itself walks, ignoring stray files like a
/// category's `README.md` that sit alongside skill dirs but aren't one.
fn experimental_skill_file_count(package: &str) -> u64 {
    fn walk(dir: &Path, n: &mut u64) {
        if dir.join("SKILL.md").is_file() {
            fn count_all(dir: &Path, n: &mut u64) {
                for entry in std::fs::read_dir(dir).expect("read dir") {
                    let path = entry.expect("dir entry").path();
                    if path.is_dir() {
                        count_all(&path, n);
                    } else {
                        *n += 1;
                    }
                }
            }
            count_all(dir, n);
            return;
        }
        for entry in std::fs::read_dir(dir).expect("read dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                walk(&path, n);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../experimental")
        .join(package)
        .join("skills");
    let mut n = 0;
    walk(&root, &mut n);
    n
}

#[test]
fn init_experimental_installs_a_vendored_package_prefixed() {
    let repo = Repo::new();
    let data = repo.data(&["init", "--claude", "--experimental=mattpocock"]);

    assert_eq!(
        data["totals"]["installed"],
        embedded_skill_file_count() + experimental_skill_file_count("mattpocock")
    );
    assert_eq!(data["totals"]["conflicts"], 0);

    let skill_md = repo.path().join(".claude/skills/mattpocock-tdd/SKILL.md");
    assert!(skill_md.exists());
    assert!(std::fs::read_to_string(&skill_md)
        .expect("read")
        .contains("name: mattpocock-tdd"));

    let reference = repo.path().join(".claude/skills/mattpocock-tdd/mocking.md");
    assert!(reference.exists());

    // Nothing lands unprefixed, and no bare `.claude/skills/tdd/` appears.
    assert!(!repo.path().join(".claude/skills/tdd").exists());
}

#[test]
fn init_experimental_is_idempotent_on_a_second_run() {
    let repo = Repo::new();
    repo.data(&["init", "--claude", "--experimental=mattpocock"]);
    let data = repo.data(&["init", "--claude", "--experimental=mattpocock"]);

    assert_eq!(data["totals"]["installed"], 0);
    assert_eq!(
        data["totals"]["unchanged"],
        embedded_skill_file_count() + experimental_skill_file_count("mattpocock")
    );
    assert_eq!(data["totals"]["conflicts"], 0);
}

#[test]
fn init_experimental_unknown_package_names_what_is_available() {
    let repo = Repo::new();
    let (code, message) = repo.error(&["init", "--claude", "--experimental=nope"]);

    assert_eq!(code, 1, "expected refusal, not misuse");
    assert!(message.contains("nope"), "got: {message}");
    assert!(message.contains("mattpocock"), "got: {message}");
}

#[test]
fn init_claude_and_experimental_together_install_both() {
    let repo = Repo::new();
    let data = repo.data(&["init", "--claude", "--experimental=mattpocock"]);

    assert_eq!(
        data["totals"]["installed"],
        embedded_skill_file_count() + experimental_skill_file_count("mattpocock")
    );

    assert!(repo
        .path()
        .join(".claude/skills/gist-outline/SKILL.md")
        .exists());
    assert!(repo
        .path()
        .join(".claude/skills/mattpocock-tdd/SKILL.md")
        .exists());
}

#[test]
fn init_codex_installs_into_dot_agents_skills() {
    let repo = Repo::new();
    let data = repo.data(&["init", "--codex"]);

    assert_eq!(data["totals"]["installed"], embedded_skill_file_count());
    assert_eq!(data["totals"]["conflicts"], 0);

    let outline = repo.path().join(".agents/skills/gist-outline/SKILL.md");
    assert!(outline.exists());
    assert!(std::fs::read_to_string(&outline)
        .expect("read")
        .contains("name: gist-outline"));

    // Codex is a separate target, not a rename of the Claude one.
    assert!(!repo.path().join(".claude/skills").exists());
}

#[test]
fn init_claude_and_codex_together_install_identical_content_into_both() {
    let repo = Repo::new();
    let data = repo.data(&["init", "--claude", "--codex"]);

    assert_eq!(data["totals"]["installed"], 2 * embedded_skill_file_count());
    assert_eq!(data["totals"]["conflicts"], 0);

    let claude = repo.path().join(".claude/skills/gist-outline/SKILL.md");
    let codex = repo.path().join(".agents/skills/gist-outline/SKILL.md");
    assert!(claude.exists());
    assert!(codex.exists());
    assert_eq!(
        std::fs::read(&claude).expect("read"),
        std::fs::read(&codex).expect("read"),
        "same skill, byte-identical regardless of target"
    );
}

#[test]
fn init_codex_experimental_reuses_the_prefix_rewrite() {
    let repo = Repo::new();
    let data = repo.data(&["init", "--codex", "--experimental=mattpocock"]);

    assert_eq!(
        data["totals"]["installed"],
        embedded_skill_file_count() + experimental_skill_file_count("mattpocock")
    );

    let skill_md = repo.path().join(".agents/skills/mattpocock-tdd/SKILL.md");
    assert!(skill_md.exists());
    assert!(std::fs::read_to_string(&skill_md)
        .expect("read")
        .contains("name: mattpocock-tdd"));
    assert!(!repo.path().join(".agents/skills/tdd").exists());
}

#[test]
fn init_codex_is_idempotent_on_a_second_run() {
    let repo = Repo::new();
    repo.data(&["init", "--codex"]);
    let data = repo.data(&["init", "--codex"]);

    assert_eq!(data["totals"]["installed"], 0);
    assert_eq!(data["totals"]["unchanged"], embedded_skill_file_count());
    assert_eq!(data["totals"]["conflicts"], 0);
}

fn manifest(repo: &Repo, root: &str) -> Value {
    let text = std::fs::read_to_string(repo.path().join(root).join(".gist-manifest.json"))
        .expect("read manifest");
    serde_json::from_str(&text).expect("manifest is json")
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Plant a manifest naming one file, as a repo carrying its own would.
fn plant_manifest(repo: &Repo, path: &str, sha256: &str) {
    repo.write(
        ".claude/skills/.gist-manifest.json",
        &format!(r#"{{"gk_version":"x","files":[{{"path":{path:?},"sha256":"{sha256}"}}]}}"#),
    );
}

#[test]
fn uninstall_never_removes_a_file_outside_the_root_named_by_dotdot() {
    let repo = Repo::new();
    repo.write("victim.txt", "precious");
    plant_manifest(&repo, "../../victim.txt", &sha256_hex(b"precious"));

    let (code, message) = repo.error(&["init", "--claude", "--uninstall"]);

    assert_eq!(code, 1);
    assert!(message.contains("plain relative path"), "got: {message}");
    assert!(repo.path().join("victim.txt").exists());
}

#[test]
fn uninstall_never_removes_an_absolute_path_even_with_force() {
    let repo = Repo::new();
    let outside = TempDir::new().expect("temp dir");
    let victim = outside.path().join("victim.txt");
    std::fs::write(&victim, "precious").expect("write");
    plant_manifest(&repo, victim.to_str().expect("utf8"), "not-the-hash");

    let (code, _) = repo.error(&["init", "--claude", "--uninstall", "--force"]);

    assert_eq!(code, 1);
    assert!(victim.exists());
}

#[test]
fn uninstall_refuses_a_manifest_that_is_json_but_not_a_manifest() {
    let repo = Repo::new();
    repo.write(
        ".claude/skills/.gist-manifest.json",
        r#"{"gk_version":"x"}"#,
    );

    let (code, message) = repo.error(&["init", "--claude", "--uninstall"]);

    assert_eq!(code, 1);
    assert!(message.contains("nothing was removed"), "got: {message}");
}

#[test]
fn writing_the_manifest_leaves_no_temporary_file_and_clears_a_stale_one() {
    let repo = Repo::new();
    repo.write(".claude/skills/.gist-manifest.json.tmp", "left by a crash");

    repo.data(&["init", "--claude"]);

    assert!(!repo
        .path()
        .join(".claude/skills/.gist-manifest.json.tmp")
        .exists());
    assert!(manifest(&repo, ".claude/skills")["files"].is_array());
}

#[cfg(unix)]
#[test]
fn a_symlink_planted_at_the_temporary_name_is_not_written_through() {
    let repo = Repo::new();
    let outside = TempDir::new().expect("temp dir");
    let target = outside.path().join("important.txt");
    std::fs::write(&target, "important").expect("write");
    std::fs::create_dir_all(repo.path().join(".claude/skills")).expect("mkdir");
    std::os::unix::fs::symlink(
        &target,
        repo.path().join(".claude/skills/.gist-manifest.json.tmp"),
    )
    .expect("symlink");

    repo.data(&["init", "--claude"]);

    assert_eq!(std::fs::read_to_string(&target).expect("read"), "important");
    assert!(manifest(&repo, ".claude/skills")["files"].is_array());
}

#[cfg(unix)]
#[test]
fn init_refuses_to_write_through_a_symlinked_skill_directory() {
    let repo = Repo::new();
    let outside = TempDir::new().expect("temp dir");
    std::fs::create_dir_all(repo.path().join(".claude/skills")).expect("mkdir");
    std::os::unix::fs::symlink(
        outside.path(),
        repo.path().join(".claude/skills/gist-outline"),
    )
    .expect("symlink");

    let (code, message) = repo.error(&["init", "--claude", "--force"]);

    assert_eq!(code, 1);
    assert!(message.contains("symlink"), "got: {message}");
    assert_eq!(
        std::fs::read_dir(outside.path()).expect("read").count(),
        0,
        "nothing may land outside the repo"
    );
}

#[cfg(unix)]
#[test]
fn init_refuses_a_manifest_that_is_a_dangling_symlink() {
    let repo = Repo::new();
    let outside = TempDir::new().expect("temp dir");
    std::fs::create_dir_all(repo.path().join(".claude/skills")).expect("mkdir");
    std::os::unix::fs::symlink(
        outside.path().join("planted.json"),
        repo.path().join(".claude/skills/.gist-manifest.json"),
    )
    .expect("symlink");

    let (code, message) = repo.error(&["init", "--claude"]);

    assert_eq!(code, 1);
    assert!(message.contains("symlink"), "got: {message}");
    assert!(!outside.path().join("planted.json").exists());
}

#[cfg(unix)]
#[test]
fn uninstall_removes_nothing_when_a_recorded_path_crosses_a_symlink() {
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);
    let outside = TempDir::new().expect("temp dir");
    std::fs::write(outside.path().join("SKILL.md"), "precious").expect("write");
    std::fs::remove_dir_all(repo.path().join(".claude/skills/gist-outline")).expect("rm");
    std::os::unix::fs::symlink(
        outside.path(),
        repo.path().join(".claude/skills/gist-outline"),
    )
    .expect("symlink");

    let (code, message) = repo.error(&["init", "--claude", "--uninstall", "--force"]);

    assert_eq!(code, 1);
    assert!(message.contains("symlink"), "got: {message}");
    assert!(outside.path().join("SKILL.md").exists());
    assert!(
        repo.path().join(".claude/skills/gist-doc-review").exists(),
        "a refusal removes nothing, not even the files that were fine"
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_skills_root_is_refused() {
    let repo = Repo::new();
    let real = TempDir::new().expect("temp dir");
    std::fs::create_dir_all(repo.path().join(".claude")).expect("mkdir");
    std::os::unix::fs::symlink(real.path(), repo.path().join(".claude/skills")).expect("symlink");

    let (code, message) = repo.error(&["init", "--claude"]);

    assert_eq!(code, 1);
    assert!(message.contains("symlink"), "got: {message}");
    assert_eq!(
        std::fs::read_dir(real.path()).expect("read").count(),
        0,
        "nothing may land where a repo's link points"
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_dot_claude_is_refused_by_install_and_uninstall() {
    let repo = Repo::new();
    let real = TempDir::new().expect("temp dir");
    std::os::unix::fs::symlink(real.path(), repo.path().join(".claude")).expect("symlink");

    let (code, message) = repo.error(&["init", "--claude", "--force"]);
    assert_eq!(code, 1);
    assert!(message.contains("symlink"), "got: {message}");
    assert_eq!(std::fs::read_dir(real.path()).expect("read").count(), 0);

    // What a hostile repo would plant behind the link: a manifest and a
    // file it names, matching, so uninstall would remove it.
    let victim = real.path().join("skills/gist-outline/SKILL.md");
    std::fs::create_dir_all(victim.parent().expect("parent")).expect("mkdir");
    std::fs::write(&victim, "precious").expect("write");
    plant_manifest(&repo, "gist-outline/SKILL.md", &sha256_hex(b"precious"));

    let (code, message) = repo.error(&["init", "--claude", "--uninstall"]);
    assert_eq!(code, 1);
    assert!(message.contains("symlink"), "got: {message}");
    assert!(
        victim.exists(),
        "uninstall must not remove through the link"
    );
}

#[test]
fn install_checks_every_root_before_writing_any() {
    let repo = Repo::new();
    repo.write(
        ".agents/skills/.gist-manifest.json",
        "{\"files\":[{\"path\":\"../x\"}]}",
    );

    let (code, _) = repo.error(&["init", "--claude", "--codex"]);

    assert_eq!(code, 1);
    assert!(
        !repo.path().join(".claude").exists(),
        "the first root must not be installed when the second is refused"
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_found_late_in_the_skill_order_leaves_nothing_installed() {
    let repo = Repo::new();
    let outside = TempDir::new().expect("temp dir");
    std::fs::create_dir_all(repo.path().join(".claude/skills")).expect("mkdir");
    std::os::unix::fs::symlink(
        outside.path(),
        repo.path().join(".claude/skills/gist-outline"),
    )
    .expect("symlink");

    let (code, message) = repo.error(&["init", "--claude"]);

    assert_eq!(code, 1);
    assert!(message.contains("symlink"), "got: {message}");
    assert!(!repo.path().join(".claude/skills/gist-doc-review").exists());
    assert!(!repo
        .path()
        .join(".claude/skills/.gist-manifest.json")
        .exists());
}

#[cfg(unix)]
#[test]
fn uninstall_checks_every_root_before_removing_any() {
    let repo = Repo::new();
    repo.data(&["init", "--claude", "--codex"]);
    let outside = TempDir::new().expect("temp dir");
    let codex_skill = repo.path().join(".agents/skills/gist-outline");
    std::fs::remove_dir_all(&codex_skill).expect("remove");
    std::os::unix::fs::symlink(outside.path(), &codex_skill).expect("symlink");

    let (code, message) = repo.error(&["init", "--claude", "--codex", "--uninstall"]);

    assert_eq!(code, 1);
    assert!(message.contains("symlink"), "got: {message}");
    assert!(
        repo.path().join(".claude/skills/gist-doc-review").exists(),
        "a refusal in one root removes nothing in the other"
    );
}

#[cfg(unix)]
#[test]
fn an_unchanged_rerun_does_not_rewrite_the_manifest() {
    use std::os::unix::fs::MetadataExt;
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);
    let path = repo.path().join(".claude/skills/.gist-manifest.json");
    let before = std::fs::metadata(&path).expect("stat").ino();

    let data = repo.data(&["init", "--claude"]);

    assert_eq!(data["totals"]["installed"], 0);
    assert_eq!(
        std::fs::metadata(&path).expect("stat").ino(),
        before,
        "a rerun that changed nothing should not replace the file"
    );
}

#[test]
fn init_refuses_a_manifest_whose_paths_leave_the_root() {
    let repo = Repo::new();
    plant_manifest(&repo, "../../victim.txt", "any");

    let (code, message) = repo.error(&["init", "--claude"]);

    assert_eq!(code, 1);
    assert!(message.contains("plain relative path"), "got: {message}");
    assert!(!repo.path().join(".claude/skills/gist-outline").exists());
}

#[test]
fn init_claude_writes_a_manifest_after_install() {
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);

    let manifest = manifest(&repo, ".claude/skills");
    assert_eq!(manifest["gk_version"], env!("CARGO_PKG_VERSION"));
    let files = manifest["files"].as_array().expect("files array");
    assert_eq!(files.len() as u64, embedded_skill_file_count());

    let paths: Vec<&str> = files.iter().map(|f| f["path"].as_str().unwrap()).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "manifest entries are sorted by path");
    assert!(paths.contains(&"gist-outline/SKILL.md"));

    for file in files {
        let sha256 = file["sha256"].as_str().expect("sha256");
        assert_eq!(sha256.len(), 64, "sha256 is 64 hex chars, got {sha256}");
    }
}

#[test]
fn init_claude_uninstall_removes_untouched_files() {
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);

    let data = repo.data(&["init", "--claude", "--uninstall"]);
    assert_eq!(data["totals"]["removed"], embedded_skill_file_count());
    assert_eq!(data["totals"]["kept"], 0);
    assert_eq!(data["totals"]["missing"], 0);

    assert!(!repo.path().join(".claude/skills/gist-outline").exists());
    assert!(!repo.path().join(".claude/skills/gist-doc-review").exists());
    assert!(!repo
        .path()
        .join(".claude/skills/.gist-manifest.json")
        .exists());
}

#[test]
fn init_claude_uninstall_keeps_locally_modified_files_and_reports_them() {
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);
    repo.write(".claude/skills/gist-outline/SKILL.md", "locally modified\n");

    let (code, data) = repo.data_with_code(&["init", "--claude", "--uninstall"]);
    assert_eq!(code, 1);
    assert_eq!(data["totals"]["kept"], 1);
    assert_eq!(data["totals"]["removed"], embedded_skill_file_count() - 1);

    let contents =
        std::fs::read_to_string(repo.path().join(".claude/skills/gist-outline/SKILL.md"))
            .expect("read");
    assert_eq!(contents, "locally modified\n");

    // The rest, including the now-empty gist-doc-review tree, is gone.
    assert!(!repo.path().join(".claude/skills/gist-doc-review").exists());

    let manifest = manifest(&repo, ".claude/skills");
    let files = manifest["files"].as_array().expect("files array");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "gist-outline/SKILL.md");
}

#[test]
fn init_claude_uninstall_does_not_delete_a_file_that_conflicted_at_install_time() {
    let repo = Repo::new();
    repo.write(
        ".claude/skills/gist-outline/SKILL.md",
        "pre-existing, not gk's\n",
    );

    let (code, data) = repo.data_with_code(&["init", "--claude"]);
    assert_eq!(code, 1);
    assert_eq!(data["totals"]["conflicts"], 1);

    let manifest = manifest(&repo, ".claude/skills");
    let files = manifest["files"].as_array().expect("files array");
    assert!(
        !files.iter().any(|f| f["path"] == "gist-outline/SKILL.md"),
        "a conflicted file was never placed, so the manifest must not record it"
    );

    let data = repo.data(&["init", "--claude", "--uninstall"]);
    assert_eq!(data["totals"]["kept"], 0);
    assert_eq!(data["totals"]["removed"], embedded_skill_file_count() - 1);

    let contents =
        std::fs::read_to_string(repo.path().join(".claude/skills/gist-outline/SKILL.md"))
            .expect("read");
    assert_eq!(
        contents, "pre-existing, not gk's\n",
        "uninstall must not delete a file gk never placed"
    );
}

#[test]
fn a_conflict_on_rerun_still_protects_the_edited_file_on_uninstall() {
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);
    repo.write(".claude/skills/gist-outline/SKILL.md", "locally modified\n");

    let (code, _) = repo.data_with_code(&["init", "--claude"]);
    assert_eq!(code, 1);

    let (code, data) = repo.data_with_code(&["init", "--claude", "--uninstall"]);
    assert_eq!(code, 1);
    assert_eq!(data["totals"]["kept"], 1);

    let contents =
        std::fs::read_to_string(repo.path().join(".claude/skills/gist-outline/SKILL.md"))
            .expect("read");
    assert_eq!(contents, "locally modified\n");
}

#[test]
fn rerunning_without_experimental_keeps_earlier_experimental_files_tracked() {
    let repo = Repo::new();
    let skill = format!("mattpocock-{}", an_experimental_skill("mattpocock"));
    repo.data(&["init", "--claude", "--experimental=mattpocock"]);
    repo.data(&["init", "--claude"]);

    let manifest = manifest(&repo, ".claude/skills");
    let files = manifest["files"].as_array().expect("files array");
    assert!(files
        .iter()
        .any(|f| f["path"] == format!("{skill}/SKILL.md")));

    repo.data(&["init", "--claude", "--uninstall"]);
    assert!(!repo.path().join(".claude/skills").join(&skill).exists());
}

#[test]
fn an_io_failure_partway_still_records_the_files_already_placed() {
    let repo = Repo::new();
    // A directory where a skill file belongs passes every check made before
    // the first write. Skills sorted before it place fine, then the run fails
    // reading this one.
    std::fs::create_dir_all(repo.path().join(".claude/skills/gist-outline/SKILL.md"))
        .expect("mkdir");

    let (code, message) = repo.error(&["init", "--claude"]);
    assert_eq!(code, 1);
    assert!(message.contains("gist-outline"), "got: {message}");

    let manifest = manifest(&repo, ".claude/skills");
    let files = manifest["files"].as_array().expect("files array");
    let paths: Vec<&str> = files.iter().map(|f| f["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"gist-doc-review/SKILL.md"));
    assert!(!paths.iter().any(|p| p.starts_with("gist-outline")));

    let data = repo.data(&["init", "--claude", "--uninstall"]);
    assert_eq!(data["totals"]["removed"], files.len() as u64);
    assert!(!repo.path().join(".claude/skills/gist-doc-review").exists());
    assert!(repo
        .path()
        .join(".claude/skills/gist-outline/SKILL.md")
        .is_dir());
}

#[test]
fn init_refuses_to_overwrite_a_corrupt_manifest() {
    let repo = Repo::new();
    repo.write(".claude/skills/.gist-manifest.json", "not json");

    let (code, message) = repo.error(&["init", "--claude"]);
    assert_eq!(code, 1, "expected refusal, not misuse");
    assert!(message.contains(".gist-manifest.json"), "got: {message}");

    let contents = std::fs::read_to_string(repo.path().join(".claude/skills/.gist-manifest.json"))
        .expect("read");
    assert_eq!(contents, "not json");
    assert!(!repo.path().join(".claude/skills/gist-outline").exists());
}

#[test]
fn init_claude_uninstall_with_a_corrupt_manifest_is_a_zero_op() {
    let repo = Repo::new();
    repo.write(".claude/skills/.gist-manifest.json", "not json");

    let data = repo.data(&["init", "--claude", "--uninstall"]);
    assert_eq!(data["totals"]["removed"], 0);
    assert_eq!(data["totals"]["kept"], 0);
}

#[test]
fn a_modified_file_stays_protected_across_repeated_uninstalls() {
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);
    repo.write(".claude/skills/gist-outline/SKILL.md", "locally modified\n");

    for run in 1..=2 {
        let (code, data) = repo.data_with_code(&["init", "--claude", "--uninstall"]);
        assert_eq!(code, 1, "run {run}");
        assert_eq!(data["totals"]["kept"], 1, "run {run}");
    }

    let contents =
        std::fs::read_to_string(repo.path().join(".claude/skills/gist-outline/SKILL.md"))
            .expect("read");
    assert_eq!(contents, "locally modified\n");
}

#[test]
fn a_file_found_already_identical_is_recorded_and_later_uninstalled() {
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);
    std::fs::remove_file(repo.path().join(".claude/skills/.gist-manifest.json"))
        .expect("remove manifest");

    let data = repo.data(&["init", "--claude"]);
    assert_eq!(data["totals"]["unchanged"], embedded_skill_file_count());

    let manifest = manifest(&repo, ".claude/skills");
    assert_eq!(
        manifest["files"].as_array().expect("files").len() as u64,
        embedded_skill_file_count()
    );

    let data = repo.data(&["init", "--claude", "--uninstall"]);
    assert_eq!(data["totals"]["removed"], embedded_skill_file_count());
}

#[test]
fn init_experimental_repeated_flag_installs_the_package_once() {
    let repo = Repo::new();
    let data = repo.data(&[
        "init",
        "--claude",
        "--experimental=mattpocock",
        "--experimental=mattpocock",
    ]);

    assert_eq!(
        data["totals"]["installed"],
        embedded_skill_file_count() + experimental_skill_file_count("mattpocock")
    );
    assert_eq!(data["totals"]["unchanged"], 0);
}

#[cfg(unix)]
#[test]
fn uninstall_refuses_a_manifest_that_is_a_symlink() {
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);
    let outside = TempDir::new().expect("temp dir");
    let manifest_path = repo.path().join(".claude/skills/.gist-manifest.json");
    let moved = outside.path().join("manifest.json");
    std::fs::copy(&manifest_path, &moved).expect("copy");
    std::fs::remove_file(&manifest_path).expect("remove");
    std::os::unix::fs::symlink(&moved, &manifest_path).expect("symlink");

    let (code, message) = repo.error(&["init", "--claude", "--uninstall"]);

    assert_eq!(code, 1);
    assert!(message.contains("symlink"), "got: {message}");
    assert!(repo.path().join(".claude/skills/gist-outline").exists());
}

#[test]
fn init_claude_uninstall_force_removes_everything() {
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);
    repo.write(".claude/skills/gist-outline/SKILL.md", "locally modified\n");

    let data = repo.data(&["init", "--claude", "--uninstall", "--force"]);
    assert_eq!(data["totals"]["removed"], embedded_skill_file_count());
    assert_eq!(data["totals"]["kept"], 0);

    // The manifest-recorded skill trees are gone; the root itself is left
    // alone (it may hold skills of the user's own).
    assert!(!repo.path().join(".claude/skills/gist-outline").exists());
    assert!(!repo.path().join(".claude/skills/gist-doc-review").exists());
    assert!(!repo
        .path()
        .join(".claude/skills/.gist-manifest.json")
        .exists());
}

#[test]
fn init_claude_uninstall_handles_a_file_already_deleted_by_hand() {
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);
    std::fs::remove_file(repo.path().join(".claude/skills/gist-outline/SKILL.md")).expect("remove");

    let data = repo.data(&["init", "--claude", "--uninstall"]);
    assert_eq!(data["totals"]["missing"], 1);
    assert_eq!(data["totals"]["removed"], embedded_skill_file_count() - 1);
    assert_eq!(data["totals"]["kept"], 0);

    assert!(!repo.path().join(".claude/skills/gist-outline").exists());
    assert!(!repo.path().join(".claude/skills/gist-doc-review").exists());
    assert!(!repo
        .path()
        .join(".claude/skills/.gist-manifest.json")
        .exists());
}

#[test]
fn init_claude_uninstall_with_no_prior_install_is_a_zero_op_not_an_error() {
    let repo = Repo::new();
    let data = repo.data(&["init", "--claude", "--uninstall"]);

    assert_eq!(data["totals"]["removed"], 0);
    assert_eq!(data["totals"]["kept"], 0);
    assert_eq!(data["totals"]["missing"], 0);
}

#[test]
fn init_claude_uninstall_human_output_lists_kept_files_and_the_force_hint() {
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);
    repo.write(".claude/skills/gist-outline/SKILL.md", "locally modified\n");

    let (code, stdout, _) = repo.gk(&["init", "--claude", "--uninstall"]);
    assert_eq!(code, 1);
    assert!(stdout.contains("gist-outline/SKILL.md"), "got: {stdout}");
    assert!(stdout.contains("--force"), "got: {stdout}");
    assert!(!stdout.starts_with('{'));
}

#[test]
fn init_uninstall_with_experimental_is_a_refusal() {
    let repo = Repo::new();
    repo.data(&["init", "--claude", "--experimental=mattpocock"]);

    let (code, message) = repo.error(&[
        "init",
        "--claude",
        "--uninstall",
        "--experimental=mattpocock",
    ]);
    assert_eq!(code, 1, "expected refusal, not misuse");
    assert!(message.contains("--experimental"), "got: {message}");
}

#[test]
fn init_claude_uninstall_does_not_touch_a_codex_target_left_unselected() {
    let repo = Repo::new();
    repo.data(&["init", "--claude", "--codex"]);

    repo.data(&["init", "--claude", "--uninstall"]);

    assert!(!repo.path().join(".claude/skills/gist-outline").exists());
    assert!(repo
        .path()
        .join(".agents/skills/gist-outline/SKILL.md")
        .exists());
}

#[test]
fn a_renamed_file_counts_only_what_changed_in_it() {
    let repo = Repo::new();
    let body: String = (0..40)
        .map(|n| format!("export const v{n} = {n};\n"))
        .collect();
    repo.write("src/old.ts", &body);
    repo.commit("base");

    std::fs::remove_file(repo.path().join("src/old.ts")).expect("remove");
    repo.write("src/new.ts", &format!("{body}// one\n// two\n"));

    let data = repo.data(&["doc"]);
    assert_eq!(
        data["totals"]["code"], 0,
        "the moved body is not a new 40 lines of code"
    );
    assert_eq!(
        data["totals"]["doc"], 2,
        "only the added comment run counts"
    );
}

// ---- gk hook (docs/adr/0009-install-git-hooks-with-gk.md) ----

/// Run `gk` in `dir` with the user's global and system git config out of the
/// way, so a `core.hooksPath` or `commit.subjectMax` on the developer's
/// machine cannot change what any test sees.
fn gk_isolated(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_gk"))
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("run gk");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The current PATH with the directory holding the built `gk` in front, which
/// is what an installed shim needs to find it.
fn path_with_gk() -> std::ffi::OsString {
    let gk_dir = Path::new(env!("CARGO_BIN_EXE_gk"))
        .parent()
        .expect("gk has a parent directory")
        .to_path_buf();
    let mut paths = vec![gk_dir];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    std::env::join_paths(paths).expect("join PATH")
}

impl Repo {
    fn hook_file(&self) -> std::path::PathBuf {
        self.path().join(".git/hooks/commit-msg")
    }

    /// Real git, isolated, with `gk` on PATH; the outcome is returned, not asserted.
    fn git_hooked(&self, args: &[&str]) -> Output {
        Command::new("git")
            .args(args)
            .current_dir(self.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("PATH", path_with_gk())
            .output()
            .expect("run git")
    }

    /// Put `message` in a file and run the commit-msg check on it.
    fn check_message(&self, message: &str) -> (i32, String, String) {
        let file = self.path().join(".git/MESSAGE_UNDER_TEST");
        std::fs::write(&file, message).expect("write message");
        self.gk(&["hook", "commit-msg", file.to_str().expect("utf-8 path")])
    }

    fn set_config(&self, key: &str, value: &str) {
        self.git(&["config", key, value]);
    }
}

const PROJECT_PATTERN: &str = r"^([a-z]+)\(([A-Za-z]+-?[0-9]+)\): .+";

#[test]
fn commit_msg_accepts_the_generic_style_and_says_nothing() {
    let repo = Repo::new();
    let (code, stdout, stderr) = repo.check_message("Add thing\n\nBecause it was missing.\n");
    assert_eq!((code, stdout.as_str(), stderr.as_str()), (0, "", ""));
}

#[test]
fn commit_msg_rejects_a_lowercase_subject() {
    let repo = Repo::new();
    let (code, _, stderr) = repo.check_message("add thing\n");
    assert_eq!(code, 1);
    assert!(stderr.contains("uppercase"), "{stderr}");
}

#[test]
fn commit_msg_accepts_a_non_ascii_capital() {
    let repo = Repo::new();
    let (code, _, stderr) = repo.check_message("Šablona pro zprávu\n");
    assert_eq!(code, 0, "{stderr}");
}

#[test]
fn commit_msg_rejects_a_trailing_period() {
    let repo = Repo::new();
    let (code, _, stderr) = repo.check_message("Add thing.\n");
    assert_eq!(code, 1);
    assert!(stderr.contains("period"), "{stderr}");
}

#[test]
fn commit_msg_rejects_an_empty_subject() {
    let repo = Repo::new();
    let (code, _, stderr) = repo.check_message("# nothing but a comment\n");
    assert_eq!(code, 1);
    assert!(stderr.contains("empty subject"), "{stderr}");
}

#[test]
fn commit_msg_ignores_comment_lines() {
    let repo = Repo::new();
    let long_comment = format!("# {}", "a comment git strips anyway ".repeat(5));
    let (code, _, stderr) = repo.check_message(&format!("Add thing\n\n{long_comment}\n"));
    assert_eq!(code, 0, "{stderr}");
}

#[test]
fn commit_msg_requires_a_blank_second_line() {
    let repo = Repo::new();
    let (code, _, stderr) = repo.check_message("Add thing\nno blank line\n");
    assert_eq!(code, 1);
    assert!(stderr.contains("line 2 must be blank"), "{stderr}");
}

#[test]
fn commit_msg_limits_the_subject_to_72_characters() {
    let repo = Repo::new();
    let (code, _, stderr) = repo.check_message(&format!("A{}\n", "x".repeat(71)));
    assert_eq!(code, 0, "72 characters is allowed: {stderr}");

    let (code, _, stderr) = repo.check_message(&format!("A{}\n", "x".repeat(72)));
    assert_eq!(code, 1);
    assert!(stderr.contains("73 chars, max 72"), "{stderr}");
}

#[test]
fn commit_msg_notes_a_subject_over_50_characters_without_rejecting_it() {
    let repo = Repo::new();
    let (code, stdout, stderr) = repo.check_message(&format!("A{}\n", "x".repeat(59)));
    assert_eq!(code, 0);
    assert!(stderr.contains("under 50 reads better"), "{stderr}");
    assert_eq!(stdout, "", "advice is a diagnostic, not the result");
}

#[test]
fn commit_msg_limits_body_lines_to_72_characters() {
    let repo = Repo::new();
    let (code, _, stderr) = repo.check_message(&format!("Add thing\n\n{}\n", "x".repeat(73)));
    assert_eq!(code, 1);
    assert!(
        stderr.contains("body line 3 is 73 chars, max 72"),
        "{stderr}"
    );
}

#[test]
fn commit_msg_exempts_urls_trailers_and_indented_lines_from_the_body_limit() {
    let repo = Repo::new();
    let long = "x".repeat(80);
    for line in [
        format!("See https://example.com/{long}"),
        format!("Co-Authored-By: {long}"),
        format!("    {long}"),
    ] {
        let (code, _, stderr) = repo.check_message(&format!("Add thing\n\n{line}\n"));
        assert_eq!(code, 0, "{line}: {stderr}");
    }
}

#[test]
fn commit_msg_counts_characters_not_bytes() {
    let repo = Repo::new();
    let (code, _, stderr) = repo.check_message(&format!("Add thing\n\n{}\n", "—".repeat(40)));
    assert_eq!(code, 0, "40 characters is 120 bytes: {stderr}");

    let (code, _, stderr) = repo.check_message(&format!("Add thing\n\n{}\n", "—".repeat(73)));
    assert_eq!(code, 1);
    assert!(stderr.contains("73 chars"), "{stderr}");
}

#[test]
fn commit_msg_exempts_merge_fixup_squash_and_revert() {
    let repo = Repo::new();
    for subject in [
        "Merge branch 'x'.",
        "fixup! add thing.",
        "squash! add thing.",
        "Revert \"Add thing\".",
    ] {
        let (code, _, stderr) = repo.check_message(&format!("{subject}\n"));
        assert_eq!(code, 0, "{subject}: {stderr}");
    }
}

#[test]
fn commit_msg_honours_the_configured_limits() {
    let repo = Repo::new();
    repo.set_config("commit.subjectMax", "20");
    let (code, _, stderr) = repo.check_message(&format!("A{}\n", "x".repeat(20)));
    assert_eq!(code, 1);
    assert!(stderr.contains("max 20"), "{stderr}");

    repo.set_config("commit.bodyMax", "30");
    let (code, _, stderr) = repo.check_message(&format!("Add thing\n\n{}\n", "x".repeat(31)));
    assert_eq!(code, 1);
    assert!(stderr.contains("max 30"), "{stderr}");
}

#[test]
fn commit_msg_fails_loudly_on_a_limit_that_is_not_a_number() {
    let repo = Repo::new();
    repo.set_config("commit.subjectMax", "many");
    let (code, _, stderr) = repo.check_message("Add thing\n");
    assert_eq!(code, 1);
    assert!(stderr.contains("commit.subjectMax"), "{stderr}");
}

#[test]
fn commit_msg_enforces_a_project_pattern_and_drops_the_uppercase_rule() {
    let repo = Repo::new();
    repo.set_config("mr.commitPattern", PROJECT_PATTERN);

    let (code, _, stderr) = repo.check_message("feat(AB-12): add thing\n");
    assert_eq!(code, 0, "{stderr}");

    let (code, _, stderr) = repo.check_message("Add thing\n");
    assert_eq!(code, 1);
    assert!(
        stderr.contains("project requires subject matching"),
        "{stderr}"
    );
}

#[test]
fn commit_msg_fails_loudly_on_a_pattern_that_does_not_compile() {
    let repo = Repo::new();
    repo.set_config("mr.commitPattern", "(");
    let (code, _, stderr) = repo.check_message("Add thing\n");
    assert_eq!(code, 1);
    assert!(stderr.contains("mr.commitPattern"), "{stderr}");
}

#[test]
fn commit_msg_reports_a_message_file_it_cannot_read() {
    let repo = Repo::new();
    let (code, _, stderr) = repo.gk(&["hook", "commit-msg", "no-such-file"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("could not read"), "{stderr}");
}

#[test]
fn commit_msg_speaks_the_json_envelope() {
    let repo = Repo::new();
    let file = repo.path().join(".git/MESSAGE_UNDER_TEST");
    let file = file.to_str().expect("utf-8 path");

    std::fs::write(file, "Add thing\n").expect("write");
    let (code, data) = repo.data_with_code(&["hook", "commit-msg", file]);
    assert_eq!(code, 0);
    assert_eq!(data["notes"], serde_json::json!([]));

    std::fs::write(file, "add thing\n").expect("write");
    let (code, _, stderr) = repo.gk(&["hook", "commit-msg", file, "--json"]);
    assert_eq!(code, 1);
    let envelope: Value = serde_json::from_str(&stderr).expect("error envelope on stderr");
    assert_eq!(envelope["status"], "error");
}

#[test]
fn install_writes_an_executable_shim_that_git_runs() {
    use std::os::unix::fs::PermissionsExt;
    let repo = Repo::new();
    let (code, stdout, stderr) = repo.gk(&["hook", "install"]);
    assert_eq!(code, 0, "{stdout}{stderr}");

    let hook = repo.hook_file();
    let mode = std::fs::metadata(&hook)
        .expect("hook exists")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o755);
    let text = std::fs::read_to_string(&hook).expect("read hook");
    assert!(
        text.starts_with("#!/bin/sh\n# gk-hook: commit-msg\n"),
        "{text}"
    );

    let bad = repo.git_hooked(&[
        "commit",
        "--allow-empty",
        "-q",
        "-m",
        "lowercase with period.",
    ]);
    assert!(!bad.status.success(), "git accepted a bad subject");
    assert!(
        String::from_utf8_lossy(&bad.stderr).contains("uppercase"),
        "{}",
        String::from_utf8_lossy(&bad.stderr)
    );

    let good = repo.git_hooked(&["commit", "--allow-empty", "-q", "-m", "Add a thing"]);
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stderr)
    );
}

#[test]
fn install_reports_json_and_a_second_run_changes_nothing() {
    let repo = Repo::new();
    let (code, data) = repo.data_with_code(&["hook", "install"]);
    assert_eq!(code, 0);
    assert_eq!(data["hooks"][0]["name"], "commit-msg");
    assert_eq!(data["hooks"][0]["status"], "installed");
    assert!(
        data["directory"]
            .as_str()
            .expect("directory")
            .ends_with(".git/hooks"),
        "{data}"
    );
    let before = std::fs::read(repo.hook_file()).expect("read");

    let (code, data) = repo.data_with_code(&["hook", "install"]);
    assert_eq!(code, 0);
    assert_eq!(data["hooks"][0]["status"], "unchanged");
    assert_eq!(std::fs::read(repo.hook_file()).expect("read"), before);
}

#[test]
fn install_leaves_a_hook_that_is_not_gks_alone_unless_forced() {
    let repo = Repo::new();
    let mine = "#!/bin/sh\n# my own check\nexit 0\n";
    std::fs::create_dir_all(repo.path().join(".git/hooks")).expect("hooks dir");
    std::fs::write(repo.hook_file(), mine).expect("write hook");

    let (code, data) = repo.data_with_code(&["hook", "install"]);
    assert_eq!(code, 1);
    assert_eq!(data["hooks"][0]["status"], "conflict");
    assert_eq!(
        std::fs::read_to_string(repo.hook_file()).expect("read"),
        mine
    );

    let (code, data) = repo.data_with_code(&["hook", "install", "--force"]);
    assert_eq!(code, 0);
    assert_eq!(data["hooks"][0]["status"], "overwritten");
    assert!(std::fs::read_to_string(repo.hook_file())
        .expect("read")
        .contains("# gk-hook: commit-msg"));
}

#[test]
fn install_rewrites_its_own_stale_shim_without_force() {
    let repo = Repo::new();
    std::fs::create_dir_all(repo.path().join(".git/hooks")).expect("hooks dir");
    std::fs::write(
        repo.hook_file(),
        "#!/bin/sh\n# gk-hook: commit-msg\nexec an-older-gk \"$@\"\n",
    )
    .expect("write hook");

    let (code, data) = repo.data_with_code(&["hook", "install"]);
    assert_eq!(code, 0);
    assert_eq!(data["hooks"][0]["status"], "overwritten");
    assert!(std::fs::read_to_string(repo.hook_file())
        .expect("read")
        .contains("exec gk hook commit-msg"));
}

#[test]
fn install_replaces_a_symlink_only_when_forced_and_never_writes_through_it() {
    let repo = Repo::new();
    let elsewhere = TempDir::new().expect("temp dir");
    let target = elsewhere.path().join("precious");
    std::fs::write(&target, "precious\n").expect("write target");
    std::fs::create_dir_all(repo.path().join(".git/hooks")).expect("hooks dir");
    std::os::unix::fs::symlink(&target, repo.hook_file()).expect("symlink");

    let (code, data) = repo.data_with_code(&["hook", "install"]);
    assert_eq!(code, 1);
    assert_eq!(data["hooks"][0]["status"], "conflict");
    assert!(std::fs::symlink_metadata(repo.hook_file())
        .expect("stat")
        .file_type()
        .is_symlink());

    let (code, data) = repo.data_with_code(&["hook", "install", "--force"]);
    assert_eq!(code, 0);
    assert_eq!(data["hooks"][0]["status"], "overwritten");
    assert!(
        !std::fs::symlink_metadata(repo.hook_file())
            .expect("stat")
            .file_type()
            .is_symlink(),
        "the link should be replaced by a regular file"
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("read target"),
        "precious\n",
        "what the link pointed at must not be written"
    );
}

#[test]
fn a_limit_given_with_git_dash_c_reaches_the_hook() {
    let repo = Repo::new();
    repo.gk(&["hook", "install"]);

    let output = repo.git_hooked(&[
        "-c",
        "commit.subjectMax=10",
        "commit",
        "--allow-empty",
        "-q",
        "-m",
        "A subject longer than ten",
    ]);
    assert!(!output.status.success(), "git -c was ignored by the hook");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("max 10"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn install_honours_a_global_hooks_path_that_git_honours() {
    let repo = Repo::new();
    let home = TempDir::new().expect("temp dir");
    let global = home.path().join("gitconfig");
    std::fs::write(&global, "[core]\n\thooksPath = shared-hooks\n").expect("write config");

    let output = Command::new(env!("CARGO_BIN_EXE_gk"))
        .args(["hook", "install"])
        .current_dir(repo.path())
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("run gk");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("core.hooksPath"));
    assert!(!repo.hook_file().exists());
}

#[test]
fn install_ignores_a_home_gitconfig_that_git_is_told_to_ignore() {
    let repo = Repo::new();
    let home = TempDir::new().expect("temp dir");
    std::fs::write(
        home.path().join(".gitconfig"),
        "[core]\n\thooksPath = shared-hooks\n",
    )
    .expect("write config");

    let output = Command::new(env!("CARGO_BIN_EXE_gk"))
        .args(["hook", "install"])
        .current_dir(repo.path())
        .env("HOME", home.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("run gk");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(repo.hook_file().exists());
}

#[test]
fn a_sha256_repository_installs_and_checks_messages() {
    let dir = TempDir::new().expect("temp dir");
    let init = Command::new("git")
        .args(["init", "-q", "--object-format=sha256"])
        .current_dir(dir.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("run git");
    if !init.status.success() {
        return; // a git too old to make sha256 repositories
    }

    let (code, stdout, stderr) = gk_isolated(dir.path(), &["hook", "install"]);
    assert_eq!(code, 0, "{stdout}{stderr}");

    let message = dir.path().join(".git/MESSAGE_UNDER_TEST");
    std::fs::write(&message, "add thing.\n").expect("write message");
    let (code, _, stderr) = gk_isolated(
        dir.path(),
        &["hook", "commit-msg", message.to_str().expect("utf-8 path")],
    );
    assert_eq!(code, 1);
    assert!(stderr.contains("uppercase"), "{stderr}");
}

#[test]
fn install_repairs_a_shim_that_lost_its_executable_bit() {
    use std::os::unix::fs::PermissionsExt;
    let repo = Repo::new();
    repo.gk(&["hook", "install"]);
    std::fs::set_permissions(repo.hook_file(), std::fs::Permissions::from_mode(0o644))
        .expect("chmod");

    let (code, data) = repo.data_with_code(&["hook", "install"]);
    assert_eq!(code, 0);
    assert_eq!(data["hooks"][0]["status"], "overwritten");
    let mode = std::fs::metadata(repo.hook_file())
        .expect("stat")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o755);
}

#[test]
fn install_reports_a_hook_path_it_cannot_read_and_leaves_it_alone() {
    let repo = Repo::new();
    std::fs::create_dir_all(repo.hook_file()).expect("a directory where the hook goes");

    let (code, _, stderr) = repo.gk(&["hook", "install"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("could not read"), "{stderr}");
    assert!(repo.hook_file().is_dir());
}

#[test]
fn install_refuses_when_core_hooks_path_is_set() {
    let repo = Repo::new();
    repo.set_config("core.hooksPath", "shared-hooks");

    let (code, _, stderr) = repo.gk(&["hook", "install"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("core.hooksPath"), "{stderr}");
    assert!(!repo.hook_file().exists());
    assert!(!repo.path().join("shared-hooks").exists());
}

#[test]
fn install_from_a_linked_worktree_writes_the_shared_hooks_directory() {
    let repo = Repo::new();
    repo.write("a.txt", "x\n");
    repo.commit("base");
    let elsewhere = TempDir::new().expect("temp dir");
    let tree = elsewhere.path().join("tree");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        tree.to_str().expect("utf-8"),
        "-b",
        "side",
    ]);

    let (code, stdout, stderr) = gk_isolated(&tree, &["hook", "install"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(repo.hook_file().exists(), "the main repo's hooks directory");
}

#[test]
fn install_outside_a_repository_is_an_expected_failure() {
    let dir = TempDir::new().expect("temp dir");
    let (code, _, stderr) = gk_isolated(dir.path(), &["hook", "install"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("error"), "{stderr}");
}

#[test]
fn uninstall_removes_a_shim_and_keeps_a_file_that_is_not_gks() {
    let repo = Repo::new();
    repo.gk(&["hook", "install"]);

    let (code, data) = repo.data_with_code(&["hook", "install", "--uninstall"]);
    assert_eq!(code, 0);
    assert_eq!(data["hooks"][0]["status"], "removed");
    assert!(!repo.hook_file().exists());

    let (code, data) = repo.data_with_code(&["hook", "install", "--uninstall"]);
    assert_eq!(code, 0, "nothing there is not a failure");
    assert_eq!(data["hooks"][0]["status"], "missing");

    let mine = "#!/bin/sh\nexit 0\n";
    std::fs::write(repo.hook_file(), mine).expect("write hook");
    let (code, data) = repo.data_with_code(&["hook", "install", "--uninstall"]);
    assert_eq!(code, 1);
    assert_eq!(data["hooks"][0]["status"], "kept");
    assert_eq!(
        std::fs::read_to_string(repo.hook_file()).expect("read"),
        mine
    );
}

#[test]
fn uninstall_still_cleans_the_default_directory_when_core_hooks_path_is_set() {
    let repo = Repo::new();
    repo.gk(&["hook", "install"]);
    repo.set_config("core.hooksPath", "shared-hooks");

    let (code, data) = repo.data_with_code(&["hook", "install", "--uninstall"]);
    assert_eq!(code, 0);
    assert_eq!(data["hooks"][0]["status"], "removed");
    assert!(!repo.hook_file().exists());
}

#[test]
fn force_with_uninstall_is_misuse() {
    let repo = Repo::new();
    let (code, _, _) = repo.gk(&["hook", "install", "--uninstall", "--force"]);
    assert_eq!(code, 2);
}

#[test]
fn the_installed_shim_says_so_when_gk_is_not_on_path() {
    let repo = Repo::new();
    repo.gk(&["hook", "install"]);
    let message = repo.path().join(".git/MESSAGE_UNDER_TEST");
    std::fs::write(&message, "Add thing\n").expect("write message");

    let output = Command::new("/bin/sh")
        .arg(repo.hook_file())
        .arg(&message)
        .env("PATH", "/nonexistent")
        .output()
        .expect("run shim");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("gk is not on PATH"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_old_commit_msg_script_only_forwards_to_gk() {
    let repo = Repo::new();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../hooks/commit-msg.sh");
    let message = repo.path().join(".git/MESSAGE_UNDER_TEST");
    let run = |text: &str, path: std::ffi::OsString| {
        std::fs::write(&message, text).expect("write message");
        Command::new("/bin/bash")
            .arg(&script)
            .arg(&message)
            .current_dir(repo.path())
            .env("PATH", path)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("run script")
    };

    let bad = run("add thing.\n", path_with_gk());
    assert_eq!(bad.status.code(), Some(1), "gk enforces through the script");

    let no_gk = TempDir::new().expect("temp dir");
    let without_gk = run("Add thing\n", no_gk.path().into());
    assert!(
        !without_gk.status.success(),
        "with no gk on PATH the script has nothing to enforce with, and must say so"
    );
}
