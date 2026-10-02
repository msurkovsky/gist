//! End-to-end tests of `gk md-review`: a real git repository, a real `serve`
//! process, the clients run as the agent runs them, and HTTP requests made
//! as the review page makes them. Cases: docs/cases/gist-md-review.md.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const TEXT: &str = "# Top\n\nFirst paragraph here.\n\n## Sub\n\nSecond one.\n";

struct Repo {
    dir: TempDir,
}

impl Repo {
    fn new() -> Self {
        let repo = Self {
            dir: TempDir::new().expect("temp dir"),
        };
        let status = git(repo.path()).args(["init", "-q"]).status().unwrap();
        assert!(status.success());
        repo.write("docs/foo.md", TEXT);
        repo
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_gk"));
        command
            .args(args)
            .current_dir(self.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        command
    }

    /// Run `gk` to completion: exit code, stdout, stderr.
    fn gk(&self, args: &[&str]) -> (i32, String, String) {
        let output = self.command(args).output().expect("run gk");
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }

    /// Run `gk --json` expecting success; the envelope's data.
    fn data(&self, args: &[&str]) -> Value {
        let mut args = args.to_vec();
        args.push("--json");
        let (code, stdout, stderr) = self.gk(&args);
        assert_eq!(code, 0, "gk {args:?}: {stderr}");
        let value: Value = serde_json::from_str(&stdout).expect("json on stdout");
        assert_eq!(value["status"], "ok");
        value["data"].clone()
    }

    /// Run `gk --json` expecting failure: the exit code and the message.
    fn error(&self, args: &[&str]) -> (i32, String) {
        let mut args = args.to_vec();
        args.push("--json");
        let (code, _, stderr) = self.gk(&args);
        let value: Value = serde_json::from_str(&stderr).expect("error envelope on stderr");
        assert_eq!(value["status"], "error");
        (code, value["message"].as_str().unwrap().to_string())
    }

    fn store(&self) -> PathBuf {
        let reviews = self.path().join(".md-review/reviews");
        let mut dirs: Vec<_> = std::fs::read_dir(&reviews)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(dirs.len(), 1, "one store under {}", reviews.display());
        dirs.remove(0)
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.store().join("events.jsonl")).unwrap()
    }

    /// Start `serve` and read its first line.
    fn serve(&self, file: &str) -> Server {
        let mut child = self
            .command(&["md-review", "serve", file])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn serve");
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let url = line.trim().to_string();
        assert!(url.starts_with("http://127.0.0.1:"), "first line: {line:?}");
        let port = url["http://127.0.0.1:".len()..]
            .split('/')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let token = url.split("token=").nth(1).unwrap().to_string();
        Server {
            child,
            stdout,
            url,
            port,
            token,
        }
    }
}

fn git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    command
}

/// A running `serve`, killed when dropped.
struct Server {
    child: Child,
    stdout: BufReader<ChildStdout>,
    url: String,
    port: u16,
    token: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    /// A request as the page makes it: status and JSON body.
    fn page(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .proxy(None)
            .build()
            .into();
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let auth = format!("Bearer {}", self.token);
        let response = match (method, body) {
            ("GET", _) => agent.get(&url).header("Authorization", &auth).call(),
            ("PATCH", Some(body)) => agent
                .patch(&url)
                .header("Authorization", &auth)
                .send_json(body),
            ("POST", Some(body)) => agent
                .post(&url)
                .header("Authorization", &auth)
                .send_json(body),
            _ => agent.post(&url).header("Authorization", &auth).send_empty(),
        };
        let mut response = response.unwrap_or_else(|err| panic!("{method} {path}: {err}"));
        let status = response.status().as_u16();
        let value = response.body_mut().read_json().unwrap_or(Value::Null);
        (status, value)
    }

    fn comment(&self, round: u32, message: &str, quote: &str, block: u32, line: u32) -> Value {
        let (status, body) = self.page(
            "POST",
            "/api/threads",
            Some(json!({
                "round": round, "version": round, "message": message,
                "kind": "comment", "body": format!("About {quote}."),
                "anchor": {
                    "quote": quote, "prefix": "", "suffix": "", "blocks": [block],
                    "lines": [line, line], "headings": ["Top"], "version": round
                }
            })),
        );
        assert_eq!(status, 200, "{body}");
        body
    }

