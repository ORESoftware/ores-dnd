use std::convert::Infallible;

use crate::{
    telemetry_for, DndEnvelope, DndError, DndLifecyclePhase, DndOperation, DndTelemetryEvent,
    ValidationOptions,
};
use rxrust::prelude::Local;
use rxrust::ObservableFactory;

/// Re-export rxRust's operator/context prelude from the ORES reactive surface.
/// UI/WASM callers normally choose `Local`; cross-thread native callers may
/// opt into `Shared` explicitly.
pub use rxrust::prelude as rx;

/// Process-local reactive event. The envelope can contain dragged data, so this
/// event must never be serialized into telemetry, replay logs, analytics, or
/// crash-reporting state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DndReactiveEvent {
    pub phase: DndLifecyclePhase,
    pub envelope: DndEnvelope,
    pub operation: Option<DndOperation>,
    pub target_id: Option<String>,
}

impl DndReactiveEvent {
    pub fn new(
        phase: DndLifecyclePhase,
        envelope: DndEnvelope,
        operation: Option<DndOperation>,
        target_id: Option<String>,
    ) -> Result<Self, DndError> {
        envelope.validate(ValidationOptions::default())?;
        if let Some(operation) = operation {
            if !envelope.allowed_operations.contains(&operation) {
                return Err(DndError(
                    "reactive event operation is not source-allowed".to_owned(),
                ));
            }
        }
        if target_id.as_deref().is_some_and(str::is_empty) {
            return Err(DndError(
                "reactive event targetId must be a non-empty string".to_owned(),
            ));
        }
        Ok(Self {
            phase,
            envelope,
            operation,
            target_id,
        })
    }
}

/// Replay-safe current state. This deliberately has no DndItem/data field.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DndReactiveState {
    pub active: bool,
    pub phase: Option<DndLifecyclePhase>,
    pub drag_id: Option<String>,
    pub source_runtime: Option<String>,
    pub item_count: usize,
    pub operation: Option<DndOperation>,
    pub target_id: Option<String>,
}

impl DndReactiveState {
    pub fn idle() -> Self {
        Self {
            active: false,
            phase: None,
            drag_id: None,
            source_runtime: None,
            item_count: 0,
            operation: None,
            target_id: None,
        }
    }
}

pub fn reactive_state_for(event: &DndReactiveEvent) -> DndReactiveState {
    DndReactiveState {
        active: event.phase != DndLifecyclePhase::DragEnd,
        phase: Some(event.phase),
        drag_id: Some(event.envelope.drag_id.clone()),
        source_runtime: Some(event.envelope.source_runtime.clone()),
        item_count: event.envelope.items.len(),
        operation: event.operation,
        target_id: event.target_id.clone(),
    }
}

pub fn reactive_telemetry_for(event: &DndReactiveEvent) -> DndTelemetryEvent {
    telemetry_for(
        event.phase,
        &event.envelope,
        event.operation,
        event.target_id.clone(),
    )
}

/// Canonical hot rxRust subject for UI/WASM drag events. It intentionally does
/// not replay envelopes. Derive replayable state by mapping events through
/// [reactive_state_for] into a `Local::behavior_subject` when a UI needs current
/// state semantics.
pub type DndLocalEventSubject =
    rxrust::subject::LocalSubject<'static, DndReactiveEvent, Infallible>;

pub fn local_event_subject() -> DndLocalEventSubject {
    Local::subject::<DndReactiveEvent, Infallible>()
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::rx::*;
    use super::*;
    use crate::{DndItem, DndItemKind, ORES_DND_PROTOCOL};

    fn envelope() -> DndEnvelope {
        DndEnvelope {
            protocol: ORES_DND_PROTOCOL.to_owned(),
            drag_id: "drag-rx-1".to_owned(),
            source_runtime: "rust-test".to_owned(),
            allowed_operations: vec![DndOperation::Copy, DndOperation::Move],
            items: vec![DndItem {
                kind: DndItemKind::Text,
                media_type: "text/plain".to_owned(),
                data: "TOP-SECRET-DRAG-DATA".to_owned(),
                name: None,
            }],
            traceparent: None,
            form_id: None,
        }
    }

    #[test]
    fn rxrust_local_pipeline_projects_payload_free_state_and_telemetry() -> Result<(), DndError> {
        let events = vec![
            DndReactiveEvent::new(DndLifecyclePhase::DragStart, envelope(), None, None)?,
            DndReactiveEvent::new(
                DndLifecyclePhase::Drop,
                envelope(),
                Some(DndOperation::Copy),
                Some("zone-a".to_owned()),
            )?,
            DndReactiveEvent::new(
                DndLifecyclePhase::DragEnd,
                envelope(),
                Some(DndOperation::Copy),
                Some("zone-a".to_owned()),
            )?,
        ];

        let states = Rc::new(RefCell::new(Vec::new()));
        let state_sink = Rc::clone(&states);
        Local::from_iter(events.clone())
            .map(|event| reactive_state_for(&event))
            .subscribe(move |state| state_sink.borrow_mut().push(state));

        let telemetry = Rc::new(RefCell::new(Vec::new()));
        let telemetry_sink = Rc::clone(&telemetry);
        Local::from_iter(events)
            .map(|event| reactive_telemetry_for(&event))
            .subscribe(move |event| telemetry_sink.borrow_mut().push(event));

        let states_json = serde_json::to_string(&*states.borrow())?;
        let telemetry_json = serde_json::to_string(&*telemetry.borrow())?;
        assert!(!states_json.contains("TOP-SECRET-DRAG-DATA"));
        assert!(!telemetry_json.contains("TOP-SECRET-DRAG-DATA"));
        assert_eq!(states.borrow().last().map(|state| state.active), Some(false));
        assert_eq!(telemetry.borrow().len(), 3);
        Ok(())
    }

    #[test]
    fn rxrust_subject_is_hot_and_filters_drop_events() -> Result<(), DndError> {
        let mut subject = local_event_subject();
        subject.next(DndReactiveEvent::new(
            DndLifecyclePhase::DragStart,
            envelope(),
            None,
            None,
        )?);

        let drops = Rc::new(RefCell::new(Vec::new()));
        let drop_sink = Rc::clone(&drops);
        subject
            .clone()
            .filter(|event| event.phase == DndLifecyclePhase::Drop)
            .subscribe(move |event| drop_sink.borrow_mut().push(event.phase));

        assert!(drops.borrow().is_empty());
        subject.next(DndReactiveEvent::new(
            DndLifecyclePhase::DragOver,
            envelope(),
            None,
            None,
        )?);
        subject.next(DndReactiveEvent::new(
            DndLifecyclePhase::Drop,
            envelope(),
            Some(DndOperation::Copy),
            Some("zone-a".to_owned()),
        )?);
        assert_eq!(&*drops.borrow(), &[DndLifecyclePhase::Drop]);
        Ok(())
    }
}
