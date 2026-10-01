//! `gk md-review serve`: the only writer of a review. An HTTP API on
//! 127.0.0.1 for the page and the agent's clients, with a token and Host
//! check on every request; stale writes from the page get 409; the page and
//! `wait` long-poll. Async stays in this module: the store, renderer and
//! differ are synchronous and are called under one mutex.
//! docs/design/md-review-hld.md; docs/adr/0015-md-review-http-stack.md.

use axum::extract::{Path as UrlPath, Query, Request, State as Shared_};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::Notify;

use super::anchor::reanchor;
use super::api::{
    Approved, Delivery, Redelivered, Replied, Served, Started, State as ThreadState_, Status,
    Stopped, SubmitStatus, Submitted, Tally, ThreadView,
};
use super::diff::{diff, BlockDiff, Change, Deleted};
use super::record;
use super::render::{render, Block};
use super::store::{
    hash, record_path, Anchor, Approval, Author, Event, Kind, Location, Opened, Outcome, Review,
    ServerInfo, Store, ThreadState,
};

/// Longest a long-poll is held before it answers anyway.
pub const MAX_HOLD: Duration = Duration::from_secs(60);
/// How long after the last `wait` the agent still counts as listening; it
/// covers the restart after a `wait` timeout.
const GRACE_SECS: u64 = 90;
/// Largest request body; a comment is text, not a document.
const BODY_LIMIT: usize = 256 * 1024;
const MAX_ID_CHARS: usize = 64;

/// Placeholder until the review page is built; served with the page's
/// headers so they are in place and tested from the start.
const PAGE: &str = "<!doctype html>\n<meta charset=\"utf-8\">\n<title>gk md-review</title>\n<p>The review page is not built yet.</p>\n";
/// The page loads only what `serve` serves and sends nothing elsewhere.
const PAGE_CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; \
                        connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// Start serving `file`, or report the live server that already does.
/// `announce` prints the result line; after it, `serve` prints nothing
/// until it exits, on stop, a signal, or a delivered approval.
pub fn run(file: &Path, announce: impl FnOnce(&Served)) -> Result<(), String> {
    let location = Location::find(file)?;
    let working = location.read_working()?;
    render(&working)?;
    let store = match Store::open(&location, &working)? {
        Opened::Writer(store) => *store,
        Opened::Busy => {
            let info = super::client::live_server(&location)?;
            announce(&Served {
                url: url(&info),
                file: location.file().to_string(),
                reused: true,
            });
            return Ok(());
        }
    };

    let old_port = location.read_server().ok().flatten().map(|info| info.port);
    let listener = bind(old_port)?;
    let port = listener
        .local_addr()
        .map_err(|err| format!("could not read the bound port: {err}"))?
        .port();
    let info = ServerInfo {
        file: location.file().to_string(),
        pid: std::process::id(),
        port,
        token: token()?,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    // Everything that can fail happens before the URL is printed: a caller
    // that read it may act on the store at once.
    let shared = Arc::new(Server::new(store, &info)?);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("could not start the server runtime: {err}"))?;
    let listener = {
        let _context = runtime.enter();
        listener
            .set_nonblocking(true)
            .and_then(|()| tokio::net::TcpListener::from_std(listener))
            .map_err(|err| format!("could not listen: {err}"))?
    };
    shared.lock().store.write_server(&info)?;
    announce(&Served {
        url: url(&info),
        file: location.file().to_string(),
        reused: false,
    });

    runtime.block_on(async {
        axum::serve(listener, router(shared.clone()))
            .with_graceful_shutdown(shutdown(shared.clone()))
            .await
            .map_err(|err| format!("the server failed: {err}"))
    })?;
    shared.finish()
}

/// The address a client or browser uses, with the token.
pub fn url(info: &ServerInfo) -> String {
    format!("http://127.0.0.1:{}/?token={}", info.port, info.token)
}

/// Listen on 127.0.0.1, on the old port when it is free so an open tab
/// finds the server again, otherwise on any.
fn bind(old_port: Option<u16>) -> Result<std::net::TcpListener, String> {
    if let Some(port) = old_port {
        if let Ok(listener) = std::net::TcpListener::bind(("127.0.0.1", port)) {
            return Ok(listener);
        }
    }
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .map_err(|err| format!("could not listen on 127.0.0.1: {err}"))
}

fn token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|err| format!("could not make a token: {err}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Ends the server on SIGINT, SIGTERM, `stop` or a delivered approval, and
/// wakes every long-poll so none holds the shutdown up.
async fn shutdown(shared: Arc<Server>) {
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate => {}
        _ = shared.finished.notified() => {}
    }
    shared.lock().finish.get_or_insert(Finish::Stopped);
    shared.changed.notify_waiters();
}

/// Why the server is ending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Finish {
    Stopped,
    /// A client received the approval: the store goes with the server.
    Approved,
}

/// What every request shares.
pub struct Server {
    token: String,
    host: String,
    info: ServerInfo,
    state: Mutex<Live>,
    /// Woken on every change, for the long-polls.
    changed: Notify,
    /// Woken once to end the server.
    finished: Notify,
}

/// State behind the mutex: the store and what is derived from it.
struct Live {
    store: Store,
    /// Bumped on every change; the page long-polls for a different value.
    seq: u64,
    /// Rendered versions, filled as they are needed; version `n` at `n - 1`.
    rendered: Vec<Option<Arc<Vec<Block>>>>,
    /// The current version's text, held so the record can be written even
    /// when the store can no longer be read safely.
    current: Vec<u8>,
    waiters: usize,
    last_wait: Option<u64>,
    finish: Option<Finish>,
}

