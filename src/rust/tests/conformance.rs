//! Replay every DndSessionTrace from the shared corpus through the Rust
//! session state machine.
mod common;

use ores_dnd_core::{DndSession, DndSessionTrace};

#[test]
fn all_session_traces_replay_identically() {
    let traces: Vec<DndSessionTrace> = common::corpus()
        .into_iter()
        .filter(|(d, e, _, _)| d == "DndSessionTrace" && *e == "accepted")
        .map(|(_, _, file, json)| {
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("{file}: {e}"))
        })
        .collect();
    assert!(
        traces.len() >= 20,
        "expected the full trace corpus, got {}",
        traces.len()
    );
    let divergences: Vec<String> = traces
        .iter()
        .filter_map(|trace| DndSession::replay(trace).err().map(|d| d.to_string()))
        .collect();
    assert!(divergences.is_empty(), "{}", divergences.join("\n"));
}

#[test]
fn trace_replay_detects_a_divergence() {
    let mut trace: DndSessionTrace = serde_json::from_str(&common::read(
        common::contracts_dir().join("instances/DndSessionTrace/valid/basic-drop.json"),
    ))
    .unwrap();
    trace.expected[2].operation = Some(ores_dnd_core::DndOperation::Link);
    let err = DndSession::replay(&trace).unwrap_err();
    assert_eq!(err.step, 2);
}
