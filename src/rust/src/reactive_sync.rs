use std::cell::RefCell;

use rxrust::prelude::*;

use crate::{
    commit_accepted_drop, telemetry_for, DndDropResult, DndEnvelope, DndError,
    DndLifecyclePhase, DndOperation, DndTelemetryEvent, DropCommitPorts, OptoSyncPort,
    OresFormsPort, OresOtelPort,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DndSyncChannel {
    OptoSync,
    OresOtel,
}

impl DndSyncChannel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OptoSync => "opto-sync",
            Self::OresOtel => "ores-otel",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DndReactiveEvent {
    AcceptedDrop {
        drag_id: String,
        operation: DndOperation,
        target_id: Option<String>,
    },
    Lifecycle(DndTelemetryEvent),
    SupabaseSync {
        drag_id: String,
        channel: DndSyncChannel,
        ok: bool,
        target_id: Option<String>,
        error_code: Option<&'static str>,
    },
}

/// Host-provided Opto-Sync adapter. The host owns the concrete Supabase client,
/// runtime configuration, credentials, queueing, and reconciliation semantics.
pub trait OptoSyncSupabasePort: OptoSyncPort {
    fn sync_accepted_drop_to_supabase(
        &self,
        envelope: &DndEnvelope,
        result: &DndDropResult,
    ) -> Result<(), DndError>;
}

/// Host-provided ORES-OTel adapter. This boundary only receives sanitized
/// telemetry metadata; raw dragged item data is unavailable here by design.
pub trait OresOtelSupabasePort: OresOtelPort {
    fn sync_dnd_event_to_supabase(&self, event: &DndTelemetryEvent) -> Result<(), DndError>;
}

/// Small output port so Rust applications can connect any RxRust Subject or
/// other FRP sink without ores-dnd committing to a concrete scheduler/context.
pub trait DndReactiveSink {
    fn publish(&self, event: &DndReactiveEvent);
}

pub struct ReactiveDropCommitPorts<'a> {
    pub forms: Option<&'a dyn OresFormsPort>,
    pub opto_sync: Option<&'a dyn OptoSyncSupabasePort>,
    pub otel: Option<&'a dyn OresOtelSupabasePort>,
    pub reactive: Option<&'a dyn DndReactiveSink>,
}

struct OptoBaseAdapter<'a>(&'a dyn OptoSyncSupabasePort);

impl OptoSyncPort for OptoBaseAdapter<'_> {
    fn persist_accepted_drop(
        &self,
        envelope: &DndEnvelope,
        result: &DndDropResult,
    ) -> Result<(), DndError> {
        self.0.persist_accepted_drop(envelope, result)
    }
}

fn publish(reactive: Option<&dyn DndReactiveSink>, event: DndReactiveEvent) {
    if let Some(reactive) = reactive {
        reactive.publish(&event);
    }
}

fn sync_receipt(
    result: &DndDropResult,
    channel: DndSyncChannel,
    ok: bool,
) -> DndReactiveEvent {
    DndReactiveEvent::SupabaseSync {
        drag_id: result.drag_id.clone(),
        channel,
        ok,
        target_id: result.target_id.clone(),
        error_code: (!ok).then_some("sync-failed"),
    }
}

/// Commit through the canonical local accepted-drop path, then explicitly call
/// the injected Opto-Sync and ORES-OTel Supabase functions.
///
/// ores-dnd imports no Supabase SDK and owns no endpoint, table, or credential.
/// The host adapters keep those concerns behind runtime configuration.
pub fn commit_accepted_drop_reactive(
    envelope: &DndEnvelope,
    result: &DndDropResult,
    ports: ReactiveDropCommitPorts<'_>,
) -> Result<(), DndError> {
    let opto_base = ports.opto_sync.map(OptoBaseAdapter);
    let base_ports = DropCommitPorts {
        otel: None,
        opto_sync: opto_base
            .as_ref()
            .map(|adapter| adapter as &dyn OptoSyncPort),
        forms: ports.forms,
    };

    // Reuse the v1 validator and existing local side-effect boundary. OTel is
    // intentionally held back until the Opto-Sync Supabase path succeeds.
    commit_accepted_drop(envelope, result, base_ports)?;
    if !result.accepted {
        return Ok(());
    }

    let operation = result
        .operation
        .ok_or_else(|| DndError("accepted drop requires an operation".into()))?;

    publish(
        ports.reactive,
        DndReactiveEvent::AcceptedDrop {
            drag_id: result.drag_id.clone(),
            operation,
            target_id: result.target_id.clone(),
        },
    );

    if let Some(opto_sync) = ports.opto_sync {
        match opto_sync.sync_accepted_drop_to_supabase(envelope, result) {
            Ok(()) => publish(
                ports.reactive,
                sync_receipt(result, DndSyncChannel::OptoSync, true),
            ),
            Err(error) => {
                publish(
                    ports.reactive,
                    sync_receipt(result, DndSyncChannel::OptoSync, false),
                );
                return Err(error);
            }
        }
    }

    let telemetry = telemetry_for(
        DndLifecyclePhase::Drop,
        envelope,
        Some(operation),
        result.target_id.clone(),
    );
    publish(
        ports.reactive,
        DndReactiveEvent::Lifecycle(telemetry.clone()),
    );

    if let Some(otel) = ports.otel {
        otel.emit_dnd_event(&telemetry)?;
        match otel.sync_dnd_event_to_supabase(&telemetry) {
            Ok(()) => publish(
                ports.reactive,
                sync_receipt(result, DndSyncChannel::OresOtel, true),
            ),
            Err(error) => {
                publish(
                    ports.reactive,
                    sync_receipt(result, DndSyncChannel::OresOtel, false),
                );
                return Err(error);
            }
        }
    }

    Ok(())
}

