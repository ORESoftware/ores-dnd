use std::{cell::RefCell, collections::BTreeSet};

use crate::{
    commit_accepted_drop, telemetry_for, DndDropResult, DndEnvelope, DndError,
    DndLifecyclePhase, DndOperation, DndTelemetryEvent, DropCommitPorts, OptoSyncPort,
    OresFormsPort, OresOtelPort,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DndEffectStage {
    Forms,
    OptoLocal,
    OptoSupabase,
    OtelLocal,
    OtelSupabase,
}

impl DndEffectStage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Forms => "forms",
            Self::OptoLocal => "opto-local",
            Self::OptoSupabase => "opto-supabase",
            Self::OtelLocal => "otel-local",
            Self::OtelSupabase => "otel-supabase",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DndEffectStatus {
    Completed,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DndEffectReceipt {
    pub idempotency_key: String,
    pub drag_id: String,
    pub stage: DndEffectStage,
    pub status: DndEffectStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<&'static str>,
}

pub trait DndEffectJournalPort {
    fn has_completed(&self, idempotency_key: &str, stage: DndEffectStage) -> Result<bool, DndError>;
    fn mark_completed(&self, idempotency_key: &str, stage: DndEffectStage) -> Result<(), DndError>;
}

pub trait OptoSyncSupabasePort: OptoSyncPort {
    fn sync_accepted_drop_to_supabase(
        &self,
        envelope: &DndEnvelope,
        result: &DndDropResult,
        idempotency_key: &str,
    ) -> Result<(), DndError>;
}

pub trait OresOtelSupabasePort: OresOtelPort {
    fn sync_dnd_event_to_supabase(
        &self,
        event: &DndTelemetryEvent,
        idempotency_key: &str,
    ) -> Result<(), DndError>;
}

pub trait DndEffectSink {
    fn publish(&self, receipt: &DndEffectReceipt);
}

pub struct ReactiveEffectPorts<'a> {
    pub forms: Option<&'a dyn OresFormsPort>,
    pub opto_sync: Option<&'a dyn OptoSyncSupabasePort>,
    pub otel: Option<&'a dyn OresOtelSupabasePort>,
    pub journal: Option<&'a dyn DndEffectJournalPort>,
    pub receipts: Option<&'a dyn DndEffectSink>,
}

fn operation_wire(operation: DndOperation) -> &'static str {
    match operation {
        DndOperation::Copy => "copy",
        DndOperation::Move => "move",
        DndOperation::Link => "link",
    }
}

#[must_use]
pub fn dnd_effect_key(result: &DndDropResult) -> String {
    let target = result.target_id.as_deref().unwrap_or("-");
    let operation = result.operation.map(operation_wire).unwrap_or("-");
    format!(
        "ores.dnd/v1|{}:{}|{}:{}|{}:{}",
        result.drag_id.len(),
        result.drag_id,
        target.len(),
        target,
        operation.len(),
        operation,
    )
}

fn receipt(
    key: &str,
    result: &DndDropResult,
    stage: DndEffectStage,
    status: DndEffectStatus,
) -> DndEffectReceipt {
    DndEffectReceipt {
        idempotency_key: key.to_owned(),
        drag_id: result.drag_id.clone(),
        stage,
        status,
        target_id: result.target_id.clone(),
        error_code: (status == DndEffectStatus::Failed).then_some("effect-failed"),
    }
}

fn publish(
    sink: Option<&dyn DndEffectSink>,
    key: &str,
    result: &DndDropResult,
    stage: DndEffectStage,
    status: DndEffectStatus,
) {
    if let Some(sink) = sink {
        sink.publish(&receipt(key, result, stage, status));
    }
}

fn run_stage<F>(
    key: &str,
    result: &DndDropResult,
    stage: DndEffectStage,
    ports: &ReactiveEffectPorts<'_>,
    effect: F,
) -> Result<(), DndError>
where
    F: FnOnce() -> Result<(), DndError>,
{
    if let Some(journal) = ports.journal {
        match journal.has_completed(key, stage) {
            Ok(true) => {
                publish(ports.receipts, key, result, stage, DndEffectStatus::Skipped);
                return Ok(());
            }
            Ok(false) => {}
            Err(error) => {
                publish(ports.receipts, key, result, stage, DndEffectStatus::Failed);
                return Err(error);
            }
        }
    }

    if let Err(error) = effect() {
        publish(ports.receipts, key, result, stage, DndEffectStatus::Failed);
        return Err(error);
    }
    if let Some(journal) = ports.journal {
        if let Err(error) = journal.mark_completed(key, stage) {
            publish(ports.receipts, key, result, stage, DndEffectStatus::Failed);
            return Err(error);
        }
    }
    publish(ports.receipts, key, result, stage, DndEffectStatus::Completed);
    Ok(())
}

