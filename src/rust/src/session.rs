//! The drag session state machine — a pure `apply(snapshot, input) → snapshot`
//! specified by docs/DESIGN.md §Session and proven by the shared trace corpus
//! under `contracts/instances/DndSessionTrace/valid/`.

use crate::envelope::{DndDropResult, DndEnvelope, DndError, DndOperation, ValidationOptions};
use crate::policy::{evaluate_policy, DndDropPolicy, DndRejectCode};
use crate::wire;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DndSessionState {
    Idle,
    Dragging,
    OverTarget,
    Dropped,
    Cancelled,
}

impl DndSessionState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, DndSessionState::Dropped | DndSessionState::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DndSessionInputKind {
    Start,
    Enter,
    Leave,
    Drop,
    Cancel,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DndSessionInput {
    pub kind: DndSessionInputKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope: Option<DndEnvelope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<DndDropPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_operation: Option<DndOperation>,
}

impl DndSessionInput {
    fn bare(kind: DndSessionInputKind) -> Self {
        Self {
            kind,
            envelope: None,
            target_id: None,
            policy: None,
            preferred_operation: None,
        }
    }

    pub fn start(envelope: DndEnvelope) -> Self {
        Self {
            envelope: Some(envelope),
            ..Self::bare(DndSessionInputKind::Start)
        }
    }

    pub fn enter(policy: DndDropPolicy, preferred: Option<DndOperation>) -> Self {
        Self {
            target_id: Some(policy.target_id.clone()),
            policy: Some(policy),
            preferred_operation: preferred,
            ..Self::bare(DndSessionInputKind::Enter)
        }
    }

    pub fn leave(target_id: impl Into<String>) -> Self {
        Self {
            target_id: Some(target_id.into()),
            ..Self::bare(DndSessionInputKind::Leave)
        }
    }

    pub fn drop(target_id: impl Into<String>) -> Self {
        Self {
            target_id: Some(target_id.into()),
            ..Self::bare(DndSessionInputKind::Drop)
        }
    }

    pub fn cancel() -> Self {
        Self::bare(DndSessionInputKind::Cancel)
    }

    pub fn end() -> Self {
        Self::bare(DndSessionInputKind::End)
    }

