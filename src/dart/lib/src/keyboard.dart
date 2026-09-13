part of '../ores_dnd.dart';

/// Minimal driver surface for adapters that feed the canonical session.
///
/// Pure Dart can wrap [DndSession] with [DndSessionDriverAdapter]. UI wrappers
/// may implement this interface while adding notifications around [apply], so
/// keyboard input still drives the exact same state machine as pointer/native
/// drag events.
abstract interface class DndSessionDriver {
  DndSessionSnapshot get snapshot;
  DndEnvelope? get envelope;
  DndDropResult? get result;
  DndSessionSnapshot apply(DndSessionInput input);
}

/// Zero-semantics adapter that exposes an existing [DndSession] as a driver.
final class DndSessionDriverAdapter implements DndSessionDriver {
  DndSessionDriverAdapter(this.session);

  final DndSession session;

  @override
  DndSessionSnapshot get snapshot => session.snapshot;

  @override
  DndEnvelope? get envelope => session.envelope;

  @override
  DndDropResult? get result => session.result;

  @override
  DndSessionSnapshot apply(DndSessionInput input) => session.apply(input);
}

enum DndKeyboardAnnouncementKind {
  started,
  targetAccepted,
  targetRejected,
  targetRequired,
  dropped,
  cancelled,
}

/// Content-free status that a UI runtime may map to an accessibility live
/// region. Dragged item data is intentionally absent.
final class DndKeyboardAnnouncement {
  const DndKeyboardAnnouncement({
    required this.kind,
    this.dragId,
    this.targetId,
    this.operation,
    this.errorCode,
  });

  final DndKeyboardAnnouncementKind kind;
  final String? dragId;
  final String? targetId;
  final DndOperation? operation;
  final DndRejectCode? errorCode;
}

typedef DndKeyboardAnnounce = void Function(
  DndKeyboardAnnouncement announcement,
);
typedef DndKeyboardTargetChanged = void Function(
  String targetId,
  DndSessionSnapshot snapshot,
);
typedef DndKeyboardDropAccepted = Future<void> Function(
  DndEnvelope envelope,
  DndDropResult result,
);
typedef DndKeyboardDropRejected = void Function(
  DndEnvelope? envelope,
  DndDropResult result,
);

/// Framework-neutral keyboard navigation over the canonical [DndSession].
///
/// The target list order is host-defined. Moving wraps at either end; the host
/// owns visual focus through [onTargetChange]. No new wire or lifecycle state is
/// introduced here.
final class DndKeyboardController {
  DndKeyboardController({
    required this.driver,
    required List<DndDropPolicy> targets,
    this.otel,
    this.announce,
    this.onTargetChange,
    this.onDrop,
    this.onReject,
  }) : _targets = List.unmodifiable(targets) {
    final ids = <String>{};
    for (final target in _targets) {
      if (!target.isValid) {
        throw FormatException('invalid keyboard DnD policy: ${target.targetId}');
      }
      if (!ids.add(target.targetId)) {
        throw FormatException(
          'duplicate keyboard DnD target: ${target.targetId}',
        );
      }
    }
  }

  factory DndKeyboardController.forSession({
    required DndSession session,
    required List<DndDropPolicy> targets,
    OresOtelPort? otel,
    DndKeyboardAnnounce? announce,
    DndKeyboardTargetChanged? onTargetChange,
    DndKeyboardDropAccepted? onDrop,
    DndKeyboardDropRejected? onReject,
  }) => DndKeyboardController(
        driver: DndSessionDriverAdapter(session),
        targets: targets,
        otel: otel,
        announce: announce,
        onTargetChange: onTargetChange,
        onDrop: onDrop,
        onReject: onReject,
      );

  final DndSessionDriver driver;
  final List<DndDropPolicy> _targets;
  final OresOtelPort? otel;
  final DndKeyboardAnnounce? announce;
  final DndKeyboardTargetChanged? onTargetChange;
  final DndKeyboardDropAccepted? onDrop;
  final DndKeyboardDropRejected? onReject;

  int _targetIndex = -1;

  int get targetIndex => _targetIndex;
  DndDropPolicy? get activeTarget =>
      _targetIndex < 0 ? null : _targets[_targetIndex];

  DndSessionSnapshot start(DndEnvelope envelope) {
    _targetIndex = -1;
    final next = driver.apply(DndSessionInput.start(envelope));
    if (next.state == DndSessionState.dragging && driver.envelope != null) {
      _emit(DndLifecyclePhase.dragStart);
      announce?.call(
        DndKeyboardAnnouncement(
          kind: DndKeyboardAnnouncementKind.started,
          dragId: next.dragId,
        ),
      );
    }
    return next;
  }

