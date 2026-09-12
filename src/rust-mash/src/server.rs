//! axum endpoint that re-verifies a browser-reported drop with the same core
//! the browser used, then hands the verified result to the app's backend.

use axum::{
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use ores_dnd_core::{
    evaluate_policy, DndDropPolicy, DndDropResult, DndEnvelope, DndError, DndRejectCode,
    ValidationOptions, DEFAULT_MAX_PAYLOAD_BYTES,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};

/// What the browser adapter POSTs after an accepted drop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DropCommitRequest {
    pub envelope: DndEnvelope,
    pub result: DndDropResult,
}

/// The app's side of the endpoint: where policies live and how an accepted,
/// server-verified drop is committed (ores-forms / opto-sync on the server).
pub trait DropCommitBackend: Send + Sync + 'static {
    /// The policy for a zone id, or `None` if the zone does not exist.
    fn policy_for(&self, target_id: &str) -> Option<DndDropPolicy>;
    /// Commit a verified accepted drop. Called only when [`verify_drop_commit`]
    /// accepted; an error becomes HTTP 422.
    fn commit(&self, envelope: &DndEnvelope, result: &DndDropResult) -> Result<(), DndError>;
    /// Durable replay protection: has this `dragId` already been committed?
    /// The router also keeps a bounded in-memory window (see
    /// [`RouterOptions::replay_window`]); implement this for persistence
    /// across restarts and replicas. Default: never.
    fn already_committed(&self, _drag_id: &str) -> bool {
        false
    }
}

/// Error code returned when a `dragId` is committed twice (HTTP 409).
pub const DUPLICATE_DRAG: &str = "duplicate-drag";
/// Error code returned when the request lacks the `HX-Request` header (HTTP 403).
pub const MISSING_HX_REQUEST: &str = "missing-hx-request";

/// Endpoint hardening knobs.
#[derive(Debug, Clone)]
pub struct RouterOptions {
    /// Mount path; default [`DEFAULT_COMMIT_PATH`].
    pub path: String,
    /// Maximum request body; default twice the envelope limit (the result and
    /// JSON framing are small). Larger bodies are refused before parsing.
    pub body_limit_bytes: usize,
    /// How many recently committed `dragId`s the router remembers; a repeat
    /// within the window is refused with `duplicate-drag`. 0 disables.
    pub replay_window: usize,
    /// Require the `HX-Request` header the browser adapters always send. A
    /// cross-site form post cannot set custom headers, so on cookie-authenticated
    /// apps this is the CSRF guard for the endpoint. Default true.
    pub require_hx_request: bool,
}

impl Default for RouterOptions {
    fn default() -> Self {
        Self {
            path: DEFAULT_COMMIT_PATH.to_owned(),
            body_limit_bytes: 2 * DEFAULT_MAX_PAYLOAD_BYTES,
            replay_window: 4096,
            require_hx_request: true,
        }
    }
}

/// Bounded FIFO set of recently committed drag ids.
#[derive(Debug, Default)]
pub struct ReplayWindow {
    capacity: usize,
    order: VecDeque<String>,
    seen: HashSet<String>,
}