pub fn commit_accepted_drop_effects(
    envelope: &DndEnvelope,
    result: &DndDropResult,
    ports: ReactiveEffectPorts<'_>,
) -> Result<(), DndError> {
    commit_accepted_drop(
        envelope,
        result,
        DropCommitPorts {
            otel: None,
            opto_sync: None,
            forms: None,
        },
    )?;
    if !result.accepted {
        return Ok(());
    }
    let operation = result
        .operation
        .ok_or_else(|| DndError("accepted drop requires an operation".to_owned()))?;
    let key = dnd_effect_key(result);

    if let Some(forms) = ports.forms {
        run_stage(&key, result, DndEffectStage::Forms, &ports, || {
            forms.apply_accepted_drop(envelope, result)
        })?;
    }
    if let Some(opto) = ports.opto_sync {
        run_stage(&key, result, DndEffectStage::OptoLocal, &ports, || {
            opto.persist_accepted_drop(envelope, result)
        })?;
        run_stage(&key, result, DndEffectStage::OptoSupabase, &ports, || {
            opto.sync_accepted_drop_to_supabase(envelope, result, &key)
        })?;
    }

    let telemetry = telemetry_for(
        DndLifecyclePhase::Drop,
        envelope,
        Some(operation),
        result.target_id.clone(),
    );
    if let Some(otel) = ports.otel {
        run_stage(&key, result, DndEffectStage::OtelLocal, &ports, || {
            otel.emit_dnd_event(&telemetry)
        })?;
        run_stage(&key, result, DndEffectStage::OtelSupabase, &ports, || {
            otel.sync_dnd_event_to_supabase(&telemetry, &key)
        })?;
    }
    Ok(())
}

#[derive(Default)]
pub struct BufferedEffectSink {
    receipts: RefCell<Vec<DndEffectReceipt>>,
}

impl BufferedEffectSink {
    #[must_use]
    pub fn snapshot(&self) -> Vec<DndEffectReceipt> {
        self.receipts.borrow().clone()
    }
}

impl DndEffectSink for BufferedEffectSink {
    fn publish(&self, receipt: &DndEffectReceipt) {
        self.receipts.borrow_mut().push(receipt.clone());
    }
}

#[derive(Default)]
pub struct MemoryEffectJournal {
    completed: RefCell<BTreeSet<(String, DndEffectStage)>>,
}

impl DndEffectJournalPort for MemoryEffectJournal {
    fn has_completed(&self, idempotency_key: &str, stage: DndEffectStage) -> Result<bool, DndError> {
        Ok(self.completed.borrow().contains(&(idempotency_key.to_owned(), stage)))
    }

