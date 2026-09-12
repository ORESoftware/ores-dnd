import 'package:rxdart/rxdart.dart';

import 'ores_dnd.dart';

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

DndReactiveState reactiveStateFor(DndReactiveEvent event, {bool? active}) =>
    DndReactiveState(
      active: active ?? event.phase != DndLifecyclePhase.dragEnd,
      phase: event.phase,
      dragId: event.envelope.dragId,
      sourceRuntime: event.envelope.sourceRuntime,
      itemCount: event.envelope.items.length,
      operation: event.operation,
      targetId: event.targetId,
    );

enum DndLifecycleMode { strict, externalDropCompatible }

final class DndLifecycleGuard {
  DndLifecycleGuard({this.mode = DndLifecycleMode.strict});

  final DndLifecycleMode mode;
  String? _activeDragId;
  bool _dropped = false;

  bool get active => _activeDragId != null;
  String? get activeDragId => _activeDragId;

  void accept(DndLifecyclePhase phase, String dragId) {
    if (_activeDragId == null) {
      if (phase == DndLifecyclePhase.dragStart) {
        _activeDragId = dragId;
        _dropped = false;
        return;
      }
      if (phase == DndLifecyclePhase.drop &&
          mode == DndLifecycleMode.externalDropCompatible) {
        return;
      }
      throw FormatException('${phase.wire} requires an active drag-start');
    }

    if (dragId != _activeDragId) {
      throw const FormatException(
        'reactive lifecycle dragId changed before drag-end',
      );
    }
    if (_dropped) {
      if (phase != DndLifecyclePhase.dragEnd) {
        throw FormatException(
          '${phase.wire} is invalid after drop; expected drag-end',
        );
      }
      _activeDragId = null;
      _dropped = false;
      return;
    }

    switch (phase) {
      case DndLifecyclePhase.dragStart:
        throw const FormatException('duplicate drag-start before drag-end');
      case DndLifecyclePhase.dragEnter:
      case DndLifecyclePhase.dragOver:
      case DndLifecyclePhase.dragLeave:
        return;
      case DndLifecyclePhase.drop:
        _dropped = true;
        return;
      case DndLifecyclePhase.dragEnd:
        _activeDragId = null;
        _dropped = false;
        return;
    }
  }
}

final class OresDndReactiveBus {
  OresDndReactiveBus({DndLifecycleMode lifecycleMode = DndLifecycleMode.strict})
    : _events = PublishSubject<DndReactiveEvent>(),
      _state = BehaviorSubject<DndReactiveState>.seeded(DndReactiveState.idle),
      _guard = DndLifecycleGuard(mode: lifecycleMode);

  final PublishSubject<DndReactiveEvent> _events;
  final BehaviorSubject<DndReactiveState> _state;
  final DndLifecycleGuard _guard;

  Stream<DndReactiveEvent> get events => _events.stream;
  ValueStream<DndReactiveState> get state => _state.stream;
  Stream<bool> get active => state.map((value) => value.active).distinct();
  Stream<DndReactiveEvent> get drops =>
      events.where((event) => event.phase == DndLifecyclePhase.drop);
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
    final safeEnvelope = DndEnvelope.fromJson(envelope.toJson());
    if (operation != null &&
        !safeEnvelope.allowedOperations.contains(operation)) {
      throw const FormatException(
        'reactive event operation is not source-allowed',
      );
    }
    if (targetId != null && targetId.isEmpty) {
      throw const FormatException(
        'reactive event targetId must be a non-empty string',
      );
    }
    _guard.accept(phase, safeEnvelope.dragId);

    final event = DndReactiveEvent(
      phase: phase,
      envelope: safeEnvelope,
      operation: operation,
      targetId: targetId,
    );
    _events.add(event);
    _state.add(reactiveStateFor(event, active: _guard.active));
  }

  Future<void> dispose() async {
    await _events.close();
    await _state.close();
  }
}
