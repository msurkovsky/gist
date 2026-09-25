//! Cases: docs/cases/repository-checks.md. All mutations are in temporary repos.

use serde_json::Value;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

struct Fixture(TempDir);

impl Fixture {
    fn new() -> Self {
        let fixture = Self(TempDir::new().unwrap());
        fixture.git(&["init", "-q", "-b", "main"]);
        fixture.write("scripts/vendors.conf", "# name url branch\n");
        fixture.write("scripts/example.sh", "#!/bin/sh\ntrue\n");
        fixture.write("hooks/example.sh", "#!/bin/sh\ntrue\n");
        fixture.write("README.md", "`gist-demo`\n");
        fixture.write(
            "skills/gist-demo/SKILL.md",
            "---\nname: gist-demo\ndescription: A demonstration\n---\nDo the task.\n",
        );
        fixture.write("docs/cases/gist-demo.md", "# Demo cases\n");
        fixture.write("docs/adr/0001-demo.md", "# Decision\n\n## Changelog\n\n| When | Who | Why |\n|---|---|---|\n| 2026-09-25 12:00 | Test | Created |\n");
        fixture.commit("Base");
        fixture
    }

    fn path(&self) -> &Path {
        self.0.path()
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
            ])
            .args(args)
            .current_dir(self.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-qm", message]);
    }

    fn check(&self, args: &[&str]) -> (i32, Value) {
        let out = Command::new(env!("CARGO_BIN_EXE_gk"))
            .args(["check", "--json"])
            .args(args)
            .current_dir(self.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        let code = out.status.code().unwrap();
        let bytes = if out.stdout.is_empty() {
            &out.stderr
        } else {
            &out.stdout
        };
        (
            code,
            serde_json::from_slice(bytes)
                .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(bytes))),
        )
    }

    fn with_vendor() -> Self {
        let fixture = Self::new();
        fixture.write(
            "scripts/vendors.conf",
            "demo https://example.invalid/demo main\n",
        );
        fixture.commit("Register demo");
        fixture.git(&["switch", "-qc", "upstream"]);
        fixture.write("experimental/demo/file.md", "upstream content\n");
        fixture.commit("Upstream change");
        fixture.git(&["switch", "-q", "main"]);
        fixture.import("upstream");
        fixture
    }

    fn import(&self, branch: &str) {
        let sha = self.git(&["rev-parse", branch]);
        self.git(&["merge", "--no-ff", "-qm", &format!("Import demo\n\nUpstream: https://example.invalid/demo@{sha}\n\nFilter: :prefix=experimental/demo"), branch]);
    }
}

// Case: docs/cases/repository-checks.md#check-metadata
#[test]
fn check_accepts_valid_yaml_and_rejects_malformed_or_wrongly_typed_metadata() {
    let fixture = Fixture::new();
    fixture.write(
        "skills/gist-demo/SKILL.md",
        "---\nname: 'gist-demo'\ndescription: |\n  A description: with a colon\n---\nDo it.\n",
    );
    assert_eq!(fixture.check(&[]).0, 0);
    for text in [
        "---\nname: gist-demo\ndescription: unclosed\n",
        "---\nname: gist-demo\ndescription: invalid: YAML\n---\n",
        "---\nname: gist-demo\ndescription: 42\n---\n",
        "---\nname: gist-demo\ndescription: ''\n---\n",
        "---\nname: gist-demo\n---\n",
        "---\nname: gist-demo\nname: gist-demo\ndescription: duplicate\n---\n",
        "---\nname: gist-demo\ndescription: x\ndisable-model-invocation: 'true'\n---\n",
        "---\nname: wrong\ndescription: x\n---\n",
    ] {
        fixture.write("skills/gist-demo/SKILL.md", text);
        let (code, report) = fixture.check(&[]);
        assert_eq!(code, 1, "accepted {text:?}: {report}");
        assert!(report["data"]["failures"].as_u64().unwrap() > 0);
    }
}

// Case: docs/cases/repository-checks.md#check-metadata
#[test]
fn check_requires_consistent_invocation_policies() {
    let fixture = Fixture::new();
    fixture.write(
        "skills/gist-demo/SKILL.md",
        "---\nname: gist-demo\ndescription: x\ndisable-model-invocation: true\n---\n",
    );
    assert_eq!(fixture.check(&[]).0, 1);
    fixture.write(
        "skills/gist-demo/agents/openai.yaml",
        "policy:\n  allow_implicit_invocation: false\n",
    );
    assert_eq!(fixture.check(&[]).0, 0);
    fixture.write(
        "skills/gist-demo/agents/openai.yaml",
        "policy:\n  allow_implicit_invocation: 'false'\n",
    );
    assert_eq!(fixture.check(&[]).0, 1);
    fixture.write("skills/gist-demo/agents/openai.yaml", "policy: true\n");
    assert_eq!(fixture.check(&[]).0, 1);
}