impl Server {
    fn new(store: Store, info: &ServerInfo) -> Result<Self, String> {
        let current = store.read_version(store.review().version())?;
        Ok(Self {
            token: info.token.clone(),
            host: format!("127.0.0.1:{}", info.port),
            info: info.clone(),
            state: Mutex::new(Live {
                store,
                seq: 0,
                rendered: Vec::new(),
                current,
                waiters: 0,
                last_wait: None,
                finish: None,
            }),
            changed: Notify::new(),
            finished: Notify::new(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Live> {
        // A handler that panicked mid-change left nothing half written:
        // every change is one append, made before the state is folded.
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn end(&self, why: Finish) {
        self.lock().finish.get_or_insert(why);
        self.finished.notify_one();
    }

    /// After the last request: delete a delivered approval's store, or
    /// remove `server.json` so clients find no server.
    fn finish(&self) -> Result<(), String> {
        let state = self.lock();
        match state.finish {
            Some(Finish::Approved) => state.store.remove(),
            _ => state.store.clear_server(),
        }
    }
}

fn router(shared: Arc<Server>) -> Router {
    Router::new()
        .route("/", get(page))
        .route("/api/review", get(review))
        .route("/api/threads", post(post_message))
        .route("/api/threads/{id}/resolve", post(resolve))
        .route(
            "/api/messages/{id}",
            axum::routing::patch(edit).delete(delete),
        )
        .route("/api/submit", post(submit))
        .route("/api/approve", post(approve))
        .route("/api/wait", get(wait))
        .route("/api/reply", post(reply))
        .route("/api/next", post(next))
        .route("/api/status", get(status))
        .route("/api/stop", post(stop))
        .layer(axum::extract::DefaultBodyLimit::max(BODY_LIMIT))
        .layer(middleware::from_fn_with_state(shared.clone(), guard))
        .with_state(shared)
}

/// Refuse a request whose Host is not this server's address, which stops
/// DNS rebinding, or that lacks the token, which keeps other pages out.
/// Nothing behind it runs for a refused request.
async fn guard(Shared_(shared): Shared_<Arc<Server>>, request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    if host != Some(shared.host.as_str()) {
        return ApiError::new(StatusCode::FORBIDDEN, "wrong Host").into_response();
    }
    let token = bearer(request.headers()).or_else(|| {
        request.uri().query().and_then(|query| {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix("token="))
        })
    });
    if !token.is_some_and(|token| same(token.as_bytes(), shared.token.as_bytes())) {
        return ApiError::new(StatusCode::UNAUTHORIZED, "missing or wrong token").into_response();
    }
    next.run(request).await
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

/// Compare without stopping at the first difference.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A refused request: its status and a message for the person or agent.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, message)
    }

    fn bad(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    fn internal(message: String) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    fn stopping() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "the review server is stopping",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "message": self.message })),
        )
            .into_response()
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

async fn page() -> Response {
    let mut response = PAGE.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(PAGE_CSP),
    );
    // The URL carries the token.
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Wait until `ready` returns something, a change wakes us to ask again,
/// or `hold` runs out. The server ending answers every waiter at once.
async fn long_poll<T>(
    shared: &Server,
    hold: Duration,
    mut ready: impl FnMut(&mut Live) -> Result<Option<T>, ApiError>,
) -> Result<Option<T>, ApiError> {
    let deadline = tokio::time::Instant::now() + hold.min(MAX_HOLD);
    loop {
        let notified = shared.changed.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        {
            let mut state = shared.lock();
            if state.finish.is_some() {
                return Err(ApiError::stopping());
            }
            if let Some(found) = ready(&mut state)? {
                return Ok(Some(found));
            }
        }
        if tokio::time::timeout_at(deadline, notified).await.is_err() {
            return Ok(None);
        }
    }
}

#[derive(Deserialize)]
struct PollQuery {
    /// The `seq` the page already shows.
    after: Option<u64>,
    /// Seconds to hold the request.
    #[serde(default)]
    hold: u64,
}

/// The page's view of the review, at once or when it differs from `after`.
async fn review(
    Shared_(shared): Shared_<Arc<Server>>,
    Query(query): Query<PollQuery>,
) -> ApiResult<PageView> {
    let found = long_poll(&shared, Duration::from_secs(query.hold), |state| {
        if query.after == Some(state.seq) {
            Ok(None)
        } else {
            state.page_view().map(Some)
        }
    })
    .await?;
    match found {
        Some(view) => Ok(Json(view)),
        None => Ok(Json(shared.lock().page_view()?)),
    }
}

#[derive(Deserialize)]
struct PostBody {
    round: u32,
    version: u32,
    /// Chosen by the page, so a retried post is recognised.
    message: String,
    kind: Kind,
    body: String,
    /// Opens a new thread.
    anchor: Option<Anchor>,
    /// Replies in an existing thread.
    thread: Option<String>,
}

#[derive(Serialize)]
struct PostedView {
    thread: String,
    message: String,
}

/// A reviewer's comment: a new thread on an anchor, or a reply in one.
async fn post_message(
    Shared_(shared): Shared_<Arc<Server>>,
    Json(body): Json<PostBody>,
) -> ApiResult<PostedView> {
    let mut state = shared.lock();
    if state.store.review().has_message(&body.message) {
        let thread = state
            .store
            .review()
            .threads
            .iter()
            .find(|t| t.messages.iter().any(|m| m.id == body.message))
            .map(|t| t.id.clone())
            .ok_or_else(|| ApiError::conflict(format!("message {} was deleted", body.message)))?;
        return Ok(Json(PostedView {
            thread,
            message: body.message,
        }));
    }
    state.check_current(body.round, body.version)?;
    check_id(&body.message)?;
    let text = non_blank(&body.body, "a comment")?;
    if body.kind == Kind::Question {
        return Err(ApiError::bad(
            "questions are not available yet; post a comment",
        ));
    }
    let review = state.store.review();
    let (thread, anchor) = match (body.anchor, body.thread) {
        (Some(anchor), None) => {
            if anchor.version != review.version() {
                return Err(ApiError::conflict(format!(
                    "the anchor is on v{}, and the review is at v{}; reload",
                    anchor.version,
                    review.version()
                )));
            }
            if anchor.quote.trim().is_empty() {
                return Err(ApiError::bad("the anchor quotes nothing"));
            }
            (format!("t{}", review.opened + 1), Some(anchor))
        }
        (None, Some(thread)) => {
            if !review.threads.iter().any(|t| t.id == thread) {
                return Err(ApiError::new(
                    StatusCode::NOT_FOUND,
                    format!("no thread {thread}"),
                ));
            }
            (thread, None)
        }
        _ => {
            return Err(ApiError::bad(
                "give an anchor for a new thread or a thread to reply in, not both",
            ))
        }
    };
    state.append(Event::MessagePosted {
        thread: thread.clone(),
        message: body.message.clone(),
        author: Author::Human,
        kind: Kind::Comment,
        body: text,
        anchor,
        outcome: None,
    })?;
    shared.changed.notify_waiters();
    Ok(Json(PostedView {
        thread,
        message: body.message,
    }))
}

#[derive(Deserialize)]
struct EditBody {
    round: u32,
    version: u32,
    body: String,
}

#[derive(Deserialize)]
struct At {
    round: u32,
    version: u32,
}

#[derive(Serialize)]
struct Done {
    seq: u64,
}

async fn edit(
    Shared_(shared): Shared_<Arc<Server>>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<EditBody>,
) -> ApiResult<Done> {
    let mut state = shared.lock();
    state.check_current(body.round, body.version)?;
    state.check_pending(&id)?;
    let text = non_blank(&body.body, "a comment")?;
    state.append(Event::MessageEdited {
        message: id,
        body: text,
    })?;
    shared.changed.notify_waiters();
    Ok(Json(Done { seq: state.seq }))
}

async fn delete(
    Shared_(shared): Shared_<Arc<Server>>,
    UrlPath(id): UrlPath<String>,
    Json(at): Json<At>,
) -> ApiResult<Done> {
    let mut state = shared.lock();
    state.check_current(at.round, at.version)?;
    state.check_pending(&id)?;
    state.append(Event::MessageDeleted { message: id })?;
    shared.changed.notify_waiters();
    Ok(Json(Done { seq: state.seq }))
}

async fn resolve(
    Shared_(shared): Shared_<Arc<Server>>,
    UrlPath(id): UrlPath<String>,
    Json(at): Json<At>,
) -> ApiResult<Done> {
    let mut state = shared.lock();
    state.check_current(at.round, at.version)?;
    if !state.store.review().threads.iter().any(|t| t.id == id) {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            format!("no thread {id}"),
        ));
    }
    state.append(Event::ThreadResolved { thread: id })?;
    shared.changed.notify_waiters();
    Ok(Json(Done { seq: state.seq }))
}

