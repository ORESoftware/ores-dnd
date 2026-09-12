use std::convert::Infallible;

use crate::{
    telemetry_for, DndEnvelope, DndError, DndLifecyclePhase, DndOperation, DndTelemetryEvent,
    ValidationOptions,
};
use rxrust::prelude::{Local, Observer};
use rxrust::ObservableFactory;

pub use rxrust::prelude as rx;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DndLifecycleMode {
    Strict,
    ExternalDropCompatible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DndLifecycleGuard {
    mode: DndLifecycleMode,
    active_drag_id: Option<String>,
    dropped: bool,
}

impl Default for DndLifecycleGuard {
    fn default() -> Self {
        Self::new(DndLifecycleMode::Strict)
    }
}

impl DndLifecycleGuard {
    #[must_use]
    pub fn new(mode: DndLifecycleMode) -> Self {
        Self {
            mode,
            active_drag_id: None,
            dropped: false,
        }
    }

    #[must_use]
    pub fn active(&self) -> bool {
        self.active_drag_id.is_some()
    }

    #[must_use]
    pub fn active_drag_id(&self) -> Option<&str> {
        self.active_drag_id.as_deref()
    }

    pub fn accept(&mut self, event: &DndReactiveEvent) -> Result<(), DndError> {
        let drag_id = event.envelope.drag_id.as_str();
        if self.active_drag_id.is_none() {
            if event.phase == DndLifecyclePhase::DragStart {
                self.active_drag_id = Some(drag_id.to_owned());
                self.dropped = false;
                return Ok(());
            }
            if event.phase == DndLifecyclePhase::Drop
                && self.mode == DndLifecycleMode::ExternalDropCompatible
            {
                return Ok(());
            }
            return Err(DndError(format!(
                "{:?} requires an active drag-start",
                event.phase
            )));
        }

        if self.active_drag_id.as_deref() != Some(drag_id) {
            return Err(DndError(
                "reactive lifecycle dragId changed before drag-end".to_owned(),
            ));
        }
        if self.dropped {
            if event.phase != DndLifecyclePhase::DragEnd {
                return Err(DndError(
                    "reactive lifecycle event is invalid after drop; expected drag-end".to_owned(),
                ));
            }
            self.active_drag_id = None;
            self.dropped = false;
            return Ok(());
        }

        match event.phase {
            DndLifecyclePhase::DragStart => {
                Err(DndError("duplicate drag-start before drag-end".to_owned()))
            }
            DndLifecyclePhase::DragEnter
            | DndLifecyclePhase::DragOver
            | DndLifecyclePhase::DragLeave => Ok(()),
            DndLifecyclePhase::Drop => {
                self.dropped = true;
                Ok(())
            }
            DndLifecyclePhase::DragEnd => {
                self.active_drag_id = None;
                self.dropped = false;
                Ok(())
            }
        }
    }
}

pub type DndLocalEventSubject =
    rxrust::subject::LocalSubject<'static, DndReactiveEvent, Infallible>;

pub fn local_event_subject() -> DndLocalEventSubject {
    Local::subject::<DndReactiveEvent, Infallible>()
}

/// Canonical guarded emission helper for rxRust UI/WASM subjects.
pub fn guarded_next(
    subject: &mut DndLocalEventSubject,
    guard: &mut DndLifecycleGuard,
    event: DndReactiveEvent,
) -> Result<(), DndError> {
    guard.accept(&event)?;
    subject.next(event);
    Ok(())
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
        assert_eq!(
            states.borrow().last().map(|state| state.active),
            Some(false)
        );
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

    #[test]
    fn guarded_subject_rejects_invalid_order_cross_drag_and_duplicate_drop() -> Result<(), DndError>
    {
        let mut subject = local_event_subject();
        let mut guard = DndLifecycleGuard::default();
        let over = DndReactiveEvent::new(DndLifecyclePhase::DragOver, envelope(), None, None)?;
        assert!(guarded_next(&mut subject, &mut guard, over).is_err());

        guarded_next(
            &mut subject,
            &mut guard,
            DndReactiveEvent::new(DndLifecyclePhase::DragStart, envelope(), None, None)?,
        )?;
        let other = DndReactiveEvent::new(
            DndLifecyclePhase::DragEnter,
            envelope_with_id("drag-other"),
            None,
            None,
        )?;
        assert!(guarded_next(&mut subject, &mut guard, other).is_err());
        guarded_next(
            &mut subject,
            &mut guard,
            DndReactiveEvent::new(
                DndLifecyclePhase::Drop,
                envelope(),
                Some(DndOperation::Copy),
                Some("zone-a".to_owned()),
            )?,
        )?;
        let duplicate = DndReactiveEvent::new(
            DndLifecyclePhase::Drop,
            envelope(),
            Some(DndOperation::Copy),
            None,
        )?;
        assert!(guarded_next(&mut subject, &mut guard, duplicate).is_err());
        guarded_next(
            &mut subject,
            &mut guard,
            DndReactiveEvent::new(DndLifecyclePhase::DragEnd, envelope(), None, None)?,
        )?;
        assert!(!guard.active());
        Ok(())
    }

    #[test]
    fn event_options_reject_disallowed_operation_and_empty_target() {
        assert!(DndReactiveEvent::new(
            DndLifecyclePhase::DragOver,
            envelope(),
            Some(DndOperation::Link),
            None,
        )
        .is_err());
        assert!(DndReactiveEvent::new(
            DndLifecyclePhase::DragOver,
            envelope(),
            None,
            Some(String::new()),
        )
        .is_err());
    }

    #[test]
    fn external_drop_compatible_guard_accepts_idle_one_shot_drop() -> Result<(), DndError> {
        let mut guard = DndLifecycleGuard::new(DndLifecycleMode::ExternalDropCompatible);
        let event = DndReactiveEvent::new(
            DndLifecyclePhase::Drop,
            envelope_with_id("external-1"),
            Some(DndOperation::Copy),
            Some("zone-a".to_owned()),
        )?;
        guard.accept(&event)?;
        assert!(!guard.active());
        Ok(())
    }
}