/// Build a finite RxRust pipeline over payload-free reactive events.
///
/// Live applications normally implement [DndReactiveSink] using their own
/// `Local::subject()` or `Shared::subject()` so scheduler/threading policy stays
/// with the host. This helper keeps batch/test composition first-class too.
pub fn observe_reactive_events<F>(events: Vec<DndReactiveEvent>, on_next: F)
where
    F: FnMut(DndReactiveEvent) + 'static,
{
    Local::from_iter(events).subscribe(on_next);
}

/// Convenience sink useful for tests and for bridging into a host-owned
/// RxRust Subject without coupling the core to a scheduler type.
#[derive(Default)]
pub struct BufferedReactiveSink {
    events: RefCell<Vec<DndReactiveEvent>>,
}

impl BufferedReactiveSink {
    #[must_use]
    pub fn snapshot(&self) -> Vec<DndReactiveEvent> {
        self.events.borrow().clone()
    }

    pub fn observe<F>(&self, on_next: F)
    where
        F: FnMut(DndReactiveEvent) + 'static,
    {
        observe_reactive_events(self.snapshot(), on_next);
    }
}

impl DndReactiveSink for BufferedReactiveSink {
    fn publish(&self, event: &DndReactiveEvent) {
        self.events.borrow_mut().push(event.clone());
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::{decode_envelope_json, ValidationOptions};

    const VALID: &str =
        include_str!("../../../contracts/instances/DndEnvelope/valid/text-copy.json");

    struct OptoFake(Rc<RefCell<Vec<&'static str>>>);

    impl OptoSyncPort for OptoFake {
        fn persist_accepted_drop(
            &self,
            _envelope: &DndEnvelope,
            _result: &DndDropResult,
        ) -> Result<(), DndError> {
            self.0.borrow_mut().push("opto-local");
            Ok(())
        }
    }

    impl OptoSyncSupabasePort for OptoFake {
        fn sync_accepted_drop_to_supabase(
            &self,
            _envelope: &DndEnvelope,
            _result: &DndDropResult,
        ) -> Result<(), DndError> {
            self.0.borrow_mut().push("opto-supabase");
            Ok(())
        }
    }

    struct OtelFake(Rc<RefCell<Vec<&'static str>>>);

    impl OresOtelPort for OtelFake {
        fn emit_dnd_event(&self, event: &DndTelemetryEvent) -> Result<(), DndError> {
            let encoded = serde_json::to_string(event)?;
            assert!(!encoded.contains("hello"));
            self.0.borrow_mut().push("otel-local");
            Ok(())
        }
    }

    impl OresOtelSupabasePort for OtelFake {
        fn sync_dnd_event_to_supabase(
            &self,
            event: &DndTelemetryEvent,
        ) -> Result<(), DndError> {
            let encoded = serde_json::to_string(event)?;
            assert!(!encoded.contains("hello"));
            self.0.borrow_mut().push("otel-supabase");
            Ok(())
        }
    }

    #[test]
    fn reactive_commit_calls_both_supabase_ports_without_payload_telemetry() {
        let decoded = decode_envelope_json(VALID, ValidationOptions::default());
        assert!(decoded.is_ok(), "shared valid fixture must decode");
        let Ok(envelope) = decoded else {
            return;
        };
        let calls = Rc::new(RefCell::new(Vec::new()));
        let opto = OptoFake(Rc::clone(&calls));
        let otel = OtelFake(Rc::clone(&calls));
        let reactive = BufferedReactiveSink::default();
        let result = DndDropResult {
            drag_id: envelope.drag_id.clone(),
            accepted: true,
            operation: Some(DndOperation::Copy),
            target_id: Some("field-1".into()),
            error_code: None,
        };

        let committed = commit_accepted_drop_reactive(
            &envelope,
            &result,
            ReactiveDropCommitPorts {
                forms: None,
                opto_sync: Some(&opto),
                otel: Some(&otel),
                reactive: Some(&reactive),
            },
        );
        assert!(committed.is_ok());
        assert_eq!(
            calls.borrow().as_slice(),
            ["opto-local", "opto-supabase", "otel-local", "otel-supabase"]
        );

        let serialized = format!("{:?}", reactive.snapshot());
        assert!(!serialized.contains("hello"));
        assert!(reactive.snapshot().iter().any(|event| matches!(
            event,
            DndReactiveEvent::SupabaseSync {
                channel: DndSyncChannel::OptoSync,
                ok: true,
                ..
            }
        )));
    }

    #[test]
    fn rxrust_observer_receives_sanitized_events() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let capture = Rc::clone(&seen);
        observe_reactive_events(
            vec![DndReactiveEvent::SupabaseSync {
                drag_id: "drag-1".into(),
                channel: DndSyncChannel::OresOtel,
                ok: true,
                target_id: None,
                error_code: None,
            }],
            move |event| capture.borrow_mut().push(event),
        );
        assert_eq!(seen.borrow().len(), 1);
    }
}