#[derive(Deserialize)]
struct SubmitBody {
    round: u32,
    summary: Option<String>,
}

/// Hand the round to the agent. A repeat of a submit already made answers
/// as the first did and appends nothing.
async fn submit(
    Shared_(shared): Shared_<Arc<Server>>,
    Json(body): Json<SubmitBody>,
) -> ApiResult<Done> {
    let mut state = shared.lock();
    let review = state.store.review();
    if let Some(approval) = &review.approval {
        return Err(ApiError::conflict(approved(approval)));
    }
    if body.round < review.round || (body.round == review.round && review.submit.is_some()) {
        return Ok(Json(Done { seq: state.seq }));
    }
    if body.round > review.round {
        return Err(ApiError::conflict(format!(
            "there is no round {}; the review is at round {}",
            body.round, review.round
        )));
    }
    let summary = body
        .summary
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let pending = review
        .threads
        .iter()
        .flat_map(|t| &t.messages)
        .any(|m| review.is_pending(m));
    if summary.is_none() && !pending {
        return Err(ApiError::bad(
            "nothing to submit: write a comment or an overall comment, or approve",
        ));
    }
    let round = review.round;
    state.append(Event::ReviewSubmitted { round, summary })?;
    shared.changed.notify_waiters();
    Ok(Json(Done { seq: state.seq }))
}

#[derive(Deserialize)]
struct ApproveBody {
    round: u32,
    version: u32,
    note: Option<String>,
}

#[derive(Serialize)]
struct ApprovedView {
    record: String,
    /// Why the store will not be deleted, when it will be kept.
    #[serde(skip_serializing_if = "Option::is_none")]
    store_kept: Option<String>,
}

/// Approve the version on screen, at any time, and write the record before
/// anything is delivered or deleted.
async fn approve(
    Shared_(shared): Shared_<Arc<Server>>,
    Json(body): Json<ApproveBody>,
) -> ApiResult<ApprovedView> {
    let mut state = shared.lock();
    let review = state.store.review();
    if let Some(approval) = review.approval.clone() {
        if approval.round == body.round && approval.version == body.version {
            let record = state.record(&approval)?;
            let store_kept = state.store.removable().err();
            return Ok(Json(ApprovedView { record, store_kept }));
        }
        return Err(ApiError::conflict(approved(&approval)));
    }
    if body.round != review.round || body.version != review.version() {
        return Err(ApiError::conflict(stale(review, body.round, body.version)));
    }
    // Only unsubmitted comments are pending; a submitted round is the
    // agent's, not discarded.
    let discarded = review
        .threads
        .iter()
        .filter(|t| t.messages.iter().any(|m| review.is_pending(m)))
        .map(|t| t.id.clone())
        .collect();
    let event = Event::Approved {
        round: review.round,
        version: review.version(),
        hash: review.versions[review.versions.len() - 1].clone(),
        note: body
            .note
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        discarded,
    };
    state.append(event)?;
    let approval = state
        .store
        .review()
        .approval
        .clone()
        .ok_or_else(|| ApiError::internal("the approval did not fold".to_string()))?;
    let record = state.record(&approval)?;
    let store_kept = state.store.removable().err();
    shared.changed.notify_waiters();
    Ok(Json(ApprovedView { record, store_kept }))
}

#[derive(Deserialize)]
struct WaitQuery {
    #[serde(default)]
    hold: u64,
    limit: usize,
}

/// Counts a blocked `wait` for the page's agent indicator and `status`,
/// for as long as the request lives.
struct Presence(Arc<Server>);

impl Presence {
    fn enter(shared: &Arc<Server>) -> Self {
        let mut state = shared.lock();
        state.waiters += 1;
        state.last_wait = Some(now());
        if state.waiters == 1 {
            state.seq += 1;
            drop(state);
            shared.changed.notify_waiters();
        }
        Self(shared.clone())
    }
}

impl Drop for Presence {
    fn drop(&mut self) {
        let mut state = self.0.lock();
        state.waiters -= 1;
        state.last_wait = Some(now());
    }
}

