//! End-to-end tests: a real git repository, the real binary, real exit codes.
//!
//! The unit tests cover classification logic on synthetic input. These cover
//! everything between the command line and that logic — argument handling,
//! git invocation, post-image lookup, the JSON envelope, and exit codes.

use serde_json::Value;
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
        let output = Command::new(env!("CARGO_BIN_EXE_gk"))
            .args(args)
            .current_dir(self.path())
            .output()
            .expect("run gk");
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }

    /// Run `gk --json` and unwrap the success envelope.
    fn data(&self, args: &[&str]) -> Value {
        let mut with_json = args.to_vec();
        with_json.push("--json");
        let (code, stdout, stderr) = self.gk(&with_json);
        assert_eq!(code, 0, "gk {args:?} failed: {stderr}");
        let value: Value = serde_json::from_str(&stdout).expect("stdout is json");
        assert_eq!(value["status"], "ok");
        value["data"].clone()
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
    let (code, _, stderr) = repo.gk(&["doc", "--files", "nope", "--json"]);

    assert_eq!(code, 1, "expected failure, not misuse");
    let value: Value = serde_json::from_str(&stderr).expect("error envelope on stderr");
    assert_eq!(value["status"], "error");
    assert!(
        value["message"]
            .as_str()
            .expect("message")
            .contains("no such path"),
        "got: {}",
        value["message"]
    );
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
    let (code, _, stderr) = repo.gk(&["init", "--json"]);

    assert_eq!(code, 1, "expected refusal, not misuse");
    let value: Value = serde_json::from_str(&stderr).expect("error envelope on stderr");
    assert_eq!(value["status"], "error");
    let message = value["message"].as_str().expect("message");
    assert!(message.contains("--claude"), "got: {message}");
    assert!(message.contains("--codex"), "got: {message}");
}

#[test]
fn init_experimental_alone_is_still_a_refusal_no_implicit_target() {
    let repo = Repo::new();
    let (code, _, stderr) = repo.gk(&["init", "--experimental=mattpocock", "--json"]);

    assert_eq!(code, 1, "expected refusal, not a guess at which target");
    let value: Value = serde_json::from_str(&stderr).expect("error envelope on stderr");
    assert_eq!(value["status"], "error");
}

#[test]
fn init_claude_reports_a_conflict_and_leaves_the_file_untouched() {
    let repo = Repo::new();
    repo.data(&["init", "--claude"]);
    repo.write(".claude/skills/gist-outline/SKILL.md", "locally modified\n");

    let (code, stdout, _) = repo.gk(&["init", "--claude", "--json"]);
    assert_eq!(code, 1);
    let value: Value = serde_json::from_str(&stdout).expect("stdout is json");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["data"]["totals"]["conflicts"], 1);

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
    let (code, _, stderr) = repo.gk(&["init", "--claude", "--experimental=nope", "--json"]);

    assert_eq!(code, 1, "expected refusal, not misuse");
    let value: Value = serde_json::from_str(&stderr).expect("error envelope on stderr");
    assert_eq!(value["status"], "error");
    let message = value["message"].as_str().expect("message");
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
