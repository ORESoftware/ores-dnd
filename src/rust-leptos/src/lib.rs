//! `ores-dnd-leptos` — drag-and-drop for Leptos 0.8 islands and CSR apps.
//!
//! The session lives in a signal ([`use_dnd_session`]); [`DropZone`] and
//! [`DragSource`] translate HTML5 drag events into [`DndSessionInput`]s and
//! keep `data-ores-dnd-state` on their elements in sync so CSS can react.
//! External drags (from another page or app) are evaluated provisionally on
//! `DataTransfer.types` during `dragover` and definitively on `drop`, when the
//! payload becomes readable — see [`browser`].

pub mod browser;

use leptos::prelude::*;
use ores_dnd_core::{
    DndDropPolicy, DndDropResult, DndEnvelope, DndOperation, DndSession, DndSessionInput, DndSessionSnapshot,
    DndSessionState, ValidationOptions,
};

pub use ores_dnd_core;

/// A drag session shared by every source and zone of a component tree.
#[derive(Clone, Copy)]
pub struct DndSessionHandle {
    snapshot: ReadSignal<DndSessionSnapshot>,
    set_snapshot: WriteSignal<DndSessionSnapshot>,
    session: StoredValue<DndSession>,
}

impl DndSessionHandle {
    /// The reactive snapshot: read it in views (`move || handle.snapshot().get().state`).
    pub fn snapshot(&self) -> ReadSignal<DndSessionSnapshot> {
        self.snapshot
    }

    /// Apply one input; returns the new snapshot and notifies subscribers.
    pub fn apply(&self, input: &DndSessionInput) -> DndSessionSnapshot {
        let next = self.session.try_update_value(|session| session.apply(input).clone()).unwrap_or_default();
        self.set_snapshot.set(next.clone());
        next
    }

    /// The running session's envelope, if any.
    pub fn envelope(&self) -> Option<DndEnvelope> {
        self.session.with_value(|session| session.envelope().cloned())
    }

    /// Derived signal: is the pointer over a target that accepts the payload?
    pub fn is_over_accepting_target(&self) -> Signal<bool> {
        let snapshot = self.snapshot;
        Signal::derive(move || snapshot.with(|s| s.state == DndSessionState::OverTarget))
    }
}

/// Create the session signal. Call once near the root and pass the handle
/// (it is `Copy`) to every [`DropZone`] / [`DragSource`], or provide it as context.
pub fn use_dnd_session() -> DndSessionHandle {
    use_dnd_session_with(ValidationOptions::default())
}

pub fn use_dnd_session_with(options: ValidationOptions) -> DndSessionHandle {
    let (snapshot, set_snapshot) = signal(DndSessionSnapshot::IDLE);
    let session = StoredValue::new(DndSession::with_options(options));
    DndSessionHandle { snapshot, set_snapshot, session }
}

/// Provide the handle to descendants; zones/sources then find it with [`expect_dnd_session`].
pub fn provide_dnd_session() -> DndSessionHandle {
    let handle = use_dnd_session();
    provide_context(handle);
    handle
}

pub fn expect_dnd_session() -> DndSessionHandle {
    expect_context::<DndSessionHandle>()
}

/// The `data-ores-dnd-state` value for a zone: reflects the session only while
/// this zone is the active target.
pub fn zone_state_attribute(snapshot: &DndSessionSnapshot, target_id: &str) -> &'static str {
    match (&snapshot.state, snapshot.target_id.as_deref() == Some(target_id)) {
        (DndSessionState::OverTarget, true) => "accepting",
        (DndSessionState::Dragging, true) => "rejecting",
        (DndSessionState::Dragging | DndSessionState::OverTarget, false) => "dragging",
        (DndSessionState::Dropped, true) => "dropped",
        _ => "idle",
    }
}