/// The agent's long-poll: a pending approval or submit at once, else the
/// first to arrive within `hold`, else 204. Every delivery is logged; one
/// after the first is marked as a repeat.
async fn wait(
    Shared_(shared): Shared_<Arc<Server>>,
    Query(query): Query<WaitQuery>,
) -> Result<Response, ApiError> {
    let _presence = Presence::enter(&shared);
    let found = long_poll(&shared, Duration::from_secs(query.hold), |state| {
        state.deliver(query.limit)
    })
    .await?;
    match found {
        Some(delivery) => {
            if matches!(delivery, Delivery::Approved(_)) {
                shared.end(Finish::Approved);
            }
            shared.changed.notify_waiters();
            Ok(Json(delivery).into_response())
        }
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}

#[derive(Deserialize)]
struct ReplyBody {
    thread: String,
    outcome: Outcome,
    note: String,
}

/// The agent's answer to a thread of the submitted round.
async fn reply(
    Shared_(shared): Shared_<Arc<Server>>,
    Json(body): Json<ReplyBody>,
) -> ApiResult<Replied> {
    let mut state = shared.lock();
    state.check_agent_turn()?;
    if body.outcome == Outcome::Answered {
        return Err(ApiError::bad("a reply is applied or declined"));
    }
    if !state
        .store
        .review()
        .threads
        .iter()
        .any(|t| t.id == body.thread)
    {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            format!("no thread {} in {}", body.thread, shared.info.file),
        ));
    }
    let note = non_blank(&body.note, "a note")?;
    state.append(Event::MessagePosted {
        thread: body.thread.clone(),
        message: format!("a-{}", &token().map_err(ApiError::internal)?[..16]),
        author: Author::Agent,
        kind: Kind::Comment,
        body: note,
        anchor: None,
        outcome: Some(body.outcome),
    })?;
    shared.changed.notify_waiters();
    Ok(Json(Replied {
        file: shared.info.file.clone(),
        thread: body.thread,
        outcome: body.outcome,
    }))
}

/// Close the submitted round: snapshot the working file as the next
/// version, even when unchanged, and start the next round.
async fn next(Shared_(shared): Shared_<Arc<Server>>) -> ApiResult<Started> {
    let mut state = shared.lock();
    state.check_agent_turn()?;
    let working = state
        .store
        .location()
        .read_working()
        .map_err(ApiError::conflict)?;
    let blocks = render(&working)
        .map_err(|err| ApiError::conflict(format!("{}: {err}", shared.info.file)))?;
    let review = state.store.review();
    let (round, version) = (review.round + 1, review.version() + 1);
    let hash = state
        .store
        .write_version(version, &working)
        .map_err(ApiError::internal)?;
    state.append(Event::RoundStarted {
        round,
        version,
        hash,
    })?;
    state.cache(version, blocks);
    state.current = working;
    let changed = state.diff(version)?.changed();
    let threads = state.store.review().threads.clone();
    let mut orphaned = Vec::new();
    for thread in threads.iter().filter(|t| t.state == ThreadState::Open) {
        if state.thread_view(thread)?.orphaned {
            orphaned.push(thread.id.clone());
        }
    }
    shared.changed.notify_waiters();
    Ok(Json(Started {
        file: shared.info.file.clone(),
        round,
        version,
        changed,
        orphaned,
    }))
}

#[derive(Deserialize)]
struct StatusQuery {
    round: Option<u32>,
    limit: usize,
    #[serde(default)]
    offset: usize,
}

async fn status(
    Shared_(shared): Shared_<Arc<Server>>,
    Query(query): Query<StatusQuery>,
) -> ApiResult<Status> {
    let mut state = shared.lock();
    let review = state.store.review().clone();
    let listed: Vec<_> = match query.round {
        Some(round) => review.round_threads(round).cloned().collect(),
        None => review
            .threads
            .iter()
            .filter(|t| t.state == ThreadState::Open)
            .cloned()
            .collect(),
    };
    let mut threads = Vec::new();
    for thread in listed.iter().skip(query.offset).take(query.limit) {
        threads.push(state.thread_view(thread)?);
    }
    Ok(Json(Status {
        file: review.file.clone(),
        url: url(&shared.info),
        round: review.round,
        version: review.version(),
        submit: review.submit.as_ref().map(|submit| SubmitStatus {
            at: submit.at,
            first_delivery: submit.first_delivery,
        }),
        approved: review.approval.is_some(),
        waiters: state.waiters,
        last_wait: state.last_wait,
        file_differs: state.file_differs(review.version()),
        listed_round: query.round,
        truncated: query.offset + threads.len() < listed.len(),
        threads,
        total: listed.len(),
        offset: query.offset,
    }))
}

async fn stop(Shared_(shared): Shared_<Arc<Server>>) -> ApiResult<Stopped> {
    shared.end(Finish::Stopped);
    Ok(Json(Stopped {
        file: shared.info.file.clone(),
    }))
}

impl Live {
    fn append(&mut self, event: Event) -> Result<(), ApiError> {
        self.store.append(event).map_err(ApiError::internal)?;
        self.seq += 1;
        Ok(())
    }

    /// Refuse a page write made against another round or version, or after
    /// the round was submitted or the review approved.
    fn check_current(&self, round: u32, version: u32) -> Result<(), ApiError> {
        let review = self.store.review();
        if let Some(approval) = &review.approval {
            return Err(ApiError::conflict(approved(approval)));
        }
        if round != review.round || version != review.version() || review.submit.is_some() {
            return Err(ApiError::conflict(stale(review, round, version)));
        }
        Ok(())
    }

