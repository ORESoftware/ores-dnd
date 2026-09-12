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

DndReactiveState reactiveStateFor(DndReactiveEvent event) => DndReactiveState(
      active: event.phase != DndLifecyclePhase.dragEnd,
      phase: event.phase,
      dragId: event.envelope.dragId,
      sourceRuntime: event.envelope.sourceRuntime,
      itemCount: event.envelope.items.length,
      operation: event.operation,
      targetId: event.targetId,
    );

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
    final event = DndReactiveEvent(
      phase: phase,
      envelope: safeEnvelope,
      operation: operation,
      targetId: targetId,
    );
    _events.add(event);
    _state.add(reactiveStateFor(event));
  }

  Future<void> dispose() async {
    await _events.close();
    await _state.close();
  }
}
