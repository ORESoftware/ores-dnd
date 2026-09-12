part of '../ores_dnd.dart';

/// Observable states of one drag session.
enum DndSessionState {
  idle('idle'),
  dragging('dragging'),
  overTarget('over-target'),
  dropped('dropped'),
  cancelled('cancelled');

  const DndSessionState(this.wire);
  final String wire;
  bool get isTerminal => this == dropped || this == cancelled;

  static DndSessionState parse(Object? value) => DndSessionState.values.firstWhere(
        (candidate) => candidate.wire == value,
        orElse: () => throw FormatException('unsupported session state: $value'),
      );
}

/// Inputs a host feeds into the session state machine.
enum DndSessionInputKind {
  start('start'),
  enter('enter'),
  leave('leave'),
  drop('drop'),
  cancel('cancel'),
  end('end');

  const DndSessionInputKind(this.wire);
  final String wire;

  static DndSessionInputKind parse(Object? value) => DndSessionInputKind.values.firstWhere(
        (candidate) => candidate.wire == value,
        orElse: () => throw FormatException('unsupported session input kind: $value'),
      );
}

final class DndSessionInput {
  const DndSessionInput({required this.kind, this.envelope, this.targetId, this.policy, this.preferredOperation});

  const DndSessionInput.start(DndEnvelope envelope) : this(kind: DndSessionInputKind.start, envelope: envelope);
  DndSessionInput.enter(DndDropPolicy policy, {DndOperation? preferred})
      : this(kind: DndSessionInputKind.enter, targetId: policy.targetId, policy: policy, preferredOperation: preferred);
  const DndSessionInput.leave(String targetId) : this(kind: DndSessionInputKind.leave, targetId: targetId);
  const DndSessionInput.drop(String targetId) : this(kind: DndSessionInputKind.drop, targetId: targetId);
  const DndSessionInput.cancel() : this(kind: DndSessionInputKind.cancel);
  const DndSessionInput.end() : this(kind: DndSessionInputKind.end);

  final DndSessionInputKind kind;
  final DndEnvelope? envelope;
  final String? targetId;
  final DndDropPolicy? policy;
  final DndOperation? preferredOperation;

  factory DndSessionInput.fromJson(Map<String, Object?> json) {
    _rejectUnknown(json, const {'kind', 'envelope', 'targetId', 'policy', 'preferredOperation'}, 'session input');
    final envelope = json['envelope'];
    if (envelope != null && envelope is! Map) throw const FormatException('envelope must be an object');
    final policy = json['policy'];
    if (policy != null && policy is! Map) throw const FormatException('policy must be an object');
    final preferred = json['preferredOperation'];
    return DndSessionInput(
      kind: DndSessionInputKind.parse(json['kind']),
      envelope: envelope == null ? null : DndEnvelope.fromJson((envelope as Map).cast<String, Object?>(), structural: true),
      targetId: Wire.optionalSafeId(json['targetId'], 'targetId'),
      policy: policy == null ? null : DndDropPolicy.fromJson((policy as Map).cast<String, Object?>()),
      preferredOperation: preferred == null ? null : DndOperationWire.parse(preferred),
    );
  }

  Map<String, Object?> toJson() => {
        'kind': kind.wire,
        if (envelope != null) 'envelope': envelope!.toJson(),
        if (targetId != null) 'targetId': targetId,
        if (policy != null) 'policy': policy!.toJson(),
        if (preferredOperation != null) 'preferredOperation': preferredOperation!.wire,
      };
}

final class DndSessionSnapshot {
  const DndSessionSnapshot({required this.state, this.dragId, this.targetId, this.operation, this.errorCode});

  static const idle = DndSessionSnapshot(state: DndSessionState.idle);

  final DndSessionState state;
  final String? dragId;
  final String? targetId;
  final DndOperation? operation;
  final DndRejectCode? errorCode;

  bool get isOverAcceptingTarget => state == DndSessionState.overTarget;