    /// Refuse an edit or delete of anything but the reviewer's own message
    /// not yet submitted.
    fn check_pending(&self, id: &str) -> Result<(), ApiError> {
        let review = self.store.review();
        let message = review
            .threads
            .iter()
            .flat_map(|t| &t.messages)
            .find(|m| m.id == id)
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, format!("no message {id}")))?;
        if !review.is_pending(message) {
            return Err(ApiError::conflict(format!(
                "message {id} was already sent; only pending comments can change"
            )));
        }
        Ok(())
    }

    /// Refuse `reply` and `next` unless a submitted round waits for them.
    fn check_agent_turn(&self) -> Result<(), ApiError> {
        let review = self.store.review();
        if let Some(approval) = &review.approval {
            return Err(ApiError::conflict(approved(approval)));
        }
        if review.submit.is_none() {
            return Err(ApiError::conflict(format!(
                "round {} is not submitted; wait for the reviewer with gk md-review wait {}",
                review.round, review.file
            )));
        }
        Ok(())
    }

    fn cache(&mut self, version: u32, blocks: Vec<Block>) {
        let index = version as usize - 1;
        if self.rendered.len() <= index {
            self.rendered.resize(index + 1, None);
        }
        self.rendered[index] = Some(Arc::new(blocks));
    }

    fn blocks(&mut self, version: u32) -> Result<Arc<Vec<Block>>, ApiError> {
        let index = version as usize - 1;
        if let Some(Some(blocks)) = self.rendered.get(index) {
            return Ok(blocks.clone());
        }
        let bytes = self
            .store
            .read_version(version)
            .map_err(ApiError::internal)?;
        let blocks =
            render(&bytes).map_err(|err| ApiError::internal(format!("v{version}: {err}")))?;
        self.cache(version, blocks);
        Ok(self.rendered[index].clone().unwrap_or_default())
    }

    /// A thread with its anchor carried from the version it was made on to
    /// the current one, round by round; orphaned once a round loses it.
    fn thread_view(&mut self, thread: &super::store::Thread) -> Result<ThreadView, ApiError> {
        let current = self.store.review().version();
        let mut anchor = thread.anchor.clone();
        let mut orphaned = false;
        while anchor.version < current {
            let old = self.blocks(anchor.version)?;
            let new = self.blocks(anchor.version + 1)?;
            match reanchor(&anchor, &old, &new, anchor.version + 1) {
                Some(moved) => anchor = moved,
                None => {
                    orphaned = true;
                    break;
                }
            }
        }
        let (state, applied_in) = match thread.state {
            ThreadState::Open => (ThreadState_::Open, None),
            ThreadState::Resolved => (ThreadState_::Resolved, None),
            ThreadState::Applied(round) => (ThreadState_::Applied, Some(round)),
        };
        Ok(ThreadView {
            id: thread.id.clone(),
            state,
            applied_in,
            anchor,
            orphaned,
            messages: thread.messages.clone(),
        })
    }

    fn thread_views(&mut self) -> Result<Vec<ThreadView>, ApiError> {
        let threads = self.store.review().threads.clone();
        threads.iter().map(|t| self.thread_view(t)).collect()
    }

    /// Whether the working file is not version `version`, or cannot be read.
    fn file_differs(&self, version: u32) -> bool {
        let review = self.store.review();
        let expected = &review.versions[version as usize - 1];
        self.store
            .location()
            .read_working()
            .map_or(true, |bytes| &hash(&bytes) != expected)
    }

    /// The pending approval or submit, logged as delivered.
    fn deliver(&mut self, limit: usize) -> Result<Option<Delivery>, ApiError> {
        let review = self.store.review().clone();
        if let Some(approval) = &review.approval {
            if !approval.delivered {
                self.append(Event::ReviewDelivered {
                    round: approval.round,
                    approved: true,
                })?;
            }
            return Ok(Some(Delivery::Approved(self.approved_view(approval)?)));
        }
        let Some(submit) = &review.submit else {
            return Ok(None);
        };
        self.append(Event::ReviewDelivered {
            round: review.round,
            approved: false,
        })?;
        let in_round: Vec<_> = review.round_threads(review.round).cloned().collect();
        let mut threads = Vec::new();
        for thread in in_round.iter().take(limit) {
            threads.push(self.thread_view(thread)?);
        }
        let redelivered = submit.first_delivery.map(|first| Redelivered {
            first,
            replied: in_round
                .iter()
                .filter(|t| {
                    t.messages
                        .iter()
                        .any(|m| m.author == Author::Agent && m.round == review.round)
                })
                .map(|t| t.id.clone())
                .collect(),
        });
        Ok(Some(Delivery::ReviewSubmitted(Submitted {
            file: review.file.clone(),
            round: review.round,
            version: review.version(),
            summary: submit.summary.clone(),
            truncated: threads.len() < in_round.len(),
            threads,
            total: in_round.len(),
            redelivered,
            file_differs: self.file_differs(review.version()),
        })))
    }

    fn approved_view(&mut self, approval: &Approval) -> Result<Approved, ApiError> {
        let record = self.record(approval)?;
        let review = self.store.review().clone();
        let mut tally = Tally::default();
        for thread in &review.threads {
            tally.total += 1;
            match thread.state {
                ThreadState::Open => tally.open += 1,
                ThreadState::Resolved => tally.resolved += 1,
                ThreadState::Applied(_) => tally.applied += 1,
            }
        }
        Ok(Approved {
            file: review.file.clone(),
            round: approval.round,
            version: approval.version,
            note: approval.note.clone(),
            record,
            file_differs: self.file_differs(approval.version),
            threads: tally,
            discarded: approval.discarded.clone(),
        })
    }

    /// Write the approval's record unless it exists, and return its path.
    fn record(&mut self, approval: &Approval) -> Result<String, ApiError> {
        let threads = self.thread_views()?;
        // Approval is only ever of the current version.
        let approved = self.current.clone();
        let working = self.store.location().read_working();
        let file = self.store.review().file.clone();
        let text = record::compose(&record::Input {
            file: &file,
            approval,
            threads: &threads,
            approved: &approved,
            working: &working,
        });
        self.store
            .write_record(&record::name(&file, approval.at), text.as_bytes())
            .map_err(ApiError::internal)
    }

    /// The block diff from the version before `version`, which is at least 2.
    fn diff(&mut self, version: u32) -> Result<BlockDiff, ApiError> {
        let old = self.blocks(version - 1)?;
        let new = self.blocks(version)?;
        Ok(diff(
            &old.iter().map(|b| b.source.as_str()).collect::<Vec<_>>(),
            &new.iter().map(|b| b.source.as_str()).collect::<Vec<_>>(),
        ))
    }

    /// Block changes from the version before `version` to it, for the page.
    fn changes(&mut self, version: u32) -> Result<Option<Changes>, ApiError> {
        if version < 2 {
            return Ok(None);
        }
        let old = self.blocks(version - 1)?;
        let result = self.diff(version)?;
        Ok(Some(Changes {
            since: version - 1,
            blocks: result
                .blocks
                .iter()
                .map(|&change| BlockChange {
                    change,
                    old_source: match change {
                        Change::Changed { old: was } => Some(old[was].source.clone()),
                        _ => None,
                    },
                })
                .collect(),
            deleted: result
                .deleted
                .iter()
                .map(|&deleted| DeletedBlock {
                    deleted,
                    html: old[deleted.old].html.clone(),
                    source: old[deleted.old].source.clone(),
                })
                .collect(),
        }))
    }

    fn page_view(&mut self) -> Result<PageView, ApiError> {
        let review = self.store.review().clone();
        let version = review.version();
        let phase = if review.approval.is_some() {
            Phase::Approved
        } else if review.submit.is_some() {
            Phase::Submitted
        } else {
            Phase::Open
        };
        let approval = match &review.approval {
            Some(approval) => Some(ApprovalView {
                round: approval.round,
                version: approval.version,
                note: approval.note.clone(),
                record: record_path(&record::name(&review.file, approval.at)),
            }),
            None => None,
        };
        Ok(PageView {
            seq: self.seq,
            file: review.file.clone(),
            round: review.round,
            version,
            phase,
            blocks: self.blocks(version)?.to_vec(),
            changes: self.changes(version)?,
            threads: self.thread_views()?,
            summary: review.submit.as_ref().and_then(|s| s.summary.clone()),
            agent: AgentView {
                listening: self.waiters > 0
                    || self
                        .last_wait
                        .is_some_and(|at| now().saturating_sub(at) < GRACE_SECS),
                last_wait: self.last_wait,
            },
            file_differs: self.file_differs(version),
            approval,
        })
    }
}

