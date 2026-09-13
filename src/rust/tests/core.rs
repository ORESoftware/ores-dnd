mod common;

use ores_dnd_core::*;

fn valid() -> String {
    common::read(common::contracts_dir().join("instances/DndEnvelope/valid/text-copy.json"))
}

#[test]
fn shared_fixture_round_trips() {
    let env = decode_envelope_json(&valid(), ValidationOptions::default()).unwrap();
    let encoded = encode_envelope_json(&env, ValidationOptions::default()).unwrap();
    assert_eq!(
        decode_envelope_json(&encoded, ValidationOptions::default()).unwrap(),
        env
    );
}

#[test]
fn unknown_operation_fails_closed() {
    let json =
        common::read(common::contracts_dir().join("instances/DndEnvelope/invalid/unknown-op.json"));
    assert!(decode_envelope_json(&json, ValidationOptions::default()).is_err());
}

#[test]
fn unknown_property_fails_closed() {
    let with_unknown = valid().replacen("\"protocol\"", "\"secret\":\"x\",\"protocol\"", 1);
    assert!(decode_envelope_json(&with_unknown, ValidationOptions::default()).is_err());
}

#[test]
fn payload_limit_is_checked_before_parse() {
    let options = ValidationOptions {
        max_payload_bytes: 8,
        max_items: 64,
    };
    assert!(decode_envelope_json("this is not json", options)
        .unwrap_err()
        .0
        .contains("too large"));
}

#[test]
fn negotiation_is_deterministic() {
    use DndOperation::*;
    assert_eq!(
        negotiate_operation(&[Copy, Move], &[Copy, Move], None),
        Some(Move)
    );
    assert_eq!(
        negotiate_operation(&[Copy, Move], &[Copy, Move], Some(Copy)),
        Some(Copy)
    );
    assert_eq!(negotiate_operation(&[Copy], &[Move], None), None);
    assert_eq!(
        negotiate_operation(&[Copy, Link], &[Link], Some(Move)),
        Some(Link)
    );
}

#[test]
fn effect_allowed_matches_html5_keywords() {
    use DndOperation::*;
    assert_eq!(effect_allowed_for(&[Copy, Move, Link]), "all");
    assert_eq!(effect_allowed_for(&[Copy, Move]), "copyMove");
    assert_eq!(effect_allowed_for(&[Link]), "link");
    assert_eq!(effect_allowed_for(&[]), "none");
}

#[test]
fn telemetry_does_not_contain_item_data() {
    let env = decode_envelope_json(&valid(), ValidationOptions::default()).unwrap();
    let event = telemetry_for(
        DndLifecyclePhase::Drop,
        &env,
        Some(DndOperation::Copy),
        None,
    );
    let json = serde_json::to_string(&event).unwrap();
    assert!(!json.contains("hello"));
    assert_eq!(event.item_count, 1);
}

#[test]
fn policy_evaluation_order_is_fixed() {
    use DndItemKind::*;
    use DndOperation::*;
    let env = decode_envelope_json(&valid(), ValidationOptions::default()).unwrap();
    // operation is checked first even when the kind would also fail
    let p = DndDropPolicy::new("z", &[Link], &[Json]);
    assert_eq!(
        evaluate_policy(&env, &p, None),
        Err(DndRejectCode::NoCommonOperation)
    );
    let p = DndDropPolicy::new("z", &[Copy], &[Json]);
    assert_eq!(
        evaluate_policy(&env, &p, None),
        Err(DndRejectCode::ItemKindNotAccepted)
    );
    let p = DndDropPolicy::new("z", &[Copy], &[Text])
        .with_media_types(["text/markdown"])
        .with_max_total_bytes(1);
    assert_eq!(
        evaluate_policy(&env, &p, None),
        Err(DndRejectCode::MediaTypeNotAccepted)
    );
    let p = DndDropPolicy::new("z", &[Copy], &[Text]).with_max_total_bytes(4);
    assert_eq!(
        evaluate_policy(&env, &p, None),
        Err(DndRejectCode::PayloadTooLarge)
    );
    let p = DndDropPolicy::new("z", &[Copy], &[Text]).with_max_total_bytes(5);
    assert_eq!(evaluate_policy(&env, &p, None), Ok(Copy));
}