/// A drop target. `on_drop` receives the verified envelope and the terminal
/// result; wire it to `commit_accepted_drop` with the app's ports.
#[component]
pub fn DropZone(
    handle: DndSessionHandle,
    policy: DndDropPolicy,
    #[prop(optional, into)] on_drop: Option<Callback<(DndEnvelope, DndDropResult)>>,
    #[prop(optional, into)] class: Option<String>,
    children: Children,
) -> impl IntoView {
    let target_id = policy.target_id.clone();
    let policy = StoredValue::new(policy);
    let state_attr = {
        let target_id = target_id.clone();
        let snapshot = handle.snapshot();
        move || snapshot.with(|s| zone_state_attribute(s, &target_id))
    };
    let policy_json = policy.with_value(|p| serde_json::to_string(p).unwrap_or_default());

    let enter = {
        let target_id = target_id.clone();
        move |ev: web_sys::DragEvent| {
            let preferred = browser::preferred_operation(&ev);
            if handle.snapshot().with_untracked(|s| s.state == DndSessionState::Idle) {
                if let Some(dt) = ev.data_transfer() {
                    let provisional = browser::provisional_envelope(&browser::data_transfer_types(&dt));
                    handle.apply(&DndSessionInput::start(provisional));
                }
            }
            let next = handle.apply(&DndSessionInput::enter(policy.get_value(), preferred));
            if next.state == DndSessionState::OverTarget && next.target_id.as_deref() == Some(&target_id) {
                ev.prevent_default();
                if let (Some(dt), Some(op)) = (ev.data_transfer(), next.operation) {
                    dt.set_drop_effect(browser::drop_effect_for(op));
                }
            }
        }
    };
    let over = enter.clone();
    let leave = {
        let target_id = target_id.clone();
        move |_ev: web_sys::DragEvent| {
            handle.apply(&DndSessionInput::leave(target_id.clone()));
        }
    };
    let drop = {
        let target_id = target_id.clone();
        move |ev: web_sys::DragEvent| {
            ev.prevent_default();
            let preferred = browser::preferred_operation(&ev);
            // The payload is readable now: replace a provisional session with the real one.
            if let Some(dt) = ev.data_transfer() {
                if let Ok(envelope) = browser::read_envelope(&dt, ValidationOptions::default()) {
                    let current_is_real = handle.envelope().is_some_and(|e| e.drag_id == envelope.drag_id);
                    if !current_is_real {
                        handle.apply(&DndSessionInput::start(envelope));
                        handle.apply(&DndSessionInput::enter(policy.get_value(), preferred));
                    }
                }
            }
            let snapshot = handle.apply(&DndSessionInput::drop(target_id.clone()));
            if let (Some(cb), Some(result), Some(envelope)) = (on_drop, snapshot.result(), handle.envelope()) {
                cb.run((envelope, result));
            }
        }
    };

    view! {
        <div
            class=class
            data-ores-dnd-zone=target_id
            data-ores-dnd-policy=policy_json
            data-ores-dnd-state=state_attr
            on:dragenter=enter
            on:dragover=over
            on:dragleave=leave
            on:drop=drop
        >
            {children()}
        </div>
    }
}

/// A drag source. Writes the envelope into `DataTransfer` on `dragstart`,
/// starts the session, and ends it on `dragend`.
#[component]
pub fn DragSource(
    handle: DndSessionHandle,
    envelope: DndEnvelope,
    #[prop(optional, into)] class: Option<String>,
    children: Children,
) -> impl IntoView {
    let envelope = StoredValue::new(envelope);
    let source_json = envelope.with_value(|e| serde_json::to_string(e).unwrap_or_default());
    let start = move |ev: web_sys::DragEvent| {
        let envelope = envelope.get_value();
        if let Some(dt) = ev.data_transfer() {
            if browser::write_envelope(&dt, &envelope).is_err() {
                ev.prevent_default();
                return;
            }
        }
        handle.apply(&DndSessionInput::start(envelope));
    };
    let end = move |_ev: web_sys::DragEvent| {
        handle.apply(&DndSessionInput::end());
    };
    view! {
        <div class=class draggable="true" data-ores-dnd-source=source_json on:dragstart=start on:dragend=end>
            {children()}
        </div>
    }
}

/// Convenience: the operation a zone would perform for the running session.
pub fn negotiated_operation(handle: &DndSessionHandle) -> Option<DndOperation> {
    handle.snapshot().with_untracked(|s| s.operation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use leptos::reactive::owner::Owner;
    use ores_dnd_core::{decode_envelope_json, DndItemKind};

    const VALID: &str = include_str!("../../../contracts/instances/DndEnvelope/valid/text-copy.json");

    #[test]
    fn handle_drives_the_shared_state_machine_reactively() {
        let owner = Owner::new();
        owner.set();
        let handle = use_dnd_session();
        let envelope = decode_envelope_json(VALID, ValidationOptions::default()).unwrap();
        assert_eq!(handle.snapshot().get_untracked(), DndSessionSnapshot::IDLE);
        handle.apply(&DndSessionInput::start(envelope.clone()));
        assert_eq!(handle.envelope(), Some(envelope));
        let policy = DndDropPolicy::new("zone-a", &[DndOperation::Copy], &[DndItemKind::Text]);
        let over = handle.apply(&DndSessionInput::enter(policy, None));
        assert!(over.is_over_accepting_target());
        assert!(handle.is_over_accepting_target().get_untracked());
        assert_eq!(zone_state_attribute(&over, "zone-a"), "accepting");
        assert_eq!(zone_state_attribute(&over, "zone-b"), "dragging");
        let dropped = handle.apply(&DndSessionInput::drop("zone-a"));
        assert_eq!(zone_state_attribute(&dropped, "zone-a"), "dropped");
        assert!(dropped.result().unwrap().accepted);
        assert_eq!(negotiated_operation(&handle), Some(DndOperation::Copy));
    }

    #[test]
    fn rejecting_zone_state_is_visible_to_css() {
        let owner = Owner::new();
        owner.set();
        let handle = use_dnd_session();
        let envelope = decode_envelope_json(VALID, ValidationOptions::default()).unwrap();
        handle.apply(&DndSessionInput::start(envelope));
        let policy = DndDropPolicy::new("zone-json", &[DndOperation::Copy], &[DndItemKind::Json]);
        let snapshot = handle.apply(&DndSessionInput::enter(policy, None));
        assert_eq!(zone_state_attribute(&snapshot, "zone-json"), "rejecting");
        assert_eq!(zone_state_attribute(&DndSessionSnapshot::IDLE, "zone-json"), "idle");
    }
}