/// Everything the page draws, in one answer.
#[derive(Serialize)]
struct PageView {
    seq: u64,
    file: String,
    round: u32,
    version: u32,
    phase: Phase,
    blocks: Vec<Block>,
    #[serde(skip_serializing_if = "Option::is_none")]
    changes: Option<Changes>,
    threads: Vec<ThreadView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    agent: AgentView,
    file_differs: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    approval: Option<ApprovalView>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    /// The reviewer may comment.
    Open,
    /// The agent is revising; the page is read-only.
    Submitted,
    Approved,
}

#[derive(Serialize)]
struct AgentView {
    listening: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_wait: Option<u64>,
}

#[derive(Serialize)]
struct ApprovalView {
    round: u32,
    version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    record: String,
}

#[derive(Serialize)]
struct Changes {
    since: u32,
    blocks: Vec<BlockChange>,
    deleted: Vec<DeletedBlock>,
}

#[derive(Serialize)]
struct BlockChange {
    #[serde(flatten)]
    change: Change,
    /// The block it replaced, for "old source on demand".
    #[serde(skip_serializing_if = "Option::is_none")]
    old_source: Option<String>,
}

#[derive(Serialize)]
struct DeletedBlock {
    #[serde(flatten)]
    deleted: Deleted,
    html: String,
    source: String,
}

fn approved(approval: &Approval) -> String {
    format!(
        "the review was approved at round {}; nothing more can be written to it",
        approval.round
    )
}

fn stale(review: &Review, round: u32, version: u32) -> String {
    if review.submit.is_some() && round == review.round {
        format!("round {round} is submitted and the agent is revising; reload when the next round starts — drafts are kept")
    } else {
        format!(
            "this page shows round {round}, v{version}, and the review is at round {}, v{}; reload — drafts are kept",
            review.round,
            review.version()
        )
    }
}

fn check_id(id: &str) -> Result<(), ApiError> {
    if id.is_empty()
        || id.len() > MAX_ID_CHARS
        || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(ApiError::bad(
            "a message id is 1 to 64 letters, digits or dashes",
        ));
    }
    Ok(())
}