// Case: docs/cases/repository-checks.md#check-resources
#[test]
fn check_resolves_resources_from_each_document_and_refuses_missing_or_escaping_paths() {
    let fixture = Fixture::new();
    fixture.write("skills/gist-demo/SKILL.md", "---\nname: gist-demo\ndescription: x\n---\n[Guide](references/guide.md)\n[External](https://example.invalid)\n[Here](#here)\n```md\n[Example](missing-example.md)\n```\n");
    assert_eq!(fixture.check(&[]).0, 1);
    fixture.write(
        "skills/gist-demo/references/guide.md",
        "[Data](data.json)\n",
    );
    fixture.write("skills/gist-demo/references/data.json", "{}\n");
    assert_eq!(fixture.check(&[]).0, 0);
    fixture.write(
        "skills/gist-demo/references/guide.md",
        "[Outside](../../../README.md)\n",
    );
    let (code, report) = fixture.check(&[]);
    assert_eq!(code, 1);
    assert!(report.to_string().contains("escapes skill"));
}

// Case: docs/cases/repository-checks.md#check-repository
#[test]
fn check_reports_convention_failures_even_when_details_are_truncated() {
    let fixture = Fixture::new();
    assert_eq!(fixture.check(&[]).0, 0);
    fixture.write("README.md", "omitted\n");
    fixture.write("scripts/example.sh", "if\n");
    fixture.write("docs/adr/0001-demo.md", "# Decision\n");
    std::fs::remove_file(fixture.path().join("docs/cases/gist-demo.md")).unwrap();
    let (code, report) = fixture.check(&["--limit", "0"]);
    assert_eq!(code, 1);
    assert_eq!(report["data"]["failures"], 4);
    assert_eq!(report["data"]["truncated"], true);
    assert!(report["data"]["errors"].as_array().unwrap().is_empty());
    let out = Command::new(env!("CARGO_BIN_EXE_gk"))
        .arg("check")
        .current_dir(fixture.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8(out.stdout).unwrap().contains("FAIL"));
}

// Case: docs/cases/repository-checks.md#check-vendor-tree
#[test]
fn check_detects_vendor_edits_through_an_ordinary_merge() {
    let fixture = Fixture::with_vendor();
    assert_eq!(fixture.check(&["--vendors-only"]).0, 0);
    fixture.git(&["switch", "-qc", "feature"]);
    fixture.write("experimental/demo/file.md", "local edit\n");
    fixture.commit("Edit vendor");
    fixture.git(&["switch", "-q", "main"]);
    fixture.git(&["merge", "--no-ff", "-qm", "Merge feature", "feature"]);
    let (code, report) = fixture.check(&["--vendors-only"]);
    assert_eq!(code, 1);
    assert!(report.to_string().contains("differs from imported content"));
}

// Case: docs/cases/repository-checks.md#check-vendor-tree
#[test]
fn check_accepts_an_updated_import_merged_from_a_feature_branch() {
    let fixture = Fixture::with_vendor();
    fixture.git(&["switch", "-q", "upstream"]);
    fixture.write("experimental/demo/file.md", "new upstream content\n");
    fixture.commit("Upstream update");
    fixture.git(&["switch", "-qc", "feature", "main"]);
    fixture.import("upstream");
    fixture.git(&["switch", "-q", "main"]);
    fixture.git(&["merge", "--no-ff", "-qm", "Merge update", "feature"]);
    assert_eq!(fixture.check(&["--vendors-only"]).0, 0);
}

// Case: docs/cases/repository-checks.md#check-vendor-worktree
#[test]
fn check_detects_all_vendor_worktree_states_including_index_only_changes() {
    for state in ["unstaged", "staged", "untracked", "index-only"] {
        let fixture = Fixture::with_vendor();
        match state {
            "untracked" => fixture.write("experimental/demo/extra.md", "extra\n"),
            _ => {
                fixture.write("experimental/demo/file.md", "local edit\n");
                if state != "unstaged" {
                    fixture.git(&["add", "experimental"]);
                }
                if state == "index-only" {
                    fixture.write("experimental/demo/file.md", "upstream content\n");
                }
            }
        }
        assert_eq!(fixture.check(&["--vendors-only"]).0, 1, "missed {state}");
    }
}