    /// The structural rules both schema authorities check for an input. An
    /// embedded envelope is checked structurally only (its protocol version
    /// is the session's decision at `start`).
    pub fn structural(&self) -> Result<(), DndError> {
        if let Some(envelope) = &self.envelope {
            envelope.structural()?;
        }
        wire::check_opt_safe_id(self.target_id.as_deref(), "targetId")?;
        if let Some(policy) = &self.policy {
            policy.structural()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DndSessionSnapshot {
    pub state: DndSessionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drag_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<DndOperation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<DndRejectCode>,
}

impl DndSessionSnapshot {
    pub const IDLE: DndSessionSnapshot = DndSessionSnapshot {
        state: DndSessionState::Idle,
        drag_id: None,
        target_id: None,
        operation: None,
        error_code: None,
    };

    fn dragging(drag_id: &str) -> Self {
        Self {
            state: DndSessionState::Dragging,
            drag_id: Some(drag_id.to_owned()),
            ..Self::IDLE
        }
    }

    /// True while the pointer is over a target that accepts the payload.
    pub fn is_over_accepting_target(&self) -> bool {
        self.state == DndSessionState::OverTarget
    }

    /// The final outcome once the session is terminal.
    pub fn result(&self) -> Option<DndDropResult> {
        let drag_id = self.drag_id.clone()?;
        match self.state {
            DndSessionState::Dropped => Some(DndDropResult {
                drag_id,
                accepted: true,
                operation: self.operation,
                target_id: self.target_id.clone(),
                error_code: None,
            }),
            DndSessionState::Cancelled => Some(DndDropResult {
                drag_id,
                accepted: false,
                operation: None,
                target_id: self.target_id.clone(),
                error_code: self.error_code.map(|code| code.wire().to_owned()),
            }),
            _ => None,
        }
    }
}

impl DndSessionSnapshot {
    pub fn structural(&self) -> Result<(), DndError> {
        wire::check_opt_safe_id(self.drag_id.as_deref(), "dragId")?;
        wire::check_opt_safe_id(self.target_id.as_deref(), "targetId")
    }
}

impl Default for DndSessionSnapshot {
    fn default() -> Self {
        Self::IDLE
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DndSessionTrace {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub inputs: Vec<DndSessionInput>,
    pub expected: Vec<DndSessionSnapshot>,
}

impl DndSessionTrace {
    pub fn structural(&self) -> Result<(), DndError> {
        if !wire::is_trace_id(&self.id) {
            return Err(DndError(
                "trace id must match ^[a-z0-9][a-z0-9._-]{0,127}$".into(),
            ));
        }
        if self
            .description
            .as_deref()
            .is_some_and(|d| d.chars().count() > wire::TRACE_DESCRIPTION_MAX)
        {
            return Err(DndError("description exceeds 512 characters".into()));
        }
        wire::check_len(self.inputs.len(), 1, wire::TRACE_STEPS_MAX, "inputs")?;
        wire::check_len(self.expected.len(), 1, wire::TRACE_STEPS_MAX, "expected")?;
        if self.inputs.len() != self.expected.len() {
            return Err(DndError(
                "trace inputs and expected must have the same length".into(),
            ));
        }
        for input in &self.inputs {
            input.structural()?;
        }
        for snapshot in &self.expected {
            snapshot.structural()?;
        }
        Ok(())
    }
}

/// One drag session. Hosts keep one per drag source (or one global one) and
/// feed it native events translated to [`DndSessionInput`]s.
#[derive(Debug, Clone, Default)]
pub struct DndSession {
    snapshot: DndSessionSnapshot,
    envelope: Option<DndEnvelope>,
    options: ValidationOptions,
}

impl DndSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: ValidationOptions) -> Self {
        Self {
            options,
            ..Self::default()
        }
    }

    pub fn snapshot(&self) -> &DndSessionSnapshot {
        &self.snapshot
    }

    /// The envelope of the running (or just finished) session.
    pub fn envelope(&self) -> Option<&DndEnvelope> {
        self.envelope.as_ref()
    }

    /// Apply one input and return the new snapshot. Malformed inputs (an
    /// `enter` without a policy, a `leave` without a target) are ignored.
    pub fn apply(&mut self, input: &DndSessionInput) -> &DndSessionSnapshot {
        let next = self.next(input);
        self.snapshot = next;
        &self.snapshot
    }

    fn next(&mut self, input: &DndSessionInput) -> DndSessionSnapshot {
        let current = &self.snapshot;
        match input.kind {
            DndSessionInputKind::Start => {
                let valid = input
                    .envelope
                    .as_ref()
                    .filter(|envelope| envelope.validate(self.options).is_ok())
                    .cloned();
                match valid {
                    Some(envelope) => {
                        let snapshot = DndSessionSnapshot::dragging(&envelope.drag_id);
                        self.envelope = Some(envelope);
                        snapshot
                    }
                    None => {
                        self.envelope = None;
                        DndSessionSnapshot {
                            error_code: Some(DndRejectCode::InvalidEnvelope),
                            ..DndSessionSnapshot::IDLE
                        }
                    }
                }
            }
            _ if current.state == DndSessionState::Idle
                || current.state.is_terminal()
                || input.structural().is_err() =>
            {
                current.clone()
            }
            DndSessionInputKind::Enter => {
                let (Some(policy), Some(envelope)) =
                    (input.policy.as_ref(), self.envelope.as_ref())
                else {
                    return current.clone();
                };
                if policy.validate().is_err() {
                    return current.clone();
                }
                let target_id = input
                    .target_id
                    .clone()
                    .unwrap_or_else(|| policy.target_id.clone());
                let drag_id = current.drag_id.clone();
                match evaluate_policy(envelope, policy, input.preferred_operation) {
                    Ok(operation) => DndSessionSnapshot {
                        state: DndSessionState::OverTarget,
                        drag_id,
                        target_id: Some(target_id),
                        operation: Some(operation),
                        error_code: None,
                    },
                    Err(code) => DndSessionSnapshot {
                        state: DndSessionState::Dragging,
                        drag_id,
                        target_id: Some(target_id),
                        operation: None,
                        error_code: Some(code),
                    },
                }
            }
            DndSessionInputKind::Leave => match (&input.target_id, &current.target_id) {
                (Some(left), Some(active)) if left == active => DndSessionSnapshot {
                    state: DndSessionState::Dragging,
                    drag_id: current.drag_id.clone(),
                    ..DndSessionSnapshot::IDLE
                },
                _ => current.clone(),
            },
            DndSessionInputKind::Drop => {
                let dropped_on = input.target_id.clone();
                let cancelled = |code: DndRejectCode| DndSessionSnapshot {
                    state: DndSessionState::Cancelled,
                    drag_id: current.drag_id.clone(),
                    target_id: dropped_on.clone(),
                    operation: None,
                    error_code: Some(code),
                };
                match (&current.target_id, &dropped_on) {
                    (Some(active), Some(target)) if active == target => {
                        if current.state == DndSessionState::OverTarget {
                            DndSessionSnapshot {
                                state: DndSessionState::Dropped,
                                ..current.clone()
                            }
                        } else {
                            cancelled(current.error_code.unwrap_or(DndRejectCode::NoActiveTarget))
                        }
                    }
                    (Some(_), _) => cancelled(DndRejectCode::TargetMismatch),
                    (None, _) => cancelled(DndRejectCode::NoActiveTarget),
                }
            }
            DndSessionInputKind::Cancel | DndSessionInputKind::End => DndSessionSnapshot {
                state: DndSessionState::Cancelled,
                drag_id: current.drag_id.clone(),
                error_code: Some(DndRejectCode::Cancelled),
                ..DndSessionSnapshot::IDLE
            },
        }
    }

    /// Replay a trace from the idle state; returns the first divergence.
    pub fn replay(trace: &DndSessionTrace) -> Result<(), Box<TraceDivergence>> {
        if trace.inputs.len() != trace.expected.len() {
            return Err(Box::new(TraceDivergence {
                trace_id: trace.id.clone(),
                step: trace.inputs.len().min(trace.expected.len()),
                expected: None,
                actual: None,
            }));
        }
        let mut session = DndSession::new();
        for (step, (input, expected)) in trace.inputs.iter().zip(&trace.expected).enumerate() {
            let actual = session.apply(input).clone();
            if &actual != expected {
                return Err(Box::new(TraceDivergence {
                    trace_id: trace.id.clone(),
                    step,
                    expected: Some(expected.clone()),
                    actual: Some(actual),
                }));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceDivergence {
    pub trace_id: String,
    pub step: usize,
    pub expected: Option<DndSessionSnapshot>,
    pub actual: Option<DndSessionSnapshot>,
}

impl std::fmt::Display for TraceDivergence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "trace {} diverged at step {}: expected {:?}, got {:?}",
            self.trace_id, self.step, self.expected, self.actual
        )
    }
}

impl std::error::Error for TraceDivergence {}
