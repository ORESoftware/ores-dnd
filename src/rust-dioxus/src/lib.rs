//! `ores-dnd-dioxus` — drag-and-drop for Dioxus 0.7 web, desktop and mobile.
//!
//! Dioxus exposes a portable `DataTransfer`, so one adapter serves every
//! renderer: [`DragSource`] writes the envelope on `ondragstart`, [`DropZone`]
//! feeds `ondragenter`/`ondragover`/`ondragleave`/`ondrop` into the shared
//! session state machine, and the reactive [`DndSessionHandle`] snapshot drives
//! the UI. Intra-app drags are evaluated exactly (the envelope is known from
//! `start`); external drags are evaluated provisionally until `drop`, when the
//! payload becomes readable.

use dioxus::html::{Modifiers, ModifiersInteraction};
use dioxus::prelude::*;
use ores_dnd_core::{
    decode_envelope_json, effect_allowed_for, encode_envelope_json, DndDropPolicy, DndDropResult,
    DndEnvelope, DndItem, DndItemKind, DndOperation, DndSession, DndSessionInput,
    DndSessionSnapshot, DndSessionState, ValidationOptions, ORES_DND_MIME, ORES_DND_PROTOCOL,
};

pub use ores_dnd_core;

/// A drag session shared by every source and zone of a component tree.
#[derive(Clone, Copy, PartialEq)]
pub struct DndSessionHandle {
    snapshot: Signal<DndSessionSnapshot>,
    session: Signal<DndSession>,
}

impl DndSessionHandle {
    /// The reactive snapshot; reading it in `rsx!` re-renders on change.
    pub fn snapshot(&self) -> Signal<DndSessionSnapshot> {
        self.snapshot
    }

    /// Apply one input; returns the new snapshot and notifies subscribers.
    pub fn apply(&self, input: &DndSessionInput) -> DndSessionSnapshot {
        let mut session = self.session;
        let next = session.write().apply(input).clone();
        let mut snapshot = self.snapshot;
        snapshot.set(next.clone());
        next
    }

    /// The running session's envelope, if any.
    pub fn envelope(&self) -> Option<DndEnvelope> {
        self.session.peek().envelope().cloned()
    }

    /// The negotiated operation while over an accepting target.
    pub fn operation(&self) -> Option<DndOperation> {
        self.snapshot.peek().operation
    }
}

/// Create the session signals. Call once near the root; the handle is `Copy`.
pub fn use_dnd_session() -> DndSessionHandle {
    use_dnd_session_with(ValidationOptions::default())
}

pub fn use_dnd_session_with(options: ValidationOptions) -> DndSessionHandle {
    let snapshot = use_signal(|| DndSessionSnapshot::IDLE);
    let session = use_signal(move || DndSession::with_options(options));
    DndSessionHandle { snapshot, session }
}

/// Create the session and provide it as context for [`use_dnd_context`].
pub fn provide_dnd_session() -> DndSessionHandle {
    let handle = use_dnd_session();
    use_context_provider(move || handle)
}

pub fn use_dnd_context() -> DndSessionHandle {
    use_context::<DndSessionHandle>()
}

/// The `data-ores-dnd-state` value for a zone: reflects the session only while
/// this zone is the active target.
pub fn zone_state_attribute(snapshot: &DndSessionSnapshot, target_id: &str) -> &'static str {
    match (
        &snapshot.state,
        snapshot.target_id.as_deref() == Some(target_id),
    ) {
        (DndSessionState::OverTarget, true) => "accepting",
        (DndSessionState::Dragging, true) => "rejecting",
        (DndSessionState::Dragging | DndSessionState::OverTarget, false) => "dragging",
        (DndSessionState::Dropped, true) => "dropped",
        _ => "idle",
    }
}

/// Modifier keys express a preference the same way native file managers do:
/// Ctrl/⌥ → copy, Shift → move, Ctrl+Shift/⌘ → link.
pub fn preferred_operation(modifiers: Modifiers) -> Option<DndOperation> {
    let ctrl = modifiers.contains(Modifiers::CONTROL) || modifiers.contains(Modifiers::ALT);
    let shift = modifiers.contains(Modifiers::SHIFT);
    let meta = modifiers.contains(Modifiers::META);
    match (ctrl, shift, meta) {
        (true, true, _) | (_, _, true) => Some(DndOperation::Link),
        (true, false, false) => Some(DndOperation::Copy),
        (false, true, false) => Some(DndOperation::Move),
        _ => None,
    }
}

