import 'package:rxdart/rxdart.dart';

import 'ores_dnd.dart';

/// Process-local reactive event. [envelope] may contain dragged data, so this
/// event must not be persisted as replay history or forwarded to telemetry.
final class DndReactiveEvent {
  const DndReactiveEvent({
    required this.phase,
    required this.envelope,
    this.operation,
    this.targetId,
  });

  final DndLifecyclePhase phase;
  final DndEnvelope envelope;
  final DndOperation? operation;
  final String? targetId;
}

/// Replay-safe reactive state. It intentionally contains metadata only and
/// never includes DndItem.data or other dragged payload values.
final class DndReactiveState {
  const DndReactiveState({
    required this.active,
    required this.phase,
    required this.dragId,
    required this.sourceRuntime,
    required this.itemCount,
    required this.operation,
    required this.targetId,
  });

  static const idle = DndReactiveState(
    active: false,
    phase: null,
    dragId: null,
    sourceRuntime: null,
    itemCount: 0,
    operation: null,
    targetId: null,
  );

  final bool active;
  final DndLifecyclePhase? phase;
  final String? dragId;
  final String? sourceRuntime;
  final int itemCount;
  final DndOperation? operation;
  final String? targetId;

  Map<String, Object?> toJson() => {
        'active': active,
        'phase': phase?.wire,
        'dragId': dragId,
        'sourceRuntime': sourceRuntime,
        'itemCount': itemCount,
        'operation': operation?.wire,
        'targetId': targetId,
      };
}

const Set<DndLifecyclePhase> _startPhases = {
  DndLifecyclePhase.dragStart,
  DndLifecyclePhase.dragEnter,
};

const Set<DndLifecyclePhase> _losslessPhases = {
  DndLifecyclePhase.drop,
  DndLifecyclePhase.dragEnd,
};

/// `drag-over` may be sampled/coalesced for presentation work.
bool isHighFrequencyLifecyclePhase(DndLifecyclePhase phase) =>
    phase == DndLifecyclePhase.dragOver;

/// `drop` and `drag-end` must never be throttled away.
bool isLosslessLifecyclePhase(DndLifecyclePhase phase) =>
    _losslessPhases.contains(phase);

DndReactiveState reactiveStateFor(DndReactiveEvent event) => DndReactiveState(
      active: event.phase != DndLifecyclePhase.dragEnd,
      phase: event.phase,
      dragId: event.envelope.dragId,
      sourceRuntime: event.envelope.sourceRuntime,
      itemCount: event.envelope.items.length,
      operation: event.operation,
      targetId: event.targetId,
    );

void _assertReactiveEventSemantics(DndReactiveEvent event) {
  final operation = event.operation;
  if (operation != null && !event.envelope.allowedOperations.contains(operation)) {
    throw const FormatException('reactive event operation is not source-allowed');
  }
  final targetId = event.targetId;
  if (targetId != null && targetId.isEmpty) {
    throw const FormatException('reactive event targetId must be a non-empty string');
  }
  if (event.phase == DndLifecyclePhase.drop) {
    if (operation == null) {
      throw const FormatException('drop event requires a negotiated operation');
    }
    if (targetId == null) {
      throw const FormatException('drop event requires a targetId');
    }
  }
}

bool _canTransition(DndLifecyclePhase? previous, DndLifecyclePhase next) =>
    switch (previous) {
      null => _startPhases.contains(next),
      DndLifecyclePhase.dragStart =>
        next == DndLifecyclePhase.dragEnter ||
            next == DndLifecyclePhase.dragOver ||
            next == DndLifecyclePhase.drop ||
            next == DndLifecyclePhase.dragEnd,
      DndLifecyclePhase.dragEnter =>
        next == DndLifecyclePhase.dragOver ||
            next == DndLifecyclePhase.dragLeave ||
            next == DndLifecyclePhase.drop ||
            next == DndLifecyclePhase.dragEnd,
      DndLifecyclePhase.dragOver =>
        next == DndLifecyclePhase.dragOver ||
            next == DndLifecyclePhase.dragLeave ||
            next == DndLifecyclePhase.drop ||
            next == DndLifecyclePhase.dragEnd,
      DndLifecyclePhase.dragLeave =>
        next == DndLifecyclePhase.dragEnter ||
            next == DndLifecyclePhase.dragOver ||
            next == DndLifecyclePhase.dragEnd,
      DndLifecyclePhase.drop => next == DndLifecyclePhase.dragEnd,
      DndLifecyclePhase.dragEnd => _startPhases.contains(next),
    };