// Case: docs/cases/repository-checks.md#check-vendor-tree
#[test]
fn review_fix_vendor_registry_allows_an_omitted_default_branch() {
    let fixture = Fixture::with_vendor();
    fixture.write(
        "scripts/vendors.conf",
        "demo https://example.invalid/demo\n",
    );
    assert_eq!(fixture.check(&["--vendors-only"]).0, 0);
    for invalid in ["demo\n", "demo https://example.invalid/demo main extra\n"] {
        fixture.write("scripts/vendors.conf", invalid);
        assert_eq!(fixture.check(&["--vendors-only"]).0, 1);
    }
}

// Case: docs/cases/repository-checks.md#check-vendor-worktree
#[test]
fn review_fix_ignored_vendor_files_do_not_hide_tracked_or_untracked_changes() {
    let fixture = Fixture::with_vendor();
    fixture.write(".gitignore", "*.swp\nbuild/\nfile.md\n");
    fixture.commit("Ignore local artifacts");
    fixture.write("experimental/demo/.file.md.swp", "editor state\n");
    fixture.write("experimental/.root.swp", "editor state\n");
    fixture.write("experimental/build/output", "ignored directory\n");
    assert_eq!(fixture.check(&["--vendors-only"]).0, 0);
    fixture.write(
        "experimental/demo/file.md",
        "tracked edit despite ignore pattern\n",
    );
    assert_eq!(fixture.check(&["--vendors-only"]).0, 1);
    fixture.write("experimental/demo/file.md", "upstream content\n");
    fixture.write("experimental/demo/new.md", "untracked\n");
    assert_eq!(fixture.check(&["--vendors-only"]).0, 1);
}

// Case: docs/cases/repository-checks.md#check-resources
#[test]
fn review_fix_inline_code_is_not_a_resource_link() {
    let fixture = Fixture::new();
    let header = "---\nname: gist-demo\ndescription: x\n---\n";
    fixture.write("skills/gist-demo/SKILL.md", &format!("{header}Example: `[x](missing.md)` and ``[x](also`missing.md)``.\n[Real `label`](exists.md)\n"));
    fixture.write("skills/gist-demo/exists.md", "present\n");
    assert_eq!(fixture.check(&[]).0, 0);
    fixture.write(
        "skills/gist-demo/SKILL.md",
        &format!("{header}Example: `[x](missing.md)`; actual [x](missing.md).\n"),
    );
    assert_eq!(fixture.check(&[]).0, 1);
}

// Case: docs/cases/repository-checks.md#check-resources
#[test]
fn review_fix_percent_encoded_resource_paths_are_decoded_before_lookup() {
    let fixture = Fixture::new();
    let header = "---\nname: gist-demo\ndescription: x\n---\n";
    fixture.write(
        "skills/gist-demo/SKILL.md",
        &format!(
            "{header}[Space](my%20file.md) [Hash](hash%23file.md#section) [UTF8](caf%C3%A9.md)\n"
        ),
    );
    for file in ["my file.md", "hash#file.md", "café.md"] {
        fixture.write(&format!("skills/gist-demo/{file}"), "present\n");
    }
    assert_eq!(fixture.check(&[]).0, 0);
    fixture.write(
        "skills/gist-demo/SKILL.md",
        &format!("{header}[Escape](%2e%2e/%2e%2e/README.md)\n"),
    );
    let (code, report) = fixture.check(&[]);
    assert_eq!(code, 1);
    assert!(report.to_string().contains("escapes skill"));
}

// Case: docs/cases/repository-checks.md#check-vendor-tree
#[test]
fn check_refuses_missing_import_history_and_unregistered_vendor_content() {
    let fixture = Fixture::new();
    fixture.write("experimental/unknown/file.md", "unregistered\n");
    assert_eq!(fixture.check(&["--vendors-only"]).0, 1);
    fixture.commit("Commit unregistered content");
    assert_eq!(fixture.check(&["--vendors-only"]).0, 1);
    fixture.write(
        "scripts/vendors.conf",
        "demo https://example.invalid/demo main\n",
    );
    let (code, report) = fixture.check(&["--vendors-only"]);
    assert_eq!(code, 1);
    assert_eq!(report["status"], "error");
    assert!(report["message"]
        .as_str()
        .unwrap()
        .contains("no reachable import"));
}
