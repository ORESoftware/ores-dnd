use std::convert::Infallible;

use rxrust::prelude::*;

use crate::{
    commit_accepted_drop, telemetry_for, DndDropResult, DndEnvelope, DndError,
    DndLifecyclePhase, DndOperation, DndTelemetryEvent, DropCommitPorts, OptoSyncPort,
    OresFormsPort, OresOtelPort,
};
use crate::reactive::{DndLocalEventSubject, DndReactiveEvent};

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

/// Payload-free receipt safe for reactive UI/state composition.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DndSupabaseSyncReceipt {
    pub drag_id: String,
    pub channel: &'static str,
    pub backend: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<&'static str>,
}

pub trait OptoSyncSupabasePort: OptoSyncPort {
    fn sync_accepted_drop_to_supabase(
        &self,
        envelope: &DndEnvelope,
        result: &DndDropResult,
    ) -> Result<(), DndError>;
}

pub trait OresOtelSupabasePort: OresOtelPort {
    fn sync_dnd_event_to_supabase(&self, event: &DndTelemetryEvent) -> Result<(), DndError>;
}

pub type DndLocalSyncSubject =
    rxrust::subject::LocalSubject<'static, DndSupabaseSyncReceipt, Infallible>;

#[must_use]
pub fn local_sync_subject() -> DndLocalSyncSubject {
    Local::subject::<DndSupabaseSyncReceipt, Infallible>()
}

pub struct SupabaseDropCommitPorts<'a> {
    pub forms: Option<&'a dyn OresFormsPort>,
    pub opto_sync: Option<&'a dyn OptoSyncSupabasePort>,
    pub otel: Option<&'a dyn OresOtelSupabasePort>,
    pub lifecycle: Option<&'a mut DndLocalEventSubject>,
    pub sync: Option<&'a mut DndLocalSyncSubject>,
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

fn receipt(
    result: &DndDropResult,
    channel: DndSyncChannel,
    ok: bool,
) -> DndSupabaseSyncReceipt {
    DndSupabaseSyncReceipt {
        drag_id: result.drag_id.clone(),
        channel: channel.as_str(),
        backend: "supabase",
        ok,
        target_id: result.target_id.clone(),
        error_code: (!ok).then_some("sync-failed"),
    }
}

fn publish_sync(
    sync: Option<&mut DndLocalSyncSubject>,
    value: DndSupabaseSyncReceipt,
) {
    if let Some(sync) = sync {
        sync.next(value);
    }
}

/// Runs canonical accepted-drop validation/local effects and then calls the
/// injected Opto-Sync and ORES-OTel Supabase methods.
///
/// No Supabase SDK, URL, table or credential is owned by ores-dnd.
pub fn commit_accepted_drop_with_supabase(
    envelope: &DndEnvelope,
    result: &DndDropResult,
    mut ports: SupabaseDropCommitPorts<'_>,
) -> Result<(), DndError> {
    let opto_base = ports.opto_sync.map(OptoBaseAdapter);
    commit_accepted_drop(
        envelope,
        result,
        DropCommitPorts {
            forms: ports.forms,
            opto_sync: opto_base
                .as_ref()
                .map(|adapter| adapter as &dyn OptoSyncPort),
            otel: None,
        },
    )?;
    if !result.accepted {
        return Ok(());
    }

    let operation = result
        .operation
        .ok_or_else(|| DndError("accepted drop requires an operation".to_owned()))?;

    if let Some(opto_sync) = ports.opto_sync {
        match opto_sync.sync_accepted_drop_to_supabase(envelope, result) {
            Ok(()) => publish_sync(
                ports.sync.as_deref_mut(),
                receipt(result, DndSyncChannel::OptoSync, true),
            ),
            Err(error) => {
                publish_sync(
                    ports.sync.as_deref_mut(),
                    receipt(result, DndSyncChannel::OptoSync, false),
                );
                return Err(error);
            }
        }
    }

    if let Some(lifecycle) = ports.lifecycle.as_deref_mut() {
        lifecycle.next(DndReactiveEvent::new(
            DndLifecyclePhase::Drop,
            envelope.clone(),
            Some(operation),
            result.target_id.clone(),
        )?);
    }

    let telemetry = telemetry_for(
        DndLifecyclePhase::Drop,
        envelope,
        Some(operation),
        result.target_id.clone(),
    );
    if let Some(otel) = ports.otel {
        otel.emit_dnd_event(&telemetry)?;
        match otel.sync_dnd_event_to_supabase(&telemetry) {
            Ok(()) => publish_sync(
                ports.sync.as_deref_mut(),
                receipt(result, DndSyncChannel::OresOtel, true),
            ),
            Err(error) => {
                publish_sync(
                    ports.sync.as_deref_mut(),
                    receipt(result, DndSyncChannel::OresOtel, false),
                );
                return Err(error);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

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
            assert!(!serde_json::to_string(event)?.contains("hello"));
            self.0.borrow_mut().push("otel-local");
            Ok(())
        }
    }

    impl OresOtelSupabasePort for OtelFake {
        fn sync_dnd_event_to_supabase(
            &self,
            event: &DndTelemetryEvent,
        ) -> Result<(), DndError> {
            assert!(!serde_json::to_string(event)?.contains("hello"));
            self.0.borrow_mut().push("otel-supabase");
            Ok(())
        }
    }

    #[test]
    fn both_supabase_hooks_run_after_local_opto_and_keep_payload_out_of_receipts() {
        let decoded = decode_envelope_json(VALID, ValidationOptions::default());
        assert!(decoded.is_ok());
        let Ok(envelope) = decoded else {
            return;
        };
        let calls = Rc::new(RefCell::new(Vec::new()));
        let opto = OptoFake(Rc::clone(&calls));
        let otel = OtelFake(Rc::clone(&calls));
        let mut lifecycle = crate::reactive::local_event_subject();
        let mut sync = local_sync_subject();
        let receipts = Rc::new(RefCell::new(Vec::new()));
        let receipt_sink = Rc::clone(&receipts);
        sync.clone().subscribe(move |value| receipt_sink.borrow_mut().push(value));

        let result = DndDropResult {
            drag_id: envelope.drag_id.clone(),
            accepted: true,
            operation: Some(DndOperation::Copy),
            target_id: Some("field-1".to_owned()),
            error_code: None,
        };
        let committed = commit_accepted_drop_with_supabase(
            &envelope,
            &result,
            SupabaseDropCommitPorts {
                forms: None,
                opto_sync: Some(&opto),
                otel: Some(&otel),
                lifecycle: Some(&mut lifecycle),
                sync: Some(&mut sync),
            },
        );
        assert!(committed.is_ok());
        assert_eq!(
            calls.borrow().as_slice(),
            ["opto-local", "opto-supabase", "otel-local", "otel-supabase"]
        );
        let json = serde_json::to_string(&*receipts.borrow());
        assert!(json.is_ok());
        assert!(!json.unwrap_or_default().contains("hello"));
        assert_eq!(receipts.borrow().len(), 2);
    }
}