/// Fail-closed lifecycle tracker shared by Dart and Flutter consumers.
///
/// A session may begin with `drag-start` for an in-app drag or `drag-enter` for
/// an external/system drag. An active session may not silently switch drag IDs.
final class DndLifecycleTracker {
  String? _dragId;
  DndLifecyclePhase? _phase;

  String? get dragId => _dragId;
  DndLifecyclePhase? get phase => _phase;

  DndReactiveState accept(DndReactiveEvent event) {
    _assertReactiveEventSemantics(event);

    final currentDragId = _dragId;
    if (currentDragId != null && event.envelope.dragId != currentDragId) {
      throw FormatException(
        'reactive dragId switched before drag-end: '
        '$currentDragId -> ${event.envelope.dragId}',
      );
    }
    if (!_canTransition(_phase, event.phase)) {
      throw FormatException(
        'invalid reactive lifecycle transition: '
        '${_phase?.wire ?? 'idle'} -> ${event.phase.wire}',
      );
    }

    final state = reactiveStateFor(event);
    if (event.phase == DndLifecyclePhase.dragEnd) {
      reset();
    } else {
      _dragId = event.envelope.dragId;
      _phase = event.phase;
    }
    return state;
  }

  void reset() {
    _dragId = null;
    _phase = null;
  }
}

/// RxDart-backed hot event bus for drag/drop lifecycles.
///
/// Raw events use [PublishSubject], so dragged payloads are never replayed to
/// late subscribers. Only the metadata-only [state] stream uses a
/// [BehaviorSubject]. Persistence/forms remain explicit `commitAcceptedDrop`
/// effects and are never triggered by [emit].
final class OresDndReactiveBus {
  OresDndReactiveBus()
      : _events = PublishSubject<DndReactiveEvent>(),
        _state = BehaviorSubject<DndReactiveState>.seeded(DndReactiveState.idle);

  final PublishSubject<DndReactiveEvent> _events;
  final BehaviorSubject<DndReactiveState> _state;
  final DndLifecycleTracker _tracker = DndLifecycleTracker();

  Stream<DndReactiveEvent> get events => _events.stream;
  ValueStream<DndReactiveState> get state => _state.stream;
  Stream<bool> get active => state.map((value) => value.active).distinct();
  Stream<DndReactiveEvent> get drops =>
      events.where((event) => event.phase == DndLifecyclePhase.drop);
  Stream<DndReactiveEvent> get dragOvers =>
      events.where((event) => isHighFrequencyLifecyclePhase(event.phase));
  Stream<DndReactiveEvent> get lossless =>
      events.where((event) => isLosslessLifecyclePhase(event.phase));
  Stream<DndTelemetryEvent> get telemetry => events.map(
        (event) => telemetryFor(
          event.phase,
          event.envelope,
          operation: event.operation,
          targetId: event.targetId,
        ),
      );

  bool get isClosed => _events.isClosed && _state.isClosed;

  void emit(
    DndLifecyclePhase phase,
    DndEnvelope envelope, {
    DndOperation? operation,
    String? targetId,
  }) {
    if (isClosed) throw StateError('ores-dnd reactive bus is closed');
    final safeEnvelope = DndEnvelope.fromJson(envelope.toJson());
    final event = DndReactiveEvent(
      phase: phase,
      envelope: safeEnvelope,
      operation: operation,
      targetId: targetId,
    );
    final state = _tracker.accept(event);
    _events.add(event);
    _state.add(state);
  }

  Future<void> dispose() async {
    _tracker.reset();
    await _events.close();
    await _state.close();
  }
}
