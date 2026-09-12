use std::sync::Arc;

#[tokio::main]
async fn main() {
    let board = Arc::new(ores_dnd_example_mash_kanban::Board::seeded());
    let app = ores_dnd_example_mash_kanban::app(board);
    let addr = std::env::var("ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_owned());
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    eprintln!(
        "mash-kanban listening on http://{addr}  (serve @oresoftware/ores-dnd at {})",
        ores_dnd_example_mash_kanban::ADAPTER_URL
    );
    axum::serve(listener, app).await.expect("serve");
}
