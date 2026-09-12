//! axum endpoint that re-verifies a browser-reported drop with the same core
//! the browser used, then hands the verified result to the app's backend.

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use ores_dnd_core::{
    evaluate_policy, DndDropPolicy, DndDropResult, DndEnvelope, DndError, DndRejectCode, ValidationOptions,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
pub fn verify_drop_commit(request: &DropCommitRequest, backend: &dyn DropCommitBackend) -> DndDropResult {
    let DropCommitRequest { envelope, result } = request;
    if envelope.validate(ValidationOptions::default()).is_err() || result.drag_id != envelope.drag_id {
        return rejected(&result.drag_id, result.target_id.clone(), DndRejectCode::InvalidEnvelope);
    }
    let Some(target_id) = result.target_id.clone() else {
        return rejected(&result.drag_id, None, DndRejectCode::NoActiveTarget);
    };
    if !result.accepted {
        return rejected(&result.drag_id, Some(target_id), DndRejectCode::Cancelled);
    }
    let Some(policy) = backend.policy_for(&target_id) else {
        return rejected(&result.drag_id, Some(target_id), DndRejectCode::TargetMismatch);
    };
    if policy.target_id != target_id || policy.validate().is_err() {
        return rejected(&result.drag_id, Some(target_id), DndRejectCode::TargetMismatch);
    }
    match evaluate_policy(envelope, &policy, result.operation) {
        Ok(operation) if Some(operation) == result.operation => DndDropResult {
            drag_id: result.drag_id.clone(),
            accepted: true,
            operation: Some(operation),
            target_id: Some(target_id),
            error_code: None,
        },
        Ok(_) => rejected(&result.drag_id, Some(target_id), DndRejectCode::NoCommonOperation),
        Err(code) => rejected(&result.drag_id, Some(target_id), code),
    }
}

async fn drop_commit_handler(
    State(backend): State<Arc<dyn DropCommitBackend>>,
    Json(request): Json<DropCommitRequest>,
) -> (StatusCode, Json<DndDropResult>) {
    let verified = verify_drop_commit(&request, backend.as_ref());
    if !verified.accepted {
        return (StatusCode::UNPROCESSABLE_ENTITY, Json(verified));
    }
    match backend.commit(&request.envelope, &verified) {
        Ok(()) => (StatusCode::OK, Json(verified)),
        Err(error) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(DndDropResult { accepted: false, operation: None, error_code: Some(error.0), ..verified }),
        ),
    }
}

/// Default mount path used by the TypeScript htmx adapter when a zone carries
/// no explicit `data-ores-dnd-commit`.
pub const DEFAULT_COMMIT_PATH: &str = "/ores-dnd/drop";

/// A router exposing `POST <path>` (default [`DEFAULT_COMMIT_PATH`]). Its state
/// is self-contained, so it merges into any app router regardless of the
/// app's own state type: `app.merge(ores_dnd_mash::server::router(backend, None))`.
pub fn router<S>(backend: Arc<dyn DropCommitBackend>, path: Option<&str>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(path.unwrap_or(DEFAULT_COMMIT_PATH), post(drop_commit_handler))
        .with_state(backend)
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

    const VALID: &str = include_str!("../../../contracts/instances/DndEnvelope/valid/text-copy.json");

    #[derive(Default)]
    struct Backend {
        committed: Mutex<Vec<String>>,
        fail_commit: bool,
    }

    impl DropCommitBackend for Backend {
        fn policy_for(&self, target_id: &str) -> Option<DndDropPolicy> {
            match target_id {
                "zone-a" => Some(DndDropPolicy::new("zone-a", &[DndOperation::Copy, DndOperation::Move], &[DndItemKind::Text])),
                "zone-json" => Some(DndDropPolicy::new("zone-json", &[DndOperation::Copy], &[DndItemKind::Json])),
                _ => None,
            }
        }
        fn commit(&self, envelope: &DndEnvelope, result: &DndDropResult) -> Result<(), DndError> {
            if self.fail_commit {
                return Err(DndError("storage-unavailable".into()));
            }
            self.committed.lock().unwrap().push(format!("{}@{}", envelope.drag_id, result.target_id.clone().unwrap()));
            Ok(())
        }
    }

    fn request(target: &str, accepted: bool, op: Option<DndOperation>) -> DropCommitRequest {
        let envelope = decode_envelope_json(VALID, ValidationOptions::default()).unwrap();
        DropCommitRequest {
            result: DndDropResult { drag_id: envelope.drag_id.clone(), accepted, operation: op, target_id: Some(target.into()), error_code: None },
            envelope,
        }
    }

    #[test]
    fn server_reverifies_the_browser_verdict() {
        let backend = Backend::default();
        let ok = verify_drop_commit(&request("zone-a", true, Some(DndOperation::Move)), &backend);
        assert!(ok.accepted);
        // browser claims copy on a json-only zone → the policy, not the browser, decides
        let bad = verify_drop_commit(&request("zone-json", true, Some(DndOperation::Copy)), &backend);
        assert_eq!(bad.error_code.as_deref(), Some("item-kind-not-accepted"));
        // browser claims an operation the policy allows but negotiation would not pick → refused
        let link = verify_drop_commit(&request("zone-a", true, Some(DndOperation::Link)), &backend);
        assert_eq!(link.error_code.as_deref(), Some("no-common-operation"));
        let unknown = verify_drop_commit(&request("zone-zzz", true, Some(DndOperation::Copy)), &backend);
        assert_eq!(unknown.error_code.as_deref(), Some("target-mismatch"));
        let cancelled = verify_drop_commit(&request("zone-a", false, None), &backend);
        assert_eq!(cancelled.error_code.as_deref(), Some("cancelled"));
    }

    async fn post(app: Router, body: &str) -> (StatusCode, DndDropResult) {
        let response = app
            .oneshot(
                Request::post(DEFAULT_COMMIT_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_owned()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn endpoint_commits_only_verified_drops() {
        let backend = Arc::new(Backend::default());
        let app: Router = Router::new().merge(router(backend.clone() as Arc<dyn DropCommitBackend>, None));
        let body = serde_json::to_string(&request("zone-a", true, Some(DndOperation::Move))).unwrap();
        let (status, result) = post(app.clone(), &body).await;
        assert_eq!(status, StatusCode::OK);
        assert!(result.accepted);
        assert_eq!(*backend.committed.lock().unwrap(), vec!["drag-0001@zone-a"]);

        let body = serde_json::to_string(&request("zone-json", true, Some(DndOperation::Copy))).unwrap();
        let (status, result) = post(app.clone(), &body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(!result.accepted);
        assert_eq!(backend.committed.lock().unwrap().len(), 1);

        // a structurally invalid body is refused by the extractor before any policy runs
        let response = app
            .oneshot(
                Request::post(DEFAULT_COMMIT_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"envelope":{},"result":{}}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(backend.committed.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn commit_failure_is_reported_not_hidden() {
        let backend = Arc::new(Backend { fail_commit: true, ..Default::default() });
        let app: Router = router(backend as Arc<dyn DropCommitBackend>, Some("/drops"));
        let body = serde_json::to_string(&request("zone-a", true, Some(DndOperation::Copy))).unwrap();
        let response = app
            .oneshot(Request::post("/drops").header(header::CONTENT_TYPE, "application/json").body(Body::from(body)).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let result: DndDropResult = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result.error_code.as_deref(), Some("storage-unavailable"));
    }
}
