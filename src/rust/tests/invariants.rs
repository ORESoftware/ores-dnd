//! Randomised invariant tests over the session state machine: thousands of
//! seeded sequences, every snapshot checked against the properties in
//! docs/DESIGN.md §3 and docs/SECURITY.md §Invariants.
use ores_dnd_core::fuzz::{random_sequence, Xorshift64};
use ores_dnd_core::{DndSession, DndSessionInputKind, DndSessionSnapshot, DndSessionState};

fn check_invariants(seed: u64) {
    let inputs = random_sequence(seed, 24);
    let mut session = DndSession::new();
    let mut previous = DndSessionSnapshot::IDLE;
    for (step, input) in inputs.iter().enumerate() {
        let snapshot = session.apply(input).clone();
        let ctx = || {
            format!(
                "seed {seed:#x} step {step} input {:?} -> {snapshot:?}",
                input.kind
            )
        };
        // 1. every snapshot is structurally valid and round-trips through JSON
        snapshot
            .structural()
            .unwrap_or_else(|e| panic!("{}: {e}", ctx()));
        let json = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(
            serde_json::from_str::<DndSessionSnapshot>(&json).unwrap(),
            snapshot,
            "{}",
            ctx()
        );
        // 2. terminal states are absorbing for everything but start
        if previous.state.is_terminal() && input.kind != DndSessionInputKind::Start {
            assert_eq!(snapshot, previous, "{}", ctx());
        }
        match snapshot.state {
            DndSessionState::Idle => {
                assert!(
                    snapshot.drag_id.is_none()
                        && snapshot.target_id.is_none()
                        && snapshot.operation.is_none(),
                    "{}",
                    ctx()
                );
                assert!(session.envelope().is_none(), "{}", ctx());
            }
            DndSessionState::Dragging => {
                assert!(
                    snapshot.drag_id.is_some() && snapshot.operation.is_none(),
                    "{}",
                    ctx()
                );
                // 3. a rejecting target always carries its reason, and only then
                assert_eq!(
                    snapshot.target_id.is_some(),
                    snapshot.error_code.is_some(),
                    "{}",
                    ctx()
                );
            }
            DndSessionState::OverTarget => {
                let envelope = session.envelope().expect("envelope while over target");
                let op = snapshot.operation.expect("operation while over target");
                // 4. the negotiated operation is allowed by the source
                assert!(envelope.allowed_operations.contains(&op), "{}", ctx());
                assert!(
                    snapshot.target_id.is_some() && snapshot.error_code.is_none(),
                    "{}",
                    ctx()
                );
                if input.kind == DndSessionInputKind::Enter && snapshot != previous {
                    let policy = input.policy.as_ref().unwrap();
                    assert!(policy.allowed_operations.contains(&op), "{}", ctx());
                    assert_eq!(
                        snapshot.target_id.as_deref(),
                        Some(policy.target_id.as_str()),
                        "{}",
                        ctx()
                    );
                }
            }
            DndSessionState::Dropped => {
                assert!(
                    snapshot.operation.is_some()
                        && snapshot.target_id.is_some()
                        && snapshot.error_code.is_none(),
                    "{}",
                    ctx()
                );
                assert!(snapshot.result().unwrap().accepted, "{}", ctx());
                // 5. a drop only ever lands on the target that was accepting (checked at the transition)
                if snapshot != previous {
                    assert_eq!(input.kind, DndSessionInputKind::Drop, "{}", ctx());
                    assert_eq!(previous.state, DndSessionState::OverTarget, "{}", ctx());
                    assert_eq!(previous.target_id, snapshot.target_id, "{}", ctx());
                    assert_eq!(input.target_id, snapshot.target_id, "{}", ctx());
                    assert_eq!(previous.operation, snapshot.operation, "{}", ctx());
                }
            }
            DndSessionState::Cancelled => {
                assert!(
                    snapshot.error_code.is_some() && snapshot.operation.is_none(),
                    "{}",
                    ctx()
                );
                let result = snapshot.result().unwrap();
                assert!(!result.accepted, "{}", ctx());
                assert_eq!(
                    result.error_code.as_deref(),
                    snapshot.error_code.map(|c| c.wire()),
                    "{}",
                    ctx()
                );
            }
        }
        // 6. dragId only changes on start
        if input.kind != DndSessionInputKind::Start
            && !previous.state.is_terminal()
            && previous.state != DndSessionState::Idle
        {
            assert_eq!(snapshot.drag_id, previous.drag_id, "{}", ctx());
        }
        previous = snapshot;
    }
}

/// Filled in from the Rust reference run; TypeScript and Dart carry the same constants.
const XORSHIFT_SEED_7_FIRST_THREE: [u64; 3] = [
    0xd1fb_af7f_728d_2eae,
    0xeda4_6c77_629d_a6ae,
    0x16df_9d6a_c76b_d322,
];

#[test]
fn session_invariants_hold_over_random_sequences() {
    let mut rng = Xorshift64::new(0xC0FF_EE00);
    for _ in 0..3000 {
        check_invariants(rng.next_u64());
    }
}

#[test]
fn generator_is_deterministic() {
    assert_eq!(random_sequence(42, 10), random_sequence(42, 10));
    assert_ne!(random_sequence(42, 10), random_sequence(43, 10));
    let mut a = Xorshift64::new(7);
    let mut b = Xorshift64::new(7);
    assert_eq!(
        (0..5).map(|_| a.next_u64()).collect::<Vec<_>>(),
        (0..5).map(|_| b.next_u64()).collect::<Vec<_>>()
    );
    // cross-language fixture: TypeScript and Dart assert these exact values for seed 7
    let mut c = Xorshift64::new(7);
    let first: Vec<u64> = (0..3).map(|_| c.next_u64()).collect();
    assert_eq!(first, XORSHIFT_SEED_7_FIRST_THREE);
}
