/// `ores_dnd_flutter` — Flutter widgets for the `ores.dnd/v1` protocol.
///
/// [OresDraggable] and [OresDragTarget] wrap Flutter's `Draggable` /
/// `DragTarget` and feed their callbacks into the shared [DndSession] state
/// machine through an [OresDndController], so a Flutter client accepts or
/// rejects exactly what a web page or a Rust desktop app would.
library;

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:ores_dnd/ores_dnd.dart';

export 'package:ores_dnd/ores_dnd.dart';
export 'package:ores_dnd/ores_dnd_reactive.dart';
export 'package:ores_dnd/ores_dnd_reactive_effects.dart';

/// A zone's view of the session — the Flutter twin of the `data-ores-dnd-state`
/// attribute browser adapters keep on drop-zone elements.
enum OresZoneState { idle, dragging, accepting, rejecting, dropped }

/// Reflects the session only while this zone is the active target.
OresZoneState zoneStateFor(DndSessionSnapshot snapshot, String targetId) {
  final mine = snapshot.targetId == targetId;
  return switch (snapshot.state) {
    DndSessionState.overTarget => mine ? OresZoneState.accepting : OresZoneState.dragging,
    DndSessionState.dragging => mine ? OresZoneState.rejecting : OresZoneState.dragging,
    DndSessionState.dropped => mine ? OresZoneState.dropped : OresZoneState.idle,
    _ => OresZoneState.idle,
  };
}

/// Owns one [DndSession] and notifies widgets when its snapshot changes.
/// Share one controller between every source and target that may interact.
class OresDndController extends ChangeNotifier {
  OresDndController({OresDndCodec codec = const OresDndCodec(), this.otel}) : _session = DndSession(codec: codec);

  /// The default controller used when widgets are given none.
  static final OresDndController shared = OresDndController();

  final DndSession _session;

  /// Optional content-free lifecycle telemetry (ores-otel).
  final OresOtelPort? otel;

  DndSessionSnapshot get snapshot => _session.snapshot;
  DndEnvelope? get envelope => _session.envelope;
  DndDropResult? get result => _session.result;

  DndSessionSnapshot apply(DndSessionInput input) {
    final next = _session.apply(input);
    notifyListeners();
    return next;
  }

  void emit(DndLifecyclePhase phase, {DndOperation? operation, String? targetId}) {
    final port = otel;
    final current = envelope;
    if (port == null || current == null) return;
    unawaited(port.emitDndEvent(telemetryFor(phase, current, operation: operation, targetId: targetId)));
  }
}

/// A drag source. Serializes the envelope as the `Draggable<String>` data
/// (so plain `DragTarget<String>`s and [OresDragTarget]s both understand it)
/// and drives the session on start/end.
class OresDraggable extends StatelessWidget {
  const OresDraggable({
    required this.envelope,
    required this.child,
    required this.feedback,
    this.childWhenDragging,
    this.controller,
    this.codec = const OresDndCodec(),
    super.key,
  });

  final DndEnvelope envelope;
  final Widget child;
  final Widget feedback;
  final Widget? childWhenDragging;
  final OresDndController? controller;
  final OresDndCodec codec;

  OresDndController get _controller => controller ?? OresDndController.shared;

  @override
  Widget build(BuildContext context) => Draggable<String>(
        data: codec.encode(envelope),
        feedback: feedback,
        childWhenDragging: childWhenDragging,
        onDragStarted: () {
          _controller.apply(DndSessionInput.start(envelope));
          _controller.emit(DndLifecyclePhase.dragStart);
        },
        onDragEnd: (_) {
          final wasRunning = !_controller.snapshot.state.isTerminal;
          _controller.apply(const DndSessionInput.end());
          if (wasRunning) _controller.emit(DndLifecyclePhase.dragEnd);
        },
        child: child,
      );
}

typedef OresDropAccepted = Future<void> Function(DndEnvelope envelope, DndDropResult result);
typedef OresDropRejected = void Function(DndEnvelope? envelope, DndDropResult result);
typedef OresZoneBuilder = Widget Function(BuildContext context, OresZoneState state, DndSessionSnapshot snapshot);

/// A drop target governed by a [DndDropPolicy]. Decodes the dragged envelope,
/// evaluates the policy through the shared state machine, and calls
/// [onAccepted] only for a `dropped` session — wire it to `commitAcceptedDrop`.
class OresDragTarget extends StatelessWidget {
  const OresDragTarget({
    required this.policy,
    required this.builder,
    required this.onAccepted,
    this.onRejected,
    this.controller,
    this.codec = const OresDndCodec(),
    super.key,
  });

  final DndDropPolicy policy;
  final OresZoneBuilder builder;
  final OresDropAccepted onAccepted;
  final OresDropRejected? onRejected;
  final OresDndController? controller;
  final OresDndCodec codec;

  OresDndController get _controller => controller ?? OresDndController.shared;

  DndEnvelope? _decode(String data) {
    try {
      return codec.decode(data);
    } on FormatException {
      return null;
    }
  }

  /// Ensure the session is running for this payload (a drag from a plain
  /// `Draggable<String>` or another controller has no `start` yet).
  void _adopt(DndEnvelope envelope) {
    if (_controller.envelope?.dragId != envelope.dragId || _controller.snapshot.state.isTerminal) {
      _controller.apply(DndSessionInput.start(envelope));
    }
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: _controller,
        builder: (context, _) => DragTarget<String>(
          onWillAcceptWithDetails: (details) {
            final envelope = _decode(details.data);
            if (envelope == null) return false;
            _adopt(envelope);
            final wasAccepting = _controller.snapshot.isOverAcceptingTarget && _controller.snapshot.targetId == policy.targetId;
            final next = _controller.apply(DndSessionInput.enter(policy));
            final accepting = next.isOverAcceptingTarget && next.targetId == policy.targetId;
            if (accepting && !wasAccepting) _controller.emit(DndLifecyclePhase.dragEnter, operation: next.operation, targetId: policy.targetId);
            return accepting;
          },
          onLeave: (_) {
            final wasAccepting = _controller.snapshot.isOverAcceptingTarget && _controller.snapshot.targetId == policy.targetId;
            _controller.apply(DndSessionInput.leave(policy.targetId));
            if (wasAccepting) _controller.emit(DndLifecyclePhase.dragLeave, targetId: policy.targetId);
          },
          onAcceptWithDetails: (details) async {
            final envelope = _decode(details.data);
            if (envelope != null) _adopt(envelope);
            final next = _controller.apply(DndSessionInput.drop(policy.targetId));
            final result = next.result;
            if (result == null) return;
            final current = _controller.envelope;
            if (next.state == DndSessionState.dropped && current != null) {
              _controller.emit(DndLifecyclePhase.drop, operation: result.operation, targetId: policy.targetId);
              await onAccepted(current, result);
            } else {
              onRejected?.call(current, result);
            }
          },
          builder: (context, candidates, rejected) => builder(context, zoneStateFor(_controller.snapshot, policy.targetId), _controller.snapshot),
        ),
      );
}
