//! The agent's side: `wait`, `reply`, `next`, `status` and `stop` find the
//! review's server through its `server.json` and ask it over HTTP. They
//! write nothing; `serve` is the only writer. docs/design/md-review-hld.md#cli-surface.

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::Path;
use std::time::{Duration, Instant};

use super::api::{Delivery, Replied, Started, Status, Stopped, Timeout};
use super::serve::MAX_HOLD;
use super::store::{Location, Outcome, ServerInfo};

/// How long a short call waits for its answer.
const ANSWER: Duration = Duration::from_secs(30);
/// How long a liveness probe waits.
const PROBE: Duration = Duration::from_secs(2);
/// How long `serve` waits for a server that holds the lock to answer.
const STARTING: Duration = Duration::from_secs(3);

/// A live review server, its `gk` version checked against this one's.
pub struct Client {
    location: Location,
    info: ServerInfo,
}

impl Client {
    /// The server for `file`, or for the one live review when no file is
    /// named. `stop` skips the version check, since it is the remedy for a
    /// mismatch.
    pub fn find(file: Option<&Path>, check_version: bool) -> Result<Self, String> {
        let (location, info) = match file {
            Some(file) => {
                let location = Location::find(file)?;
                let info = location
                    .read_server()?
                    .ok_or_else(|| no_server(&location))?;
                if !probe(&info) {
                    return Err(no_server(&location));
                }
                (location, info)
            }
            None => {
                let cwd = std::env::current_dir()
                    .map_err(|err| format!("could not read the current directory: {err}"))?;
                let mut live: Vec<_> = Location::with_servers(&cwd)?
                    .into_iter()
                    .filter(|(_, info)| probe(info))
                    .collect();
                match live.len() {
                    0 => {
                        return Err(
                            "no review server is running here; start one with gk md-review serve <file>"
                                .to_string(),
                        )
                    }
                    1 => live.remove(0),
                    _ => {
                        let files: Vec<_> = live.iter().map(|(l, _)| l.file()).collect();
                        return Err(format!(
                            "several reviews are live: {}; name the file",
                            files.join(", ")
                        ));
                    }
                }
            }
        };
        let mine = env!("CARGO_PKG_VERSION");
        if check_version && info.version != mine {
            return Err(format!(
                "the review server for {file} runs gk {theirs}, and this is gk {mine}; \
                 stop it with gk md-review stop {file}, then start it again with gk md-review serve {file}",
                file = location.file(),
                theirs = info.version
            ));
        }
        Ok(Self { location, info })
    }

    fn file(&self) -> &str {
        self.location.file()
    }

    /// Ask the server; `None` for 204. A refusal is returned as the
    /// server's message.
    fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: Option<&impl Serialize>,
        timeout: Duration,
    ) -> Result<Option<T>, String> {
        let response = send(&self.info, method, path, body, timeout).map_err(|err| match err {
            ureq::Error::Timeout(_) => format!(
                "the review server for {} did not answer within {}s",
                self.file(),
                timeout.as_secs()
            ),
            _ => format!(
                "the review server for {file} stopped; start it again with gk md-review serve {file}",
                file = self.file()
            ),
        })?;
        read(response, self.file())
    }

    /// Return a pending submit or approval, or the first to arrive before
    /// `timeout`, asking in requests of at most `poll`.
    pub fn wait(
        &self,
        timeout: Duration,
        after: &str,
        limit: usize,
        poll: Duration,
    ) -> Result<Delivery, String> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(Delivery::Timeout(Timeout {
                    file: self.file().to_string(),
                    after: after.to_string(),
                }));
            }
            let hold = left.min(poll).min(MAX_HOLD).as_secs_f64().ceil() as u64;
            let path = format!("/api/wait?hold={hold}&limit={limit}");
            let timeout = Duration::from_secs(hold) + ANSWER;
            if let Some(delivery) = self.call::<Delivery>("GET", &path, None::<&()>, timeout)? {
                return Ok(delivery);
            }
        }
    }

    pub fn reply(&self, thread: &str, outcome: Outcome, note: &str) -> Result<Replied, String> {
        let body = serde_json::json!({ "thread": thread, "outcome": outcome, "note": note });
        self.data("POST", "/api/reply", Some(&body))
    }

    pub fn next(&self) -> Result<Started, String> {
        self.data("POST", "/api/next", None::<&()>)
    }

    pub fn status(
        &self,
        round: Option<u32>,
        limit: usize,
        offset: usize,
    ) -> Result<Status, String> {
        let mut path = format!("/api/status?limit={limit}&offset={offset}");
        if let Some(round) = round {
            path.push_str(&format!("&round={round}"));
        }
        self.data("GET", &path, None::<&()>)
    }

    pub fn stop(&self) -> Result<Stopped, String> {
        self.data("POST", "/api/stop", None::<&()>)
    }

    fn data<T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: Option<&impl Serialize>,
    ) -> Result<T, String> {
        self.call(method, path, body, ANSWER)?
            .ok_or_else(|| format!("the review server for {} answered nothing", self.file()))
    }
}

/// The live server that holds `location`'s lock, for a second `serve`. It
/// may have just taken the lock, so give it a moment to write
/// `server.json` and answer.
pub fn live_server(location: &Location) -> Result<ServerInfo, String> {
    let deadline = Instant::now() + STARTING;
    loop {
        if let Some(info) = location.read_server()? {
            if probe(&info) {
                return Ok(info);
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "another process holds the review of {} but no server answers; \
                 end that process, then run gk md-review serve again",
                location.file()
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Whether a server answers at `info`'s address and accepts its token.
fn probe(info: &ServerInfo) -> bool {
    send(info, "GET", "/api/status?limit=0", None::<&()>, PROBE)
        .is_ok_and(|response| response.status().is_success())
}

fn send(
    info: &ServerInfo,
    method: &str,
    path: &str,
    body: Option<&impl Serialize>,
    timeout: Duration,
) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        // The server is on loopback; a proxy from the environment would
        // see the token.
        .proxy(None)
        .timeout_global(Some(timeout))
        .build()
        .into();
    let address = format!("http://127.0.0.1:{}{path}", info.port);
    let bearer = format!("Bearer {}", info.token);
    match (method, body) {
        ("GET", _) => agent.get(&address).header("Authorization", &bearer).call(),
        (_, Some(body)) => agent
            .post(&address)
            .header("Authorization", &bearer)
            .send_json(body),
        (_, None) => agent
            .post(&address)
            .header("Authorization", &bearer)
            .send_empty(),
    }
}

fn read<T: DeserializeOwned>(
    mut response: ureq::http::Response<ureq::Body>,
    file: &str,
) -> Result<Option<T>, String> {
    let status = response.status();
    if status == ureq::http::StatusCode::NO_CONTENT {
        return Ok(None);
    }
    if status.is_success() {
        return response.body_mut().read_json().map(Some).map_err(|err| {
            format!("the review server for {file} sent an unreadable answer: {err}")
        });
    }
    if status == ureq::http::StatusCode::SERVICE_UNAVAILABLE {
        return Err(format!(
            "the review server for {file} stopped; start it again with gk md-review serve {file}"
        ));
    }
    let message = response
        .body_mut()
        .read_json::<serde_json::Value>()
        .ok()
        .and_then(|value| value["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| format!("the review server for {file} answered {status}"));
    Err(message)
}

fn no_server(location: &Location) -> String {
    format!(
        "no review server runs for {file}; start one with gk md-review serve {file}",
        file = location.file()
    )
}