#[test]
fn session_result_is_derived_from_terminal_snapshots() {
    let env = decode_envelope_json(&valid(), ValidationOptions::default()).unwrap();
    let mut session = DndSession::new();
    assert!(session.snapshot().result().is_none());
    session.apply(&DndSessionInput::start(env.clone()));
    session.apply(&DndSessionInput::enter(
        DndDropPolicy::new("zone-a", &[DndOperation::Move], &[DndItemKind::Text]),
        None,
    ));
    assert!(session.snapshot().is_over_accepting_target());
    let snapshot = session.apply(&DndSessionInput::drop("zone-a")).clone();
    let result = snapshot.result().unwrap();
    assert!(result.accepted);
    assert_eq!(result.operation, Some(DndOperation::Move));
    assert_eq!(result.target_id.as_deref(), Some("zone-a"));
    assert_eq!(session.envelope(), Some(&env));

    let mut session = DndSession::new();
    session.apply(&DndSessionInput::start(env));
    let snapshot = session.apply(&DndSessionInput::cancel()).clone();
    let result = snapshot.result().unwrap();
    assert!(!result.accepted);
    assert_eq!(result.error_code.as_deref(), Some("cancelled"));
}

#[test]
fn malformed_inputs_are_ignored() {
    let env = decode_envelope_json(&valid(), ValidationOptions::default()).unwrap();
    let mut session = DndSession::new();
    session.apply(&DndSessionInput::start(env));
    let before = session.snapshot().clone();
    let enter_without_policy = DndSessionInput {
        policy: None,
        ..DndSessionInput::enter(
            DndDropPolicy::new("z", &[DndOperation::Copy], &[DndItemKind::Text]),
            None,
        )
    };
    assert_eq!(session.apply(&enter_without_policy), &before);
    let leave_without_target = DndSessionInput {
        target_id: None,
        ..DndSessionInput::leave("z")
    };
    assert_eq!(session.apply(&leave_without_target), &before);
}

struct Recorder(std::cell::RefCell<Vec<&'static str>>);
impl OresOtelPort for Recorder {
    fn emit_dnd_event(&self, event: &DndTelemetryEvent) -> Result<(), DndError> {
        assert!(!serde_json::to_string(event).unwrap().contains("hello"));
        self.0.borrow_mut().push("otel");
        Ok(())
    }
}
impl OptoSyncPort for Recorder {
    fn persist_accepted_drop(&self, _: &DndEnvelope, _: &DndDropResult) -> Result<(), DndError> {
        self.0.borrow_mut().push("sync");
        Ok(())
    }
}
impl OresFormsPort for Recorder {
    fn apply_accepted_drop(&self, _: &DndEnvelope, _: &DndDropResult) -> Result<(), DndError> {
        self.0.borrow_mut().push("forms");
        Ok(())
    }
}

#[test]
fn commit_ports_run_in_order_only_for_accepted_drops() {
    let env = decode_envelope_json(&valid(), ValidationOptions::default()).unwrap();
    let rec = Recorder(Default::default());
    let ports = DropCommitPorts {
        otel: Some(&rec),
        opto_sync: Some(&rec),
        forms: Some(&rec),
    };
    let accepted = DndDropResult {
        drag_id: env.drag_id.clone(),
        accepted: true,
        operation: Some(DndOperation::Copy),
        target_id: Some("f".into()),
        error_code: None,
    };
    commit_accepted_drop(&env, &accepted, ports).unwrap();
    assert_eq!(*rec.0.borrow(), vec!["forms", "sync", "otel"]);
    rec.0.borrow_mut().clear();
    let rejected = DndDropResult {
        accepted: false,
        operation: None,
        ..accepted.clone()
    };
    commit_accepted_drop(&env, &rejected, ports).unwrap();
    assert!(rec.0.borrow().is_empty());
    let unallowed = DndDropResult {
        operation: Some(DndOperation::Link),
        ..accepted
    };
    assert!(commit_accepted_drop(&env, &unallowed, ports).is_err());
}

#[test]
fn all_framework_bindings_share_the_same_mime() {
    assert_eq!(mash::drop_zone("z").mime_type, ORES_DND_MIME);
    assert_eq!(leptos::drop_zone("z").mime_type, ORES_DND_MIME);
    assert_eq!(dioxus::drop_zone("z").mime_type, ORES_DND_MIME);
    let attrs = bindings::zone_attributes(&DndDropPolicy::new(
        "zone-a",
        &[DndOperation::Copy],
        &[DndItemKind::Text],
    ))
    .unwrap();
    assert_eq!(attrs[0], (ATTR_ZONE, "zone-a".to_owned()));
    assert!(attrs[1].1.contains("\"targetId\":\"zone-a\""));
}