  factory DndSessionSnapshot.fromJson(Map<String, Object?> json) {
    _rejectUnknown(json, const {'state', 'dragId', 'targetId', 'operation', 'errorCode'}, 'session snapshot');
    final operation = json['operation'];
    final errorCode = json['errorCode'];
    return DndSessionSnapshot(
      state: DndSessionState.parse(json['state']),
      dragId: Wire.optionalSafeId(json['dragId'], 'dragId'),
      targetId: Wire.optionalSafeId(json['targetId'], 'targetId'),
      operation: operation == null ? null : DndOperationWire.parse(operation),
      errorCode: errorCode == null ? null : DndRejectCode.parse(errorCode),
    );
  }

  Map<String, Object?> toJson() => {
        'state': state.wire,
        if (dragId != null) 'dragId': dragId,
        if (targetId != null) 'targetId': targetId,
        if (operation != null) 'operation': operation!.wire,
        if (errorCode != null) 'errorCode': errorCode!.wire,
      };

  /// The terminal [DndDropResult], or null while the session runs.
  DndDropResult? get result {
    final id = dragId;
    if (id == null) return null;
    return switch (state) {
      DndSessionState.dropped => DndDropResult(dragId: id, accepted: true, operation: operation, targetId: targetId),
      DndSessionState.cancelled => DndDropResult(dragId: id, accepted: false, targetId: targetId, errorCode: errorCode?.wire),
      _ => null,
    };
  }

  @override
  bool operator ==(Object other) =>
      other is DndSessionSnapshot &&
      other.state == state &&
      other.dragId == dragId &&
      other.targetId == targetId &&
      other.operation == operation &&
      other.errorCode == errorCode;

  @override
  int get hashCode => Object.hash(state, dragId, targetId, operation, errorCode);

  @override
  String toString() => 'DndSessionSnapshot${toJson()}';
}

/// A replayable conformance trace (contracts/instances/DndSessionTrace/valid).
final class DndSessionTrace {
  const DndSessionTrace({required this.id, required this.inputs, required this.expected, this.description});

  final String id;
  final String? description;
  final List<DndSessionInput> inputs;
  final List<DndSessionSnapshot> expected;

  factory DndSessionTrace.fromJson(Map<String, Object?> json) {
    _rejectUnknown(json, const {'id', 'description', 'inputs', 'expected'}, 'session trace');
    final inputs = json['inputs'];
    final expected = json['expected'];
    if (inputs is! List || expected is! List) throw const FormatException('inputs and expected must be arrays');
    if (!Wire.isTraceId(json['id'])) throw const FormatException(r'trace id must match ^[a-z0-9][a-z0-9._-]{0,127}$');
    Wire.checkLength(inputs.length, 1, Wire.traceStepsMax, 'inputs');
    Wire.checkLength(expected.length, 1, Wire.traceStepsMax, 'expected');
    final description = _optionalString(json['description'], 'description', allowEmpty: true);
    if (description != null && Wire.codePoints(description) > Wire.traceDescriptionMax) {
      throw const FormatException('description exceeds 512 characters');
    }
    final trace = DndSessionTrace(
      id: json['id'] as String,
      description: description,
      inputs: List.unmodifiable(inputs.map((v) {
        if (v is! Map) throw const FormatException('session input must be an object');
        return DndSessionInput.fromJson(v.cast<String, Object?>());
      })),
      expected: List.unmodifiable(expected.map((v) {
        if (v is! Map) throw const FormatException('session snapshot must be an object');
        return DndSessionSnapshot.fromJson(v.cast<String, Object?>());
      })),
    );
    if (trace.inputs.length != trace.expected.length) {
      throw const FormatException('trace inputs and expected must have the same length');
    }
    return trace;
  }
}

typedef DndSessionListener = void Function(DndSessionSnapshot snapshot, DndSessionInput input);

/// One drag session: a pure `apply(snapshot, input) → snapshot` (docs/DESIGN.md
/// §Session). Hosts keep one per drag source or one per app.
final class DndSession {
  DndSession({OresDndCodec codec = const OresDndCodec()}) : _codec = codec;

  final OresDndCodec _codec;
  DndSessionSnapshot _snapshot = DndSessionSnapshot.idle;
  DndEnvelope? _envelope;
  final List<DndSessionListener> _listeners = [];

  DndSessionSnapshot get snapshot => _snapshot;

  /// The envelope of the running (or just finished) session.
  DndEnvelope? get envelope => _envelope;

  DndDropResult? get result => _snapshot.result;

