use std::convert::Infallible;

use crate::{
    telemetry_for, DndEnvelope, DndError, DndLifecyclePhase, DndOperation, DndTelemetryEvent,
    ValidationOptions,
};
use rxrust::prelude::{Local, Observer};
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
        if phase == DndLifecyclePhase::Drop {
            if operation.is_none() {
                return Err(DndError(
                    "drop event requires a negotiated operation".to_owned(),
                ));
            }
            if target_id.is_none() {
                return Err(DndError("drop event requires a targetId".to_owned()));
            }
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

/// `drag-over` may be sampled/coalesced for presentation work.
pub const fn is_high_frequency_lifecycle_phase(phase: DndLifecyclePhase) -> bool {
    matches!(phase, DndLifecyclePhase::DragOver)
}

/// `drop` and `drag-end` are terminal-significant and must never be throttled away.
pub const fn is_lossless_lifecycle_phase(phase: DndLifecyclePhase) -> bool {
    matches!(phase, DndLifecyclePhase::Drop | DndLifecyclePhase::DragEnd)
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

const fn is_start_phase(phase: DndLifecyclePhase) -> bool {
    matches!(phase, DndLifecyclePhase::DragStart | DndLifecyclePhase::DragEnter)
}

const fn can_transition(
    previous: Option<DndLifecyclePhase>,
    next: DndLifecyclePhase,
) -> bool {
    match previous {
        None => is_start_phase(next),
        Some(DndLifecyclePhase::DragStart) => matches!(
            next,
            DndLifecyclePhase::DragEnter
                | DndLifecyclePhase::DragOver
                | DndLifecyclePhase::Drop
                | DndLifecyclePhase::DragEnd
        ),
        Some(DndLifecyclePhase::DragEnter) => matches!(
            next,
            DndLifecyclePhase::DragOver
                | DndLifecyclePhase::DragLeave
                | DndLifecyclePhase::Drop
                | DndLifecyclePhase::DragEnd
        ),
        Some(DndLifecyclePhase::DragOver) => matches!(
            next,
            DndLifecyclePhase::DragOver
                | DndLifecyclePhase::DragLeave
                | DndLifecyclePhase::Drop
                | DndLifecyclePhase::DragEnd
        ),
        Some(DndLifecyclePhase::DragLeave) => matches!(
            next,
            DndLifecyclePhase::DragEnter
                | DndLifecyclePhase::DragOver
                | DndLifecyclePhase::DragEnd
        ),
        Some(DndLifecyclePhase::Drop) => matches!(next, DndLifecyclePhase::DragEnd),
        Some(DndLifecyclePhase::DragEnd) => is_start_phase(next),
    }
}

/// Fail-closed lifecycle tracker for rxRust/native/WASM consumers.
///
/// A session may begin with `DragStart` for an in-app drag or `DragEnter` for
/// an external/system drag. Once active, events may not silently switch drag IDs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DndLifecycleTracker {
    drag_id: Option<String>,
    phase: Option<DndLifecyclePhase>,
}

impl DndLifecycleTracker {
    pub fn drag_id(&self) -> Option<&str> {
        self.drag_id.as_deref()
    }

    pub const fn phase(&self) -> Option<DndLifecyclePhase> {
        self.phase
    }

    pub fn accept(&mut self, event: &DndReactiveEvent) -> Result<DndReactiveState, DndError> {
        if let Some(current_drag_id) = self.drag_id.as_deref() {
            if event.envelope.drag_id != current_drag_id {
                return Err(DndError(format!(
                    "reactive dragId switched before drag-end: {current_drag_id} -> {}",
                    event.envelope.drag_id
                )));
            }
        }
        if !can_transition(self.phase, event.phase) {
            return Err(DndError(format!(
                "invalid reactive lifecycle transition: {:?} -> {:?}",
                self.phase, event.phase
            )));
        }

        let state = reactive_state_for(event);
        if event.phase == DndLifecyclePhase::DragEnd {
            self.reset();
        } else {
            self.drag_id = Some(event.envelope.drag_id.clone());
            self.phase = Some(event.phase);
        }
        Ok(state)
    }

    pub fn reset(&mut self) {
        self.drag_id = None;
        self.phase = None;
    }
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

/// Thin fail-closed owner for a local rxRust event subject plus lifecycle state.
/// Consumers clone [Self::events] to build operator chains; [Self::emit] remains
/// the canonical admission boundary for validated session ordering.
pub struct DndLocalReactiveBus {
    events: DndLocalEventSubject,
    tracker: DndLifecycleTracker,
}

impl Default for DndLocalReactiveBus {
    fn default() -> Self {
        Self::new()
    }
}

impl DndLocalReactiveBus {
    pub fn new() -> Self {
        Self {
            events: local_event_subject(),
            tracker: DndLifecycleTracker::default(),
        }
    }

    pub fn events(&self) -> DndLocalEventSubject {
        self.events.clone()
    }

    pub fn emit(&mut self, event: DndReactiveEvent) -> Result<DndReactiveState, DndError> {
        let state = self.tracker.accept(&event)?;
        self.events.next(event);
        Ok(state)
    }

    pub fn tracker(&self) -> &DndLifecycleTracker {
        &self.tracker
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::rx::*;
    use super::*;
    use crate::{DndItem, DndItemKind, ORES_DND_PROTOCOL};

    fn envelope_with_id(drag_id: &str) -> DndEnvelope {
        DndEnvelope {
            protocol: ORES_DND_PROTOCOL.to_owned(),
            drag_id: drag_id.to_owned(),
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

    fn envelope() -> DndEnvelope {
        envelope_with_id("drag-rx-1")
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
        assert!(is_high_frequency_lifecycle_phase(DndLifecyclePhase::DragOver));
        assert!(is_lossless_lifecycle_phase(DndLifecyclePhase::Drop));
        assert!(is_lossless_lifecycle_phase(DndLifecyclePhase::DragEnd));
        assert!(!is_lossless_lifecycle_phase(DndLifecyclePhase::DragOver));
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

    #[test]
    fn rxrust_bus_accepts_external_drag_and_tracks_lossless_terminal_events() -> Result<(), DndError> {
        let mut bus = DndLocalReactiveBus::new();
        let lossless = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&lossless);
        bus.events()
            .filter(|event| is_lossless_lifecycle_phase(event.phase))
            .subscribe(move |event| sink.borrow_mut().push(event.phase));

        bus.emit(DndReactiveEvent::new(
            DndLifecyclePhase::DragEnter,
            envelope_with_id("external-1"),
            None,
            Some("zone-a".to_owned()),
        )?)?;
        bus.emit(DndReactiveEvent::new(
            DndLifecyclePhase::DragOver,
            envelope_with_id("external-1"),
            None,
            Some("zone-a".to_owned()),
        )?)?;
        bus.emit(DndReactiveEvent::new(
            DndLifecyclePhase::Drop,
            envelope_with_id("external-1"),
            Some(DndOperation::Copy),
            Some("zone-a".to_owned()),
        )?)?;
        bus.emit(DndReactiveEvent::new(
            DndLifecyclePhase::DragEnd,
            envelope_with_id("external-1"),
            Some(DndOperation::Copy),
            Some("zone-a".to_owned()),
        )?)?;

        assert_eq!(
            &*lossless.borrow(),
            &[DndLifecyclePhase::Drop, DndLifecyclePhase::DragEnd]
        );
        assert_eq!(bus.tracker().drag_id(), None);
        assert_eq!(bus.tracker().phase(), None);
        Ok(())
    }

    #[test]
    fn rxrust_tracker_rejects_impossible_transition_and_drag_id_switch() -> Result<(), DndError> {
        let mut bus = DndLocalReactiveBus::new();
        let drop_first = DndReactiveEvent::new(
            DndLifecyclePhase::Drop,
            envelope(),
            Some(DndOperation::Copy),
            Some("zone-a".to_owned()),
        )?;
        assert!(bus.emit(drop_first).is_err());

        bus.emit(DndReactiveEvent::new(
            DndLifecyclePhase::DragStart,
            envelope_with_id("drag-a"),
            None,
            None,
        )?)?;
        let switched = DndReactiveEvent::new(
            DndLifecyclePhase::DragOver,
            envelope_with_id("drag-b"),
            None,
            None,
        )?;
        assert!(bus.emit(switched).is_err());
        Ok(())
    }

    #[test]
    fn drop_requires_source_allowed_operation_and_target() {
        assert!(DndReactiveEvent::new(
            DndLifecyclePhase::Drop,
            envelope(),
            None,
            Some("zone-a".to_owned()),
        )
        .is_err());
        assert!(DndReactiveEvent::new(
            DndLifecyclePhase::Drop,
            envelope(),
            Some(DndOperation::Copy),
            None,
        )
        .is_err());
        assert!(DndReactiveEvent::new(
            DndLifecyclePhase::DragOver,
            envelope(),
            Some(DndOperation::Link),
            None,
        )
        .is_err());
    }
}
