//! End-to-end: the page renders zones and sources with the shared attributes,
//! and a browser-reported drop is verified, committed once, and reflected.
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use ores_dnd_core::{DndDropResult, DndOperation, ATTR_POLICY, ATTR_SOURCE, ATTR_ZONE};
use ores_dnd_example_mash_kanban::{app, Board};
use ores_dnd_mash::server::{DropCommitRequest, DEFAULT_COMMIT_PATH};
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn page_renders_every_column_as_a_zone_and_every_card_as_a_source() {
    let board = Arc::new(Board::seeded());
    let response = app(board.clone()).oneshot(Request::get("/").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let html = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    for column in ["todo", "doing", "done"] {
        assert!(html.contains(&format!("{ATTR_ZONE}=\"column-{column}\"")), "{column} zone");
    }
    assert_eq!(html.matches(&format!("{ATTR_SOURCE}=")).count(), 4);
    assert!(html.contains(&format!("{ATTR_POLICY}=\"{{&quot;targetId&quot;:&quot;column-todo&quot;")));
    assert!(html.contains("import { autoBind } from \"/vendor/ores-dnd/index.js\""));
    assert!(!html.contains("<script>alert"), "card titles are escaped");
}

#[tokio::test]
async fn a_verified_drop_moves_the_card_exactly_once() {
    let board = Arc::new(Board::seeded());
    let card = board.cards().into_iter().find(|c| c.id == "card-1").unwrap();
    assert_eq!(card.column, "todo");
    let envelope = Board::envelope_for(&card);
    let request = DropCommitRequest {
        result: DndDropResult { drag_id: envelope.drag_id.clone(), accepted: true, operation: Some(DndOperation::Move), target_id: Some("column-done".into()), error_code: None },
        envelope,
    };
    let body = serde_json::to_string(&request).unwrap();
    let post = || Request::post(DEFAULT_COMMIT_PATH).header(header::CONTENT_TYPE, "application/json").header("HX-Request", "true").body(Body::from(body.clone())).unwrap();

    let response = app(board.clone()).oneshot(post()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(board.cards().into_iter().find(|c| c.id == "card-1").unwrap().column, "done");

    // the browser retries → refused as a duplicate, board unchanged
    let response = app(board.clone()).oneshot(post()).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // a copy is not something a column accepts, whatever the browser claims
    let mut copy = request.clone();
    copy.result.operation = Some(DndOperation::Copy);
    copy.envelope.drag_id = "drag-card-1-copy".into();
    copy.result.drag_id = copy.envelope.drag_id.clone();
    let body = serde_json::to_string(&copy).unwrap();
    let response = app(board.clone())
        .oneshot(Request::post(DEFAULT_COMMIT_PATH).header(header::CONTENT_TYPE, "application/json").header("HX-Request", "true").body(Body::from(body)).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let verdict: DndDropResult = serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(verdict.error_code.as_deref(), Some("no-common-operation"));
}