    fn submit(&self, round: u32, summary: Option<&str>) {
        let (status, body) = self.page(
            "POST",
            "/api/submit",
            Some(json!({ "round": round, "summary": summary })),
        );
        assert_eq!(status, 200, "{body}");
    }

    fn approve(&self, round: u32, note: &str) -> Value {
        let (status, body) = self.page(
            "POST",
            "/api/approve",
            Some(json!({ "round": round, "version": round, "note": note })),
        );
        assert_eq!(status, 200, "{body}");
        body
    }

    /// Wait for the process to exit; its exit code.
    fn exit(&mut self, within: Duration) -> i32 {
        let deadline = Instant::now() + within;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.code().unwrap_or(-1);
            }
            assert!(Instant::now() < deadline, "serve still running");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Everything `serve` printed after its first line, once it exited.
    fn rest_of_stdout(&mut self) -> String {
        let mut rest = String::new();
        std::io::Read::read_to_string(&mut self.stdout, &mut rest).unwrap();
        rest
    }
}

fn server_json(repo: &Repo) -> Value {
    serde_json::from_slice(&std::fs::read(repo.store().join("server.json")).unwrap()).unwrap()
}

/// Block until a `wait` is blocked on the server.
fn until_waiting(repo: &Repo) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while repo.data(&["md-review", "status", "docs/foo.md"])["waiters"] != 1 {
        assert!(Instant::now() < deadline, "wait never connected");
        std::thread::sleep(Duration::from_millis(20));
    }
}