  void Function() subscribe(DndSessionListener listener) {
    _listeners.add(listener);
    return () => _listeners.remove(listener);
  }

  /// Apply one input and return the new snapshot. Malformed inputs are ignored.
  DndSessionSnapshot apply(DndSessionInput input) {
    _snapshot = _next(input);
    for (final listener in List.of(_listeners)) {
      listener(_snapshot, input);
    }
    return _snapshot;
  }

  DndSessionSnapshot _next(DndSessionInput input) {
    final current = _snapshot;
    if (input.kind == DndSessionInputKind.start) {
      DndEnvelope? valid;
      final candidate = input.envelope;
      if (candidate != null) {
        try {
          valid = DndEnvelope.fromJson(candidate.toJson(), maxItems: _codec.maxItems);
        } on FormatException {
          valid = null;
        }
      }
      _envelope = valid;
      return valid == null
          ? const DndSessionSnapshot(state: DndSessionState.idle, errorCode: DndRejectCode.invalidEnvelope)
          : DndSessionSnapshot(state: DndSessionState.dragging, dragId: valid.dragId);
    }
    if (current.state == DndSessionState.idle || current.state.isTerminal) return current;
    if (input.targetId != null && !Wire.isSafeId(input.targetId)) return current; // structurally invalid input: ignored
    switch (input.kind) {
      case DndSessionInputKind.enter:
        final policy = input.policy;
        final envelope = _envelope;
        if (policy == null || envelope == null || !policy.isValid) return current;
        final targetId = input.targetId ?? policy.targetId;
        return switch (evaluatePolicy(envelope, policy, preferred: input.preferredOperation)) {
          PolicyAccepted(:final operation) =>
            DndSessionSnapshot(state: DndSessionState.overTarget, dragId: current.dragId, targetId: targetId, operation: operation),
          PolicyRejected(:final errorCode) =>
            DndSessionSnapshot(state: DndSessionState.dragging, dragId: current.dragId, targetId: targetId, errorCode: errorCode),
        };
      case DndSessionInputKind.leave:
        return input.targetId != null && current.targetId == input.targetId
            ? DndSessionSnapshot(state: DndSessionState.dragging, dragId: current.dragId)
            : current;
      case DndSessionInputKind.drop:
        DndSessionSnapshot cancelled(DndRejectCode code) =>
            DndSessionSnapshot(state: DndSessionState.cancelled, dragId: current.dragId, targetId: input.targetId, errorCode: code);
        if (current.targetId == null) return cancelled(DndRejectCode.noActiveTarget);
        if (input.targetId != current.targetId) return cancelled(DndRejectCode.targetMismatch);
        if (current.state == DndSessionState.overTarget) {
          return DndSessionSnapshot(
            state: DndSessionState.dropped,
            dragId: current.dragId,
            targetId: current.targetId,
            operation: current.operation,
            errorCode: current.errorCode,
          );
        }
        return cancelled(current.errorCode ?? DndRejectCode.noActiveTarget);
      case DndSessionInputKind.cancel:
      case DndSessionInputKind.end:
        return DndSessionSnapshot(state: DndSessionState.cancelled, dragId: current.dragId, errorCode: DndRejectCode.cancelled);
      case DndSessionInputKind.start:
        return current; // handled above
    }
  }
}

final class TraceDivergence {
  const TraceDivergence({required this.traceId, required this.step, this.expected, this.actual});
  final String traceId;
  final int step;
  final DndSessionSnapshot? expected;
  final DndSessionSnapshot? actual;

  @override
  String toString() => 'trace $traceId diverged at step $step: expected $expected, got $actual';
}

/// Replay a trace from the idle state; returns the first divergence or null.
TraceDivergence? replayTrace(DndSessionTrace trace) {
  if (trace.inputs.length != trace.expected.length) {
    return TraceDivergence(traceId: trace.id, step: trace.inputs.length < trace.expected.length ? trace.inputs.length : trace.expected.length);
  }
  final session = DndSession();
  for (var step = 0; step < trace.inputs.length; step++) {
    final actual = session.apply(trace.inputs[step]);
    if (actual != trace.expected[step]) {
      return TraceDivergence(traceId: trace.id, step: step, expected: trace.expected[step], actual: actual);
    }
  }
  return null;
}