/// An envelope standing in for an external drag whose payload is not yet
/// readable (kinds unknown → a single empty `text/plain` item).
pub fn provisional_envelope(drag_id: &str) -> DndEnvelope {
    DndEnvelope {
        protocol: ORES_DND_PROTOCOL.to_owned(),
        drag_id: drag_id.to_owned(),
        source_runtime: "external".to_owned(),
        allowed_operations: vec![DndOperation::Copy],
        items: vec![DndItem {
            kind: DndItemKind::Text,
            media_type: "text/plain".into(),
            data: String::new(),
            name: None,
        }],
        traceparent: None,
        form_id: None,
    }
}

/// Read the ores envelope from a portable DataTransfer, or synthesize one from
/// its `text/plain` content.
pub fn read_envelope(
    dt: &dioxus::html::DataTransfer,
    options: ValidationOptions,
) -> Option<DndEnvelope> {
    if let Some(json) = dt.get_data(ORES_DND_MIME).filter(|s| !s.is_empty()) {
        return decode_envelope_json(&json, options).ok();
    }
    let text = dt.get_as_text().filter(|s| !s.is_empty())?;
    let envelope = DndEnvelope {
        protocol: ORES_DND_PROTOCOL.to_owned(),
        drag_id: format!("external-text-{}", text.len()),
        source_runtime: "external".to_owned(),
        allowed_operations: vec![DndOperation::Copy],
        items: vec![DndItem {
            kind: DndItemKind::Text,
            media_type: "text/plain".into(),
            data: text,
            name: None,
        }],
        traceparent: None,
        form_id: None,
    };
    envelope.validate(options).ok().map(|_| envelope)
}

/// A drop target. `on_drop` receives the verified envelope and the terminal
/// result; wire it to `commit_accepted_drop` with the app's ports.
#[component]
pub fn DropZone(
    handle: DndSessionHandle,
    policy: DndDropPolicy,
    #[props(optional)] on_drop: Option<EventHandler<(DndEnvelope, DndDropResult)>>,
    #[props(optional)] class: Option<String>,
    children: Element,
) -> Element {
    let target_id = policy.target_id.clone();
    let policy_json = serde_json::to_string(&policy).unwrap_or_default();
    let state_attr = zone_state_attribute(&handle.snapshot().read(), &target_id);

    let enter_policy = policy.clone();
    let enter_target = target_id.clone();
    let enter = move |evt: DragEvent| {
        let preferred = preferred_operation(evt.data().modifiers());
        if handle.snapshot().peek().state == DndSessionState::Idle {
            handle.apply(&DndSessionInput::start(provisional_envelope("external")));
        }
        let next = handle.apply(&DndSessionInput::enter(enter_policy.clone(), preferred));
        if next.state == DndSessionState::OverTarget
            && next.target_id.as_deref() == Some(enter_target.as_str())
        {
            evt.prevent_default();
            if let Some(op) = next.operation {
                evt.data().data_transfer().set_drop_effect(op.wire());
            }
        }
    };
    let over = enter.clone();
    let leave_target = target_id.clone();
    let leave = move |_evt: DragEvent| {
        handle.apply(&DndSessionInput::leave(leave_target.clone()));
    };
    let drop_policy = policy.clone();
    let drop_target = target_id.clone();
    let drop = move |evt: DragEvent| {
        evt.prevent_default();
        let preferred = preferred_operation(evt.data().modifiers());
        if let Some(envelope) =
            read_envelope(&evt.data().data_transfer(), ValidationOptions::default())
        {
            let current_is_real = handle
                .envelope()
                .is_some_and(|e| e.drag_id == envelope.drag_id);
            if !current_is_real {
                handle.apply(&DndSessionInput::start(envelope));
                handle.apply(&DndSessionInput::enter(drop_policy.clone(), preferred));
            }
        }
        let snapshot = handle.apply(&DndSessionInput::drop(drop_target.clone()));
        if let (Some(cb), Some(result), Some(envelope)) =
            (on_drop, snapshot.result(), handle.envelope())
        {
            cb.call((envelope, result));
        }
    };

    rsx! {
        div {
            class: class,
            "data-ores-dnd-zone": "{target_id}",
            "data-ores-dnd-policy": "{policy_json}",
            "data-ores-dnd-state": "{state_attr}",
            ondragenter: enter,
            ondragover: over,
            ondragleave: leave,
            ondrop: drop,
            {children}
        }
    }
}

