//! A three-column kanban board: the canonical MASH wiring of ores-dnd.
//!
//! - the page is HTML-first: `ores_dnd_mash::html` renders every card as a
//!   drag source and every column as a drop zone with its policy;
//! - the browser runs `@oresoftware/ores-dnd`'s `autoBind` (loaded by
//!   `boot_script`), which drives the drag session and POSTs accepted drops;
//! - the server owns the board and the policies; it re-verifies each drop with
//!   the same core (`ores_dnd_mash::server`) and only then moves the card.

use axum::{extract::State, response::IntoResponse, routing::get, Router};
use maud::{html, Markup, DOCTYPE};
use ores_dnd_core::{
    DndDropPolicy, DndDropResult, DndEnvelope, DndError, DndItem, DndItemKind, DndOperation,
    ORES_DND_PROTOCOL,
};
use ores_dnd_mash::html::{boot_script, drag_source, drop_zone, DropZoneWiring};
use ores_dnd_mash::server::{router_with, DropCommitBackend, RouterOptions, DEFAULT_COMMIT_PATH};
use std::sync::{Arc, Mutex};

pub const COLUMNS: [&str; 3] = ["todo", "doing", "done"];
/// Media type of a card payload (`json` item).
pub const CARD_MEDIA: &str = "application/vnd.ores-dnd-example.card+json";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Card {
    pub id: String,
    pub title: String,
    pub column: String,
}

/// The board is the application's state. Drops mutate it only through
/// [`Board::commit`], after the server verified them.
#[derive(Debug, Default)]
pub struct Board {
    cards: Mutex<Vec<Card>>,
    committed: Mutex<Vec<String>>,
}

impl Board {
    pub fn seeded() -> Self {
        let board = Self::default();
        let mut cards = board.cards.lock().unwrap();
        for (i, title) in [
            "Write contract",
            "Replay traces",
            "Ship adapters",
            "Roll out to pub-lib-cores",
        ]
        .iter()
        .enumerate()
        {
            cards.push(Card {
                id: format!("card-{}", i + 1),
                title: (*title).to_owned(),
                column: if i < 2 { "todo".into() } else { "doing".into() },
            });
        }
        drop(cards);
        board
    }

    pub fn cards(&self) -> Vec<Card> {
        self.cards.lock().unwrap().clone()
    }

    /// The envelope a card is dragged as: one `json` item, move-only.
    pub fn envelope_for(card: &Card) -> DndEnvelope {
        DndEnvelope {
            protocol: ORES_DND_PROTOCOL.to_owned(),
            drag_id: format!("drag-{}", card.id),
            source_runtime: "mash-kanban".to_owned(),
            allowed_operations: vec![DndOperation::Move],
            items: vec![DndItem {
                kind: DndItemKind::Json,
                media_type: CARD_MEDIA.to_owned(),
                data: serde_json::to_string(card).unwrap(),
                name: Some(card.title.clone()),
            }],
            traceparent: None,
            form_id: None,
        }
    }

    /// Every column accepts exactly one card payload by move.
    pub fn policy_for_column(column: &str) -> DndDropPolicy {
        DndDropPolicy::new(
            format!("column-{column}"),
            &[DndOperation::Move],
            &[DndItemKind::Json],
        )
        .with_media_types([CARD_MEDIA])
        .with_max_items(1)
    }
}

impl DropCommitBackend for Board {
    fn policy_for(&self, target_id: &str) -> Option<DndDropPolicy> {
        let column = target_id.strip_prefix("column-")?;
        COLUMNS
            .contains(&column)
            .then(|| Self::policy_for_column(column))
    }

    fn commit(&self, envelope: &DndEnvelope, result: &DndDropResult) -> Result<(), DndError> {
        let card: Card = serde_json::from_str(&envelope.items[0].data)
            .map_err(|_| DndError("card-unreadable".into()))?;
        let column = result
            .target_id
            .as_deref()
            .and_then(|t| t.strip_prefix("column-"))
            .ok_or_else(|| DndError("unknown-column".into()))?;
        let mut cards = self.cards.lock().unwrap();
        let existing = cards
            .iter_mut()
            .find(|c| c.id == card.id)
            .ok_or_else(|| DndError("unknown-card".into()))?;
        existing.column = column.to_owned();
        self.committed.lock().unwrap().push(result.drag_id.clone());
        Ok(())
    }

    fn already_committed(&self, drag_id: &str) -> bool {
        self.committed.lock().unwrap().iter().any(|d| d == drag_id)
    }
}

/// Where the app serves the TypeScript adapter (zed-pkg installs it under .vendor/.zed).
pub const ADAPTER_URL: &str = "/vendor/ores-dnd/index.js";

pub fn render_board(board: &Board) -> Markup {
    let cards = board.cards();
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                title { "ores-dnd MASH kanban" }
                style { (maud::PreEscaped(CSS)) }
            }
            body {
                h1 { "Kanban — every column is a policy, every card is an envelope" }
                main class="board" {
                    @for column in COLUMNS {
                        (column_markup(column, cards.iter().filter(|c| c.column == column)))
                    }
                }
                (boot_script(ADAPTER_URL))
            }
        }
    }
}

fn column_markup<'a>(column: &str, cards: impl Iterator<Item = &'a Card>) -> Markup {
    let policy = Board::policy_for_column(column);
    let wiring = DropZoneWiring {
        commit_url: DEFAULT_COMMIT_PATH,
        swap_target: Some("main.board"),
        class: Some("column"),
    };
    let inner = html! {
        h2 { (column) }
        @for card in cards {
            (drag_source(&Board::envelope_for(card), Some("card"), html! { (card.title) }).expect("card envelope is valid"))
        }
    };
    drop_zone(&policy, &wiring, inner).expect("column policy is valid")
}

const CSS: &str = r#"
body { font: 14px system-ui, sans-serif; margin: 2rem; }
.board { display: flex; gap: 1rem; }
.column { flex: 1; min-height: 12rem; padding: .5rem; border: 2px dashed #bbb; border-radius: .5rem; }
.column[data-ores-dnd-state="accepting"] { border-color: #2a7; background: #eefbf3; }
.column[data-ores-dnd-state="rejecting"] { border-color: #c33; cursor: not-allowed; }
.column[data-ores-dnd-state="dragging"] { border-style: solid; }
.card { padding: .5rem; margin: .25rem 0; background: #fff; border: 1px solid #ccc; border-radius: .25rem; cursor: grab; }
"#;

async fn index(State(board): State<Arc<Board>>) -> impl IntoResponse {
    render_board(&board)
}

/// The whole app: the page plus the hardened drop-commit endpoint.
pub fn app(board: Arc<Board>) -> Router {
    Router::new()
        .route("/", get(index))
        .with_state(board.clone())
        .merge(router_with(
            board as Arc<dyn DropCommitBackend>,
            RouterOptions::default(),
        ))
}