    fn mark_completed(&self, idempotency_key: &str, stage: DndEffectStage) -> Result<(), DndError> {
        self.completed.borrow_mut().insert((idempotency_key.to_owned(), stage));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;
    use crate::{decode_envelope_json, ValidationOptions};

    const VALID: &str = include_str!("../../../contracts/instances/DndEnvelope/valid/text-copy.json");

    struct Forms(Rc<RefCell<Vec<&'static str>>>);
    impl OresFormsPort for Forms {
        fn apply_accepted_drop(&self, _: &DndEnvelope, _: &DndDropResult) -> Result<(), DndError> {
            self.0.borrow_mut().push("forms");
            Ok(())
        }
    }

    struct Opto(Rc<RefCell<Vec<&'static str>>>, Rc<RefCell<Vec<String>>>);
    impl OptoSyncPort for Opto {
        fn persist_accepted_drop(&self, _: &DndEnvelope, _: &DndDropResult) -> Result<(), DndError> {
            self.0.borrow_mut().push("opto-local");
            Ok(())
        }
    }
    impl OptoSyncSupabasePort for Opto {
        fn sync_accepted_drop_to_supabase(
            &self,
            _: &DndEnvelope,
            _: &DndDropResult,
            key: &str,
        ) -> Result<(), DndError> {
            self.0.borrow_mut().push("opto-supabase");
            self.1.borrow_mut().push(key.to_owned());
            Ok(())
        }
    }

    struct Otel {
        calls: Rc<RefCell<Vec<&'static str>>>,
        keys: Rc<RefCell<Vec<String>>>,
        fail_remote_once: Cell<bool>,
    }
    impl OresOtelPort for Otel {
        fn emit_dnd_event(&self, event: &DndTelemetryEvent) -> Result<(), DndError> {
            assert!(!serde_json::to_string(event)?.contains("hello"));
            self.calls.borrow_mut().push("otel-local");
            Ok(())
        }
    }
    impl OresOtelSupabasePort for Otel {
        fn sync_dnd_event_to_supabase(
            &self,
            event: &DndTelemetryEvent,
            key: &str,
        ) -> Result<(), DndError> {
            assert!(!serde_json::to_string(event)?.contains("hello"));
            self.calls.borrow_mut().push("otel-supabase");
            self.keys.borrow_mut().push(key.to_owned());
            if self.fail_remote_once.replace(false) {
                return Err(DndError("provider-token=sensitive-value".to_owned()));
            }
            Ok(())
        }
    }

    struct BrokenJournal;
    impl DndEffectJournalPort for BrokenJournal {
        fn has_completed(&self, _: &str, _: DndEffectStage) -> Result<bool, DndError> {
            Err(DndError("journal unavailable".to_owned()))
        }
        fn mark_completed(&self, _: &str, _: DndEffectStage) -> Result<(), DndError> {
            Err(DndError("journal unavailable".to_owned()))
        }
    }

    fn fixture() -> Result<DndEnvelope, DndError> {
        decode_envelope_json(VALID, ValidationOptions::default())
    }

    fn result(envelope: &DndEnvelope) -> DndDropResult {
        DndDropResult {
            drag_id: envelope.drag_id.clone(),
            accepted: true,
            operation: Some(DndOperation::Copy),
            target_id: Some("field-1".to_owned()),
            error_code: None,
        }
    }

    #[test]
    fn stable_key_and_order_are_payload_free() -> Result<(), DndError> {
        let envelope = fixture()?;
        let result = result(&envelope);
        let calls = Rc::new(RefCell::new(Vec::new()));
        let keys = Rc::new(RefCell::new(Vec::new()));
        let forms = Forms(Rc::clone(&calls));
        let opto = Opto(Rc::clone(&calls), Rc::clone(&keys));
        let otel = Otel {
            calls: Rc::clone(&calls),
            keys: Rc::clone(&keys),
            fail_remote_once: Cell::new(false),
        };
        let journal = MemoryEffectJournal::default();
        let sink = BufferedEffectSink::default();

        commit_accepted_drop_effects(
            &envelope,
            &result,
            ReactiveEffectPorts {
                forms: Some(&forms),
                opto_sync: Some(&opto),
                otel: Some(&otel),
                journal: Some(&journal),
                receipts: Some(&sink),
            },
        )?;

        assert_eq!(calls.borrow().as_slice(), ["forms", "opto-local", "opto-supabase", "otel-local", "otel-supabase"]);
        assert!(keys.borrow().iter().all(|key| key == &dnd_effect_key(&result)));
        let serialized = serde_json::to_string(&sink.snapshot())?;
        assert!(!serialized.contains("hello"));
        Ok(())
    }

    #[test]
    fn retry_skips_completed_stages_after_late_failure() -> Result<(), DndError> {
        let envelope = fixture()?;
        let result = result(&envelope);
        let calls = Rc::new(RefCell::new(Vec::new()));
        let keys = Rc::new(RefCell::new(Vec::new()));
        let forms = Forms(Rc::clone(&calls));
        let opto = Opto(Rc::clone(&calls), Rc::clone(&keys));
        let otel = Otel {
            calls: Rc::clone(&calls),
            keys: Rc::clone(&keys),
            fail_remote_once: Cell::new(true),
        };
        let journal = MemoryEffectJournal::default();
        let sink = BufferedEffectSink::default();

        let ports = || ReactiveEffectPorts {
            forms: Some(&forms),
            opto_sync: Some(&opto),
            otel: Some(&otel),
            journal: Some(&journal),
            receipts: Some(&sink),
        };
        assert!(commit_accepted_drop_effects(&envelope, &result, ports()).is_err());
        commit_accepted_drop_effects(&envelope, &result, ports())?;

        assert_eq!(calls.borrow().as_slice(), [
            "forms", "opto-local", "opto-supabase", "otel-local", "otel-supabase", "otel-supabase"
        ]);
        let receipts = sink.snapshot();
        let second = &receipts[5..];
        assert_eq!(second.iter().map(|r| (r.stage, r.status)).collect::<Vec<_>>(), vec![
            (DndEffectStage::Forms, DndEffectStatus::Skipped),
            (DndEffectStage::OptoLocal, DndEffectStatus::Skipped),
            (DndEffectStage::OptoSupabase, DndEffectStatus::Skipped),
            (DndEffectStage::OtelLocal, DndEffectStatus::Skipped),
            (DndEffectStage::OtelSupabase, DndEffectStatus::Completed),
        ]);
        let serialized = serde_json::to_string(&receipts)?;
        assert!(!serialized.contains("provider-token"));
        assert!(!serialized.contains("sensitive-value"));
        assert!(!serialized.contains("hello"));
        Ok(())
    }

    #[test]
    fn journal_lookup_failure_executes_no_external_effect() -> Result<(), DndError> {
        let envelope = fixture()?;
        let result = result(&envelope);
        let calls = Rc::new(RefCell::new(Vec::new()));
        let forms = Forms(Rc::clone(&calls));
        let sink = BufferedEffectSink::default();
        let journal = BrokenJournal;
        let outcome = commit_accepted_drop_effects(
            &envelope,
            &result,
            ReactiveEffectPorts {
                forms: Some(&forms),
                opto_sync: None,
                otel: None,
                journal: Some(&journal),
                receipts: Some(&sink),
            },
        );
        assert!(outcome.is_err());
        assert!(calls.borrow().is_empty());
        assert_eq!(sink.snapshot().len(), 1);
        assert_eq!(sink.snapshot()[0].status, DndEffectStatus::Failed);
        Ok(())
    }
}