fn non_blank(text: &str, what: &str) -> Result<String, ApiError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad(format!("{what} cannot be empty")));
    }
    Ok(trimmed.to_string())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use serde_json::{json, Value};
    use tempfile::TempDir;
    use tower::ServiceExt;

    const TOKEN: &str = "secret-token";
    const HOST: &str = "127.0.0.1:4000";
    const TEXT: &str = "# Top\n\nFirst paragraph here.\n";

    struct Fixture {
        _dir: TempDir,
        shared: Arc<Server>,
        log: std::path::PathBuf,
    }

    fn fixture() -> Fixture {
        let dir = TempDir::new().unwrap();
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap();
        assert!(status.success());
        let path = dir.path().join("docs/foo.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, TEXT).unwrap();
        let location = Location::find(&path).unwrap();
        let mut opened = None;
        // Another test thread spawning git may hold a copy of the lock's
        // descriptor for a moment; see store's tests.
        for _ in 0..100 {
            match Store::open(&location, TEXT.as_bytes()).unwrap() {
                Opened::Writer(store) => {
                    opened = Some(*store);
                    break;
                }
                Opened::Busy => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        let store = opened.expect("store still locked after a second");
        let info = ServerInfo {
            file: "docs/foo.md".to_string(),
            pid: 0,
            port: 4000,
            token: TOKEN.to_string(),
            version: "test".to_string(),
        };
        Fixture {
            log: location.store_dir().join("events.jsonl"),
            _dir: dir,
            shared: Arc::new(Server::new(store, &info).unwrap()),
        }
    }

    impl Fixture {
        fn lines(&self) -> usize {
            std::fs::read_to_string(&self.log).unwrap().lines().count()
        }

        fn call(&self, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
            self.send(method, uri, HOST, Some(TOKEN), body)
        }

        fn send(
            &self,
            method: &str,
            uri: &str,
            host: &str,
            token: Option<&str>,
            body: Option<Value>,
        ) -> (StatusCode, Value) {
            let mut request = Request::builder()
                .method(method)
                .uri(uri)
                .header(header::HOST, host);
            if let Some(token) = token {
                request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
            }
            let request = match body {
                Some(body) => request
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string())),
                None => request.body(Body::empty()),
            }
            .unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let response = router(self.shared.clone()).oneshot(request).await.unwrap();
                let status = response.status();
                let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
                (status, value)
            })
        }

        fn comment(&self, round: u32, message: &str) -> (StatusCode, Value) {
            self.call(
                "POST",
                "/api/threads",
                Some(json!({
                    "round": round, "version": round, "message": message,
                    "kind": "comment", "body": "Too vague.",
                    "anchor": {
                        "quote": "paragraph here", "prefix": "First ", "suffix": "",
                        "blocks": [1], "lines": [3, 3], "headings": ["Top"], "version": round
                    }
                })),
            )
        }

        fn submit(&self, round: u32) -> (StatusCode, Value) {
            self.call("POST", "/api/submit", Some(json!({ "round": round })))
        }
    }

    // Case: docs/cases/gist-md-review.md#client-token-and-host
    #[test]
    fn a_request_without_the_token_or_with_another_host_is_refused_and_appends_nothing() {
        let fixture = fixture();
        let before = fixture.lines();
        let body = || Some(json!({ "round": 1, "summary": "sneaky" }));
        let (status, _) = fixture.send("POST", "/api/submit", HOST, None, body());
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) = fixture.send("POST", "/api/submit", HOST, Some("wrong-token!"), body());
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) = fixture.send(
            "POST",
            "/api/submit",
            "evil.example:4000",
            Some(TOKEN),
            body(),
        );
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _) =
            fixture.send("POST", "/api/submit", "localhost:4000", Some(TOKEN), body());
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(fixture.lines(), before);

        let (status, _) = fixture.send("GET", "/?token=secret-token", HOST, None, None);
        assert_eq!(status, StatusCode::OK, "the page URL carries the token");
        for wrong in ["secret-tokem", "secret", ""] {
            let uri = format!("/?token={wrong}");
            let (status, _) = fixture.send("GET", &uri, HOST, None, None);
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{wrong:?}");
        }
    }

    // Case: docs/cases/gist-md-review.md#page-render-safety
    #[test]
    fn the_page_forbids_outside_requests_and_does_not_leak_its_url() {
        let fixture = fixture();
        let request = Request::builder()
            .uri("/?token=secret-token")
            .header(header::HOST, HOST)
            .body(Body::empty())
            .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let response = runtime
            .block_on(router(fixture.shared.clone()).oneshot(request))
            .unwrap();
        let headers = response.headers();
        let csp = headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
        assert!(csp.starts_with("default-src 'none';"), "{csp}");
        assert!(csp.contains("connect-src 'self'"), "{csp}");
        assert!(!csp.contains("http"), "{csp}");
        assert_eq!(headers[header::REFERRER_POLICY], "no-referrer");
    }

    // Case: docs/cases/gist-md-review.md#stale-write
    #[test]
    fn writes_against_another_round_or_version_get_409_and_append_nothing() {
        let fixture = fixture();
        assert_eq!(fixture.comment(1, "m-1").0, StatusCode::OK);
        let before = fixture.lines();

        assert_eq!(fixture.comment(2, "m-2").0, StatusCode::CONFLICT);
        let edit = json!({ "round": 1, "version": 2, "body": "edited" });
        assert_eq!(
            fixture.call("PATCH", "/api/messages/m-1", Some(edit)).0,
            StatusCode::CONFLICT
        );
        let at = json!({ "round": 0, "version": 1 });
        assert_eq!(
            fixture.call("DELETE", "/api/messages/m-1", Some(at)).0,
            StatusCode::CONFLICT
        );
        assert_eq!(fixture.submit(2).0, StatusCode::CONFLICT);
        let approve = json!({ "round": 1, "version": 2 });
        assert_eq!(
            fixture.call("POST", "/api/approve", Some(approve)).0,
            StatusCode::CONFLICT
        );
        assert_eq!(fixture.lines(), before);
    }

    // Case: docs/cases/gist-md-review.md#stale-write
    #[test]
    fn after_submit_the_round_is_closed_to_the_page_and_a_repeated_submit_is_idempotent() {
        let fixture = fixture();
        fixture.comment(1, "m-1");
        assert_eq!(fixture.submit(1).0, StatusCode::OK);
        let before = fixture.lines();

        assert_eq!(
            fixture.submit(1).0,
            StatusCode::OK,
            "a repeat answers as the first"
        );
        assert_eq!(fixture.comment(1, "m-2").0, StatusCode::CONFLICT);
        let edit = json!({ "round": 1, "version": 1, "body": "edited" });
        let (status, body) = fixture.call("PATCH", "/api/messages/m-1", Some(edit));
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(
            body["message"].as_str().unwrap().contains("submitted"),
            "{body}"
        );
        let at = json!({ "round": 1, "version": 1 });
        assert_eq!(
            fixture.call("DELETE", "/api/messages/m-1", Some(at)).0,
            StatusCode::CONFLICT
        );
        assert_eq!(fixture.lines(), before);
    }

    #[test]
    fn a_retried_comment_appends_nothing_and_names_its_thread() {
        let fixture = fixture();
        let (_, first) = fixture.comment(1, "m-1");
        let before = fixture.lines();
        let (status, again) = fixture.comment(1, "m-1");
        assert_eq!(status, StatusCode::OK);
        assert_eq!(again, first);
        assert_eq!(fixture.lines(), before);
        assert_eq!(fixture.comment(1, "m-2").1["thread"], "t2");
    }

    #[test]
    fn a_thread_id_is_never_reused_after_a_delete() {
        let fixture = fixture();
        fixture.comment(1, "m-1");
        fixture.comment(1, "m-2");
        let at = json!({ "round": 1, "version": 1 });
        assert_eq!(
            fixture.call("DELETE", "/api/messages/m-2", Some(at)).0,
            StatusCode::OK
        );
        assert_eq!(fixture.comment(1, "m-3").1["thread"], "t3");
    }

    #[test]
    fn a_submit_needs_a_comment_or_a_summary() {
        let fixture = fixture();
        let before = fixture.lines();
        assert_eq!(fixture.submit(1).0, StatusCode::BAD_REQUEST);
        let blank = json!({ "round": 1, "summary": "  " });
        assert_eq!(
            fixture.call("POST", "/api/submit", Some(blank)).0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(fixture.lines(), before);
        let summary = json!({ "round": 1, "summary": "Overall fine." });
        assert_eq!(
            fixture.call("POST", "/api/submit", Some(summary)).0,
            StatusCode::OK
        );
    }

    #[test]
    fn an_anchor_on_another_version_than_its_post_is_refused() {
        let fixture = fixture();
        let before = fixture.lines();
        let mismatched = json!({
            "round": 1, "version": 1, "message": "m-1", "kind": "comment", "body": "x",
            "anchor": { "quote": "Top", "prefix": "", "suffix": "", "blocks": [0],
                        "lines": [1, 1], "headings": ["Top"], "version": 2 }
        });
        assert_eq!(
            fixture.call("POST", "/api/threads", Some(mismatched)).0,
            StatusCode::CONFLICT
        );
        assert_eq!(fixture.lines(), before);
    }

    // Case: docs/cases/gist-md-review.md#stale-write
    #[test]
    fn only_the_current_rounds_reviewer_comments_can_change() {
        let fixture = fixture();
        fixture.comment(1, "m-1");
        fixture.submit(1);
        let reply = json!({ "thread": "t1", "outcome": "declined", "note": "No." });
        fixture.call("POST", "/api/reply", Some(reply));
        fixture.call("POST", "/api/next", None);
        let agent = fixture.shared.lock().store.review().threads[0].messages[1]
            .id
            .clone();
        let before = fixture.lines();

        let edit = json!({ "round": 2, "version": 2, "body": "edited" });
        for id in ["m-1", agent.as_str()] {
            let uri = format!("/api/messages/{id}");
            assert_eq!(
                fixture.call("PATCH", &uri, Some(edit.clone())).0,
                StatusCode::CONFLICT,
                "{id}"
            );
            let at = json!({ "round": 2, "version": 2 });
            assert_eq!(
                fixture.call("DELETE", &uri, Some(at)).0,
                StatusCode::CONFLICT,
                "{id}"
            );
        }
        assert_eq!(fixture.lines(), before);
    }

    #[test]
    fn a_question_waits_for_the_explain_flow() {
        let fixture = fixture();
        let before = fixture.lines();
        let question = json!({
            "round": 1, "version": 1, "message": "m-1", "kind": "question", "body": "Why?",
            "anchor": { "quote": "Top", "prefix": "", "suffix": "", "blocks": [0],
                        "lines": [1, 1], "headings": ["Top"], "version": 1 }
        });
        assert_eq!(
            fixture.call("POST", "/api/threads", Some(question)).0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(fixture.lines(), before);
    }

    #[test]
    fn the_agent_can_reply_and_close_a_round_only_after_a_submit() {
        let fixture = fixture();
        fixture.comment(1, "m-1");
        let before = fixture.lines();
        let reply = json!({ "thread": "t1", "outcome": "applied", "note": "Done." });
        assert_eq!(
            fixture.call("POST", "/api/reply", Some(reply.clone())).0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            fixture.call("POST", "/api/next", None).0,
            StatusCode::CONFLICT
        );
        assert_eq!(fixture.lines(), before);

        fixture.submit(1);
        assert_eq!(
            fixture.call("POST", "/api/reply", Some(reply)).0,
            StatusCode::OK
        );
        let answered = json!({ "thread": "t1", "outcome": "answered", "note": "?" });
        assert_eq!(
            fixture.call("POST", "/api/reply", Some(answered)).0,
            StatusCode::BAD_REQUEST,
            "answering is the answerer's"
        );
        let unknown = json!({ "thread": "t9", "outcome": "declined", "note": "?" });
        assert_eq!(
            fixture.call("POST", "/api/reply", Some(unknown)).0,
            StatusCode::NOT_FOUND
        );
        let (status, started) = fixture.call("POST", "/api/next", None);
        assert_eq!(status, StatusCode::OK);
        assert_eq!(started["round"], 2);
        assert_eq!(
            started["changed"], 0,
            "an unchanged file still makes a version"
        );
    }

    // Case: docs/cases/gist-md-review.md#approve
    #[test]
    fn after_approval_every_write_is_refused_naming_it() {
        let fixture = fixture();
        fixture.comment(1, "m-1");
        let approve = json!({ "round": 1, "version": 1, "note": "Ship it." });
        let (status, approved) = fixture.call("POST", "/api/approve", Some(approve.clone()));
        assert_eq!(status, StatusCode::OK);
        assert!(approved["record"]
            .as_str()
            .unwrap()
            .starts_with(".md-review/records/docs-foo-md-"));
        let before = fixture.lines();

        let (status, again) = fixture.call("POST", "/api/approve", Some(approve));
        assert_eq!((status, &again), (StatusCode::OK, &approved));
        let reply = json!({ "thread": "t1", "outcome": "applied", "note": "Done." });
        for (method, uri, body) in [
            ("POST", "/api/reply", Some(reply)),
            ("POST", "/api/next", None),
            (
                "POST",
                "/api/submit",
                Some(json!({ "round": 1, "summary": "x" })),
            ),
        ] {
            let (status, body) = fixture.call(method, uri, body);
            assert_eq!(status, StatusCode::CONFLICT, "{uri}");
            assert!(
                body["message"].as_str().unwrap().contains("approved"),
                "{uri}"
            );
        }
        assert_eq!(fixture.comment(1, "m-2").0, StatusCode::CONFLICT);
        assert_eq!(fixture.lines(), before);
    }

    // Case: docs/cases/gist-md-review.md#approve
    #[test]
    fn threads_pending_at_approve_are_discarded_but_submitted_ones_are_not() {
        let fixture = fixture();
        fixture.comment(1, "m-1");
        let approve = json!({ "round": 1, "version": 1 });
        fixture.call("POST", "/api/approve", Some(approve));
        let review = fixture.shared.lock().store.review().clone();
        assert_eq!(review.approval.unwrap().discarded, ["t1"]);

        let submitted = self::fixture();
        submitted.comment(1, "m-1");
        submitted.submit(1);
        let approve = json!({ "round": 1, "version": 1 });
        assert_eq!(
            submitted.call("POST", "/api/approve", Some(approve)).0,
            StatusCode::OK,
            "approve is allowed while a submit is pending"
        );
        let review = submitted.shared.lock().store.review().clone();
        assert!(review.approval.unwrap().discarded.is_empty());
    }

    #[test]
    fn the_page_long_poll_answers_at_once_when_behind_and_after_the_hold_when_current() {
        let fixture = fixture();
        let (_, view) = fixture.call("GET", "/api/review", None);
        let seq = view["seq"].as_u64().unwrap();
        assert_eq!(view["phase"], "open");
        assert_eq!(view["blocks"][1]["text"], "First paragraph here.");

        let started = std::time::Instant::now();
        let (status, same) = fixture.call("GET", &format!("/api/review?after={seq}&hold=1"), None);
        assert_eq!(status, StatusCode::OK);
        assert_eq!(same["seq"], seq);
        assert!(started.elapsed() >= Duration::from_millis(900));

        fixture.comment(1, "m-1");
        let started = std::time::Instant::now();
        let (_, changed) = fixture.call("GET", &format!("/api/review?after={seq}&hold=30"), None);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(changed["threads"][0]["id"], "t1");
    }

    #[test]
    fn wait_answers_204_when_its_hold_runs_out() {
        let fixture = fixture();
        let (status, _) = fixture.call("GET", "/api/wait?hold=1&limit=5", None);
        assert_eq!(status, StatusCode::NO_CONTENT);
        let state = fixture.shared.lock();
        assert_eq!(state.waiters, 0);
        assert!(state.last_wait.is_some());
    }
}