  DndSessionSnapshot move(int step, {DndOperation? preferred}) {
    if (step != 1 && step != -1) {
      throw ArgumentError.value(step, 'step', 'must be +1 or -1');
    }
    final current = driver.snapshot;
    if (current.state != DndSessionState.dragging &&
        current.state != DndSessionState.overTarget) {
      return current;
    }
    if (_targets.isEmpty) {
      announce?.call(
        DndKeyboardAnnouncement(
          kind: DndKeyboardAnnouncementKind.targetRequired,
          dragId: current.dragId,
        ),
      );
      return current;
    }
    final previous = activeTarget;
    if (previous != null && current.targetId == previous.targetId) {
      driver.apply(DndSessionInput.leave(previous.targetId));
    }
    if (_targetIndex < 0) {
      _targetIndex = step > 0 ? 0 : _targets.length - 1;
    } else {
      _targetIndex = (_targetIndex + step + _targets.length) % _targets.length;
    }
    return _enterActive(preferred: preferred);
  }

  DndSessionSnapshot select(
    String targetId, {
    DndOperation? preferred,
  }) {
    final current = driver.snapshot;
    if (current.state != DndSessionState.dragging &&
        current.state != DndSessionState.overTarget) {
      return current;
    }
    final index = _targets.indexWhere((target) => target.targetId == targetId);
    if (index < 0) return current;
    final previous = activeTarget;
    if (previous != null &&
        current.targetId == previous.targetId &&
        previous.targetId != targetId) {
      driver.apply(DndSessionInput.leave(previous.targetId));
    }
    _targetIndex = index;
    return _enterActive(preferred: preferred);
  }

  DndDropResult? drop() {
    final target = activeTarget;
    final envelopeBefore = driver.envelope;
    if (target == null) {
      announce?.call(
        DndKeyboardAnnouncement(
          kind: DndKeyboardAnnouncementKind.targetRequired,
          dragId: driver.snapshot.dragId,
        ),
      );
      return null;
    }
    final next = driver.apply(DndSessionInput.drop(target.targetId));
    final result = next.result;
    if (result == null) return null;
    if (next.state == DndSessionState.dropped && envelopeBefore != null) {
      _emit(
        DndLifecyclePhase.drop,
        operation: result.operation,
        targetId: target.targetId,
        envelope: envelopeBefore,
      );
      announce?.call(
        DndKeyboardAnnouncement(
          kind: DndKeyboardAnnouncementKind.dropped,
          dragId: result.dragId,
          targetId: target.targetId,
          operation: result.operation,
        ),
      );
      final accepted = onDrop;
      if (accepted != null) unawaited(accepted(envelopeBefore, result));
    } else {
      announce?.call(
        DndKeyboardAnnouncement(
          kind: DndKeyboardAnnouncementKind.targetRejected,
          dragId: result.dragId,
          targetId: target.targetId,
          errorCode: result.errorCode == null
              ? null
              : DndRejectCode.parse(result.errorCode),
        ),
      );
      onReject?.call(envelopeBefore, result);
    }
    return result;
  }

  DndDropResult? cancel() {
    final before = driver.snapshot;
    if (before.state == DndSessionState.idle || before.state.isTerminal) {
      return driver.result;
    }
    final envelopeBefore = driver.envelope;
    final next = driver.apply(const DndSessionInput.cancel());
    final result = next.result;
    if (result != null) {
      _emit(DndLifecyclePhase.dragEnd, envelope: envelopeBefore);
      announce?.call(
        DndKeyboardAnnouncement(
          kind: DndKeyboardAnnouncementKind.cancelled,
          dragId: result.dragId,
          errorCode: DndRejectCode.cancelled,
        ),
      );
      onReject?.call(envelopeBefore, result);
    }
    return result;
  }

  DndSessionSnapshot _enterActive({DndOperation? preferred}) {
    final target = activeTarget;
    if (target == null) return driver.snapshot;
    final next = driver.apply(
      DndSessionInput.enter(target, preferred: preferred),
    );
    onTargetChange?.call(target.targetId, next);
    if (next.state == DndSessionState.overTarget) {
      _emit(
        DndLifecyclePhase.dragEnter,
        operation: next.operation,
        targetId: target.targetId,
      );
      announce?.call(
        DndKeyboardAnnouncement(
          kind: DndKeyboardAnnouncementKind.targetAccepted,
          dragId: next.dragId,
          targetId: target.targetId,
          operation: next.operation,
        ),
      );
    } else {
      announce?.call(
        DndKeyboardAnnouncement(
          kind: DndKeyboardAnnouncementKind.targetRejected,
          dragId: next.dragId,
          targetId: target.targetId,
          errorCode: next.errorCode,
        ),
      );
    }
    return next;
  }

  void _emit(
    DndLifecyclePhase phase, {
    DndOperation? operation,
    String? targetId,
    DndEnvelope? envelope,
  }) {
    final port = otel;
    final current = envelope ?? driver.envelope;
    if (port == null || current == null) return;
    unawaited(
      port.emitDndEvent(
        telemetryFor(
          phase,
          current,
          operation: operation,
          targetId: targetId,
        ),
      ),
    );
  }
}