impl ReplayWindow {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            order: VecDeque::with_capacity(capacity.min(4096)),
            seen: HashSet::new(),
        }
    }

    pub fn contains(&self, drag_id: &str) -> bool {
        self.seen.contains(drag_id)
    }

    /// Record a commit; returns false if it was already present.
    pub fn insert(&mut self, drag_id: &str) -> bool {
        if self.capacity == 0 {
            return true;
        }
        if !self.seen.insert(drag_id.to_owned()) {
            return false;
        }
        self.order.push_back(drag_id.to_owned());
        while self.order.len() > self.capacity {
            if let Some(old) = self.order.pop_front() {
                self.seen.remove(&old);
            }
        }
        true
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

#[derive(Clone)]
struct CommitState {
    backend: Arc<dyn DropCommitBackend>,
    window: Arc<Mutex<ReplayWindow>>,
    require_hx_request: bool,
}

fn rejected(drag_id: &str, target_id: Option<String>, code: DndRejectCode) -> DndDropResult {
    DndDropResult {
        drag_id: drag_id.to_owned(),
        accepted: false,
        operation: None,
        target_id,
        error_code: Some(code.wire().to_owned()),
    }
}

/// Pure verification: the server never trusts the browser's verdict. The
/// envelope is re-validated, the zone's policy re-evaluated with the reported
/// operation as the preference, and the result must agree.
pub fn verify_drop_commit(
    request: &DropCommitRequest,
    backend: &dyn DropCommitBackend,
) -> DndDropResult {
    let DropCommitRequest { envelope, result } = request;
    if envelope.validate(ValidationOptions::default()).is_err()
        || result.drag_id != envelope.drag_id
    {
        return rejected(
            &result.drag_id,
            result.target_id.clone(),
            DndRejectCode::InvalidEnvelope,
        );
    }
    let Some(target_id) = result.target_id.clone() else {
        return rejected(&result.drag_id, None, DndRejectCode::NoActiveTarget);
    };
    if !result.accepted {
        return rejected(&result.drag_id, Some(target_id), DndRejectCode::Cancelled);
    }
    let Some(policy) = backend.policy_for(&target_id) else {
        return rejected(
            &result.drag_id,
            Some(target_id),
            DndRejectCode::TargetMismatch,
        );
    };
    if policy.target_id != target_id || policy.validate().is_err() {
        return rejected(
            &result.drag_id,
            Some(target_id),
            DndRejectCode::TargetMismatch,
        );
    }
    match evaluate_policy(envelope, &policy, result.operation) {
        Ok(operation) if Some(operation) == result.operation => DndDropResult {
            drag_id: result.drag_id.clone(),
            accepted: true,
            operation: Some(operation),
            target_id: Some(target_id),
            error_code: None,
        },
        Ok(_) => rejected(
            &result.drag_id,
            Some(target_id),
            DndRejectCode::NoCommonOperation,
        ),
        Err(code) => rejected(&result.drag_id, Some(target_id), code),
    }
}

/// Backends report failures as wire-safe `ErrorCode`s (lowercase kebab-case);
/// anything else — an exception message, a path, a query — collapses to
/// `commit-failed` so no internal detail leaks to the browser.
pub fn commit_error_code(error: &DndError) -> String {
    if ores_dnd_core::wire::is_error_code(&error.0) {
        error.0.clone()
    } else {
        "commit-failed".to_owned()
    }
}

fn reply(status: StatusCode, result: DndDropResult) -> Response {
    let mut response = (status, Json(result)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn drop_commit_handler(
    State(state): State<CommitState>,
    headers: HeaderMap,
    Json(request): Json<DropCommitRequest>,
) -> Response {
    let refuse = |code: &str| DndDropResult {
        drag_id: request.result.drag_id.clone(),
        accepted: false,
        operation: None,
        target_id: request.result.target_id.clone(),
        error_code: Some(code.to_owned()),
    };
    if state.require_hx_request && !headers.contains_key("hx-request") {
        return reply(StatusCode::FORBIDDEN, refuse(MISSING_HX_REQUEST));
    }
    let verified = verify_drop_commit(&request, state.backend.as_ref());
    if !verified.accepted {
        return reply(StatusCode::UNPROCESSABLE_ENTITY, verified);
    }
    let duplicate = state.backend.already_committed(&verified.drag_id)
        || state
            .window
            .lock()
            .map(|w| w.contains(&verified.drag_id))
            .unwrap_or(false);
    if duplicate {
        return reply(StatusCode::CONFLICT, refuse(DUPLICATE_DRAG));
    }
    match state.backend.commit(&request.envelope, &verified) {
        Ok(()) => {
            if let Ok(mut window) = state.window.lock() {
                window.insert(&verified.drag_id);
            }
            reply(StatusCode::OK, verified)
        }
        Err(error) => reply(
            StatusCode::UNPROCESSABLE_ENTITY,
            DndDropResult {
                accepted: false,
                operation: None,
                error_code: Some(commit_error_code(&error)),
                ..verified
            },
        ),
    }
}

/// Default mount path used by the TypeScript htmx adapter when a zone carries
/// no explicit `data-ores-dnd-commit`.
pub const DEFAULT_COMMIT_PATH: &str = "/ores-dnd/drop";

/// A router exposing `POST <path>` (default [`DEFAULT_COMMIT_PATH`]) with the
/// default [`RouterOptions`]. Its state is self-contained, so it merges into
/// any app router regardless of the app's own state type:
/// `app.merge(ores_dnd_mash::server::router(backend, None))`.
pub fn router<S>(backend: Arc<dyn DropCommitBackend>, path: Option<&str>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let mut options = RouterOptions::default();
    if let Some(path) = path {
        options.path = path.to_owned();
    }
    router_with(backend, options)
}

/// [`router`] with explicit hardening options.
pub fn router_with<S>(backend: Arc<dyn DropCommitBackend>, options: RouterOptions) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let state = CommitState {
        backend,
        window: Arc::new(Mutex::new(ReplayWindow::new(options.replay_window))),
        require_hx_request: options.require_hx_request,
    };
    Router::new()
        .route(&options.path, post(drop_commit_handler))
        .layer(DefaultBodyLimit::max(options.body_limit_bytes))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{header, Request};
    use http_body_util::BodyExt;
    use ores_dnd_core::{decode_envelope_json, DndItemKind, DndOperation};
    use std::sync::Mutex;
    use tower::ServiceExt;

    const VALID: &str =
        include_str!("../../../contracts/instances/DndEnvelope/valid/text-copy.json");

    #[derive(Default)]
    struct Backend {
        committed: Mutex<Vec<String>>,
        fail_commit: bool,
    }

    impl DropCommitBackend for Backend {
        fn policy_for(&self, target_id: &str) -> Option<DndDropPolicy> {
            match target_id {
                "zone-a" => Some(DndDropPolicy::new(
                    "zone-a",
                    &[DndOperation::Copy, DndOperation::Move],
                    &[DndItemKind::Text],
                )),
                "zone-json" => Some(DndDropPolicy::new(
                    "zone-json",
                    &[DndOperation::Copy],
                    &[DndItemKind::Json],
                )),
                _ => None,
            }
        }
        fn commit(&self, envelope: &DndEnvelope, result: &DndDropResult) -> Result<(), DndError> {
            if self.fail_commit {
                return Err(DndError("storage-unavailable".into()));
            }
            self.committed.lock().unwrap().push(format!(
                "{}@{}",
                envelope.drag_id,
                result.target_id.clone().unwrap()
            ));
            Ok(())
        }
    }

    fn request(target: &str, accepted: bool, op: Option<DndOperation>) -> DropCommitRequest {
        let envelope = decode_envelope_json(VALID, ValidationOptions::default()).unwrap();
        DropCommitRequest {
            result: DndDropResult {
                drag_id: envelope.drag_id.clone(),
                accepted,
                operation: op,
                target_id: Some(target.into()),
                error_code: None,
            },
            envelope,
        }
    }

    #[test]
    fn server_reverifies_the_browser_verdict() {
        let backend = Backend::default();
        let ok = verify_drop_commit(&request("zone-a", true, Some(DndOperation::Move)), &backend);
        assert!(ok.accepted);
        // browser claims copy on a json-only zone → the policy, not the browser, decides
        let bad = verify_drop_commit(
            &request("zone-json", true, Some(DndOperation::Copy)),
            &backend,
        );
        assert_eq!(bad.error_code.as_deref(), Some("item-kind-not-accepted"));
        // browser claims an operation the policy allows but negotiation would not pick → refused
        let link = verify_drop_commit(&request("zone-a", true, Some(DndOperation::Link)), &backend);
        assert_eq!(link.error_code.as_deref(), Some("no-common-operation"));
        let unknown = verify_drop_commit(
            &request("zone-zzz", true, Some(DndOperation::Copy)),
            &backend,
        );
        assert_eq!(unknown.error_code.as_deref(), Some("target-mismatch"));
        let cancelled = verify_drop_commit(&request("zone-a", false, None), &backend);
        assert_eq!(cancelled.error_code.as_deref(), Some("cancelled"));
    }

    async fn post_at(
        app: Router,
        path: &str,
        body: &str,
        hx: bool,
    ) -> (StatusCode, DndDropResult, HeaderMap) {
        let mut req = Request::post(path).header(header::CONTENT_TYPE, "application/json");
        if hx {
            req = req.header("HX-Request", "true");
        }
        let response = app
            .oneshot(req.body(Body::from(body.to_owned())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap(), headers)
    }

    async fn post(app: Router, body: &str) -> (StatusCode, DndDropResult) {
        let (status, result, _) = post_at(app, DEFAULT_COMMIT_PATH, body, true).await;
        (status, result)
    }

    #[tokio::test]
    async fn endpoint_commits_only_verified_drops() {
        let backend = Arc::new(Backend::default());
        let app: Router =
            Router::new().merge(router(backend.clone() as Arc<dyn DropCommitBackend>, None));
        let body =
            serde_json::to_string(&request("zone-a", true, Some(DndOperation::Move))).unwrap();
        let (status, result) = post(app.clone(), &body).await;
        assert_eq!(status, StatusCode::OK);
        assert!(result.accepted);
        assert_eq!(*backend.committed.lock().unwrap(), vec!["drag-0001@zone-a"]);

        let body =
            serde_json::to_string(&request("zone-json", true, Some(DndOperation::Copy))).unwrap();
        let (status, result) = post(app.clone(), &body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(!result.accepted);
        assert_eq!(backend.committed.lock().unwrap().len(), 1);

        // a structurally invalid body is refused by the extractor before any policy runs
        let response = app
            .oneshot(
                Request::post(DEFAULT_COMMIT_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("HX-Request", "true")
                    .body(Body::from(r#"{"envelope":{},"result":{}}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(backend.committed.lock().unwrap().len(), 1);
    }

    #[test]
    fn commit_errors_never_leak_internal_text() {
        assert_eq!(
            commit_error_code(&DndError("storage-unavailable".into())),
            "storage-unavailable"
        );
        assert_eq!(
            commit_error_code(&DndError(
                "Postgres: relation \"drops\" does not exist".into()
            )),
            "commit-failed"
        );
    }

    #[tokio::test]
    async fn commit_failure_is_reported_not_hidden() {
        let backend = Arc::new(Backend {
            fail_commit: true,
            ..Default::default()
        });
        let app: Router = router(backend as Arc<dyn DropCommitBackend>, Some("/drops"));
        let body =
            serde_json::to_string(&request("zone-a", true, Some(DndOperation::Copy))).unwrap();
        let (status, result, _) = post_at(app, "/drops", &body, true).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(result.error_code.as_deref(), Some("storage-unavailable"));
    }

    #[tokio::test]
    async fn a_drag_id_commits_once() {
        let backend = Arc::new(Backend::default());
        let app: Router = router(backend.clone() as Arc<dyn DropCommitBackend>, None);
        let body =
            serde_json::to_string(&request("zone-a", true, Some(DndOperation::Move))).unwrap();
        let (status, _, headers) = post_at(app.clone(), DEFAULT_COMMIT_PATH, &body, true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
        let (status, result, _) = post_at(app.clone(), DEFAULT_COMMIT_PATH, &body, true).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(result.error_code.as_deref(), Some(DUPLICATE_DRAG));
        assert_eq!(
            backend.committed.lock().unwrap().len(),
            1,
            "the replayed drop never reaches the backend"
        );
    }

    #[tokio::test]
    async fn hx_request_header_is_required_by_default_and_optional_on_request() {
        let backend = Arc::new(Backend::default());
        let body =
            serde_json::to_string(&request("zone-a", true, Some(DndOperation::Copy))).unwrap();
        let strict: Router = router(backend.clone() as Arc<dyn DropCommitBackend>, None);
        let (status, result, _) = post_at(strict, DEFAULT_COMMIT_PATH, &body, false).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(result.error_code.as_deref(), Some(MISSING_HX_REQUEST));
        assert!(backend.committed.lock().unwrap().is_empty());
        let relaxed: Router = router_with(
            backend.clone() as Arc<dyn DropCommitBackend>,
            RouterOptions {
                require_hx_request: false,
                ..RouterOptions::default()
            },
        );
        let (status, _, _) = post_at(relaxed, DEFAULT_COMMIT_PATH, &body, false).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn oversized_bodies_are_refused_before_parsing() {
        let backend = Arc::new(Backend::default());
        let app: Router = router_with(
            backend.clone() as Arc<dyn DropCommitBackend>,
            RouterOptions {
                body_limit_bytes: 256,
                ..RouterOptions::default()
            },
        );
        let mut req = request("zone-a", true, Some(DndOperation::Copy));
        req.envelope.items[0].data = "x".repeat(1024);
        let body = serde_json::to_string(&req).unwrap();
        let response = app
            .oneshot(
                Request::post(DEFAULT_COMMIT_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("HX-Request", "true")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert!(backend.committed.lock().unwrap().is_empty());
    }

    #[test]
    fn replay_window_is_bounded_fifo() {
        let mut window = ReplayWindow::new(2);
        assert!(window.insert("a") && window.insert("b"));
        assert!(!window.insert("a"), "a repeat is reported");
        assert!(window.insert("c"));
        assert_eq!(window.len(), 2);
        assert!(!window.contains("a") && window.contains("b") && window.contains("c"));
        let mut disabled = ReplayWindow::new(0);
        assert!(disabled.insert("x") && disabled.insert("x") && disabled.is_empty());
    }

    #[test]
    fn backend_durable_dedupe_is_consulted() {
        struct Durable;
        impl DropCommitBackend for Durable {
            fn policy_for(&self, target_id: &str) -> Option<DndDropPolicy> {
                Some(DndDropPolicy::new(
                    target_id,
                    &[DndOperation::Copy],
                    &[DndItemKind::Text],
                ))
            }
            fn commit(&self, _: &DndEnvelope, _: &DndDropResult) -> Result<(), DndError> {
                Ok(())
            }
            fn already_committed(&self, drag_id: &str) -> bool {
                drag_id == "drag-0001"
            }
        }
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let app: Router = router(Arc::new(Durable) as Arc<dyn DropCommitBackend>, None);
            let body =
                serde_json::to_string(&request("zone-a", true, Some(DndOperation::Copy))).unwrap();
            let (status, result, _) = post_at(app, DEFAULT_COMMIT_PATH, &body, true).await;
            assert_eq!(status, StatusCode::CONFLICT);
            assert_eq!(result.error_code.as_deref(), Some(DUPLICATE_DRAG));
        });
    }
}
