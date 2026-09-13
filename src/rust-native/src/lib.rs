//! Framework-neutral native desktop bridge for `ores.dnd/v1`.
//!
//! Native UI toolkits disagree about event names and payload APIs, but they do
//! not need a second drag state machine. This crate only translates host events
//! into [`ores_dnd_core::DndSessionInput`] transitions.
//!
//! In particular, an external/system drag **never** enters a target from idle:
//! [`NativeDndBridge::external_enter`] always synthesizes canonical `start`
//! before `enter`. A definitive drop may replace a provisional envelope and is
//! re-evaluated through the same target policy before `drop`.
//!
//! File promises, temp-file ownership, path access, sandbox bookmarks and OS
//! clipboard APIs remain host responsibilities. Do not serialize local paths or
//! credentials into an envelope merely to use this bridge; use bounded bytes or
//! an application-owned opaque/versioned handoff manifest where appropriate.

use ores_dnd_core::{
    DndDropPolicy, DndDropResult, DndEnvelope, DndOperation, DndSession, DndSessionInput,
    DndSessionSnapshot, DndSessionState, ValidationOptions,
};

#[derive(Debug, Clone, Default)]
pub struct NativeDndBridge {
    session: DndSession,
}

impl NativeDndBridge {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: ValidationOptions) -> Self {
        Self {
            session: DndSession::with_options(options),
        }
    }

    pub fn session(&self) -> &DndSession {
        &self.session
    }

    pub fn snapshot(&self) -> &DndSessionSnapshot {
        self.session.snapshot()
    }

    pub fn envelope(&self) -> Option<&DndEnvelope> {
        self.session.envelope()
    }

    /// Begin an in-process/native drag whose definitive envelope is already
    /// available to the host.
    pub fn start(&mut self, envelope: DndEnvelope) -> DndSessionSnapshot {
        self.session
            .apply(&DndSessionInput::start(envelope))
            .clone()
    }

    /// Apply a target enter to an already-started session.
    ///
    /// Calling this from idle intentionally does nothing; external/system hosts
    /// must use [`Self::external_enter`] so the canonical session opener is not
    /// bypassed.
    pub fn enter(
        &mut self,
        policy: DndDropPolicy,
        preferred: Option<DndOperation>,
    ) -> DndSessionSnapshot {
        self.session
            .apply(&DndSessionInput::enter(policy, preferred))
            .clone()
    }

    /// Normalize an OS-level external drag enter. `provisional` may contain
    /// content-free item descriptors (kind/media type/name) when the toolkit
    /// withholds real data until drop.
    pub fn external_enter(
        &mut self,
        provisional: DndEnvelope,
        policy: DndDropPolicy,
        preferred: Option<DndOperation>,
    ) -> DndSessionSnapshot {
        let started = self.start(provisional);
        if started.state != DndSessionState::Dragging {
            return started;
        }
        self.enter(policy, preferred)
    }

    pub fn leave(&mut self, target_id: impl Into<String>) -> DndSessionSnapshot {
        self.session
            .apply(&DndSessionInput::leave(target_id))
            .clone()
    }

    /// Drop the currently admitted envelope on `target_id`.
    pub fn drop_current(&mut self, target_id: impl Into<String>) -> Option<DndDropResult> {
        let target_id = target_id.into();
        let snapshot = self
            .session
            .apply(&DndSessionInput::drop(target_id))
            .clone();
        snapshot.result()
    }

    /// Replace a protected/provisional external envelope with the real drop
    /// payload, re-enter the same policy and then drop. This is deliberately a
    /// fresh `start` so provisional acceptance can never waive definitive byte,
    /// item, form, media-type or operation checks.
    pub fn external_drop(
        &mut self,
        definitive: DndEnvelope,
        policy: DndDropPolicy,
        preferred: Option<DndOperation>,
    ) -> Option<DndDropResult> {
        let target_id = policy.target_id.clone();
        let started = self.start(definitive);
        if started.state != DndSessionState::Dragging {
            return started.result();
        }
        self.enter(policy, preferred);
        self.drop_current(target_id)
    }

    pub fn cancel(&mut self) -> Option<DndDropResult> {
        let snapshot = self.session.apply(&DndSessionInput::cancel()).clone();
        snapshot.result()
    }

    pub fn end(&mut self) -> Option<DndDropResult> {
        let snapshot = self.session.apply(&DndSessionInput::end()).clone();
        snapshot.result()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ores_dnd_core::{DndItem, DndItemKind, DndRejectCode, ORES_DND_PROTOCOL};

    fn envelope(drag_id: &str, data: &str) -> DndEnvelope {
        DndEnvelope {
            protocol: ORES_DND_PROTOCOL.to_owned(),
            drag_id: drag_id.to_owned(),
            source_runtime: "native-test".to_owned(),
            allowed_operations: vec![DndOperation::Copy, DndOperation::Move],
            items: vec![DndItem {
                kind: DndItemKind::Text,
                media_type: "text/plain".to_owned(),
                data: data.to_owned(),
                name: None,
            }],
            traceparent: None,
            form_id: None,
        }
    }

    fn text_policy() -> DndDropPolicy {
        DndDropPolicy::new(
            "native-zone",
            &[DndOperation::Copy, DndOperation::Move],
            &[DndItemKind::Text],
        )
    }

    #[test]
    fn direct_enter_from_idle_is_ignored() {
        let mut bridge = NativeDndBridge::new();
        let snapshot = bridge.enter(text_policy(), None);
        assert_eq!(snapshot.state, DndSessionState::Idle);
        assert!(bridge.envelope().is_none());
    }

    #[test]
    fn external_enter_synthesizes_start_before_policy_evaluation() {
        let mut bridge = NativeDndBridge::new();
        let snapshot = bridge.external_enter(envelope("external-1", ""), text_policy(), None);
        assert_eq!(snapshot.state, DndSessionState::OverTarget);
        assert_eq!(snapshot.drag_id.as_deref(), Some("external-1"));
        assert_eq!(snapshot.target_id.as_deref(), Some("native-zone"));
        assert_eq!(snapshot.operation, Some(DndOperation::Move));
    }

    #[test]
    fn definitive_external_drop_rechecks_real_payload_limits() {
        let policy = text_policy().with_max_total_bytes(2);
        let mut bridge = NativeDndBridge::new();
        let provisional = bridge.external_enter(envelope("external-2", ""), policy.clone(), None);
        assert_eq!(provisional.state, DndSessionState::OverTarget);

        let result = bridge
            .external_drop(envelope("external-2", "toolarge"), policy, None)
            .expect("terminal definitive result");
        assert!(!result.accepted);
        assert_eq!(result.error_code.as_deref(), Some(DndRejectCode::PayloadTooLarge.wire()));
        assert_eq!(bridge.snapshot().state, DndSessionState::Cancelled);
    }

    #[test]
    fn accepted_external_drop_uses_definitive_envelope_and_preference() {
        let mut bridge = NativeDndBridge::new();
        bridge.external_enter(envelope("external-3", ""), text_policy(), None);
        let result = bridge
            .external_drop(
                envelope("external-3", "hello"),
                text_policy(),
                Some(DndOperation::Copy),
            )
            .expect("drop result");
        assert!(result.accepted);
        assert_eq!(result.operation, Some(DndOperation::Copy));
        assert_eq!(bridge.envelope().expect("definitive envelope").items[0].data, "hello");
    }

    #[test]
    fn leave_then_drop_cannot_reuse_old_acceptance() {
        let mut bridge = NativeDndBridge::new();
        bridge.external_enter(envelope("external-4", ""), text_policy(), None);
        let snapshot = bridge.leave("native-zone");
        assert_eq!(snapshot.state, DndSessionState::Dragging);
        let result = bridge.drop_current("native-zone").expect("cancelled result");
        assert!(!result.accepted);
        assert_eq!(result.error_code.as_deref(), Some(DndRejectCode::NoActiveTarget.wire()));
    }

    #[test]
    fn a_new_external_drag_can_restart_a_terminal_session() {
        let mut bridge = NativeDndBridge::new();
        bridge.external_enter(envelope("external-5", ""), text_policy(), None);
        bridge.cancel();
        assert_eq!(bridge.snapshot().state, DndSessionState::Cancelled);
        let next = bridge.external_enter(envelope("external-6", ""), text_policy(), None);
        assert_eq!(next.state, DndSessionState::OverTarget);
        assert_eq!(next.drag_id.as_deref(), Some("external-6"));
    }
}