// Case: docs/cases/gist-md-review.md#serve-start
#[cfg(unix)]
#[test]
fn serve_starts_a_store_and_prints_only_the_url() {
    use std::os::unix::fs::PermissionsExt;
    let repo = Repo::new();
    let mut server = repo.serve("docs/foo.md");
    let store = repo.store();
    assert_eq!(std::fs::read_to_string(store.join("v1.md")).unwrap(), TEXT);
    let first: Value = serde_json::from_str(repo.log().lines().next().unwrap()).unwrap();
    assert_eq!(first["event"], "review_started");
    assert_eq!(first["format"], 1);
    assert_eq!(first["file"], "docs/foo.md");
    let mode = std::fs::metadata(store.join("server.json"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    let info = server_json(&repo);
    assert_eq!(info["port"], server.port);
    assert_eq!(info["token"], server.token);
    assert_eq!(info["pid"], server.child.id());
    assert_eq!(info["version"], env!("CARGO_PKG_VERSION"));
    let exclude = std::fs::read_to_string(repo.path().join(".git/info/exclude")).unwrap();
    assert!(exclude.lines().any(|line| line == ".md-review/"));
    assert!(!repo.path().join(".gitignore").exists());

    assert_eq!(repo.gk(&["md-review", "stop", "docs/foo.md"]).0, 0);
    assert_eq!(server.exit(Duration::from_secs(5)), 0);
    assert_eq!(server.rest_of_stdout(), "", "nothing follows the URL");
}

// Case: docs/cases/gist-md-review.md#serve-start
#[test]
fn serve_with_json_prints_its_result_on_one_line() {
    let repo = Repo::new();
    let mut child = repo
        .command(&["md-review", "serve", "docs/foo.md", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let _ = child.kill();
    let _ = child.wait();
    let value: Value = serde_json::from_str(&line).expect("one line of json");
    assert_eq!(value["data"]["file"], "docs/foo.md");
    assert_eq!(value["data"]["reused"], false);
    assert!(value["data"]["url"]
        .as_str()
        .unwrap()
        .starts_with("http://127.0.0.1:"));
}

// Case: docs/cases/gist-md-review.md#serve-reuse
#[test]
fn a_second_serve_prints_the_live_url_and_appends_nothing() {
    let repo = Repo::new();
    let server = repo.serve("docs/foo.md");
    let log = repo.log();
    let (code, stdout, _) = repo.gk(&["md-review", "serve", "docs/foo.md"]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), server.url);
    assert_eq!(repo.log(), log);
    assert_eq!(server_json(&repo)["pid"], server.child.id());
}

// Case: docs/cases/gist-md-review.md#serve-resume
#[test]
fn a_dead_server_is_replaced_and_the_review_resumes_on_its_port() {
    let repo = Repo::new();
    let mut server = repo.serve("docs/foo.md");
    server.comment(1, "m-1", "paragraph here", 1, 3);
    let port = server.port;
    server.child.kill().unwrap();
    server.exit(Duration::from_secs(5));

    let resumed = repo.serve("docs/foo.md");
    assert_eq!(resumed.port, port, "the old port was free");
    assert_eq!(resumed.token, server.token, "an open page reconnects");
    assert_eq!(server_json(&repo)["pid"], resumed.child.id());
    let (_, view) = resumed.page("GET", "/api/review", None);
    assert_eq!(view["round"], 1);
    assert_eq!(view["threads"][0]["id"], "t1");
}

// Case: docs/cases/gist-md-review.md#serve-resume
#[test]
fn a_taken_old_port_moves_the_review_to_a_new_one() {
    let repo = Repo::new();
    let mut server = repo.serve("docs/foo.md");
    let port = server.port;
    server.child.kill().unwrap();
    server.exit(Duration::from_secs(5));
    let _taken = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();

    let resumed = repo.serve("docs/foo.md");
    assert_ne!(resumed.port, port);
}

// Case: docs/cases/gist-md-review.md#serve-refuse
#[test]
fn serve_refuses_a_missing_or_oversized_file_and_writes_nothing() {
    let repo = Repo::new();
    let (code, message) = repo.error(&["md-review", "serve", "docs/none.md"]);
    assert_eq!(code, 1);
    assert!(message.contains("no such file"), "{message}");

    repo.write("docs/big.md", &"a".repeat(1024 * 1024 + 1));
    let (code, message) = repo.error(&["md-review", "serve", "docs/big.md"]);
    assert_eq!(code, 1);
    assert!(message.contains("1 MiB"), "{message}");
    assert!(!repo.path().join(".md-review").exists());
}

// Case: docs/cases/gist-md-review.md#serve-refuse
#[test]
fn serve_refuses_a_log_of_an_unknown_format_and_leaves_it() {
    let repo = Repo::new();
    let mut server = repo.serve("docs/foo.md");
    repo.gk(&["md-review", "stop", "docs/foo.md"]);
    server.exit(Duration::from_secs(5));
    let log = repo.store().join("events.jsonl");
    let newer = "{\"at\":1,\"event\":\"review_started\",\"format\":9,\"file\":\"docs/foo.md\"}\n";
    std::fs::write(&log, newer).unwrap();

    let (code, message) = repo.error(&["md-review", "serve", "docs/foo.md"]);
    assert_eq!(code, 1);
    assert!(message.contains("format 9"), "{message}");
    assert_eq!(std::fs::read_to_string(&log).unwrap(), newer);
}

/// Run `serve --detach` to completion, failing the test if it holds its
/// output open, as it would if the server inherited it.
fn detach(repo: &Repo, args: &[&str]) -> (i32, String, String) {
    let mut command_args = vec!["md-review", "serve", "--detach"];
    command_args.extend_from_slice(args);
    let mut child = repo
        .command(&command_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut out, mut err) = (String::new(), String::new());
        let _ = std::io::Read::read_to_string(&mut stdout, &mut out);
        let _ = std::io::Read::read_to_string(&mut stderr, &mut err);
        let _ = sender.send((out, err));
    });
    let (out, err) = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("serve --detach returns and closes its output");
    let code = child.wait().unwrap().code().unwrap_or(-1);
    (code, out, err)
}

/// A detached server, killed when dropped.
struct Detached(u64);

impl Drop for Detached {
    fn drop(&mut self) {
        let _ = Command::new("kill")
            .arg(self.0.to_string())
            .stderr(Stdio::null())
            .status();
    }
}

fn alive(pid: u64) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

// Case: docs/cases/gist-md-review.md#serve-detach
#[cfg(unix)]
#[test]
fn a_detached_serve_prints_the_url_and_runs_on_in_its_own_group() {
    let repo = Repo::new();
    let (code, stdout, stderr) = detach(&repo, &["docs/foo.md"]);
    assert_eq!(code, 0, "{stderr}");
    let info = server_json(&repo);
    let pid = info["pid"].as_u64().unwrap();
    let _server = Detached(pid);
    let url = format!(
        "http://127.0.0.1:{}/?token={}",
        info["port"],
        info["token"].as_str().unwrap()
    );
    assert_eq!(stdout, format!("{url}\n"), "only the URL");
    assert_eq!(stderr, "");

    let group = Command::new("ps")
        .args(["-o", "pgid=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let group = String::from_utf8_lossy(&group.stdout).trim().to_string();
    assert_eq!(group, pid.to_string(), "its own process group");

    let status = repo.data(&["md-review", "status", "docs/foo.md"]);
    assert_eq!(status["url"], url);
    let (code, again, _) = detach(&repo, &["docs/foo.md"]);
    assert_eq!(code, 0);
    assert_eq!(again, stdout, "a live server is reused");

    assert_eq!(repo.gk(&["md-review", "stop", "docs/foo.md"]).0, 0);
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(pid) {
        assert!(Instant::now() < deadline, "serve still running");
        std::thread::sleep(Duration::from_millis(20));
    }
}

// Case: docs/cases/gist-md-review.md#serve-detach
#[test]
fn a_detached_serve_prints_its_json_result_on_one_line() {
    let repo = Repo::new();
    let (code, stdout, _) = detach(&repo, &["docs/foo.md", "--json"]);
    assert_eq!(code, 0);
    let _server = Detached(server_json(&repo)["pid"].as_u64().unwrap());
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    let value: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["data"]["file"], "docs/foo.md");
    assert_eq!(value["data"]["reused"], false);
}

// Case: docs/cases/gist-md-review.md#serve-detach
#[test]
fn a_detached_serve_reports_a_refusal_with_its_exit_code() {
    let repo = Repo::new();
    let (code, stdout, stderr) = detach(&repo, &["docs/none.md"]);
    assert_eq!(code, 1);
    assert_eq!(stdout, "");
    assert!(stderr.starts_with("error: "), "{stderr}");
    assert!(stderr.contains("no such file"), "{stderr}");

    let (code, _, stderr) = detach(&repo, &["docs/none.md", "--json"]);
    assert_eq!(code, 1);
    let value: Value = serde_json::from_str(&stderr).expect("error envelope on stderr");
    assert_eq!(value["status"], "error");
    assert!(!repo.path().join(".md-review").exists());
}

// Case: docs/cases/gist-md-review.md#stop
#[test]
fn stop_ends_the_server_keeps_the_store_and_serve_resumes_it() {
    let repo = Repo::new();
    let mut server = repo.serve("docs/foo.md");
    server.comment(1, "m-1", "paragraph here", 1, 3);
    let (code, stdout, _) = repo.gk(&["md-review", "stop", "docs/foo.md"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("the review is kept"), "{stdout}");
    assert_eq!(server.exit(Duration::from_secs(5)), 0);
    assert!(repo.store().join("events.jsonl").is_file());
    let (code, message) = repo.error(&["md-review", "status", "docs/foo.md"]);
    assert_eq!(code, 1);
    assert!(message.contains("no review server runs"), "{message}");

    // The page's URL still works: same port when free, same token.
    let resumed = repo.serve("docs/foo.md");
    assert_eq!(resumed.url, server.url);
    let (_, view) = resumed.page("GET", "/api/review", None);
    assert_eq!(view["threads"][0]["id"], "t1");
}

// Case: docs/cases/gist-md-review.md#stop
#[test]
fn a_page_open_across_a_restart_is_answered_at_once() {
    let repo = Repo::new();
    let mut server = repo.serve("docs/foo.md");
    let (_, before) = server.page("GET", "/api/review", None);
    repo.gk(&["md-review", "stop", "docs/foo.md"]);
    server.exit(Duration::from_secs(5));

    // Nothing changed, yet the page must learn it reached a new server
    // rather than wait out its hold on the old one's numbering.
    let resumed = repo.serve("docs/foo.md");
    let started = std::time::Instant::now();
    let uri = format!("/api/review?after={}&hold=30", before["seq"]);
    let (_, view) = resumed.page("GET", &uri, None);
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_ne!(view["seq"], before["seq"]);
}

#[cfg(unix)]
#[test]
fn a_signal_ends_the_server_promptly_even_with_a_wait_blocked() {
    let repo = Repo::new();
    let mut server = repo.serve("docs/foo.md");
    let wait = repo
        .command(&[
            "--json",
            "md-review",
            "wait",
            "docs/foo.md",
            "--timeout",
            "60s",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    until_waiting(&repo);
    let status = Command::new("kill")
        .args(["-TERM", &server.child.id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(server.exit(Duration::from_secs(3)), 0);
    let output = wait.wait_with_output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "a stopped server is an outcome"
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["event"], "stopped");
    assert!(repo.store().join("events.jsonl").is_file());
}

// Case: docs/cases/gist-md-review.md#client-no-server
#[test]
fn clients_without_a_server_exit_1_naming_serve() {
    let repo = Repo::new();
    for args in [
        vec![
            "md-review",
            "reply",
            "docs/foo.md",
            "t1",
            "--outcome",
            "applied",
            "--note",
            "x",
        ],
        vec!["md-review", "next", "docs/foo.md"],
        vec!["md-review", "status", "docs/foo.md"],
    ] {
        let (code, message) = repo.error(&args);
        assert_eq!(code, 1, "{args:?}");
        assert!(
            message.contains("gk md-review serve"),
            "{args:?}: {message}"
        );
    }

    let mut server = repo.serve("docs/foo.md");
    server.child.kill().unwrap();
    server.exit(Duration::from_secs(5));
    let (code, message) = repo.error(&["md-review", "status", "docs/foo.md"]);
    assert_eq!(code, 1, "a server.json left by a dead server is no server");
    assert_eq!(
        message,
        "no review server runs for docs/foo.md; start one with gk md-review serve docs/foo.md"
    );
}

// Case: docs/cases/gist-md-review.md#client-version-mismatch
#[test]
fn a_client_of_another_version_exits_1_naming_stop_and_stop_still_works() {
    let repo = Repo::new();
    let mut server = repo.serve("docs/foo.md");
    let path = repo.store().join("server.json");
    let mut info = server_json(&repo);
    info["version"] = json!("0.0.0-old");
    std::fs::write(&path, info.to_string()).unwrap();

    for args in [
        vec!["md-review", "status", "docs/foo.md"],
        vec!["md-review", "wait", "docs/foo.md", "--timeout", "1s"],
    ] {
        let (code, message) = repo.error(&args);
        assert_eq!(code, 1);
        assert!(
            message.contains("gk md-review stop docs/foo.md"),
            "{message}"
        );
        assert!(message.contains("0.0.0-old"), "{message}");
    }
    assert_eq!(repo.gk(&["md-review", "stop", "docs/foo.md"]).0, 0);
    assert_eq!(server.exit(Duration::from_secs(5)), 0);
}

// Case: docs/cases/gist-md-review.md#client-no-server
#[test]
fn a_wait_without_a_server_reports_it_stopped() {
    let repo = Repo::new();
    let data = repo.data(&["md-review", "wait", "docs/foo.md", "--timeout", "1s"]);
    assert_eq!(data["event"], "stopped");
    assert_eq!(data["file"], "docs/foo.md");
}

// Case: docs/cases/gist-md-review.md#client-file-required
#[test]
fn every_client_requires_the_file() {
    let repo = Repo::new();
    for command in ["wait", "next", "status", "stop"] {
        let (code, _, _) = repo.gk(&["md-review", command]);
        assert_eq!(code, 2, "{command}");
    }
    let (code, _, _) = repo.gk(&[
        "md-review",
        "reply",
        "t1",
        "--outcome",
        "applied",
        "--note",
        "x",
    ]);
    assert_eq!(code, 2, "reply takes the file and the thread");
}

// Case: docs/cases/gist-md-review.md#wait-pending
#[test]
fn a_submit_made_before_wait_is_returned_at_once() {
    let repo = Repo::new();
    let server = repo.serve("docs/foo.md");
    server.comment(1, "m-1", "paragraph here", 1, 3);
    server.submit(1, Some("Tighten it."));

    let started = Instant::now();
    let data = repo.data(&["md-review", "wait", "docs/foo.md", "--timeout", "30s"]);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(data["event"], "review_submitted");
    assert_eq!(data["round"], 1);
    assert_eq!(data["summary"], "Tighten it.");
    assert_eq!(data["threads"][0]["id"], "t1");
    assert_eq!(data["threads"][0]["anchor"]["lines"], json!([3, 3]));
    assert!(data.get("redelivered").is_none());
    assert!(data.get("file_differs").is_none());
}

// Case: docs/cases/gist-md-review.md#wait-blocks
#[test]
fn a_blocked_wait_returns_the_submit_when_it_arrives_in_both_renderings() {
    for json in [false, true] {
        let repo = Repo::new();
        let server = repo.serve("docs/foo.md");
        let mut args = vec!["md-review", "wait", "docs/foo.md", "--timeout", "30s"];
        if json {
            args.push("--json");
        }
        let wait = repo
            .command(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        until_waiting(&repo);
        server.comment(1, "m-1", "paragraph here", 1, 3);
        server.submit(1, None);
        let output = wait.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(0));
        let stdout = String::from_utf8(output.stdout).unwrap();
        if json {
            let value: Value = serde_json::from_str(&stdout).unwrap();
            assert_eq!(value["data"]["event"], "review_submitted");
            assert_eq!(
                value["data"]["threads"][0]["messages"][0]["body"],
                "About paragraph here."
            );
        } else {
            assert!(
                stdout.starts_with("review submitted · docs/foo.md · round 1 · 1 thread\n"),
                "{stdout}"
            );
            assert!(
                stdout.contains(
                    "[t1] comment · line 3 · Top\n  > paragraph here\n  About paragraph here.\n"
                ),
                "{stdout}"
            );
            assert!(
                stdout
                    .trim_end()
                    .ends_with("then gk md-review next docs/foo.md"),
                "{stdout}"
            );
        }
    }
}

// Case: docs/cases/gist-md-review.md#wait-timeout
#[test]
fn wait_times_out_with_a_result_in_both_renderings_and_requires_a_timeout() {
    let repo = Repo::new();
    let _server = repo.serve("docs/foo.md");

    let started = Instant::now();
    let data = repo.data(&[
        "md-review",
        "wait",
        "docs/foo.md",
        "--timeout",
        "3s",
        "--poll",
        "1s",
    ]);
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_secs(3) && elapsed < Duration::from_secs(8),
        "{elapsed:?}"
    );
    assert_eq!(data, json!({ "event": "timeout", "after": "3s" }));

    let (code, stdout, _) = repo.gk(&["md-review", "wait", "docs/foo.md", "--timeout", "1s"]);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("timeout after 1s"), "{stdout}");

    let (code, _, stderr) = repo.gk(&["md-review", "wait", "docs/foo.md"]);
    assert_eq!(code, 2, "{stderr}");
}

// Case: docs/cases/gist-md-review.md#wait-redelivered
#[test]
fn a_submit_delivered_again_is_marked_with_the_threads_replied_to() {
    let repo = Repo::new();
    let server = repo.serve("docs/foo.md");
    server.comment(1, "m-1", "paragraph here", 1, 3);
    server.comment(1, "m-2", "Second one", 3, 7);
    server.submit(1, None);
    let first = repo.data(&["md-review", "wait", "docs/foo.md", "--timeout", "5s"]);
    assert!(first.get("redelivered").is_none());
    repo.data(&[
        "md-review",
        "reply",
        "docs/foo.md",
        "t1",
        "--outcome",
        "applied",
        "--note",
        "Done.",
    ]);

    let again = repo.data(&["md-review", "wait", "docs/foo.md", "--timeout", "5s"]);
    assert_eq!(again["redelivered"]["replied"], json!(["t1"]));
    assert!(again["redelivered"]["first"].as_u64().unwrap() > 0);
    let (_, human, _) = repo.gk(&["md-review", "wait", "docs/foo.md", "--timeout", "5s"]);
    assert!(
        human
            .lines()
            .nth(1)
            .unwrap()
            .starts_with("redelivered · first "),
        "{human}"
    );
    assert!(
        human.lines().nth(1).unwrap().ends_with(" · replied t1"),
        "{human}"
    );
}

// Case: docs/cases/gist-md-review.md#wait-bounded
#[test]
fn a_submit_over_the_limit_is_truncated_and_status_pages_through_it() {
    let repo = Repo::new();
    let server = repo.serve("docs/foo.md");
    server.comment(1, "m-1", "Top", 0, 1);
    server.comment(1, "m-2", "paragraph here", 1, 3);
    server.comment(1, "m-3", "Second one", 3, 7);
    server.submit(1, None);

    let data = repo.data(&[
        "md-review",
        "wait",
        "docs/foo.md",
        "--timeout",
        "5s",
        "--limit",
        "2",
    ]);
    assert_eq!(data["truncated"], true);
    assert_eq!(data["total"], 3);
    assert_eq!(data["threads"].as_array().unwrap().len(), 2);

    let page = repo.data(&[
        "md-review",
        "status",
        "docs/foo.md",
        "--round",
        "1",
        "--offset",
        "2",
        "--limit",
        "2",
    ]);
    assert_eq!(page["total"], 3);
    assert_eq!(page["threads"][0]["id"], "t3");
    assert!(page.get("truncated").is_none());
}

// Case: docs/cases/gist-md-review.md#reply-and-next
// Case: docs/cases/gist-md-review.md#reanchor
#[test]
fn reply_and_next_record_outcomes_snapshot_and_reanchor() {
    let repo = Repo::new();
    let server = repo.serve("docs/foo.md");
    server.comment(1, "m-1", "paragraph here", 1, 3);
    server.comment(1, "m-2", "Second one", 3, 7);
    server.submit(1, None);
    repo.data(&["md-review", "wait", "docs/foo.md", "--timeout", "5s"]);

    repo.write(
        "docs/foo.md",
        "# Top\n\nNew intro.\n\nFirst **paragraph** here, sharper.\n\n## Sub\n\nThird one.\n",
    );
    let replied = repo.data(&[
        "md-review",
        "reply",
        "docs/foo.md",
        "t1",
        "--outcome",
        "applied",
        "--note",
        "Sharper.",
    ]);
    assert_eq!(replied["outcome"], "applied");
    repo.data(&[
        "md-review",
        "reply",
        "docs/foo.md",
        "t2",
        "--outcome",
        "declined",
        "--note",
        "Kept as is.",
    ]);
    let started = repo.data(&["md-review", "next", "docs/foo.md"]);
    assert_eq!(started["round"], 2);
    assert_eq!(started["version"], 2);
    assert_eq!(started["orphaned"], json!(["t2"]));
    let v2 = std::fs::read_to_string(repo.store().join("v2.md")).unwrap();
    assert!(v2.contains("sharper"));
    assert!(repo
        .log()
        .lines()
        .any(|line| line.contains("\"event\":\"round_started\"")));

    let status = repo.data(&["md-review", "status", "docs/foo.md"]);
    assert_eq!(status["round"], 2);
    let threads = status["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 1, "t1 was applied; only t2 is open");
    assert_eq!(threads[0]["orphaned"], true);
    assert_eq!(threads[0]["messages"][1]["outcome"], "declined");

    let (_, view) = server.page("GET", "/api/review", None);
    assert_eq!(
        view["threads"][0]["anchor"]["lines"],
        json!([5, 5]),
        "t1 follows its text"
    );
    assert_eq!(view["changes"]["since"], 1);

    let (code, message) = repo.error(&["md-review", "next", "docs/foo.md"]);
    assert_eq!(code, 1);
    assert!(message.contains("not submitted"), "{message}");

    // An approval after the agent edited again covers v2, not the file.
    repo.write("docs/foo.md", "# Top\n\nRewritten after round 2.\n");
    let approved = server.approve(2, "");
    let record =
        std::fs::read_to_string(repo.path().join(approved["record"].as_str().unwrap())).unwrap();
    let approved_text = record.split("## Approved text").nth(1).unwrap();
    assert!(
        approved_text.contains("First **paragraph** here, sharper."),
        "{record}"
    );
}

// Case: docs/cases/gist-md-review.md#reply-and-next
#[test]
fn a_round_with_replies_and_no_edit_still_makes_a_version() {
    let repo = Repo::new();
    let server = repo.serve("docs/foo.md");
    server.comment(1, "m-1", "paragraph here", 1, 3);
    server.submit(1, None);
    repo.data(&[
        "md-review",
        "reply",
        "docs/foo.md",
        "t1",
        "--outcome",
        "declined",
        "--note",
        "Right as is.",
    ]);
    let started = repo.data(&["md-review", "next", "docs/foo.md"]);
    assert_eq!(started["version"], 2);
    assert_eq!(started["changed"], 0);
    assert_eq!(
        std::fs::read_to_string(repo.store().join("v2.md")).unwrap(),
        TEXT
    );
}

// Case: docs/cases/gist-md-review.md#file-drift
#[test]
fn a_file_edited_since_the_version_read_is_flagged_on_delivery() {
    let repo = Repo::new();
    let server = repo.serve("docs/foo.md");
    server.comment(1, "m-1", "paragraph here", 1, 3);
    server.submit(1, None);
    repo.write("docs/foo.md", "# Top\n\nSomeone else edited this.\n");

    let data = repo.data(&["md-review", "wait", "docs/foo.md", "--timeout", "5s"]);
    assert_eq!(data["file_differs"], true);
    let (_, human, _) = repo.gk(&["md-review", "wait", "docs/foo.md", "--timeout", "5s"]);
    assert!(
        human.contains(
            "warning: docs/foo.md changed since round 1 was read; line numbers refer to v1"
        ),
        "{human}"
    );
}

// Case: docs/cases/gist-md-review.md#wait-approved
// Case: docs/cases/gist-md-review.md#approve
#[test]
fn an_approval_is_delivered_then_the_store_is_deleted_and_serve_exits() {
    let repo = Repo::new();
    let mut server = repo.serve("docs/foo.md");
    server.comment(1, "m-1", "paragraph here", 1, 3);
    let approved = server.approve(1, "Ship it.");
    let record = approved["record"].as_str().unwrap().to_string();
    assert!(
        repo.path().join(&record).is_file(),
        "written before delivery"
    );
    assert!(repo.store().is_dir(), "kept until delivered");
    let (code, message) = repo.error(&[
        "md-review",
        "reply",
        "docs/foo.md",
        "t1",
        "--outcome",
        "applied",
        "--note",
        "x",
    ]);
    assert_eq!(code, 1);
    assert!(message.contains("approved"), "{message}");

    let data = repo.data(&["md-review", "wait", "docs/foo.md", "--timeout", "5s"]);
    assert_eq!(data["event"], "approved");
    assert_eq!(data["round"], 1);
    assert_eq!(data["version"], 1);
    assert_eq!(data["note"], "Ship it.");
    assert_eq!(data["record"], record);
    assert_eq!(data["file_differs"], false);
    assert_eq!(data["discarded"], json!(["t1"]));
    assert!(data.get("store_kept").is_none());

    assert_eq!(server.exit(Duration::from_secs(5)), 0);
    assert!(!repo
        .path()
        .join(".md-review/reviews")
        .read_dir()
        .unwrap()
        .any(|_| true));
    let text = std::fs::read_to_string(repo.path().join(&record)).unwrap();
    assert!(text.contains("> Ship it."), "{text}");
    assert!(text.contains("### t1 · open"), "{text}");
}

// Case: docs/cases/gist-md-review.md#approve-drifted
#[test]
fn an_approval_of_a_drifted_file_says_so_and_the_record_keeps_the_approved_text() {
    let repo = Repo::new();
    let server = repo.serve("docs/foo.md");
    repo.write("docs/foo.md", "# Top\n\nThe agent kept editing.\n");
    let approved = server.approve(1, "");
    let data = repo.data(&["md-review", "wait", "docs/foo.md", "--timeout", "5s"]);
    assert_eq!(data["file_differs"], true);
    assert!(data.get("note").is_none());
    let text =
        std::fs::read_to_string(repo.path().join(approved["record"].as_str().unwrap())).unwrap();
    assert!(text.contains("## Approved text"), "{text}");
    assert!(text.contains("First paragraph here."), "{text}");
    assert!(text.contains("The agent kept editing."), "{text}");
}

// Case: docs/cases/gist-md-review.md#approve-confined
#[cfg(unix)]
#[test]
fn a_store_swapped_for_a_symlink_is_kept_and_the_record_still_written() {
    let repo = Repo::new();
    let mut server = repo.serve("docs/foo.md");
    let store = repo.store();
    let moved = repo.path().join("moved-store");
    std::fs::rename(&store, &moved).unwrap();
    std::os::unix::fs::symlink(&moved, &store).unwrap();

    let approved = server.approve(1, "Fine.");
    assert!(repo
        .path()
        .join(approved["record"].as_str().unwrap())
        .is_file());
    assert!(
        approved["store_kept"].as_str().unwrap().contains("symlink"),
        "{approved}"
    );

    // Clients do not follow the symlink to the server's address either.
    let (code, message) = repo.error(&["md-review", "wait", "docs/foo.md", "--timeout", "1s"]);
    assert_eq!(code, 1);
    assert!(message.contains("symlink"), "{message}");
    server.child.kill().unwrap();
    server.exit(Duration::from_secs(5));
    assert!(
        moved.join("events.jsonl").is_file(),
        "the target is not deleted"
    );
    assert!(std::fs::symlink_metadata(&store)
        .unwrap()
        .file_type()
        .is_symlink());
}