/// A drag source: writes the envelope into `DataTransfer` on `dragstart`,
/// starts the session, and ends it on `dragend`.
#[component]
pub fn DragSource(
    handle: DndSessionHandle,
    envelope: DndEnvelope,
    #[props(optional)] class: Option<String>,
    children: Element,
) -> Element {
    let source_json =
        encode_envelope_json(&envelope, ValidationOptions::default()).unwrap_or_default();
    let start_envelope = envelope.clone();
    let start = move |evt: DragEvent| {
        let dt = evt.data().data_transfer();
        match encode_envelope_json(&start_envelope, ValidationOptions::default()) {
            Ok(json) => {
                let _ = dt.set_data(ORES_DND_MIME, &json);
                dt.set_effect_allowed(effect_allowed_for(&start_envelope.allowed_operations));
                if let Some(text) = start_envelope.text_fallback() {
                    let _ = dt.set_data("text/plain", text);
                }
                handle.apply(&DndSessionInput::start(start_envelope.clone()));
            }
            Err(_) => evt.prevent_default(),
        }
    };
    let end = move |_evt: DragEvent| {
        handle.apply(&DndSessionInput::end());
    };
    rsx! {
        div {
            class: class,
            draggable: "true",
            "data-ores-dnd-source": "{source_json}",
            ondragstart: start,
            ondragend: end,
            {children}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::core::VirtualDom;
    use ores_dnd_core::decode_envelope_json;
    use std::cell::RefCell;

    const VALID: &str =
        include_str!("../../../contracts/instances/DndEnvelope/valid/text-copy.json");

    thread_local! {
        static HANDLE: RefCell<Option<DndSessionHandle>> = const { RefCell::new(None) };
    }

    fn app() -> Element {
        let handle = provide_dnd_session();
        HANDLE.with(|h| *h.borrow_mut() = Some(handle));
        let envelope = decode_envelope_json(VALID, ValidationOptions::default()).unwrap();
        let policy = DndDropPolicy::new("zone-a", &[DndOperation::Copy], &[DndItemKind::Text]);
        rsx! {
            DragSource { handle, envelope, "card" }
            DropZone { handle, policy, "drop here" }
        }
    }

    #[test]
    fn components_mount_and_the_handle_drives_the_shared_machine() {
        let mut dom = VirtualDom::new(app);
        dom.rebuild_in_place();
        let handle = HANDLE.with(|h| h.borrow().unwrap());
        dom.in_runtime(|| {
            assert_eq!(*handle.snapshot().peek(), DndSessionSnapshot::IDLE);
            let envelope = decode_envelope_json(VALID, ValidationOptions::default()).unwrap();
            handle.apply(&DndSessionInput::start(envelope.clone()));
            assert_eq!(handle.envelope(), Some(envelope));
            let policy = DndDropPolicy::new("zone-a", &[DndOperation::Copy], &[DndItemKind::Text]);
            let over = handle.apply(&DndSessionInput::enter(policy, None));
            assert_eq!(zone_state_attribute(&over, "zone-a"), "accepting");
            assert_eq!(handle.operation(), Some(DndOperation::Copy));
            let dropped = handle.apply(&DndSessionInput::drop("zone-a"));
            assert!(dropped.result().unwrap().accepted);
        });
        // the zone re-renders from the updated snapshot without panicking
        dom.render_immediate(&mut dioxus::core::NoOpMutations);
    }

    #[test]
    fn modifier_preferences_and_provisional_envelopes() {
        assert_eq!(
            preferred_operation(Modifiers::CONTROL),
            Some(DndOperation::Copy)
        );
        assert_eq!(
            preferred_operation(Modifiers::SHIFT),
            Some(DndOperation::Move)
        );
        assert_eq!(
            preferred_operation(Modifiers::META),
            Some(DndOperation::Link)
        );
        assert_eq!(preferred_operation(Modifiers::empty()), None);
        assert!(provisional_envelope("external")
            .validate(ValidationOptions::default())
            .is_ok());
        assert_eq!(zone_state_attribute(&DndSessionSnapshot::IDLE, "z"), "idle");
    }
}
