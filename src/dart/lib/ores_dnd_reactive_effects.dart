import 'package:rxdart/rxdart.dart';

import 'ores_dnd.dart';

enum DndEffectStage { forms, optoLocal, optoSupabase, otelLocal, otelSupabase }

enum DndEffectStatus { completed, skipped, failed }

extension DndEffectStageWire on DndEffectStage {
  String get wire => switch (this) {
    DndEffectStage.forms => 'forms',
    DndEffectStage.optoLocal => 'opto-local',
    DndEffectStage.optoSupabase => 'opto-supabase',
    DndEffectStage.otelLocal => 'otel-local',
    DndEffectStage.otelSupabase => 'otel-supabase',
  };
}

extension DndEffectStatusWire on DndEffectStatus {
  String get wire => name;
}

final class DndEffectReceipt {
  const DndEffectReceipt({
    required this.idempotencyKey,
    required this.dragId,
    required this.stage,
    required this.status,
    this.targetId,
    this.errorCode,
  });

  final String idempotencyKey;
  final String dragId;
  final DndEffectStage stage;
  final DndEffectStatus status;
  final String? targetId;
  final String? errorCode;

  Map<String, Object?> toJson() => {
    'idempotencyKey': idempotencyKey,
    'dragId': dragId,
    'stage': stage.wire,
    'status': status.wire,
    if (targetId != null) 'targetId': targetId,
    if (errorCode != null) 'errorCode': errorCode,
  };
}

abstract interface class DndEffectJournalPort {
  Future<bool> hasCompleted(String idempotencyKey, DndEffectStage stage);
  Future<void> markCompleted(String idempotencyKey, DndEffectStage stage);
}

abstract interface class OptoSyncSupabasePort implements OptoSyncPort {
  Future<void> syncAcceptedDropToSupabase(
    DndEnvelope envelope,
    DndDropResult result,
    String idempotencyKey,
  );
}

abstract interface class OresOtelSupabasePort implements OresOtelPort {
  Future<void> syncDndEventToSupabase(
    DndTelemetryEvent event,
    String idempotencyKey,
  );
}

final class OresDndEffectBus {
  // Effect receipts are part of the commit completion contract: once
  // commitAcceptedDropEffects resolves, all receipts it emitted are already
  // visible to subscribers. Raw drag lifecycle streams intentionally retain
  // their normal asynchronous RxDart delivery semantics.
  OresDndEffectBus() : _receipts = PublishSubject<DndEffectReceipt>(sync: true);

  final PublishSubject<DndEffectReceipt> _receipts;
  Stream<DndEffectReceipt> get receipts => _receipts.stream;
  Stream<DndEffectReceipt> get failures =>
      receipts.where((receipt) => receipt.status == DndEffectStatus.failed);

  void publish(DndEffectReceipt receipt) => _receipts.add(receipt);
  Future<void> dispose() => _receipts.close();
}

String _component(String? value) => Uri.encodeComponent(value ?? '-');

String dndEffectKey(DndDropResult result) => [
  'ores.dnd/v1',
  _component(result.dragId),
  _component(result.targetId),
  _component(result.operation?.wire),
].join(':');

DndEffectReceipt _receipt(
  String key,
  DndDropResult result,
  DndEffectStage stage,
  DndEffectStatus status,
) => DndEffectReceipt(
  idempotencyKey: key,
  dragId: result.dragId,
  stage: stage,
  status: status,
  targetId: result.targetId,
  errorCode: status == DndEffectStatus.failed ? 'effect-failed' : null,
);

Future<void> _runStage(
  String key,
  DndDropResult result,
  DndEffectStage stage,
  DndEffectJournalPort? journal,
  OresDndEffectBus? bus,
  Future<void> Function() effect,
) async {
  if (journal != null && await journal.hasCompleted(key, stage)) {
    bus?.publish(_receipt(key, result, stage, DndEffectStatus.skipped));
    return;
  }
  try {
    await effect();
    await journal?.markCompleted(key, stage);
    bus?.publish(_receipt(key, result, stage, DndEffectStatus.completed));
  } catch (_) {
    bus?.publish(_receipt(key, result, stage, DndEffectStatus.failed));
    rethrow;
  }
}

Future<void> commitAcceptedDropEffects(
  DndEnvelope envelope,
  DndDropResult result, {
  OresFormsPort? forms,
  OptoSyncSupabasePort? optoSync,
  OresOtelSupabasePort? otel,
  DndEffectJournalPort? journal,
  OresDndEffectBus? receipts,
}) async {
  await commitAcceptedDrop(envelope, result);
  if (!result.accepted) return;

  final operation = result.operation;
  if (operation == null) {
    throw const FormatException('accepted drop requires an operation');
  }
  final key = dndEffectKey(result);

  if (forms != null) {
    await _runStage(
      key,
      result,
      DndEffectStage.forms,
      journal,
      receipts,
      () => forms.applyAcceptedDrop(envelope, result),
    );
  }
  if (optoSync != null) {
    await _runStage(
      key,
      result,
      DndEffectStage.optoLocal,
      journal,
      receipts,
      () => optoSync.persistAcceptedDrop(envelope, result),
    );
    await _runStage(
      key,
      result,
      DndEffectStage.optoSupabase,
      journal,
      receipts,
      () => optoSync.syncAcceptedDropToSupabase(envelope, result, key),
    );
  }

  final telemetry = telemetryFor(
    DndLifecyclePhase.drop,
    envelope,
    operation: operation,
    targetId: result.targetId,
  );
  if (otel != null) {
    await _runStage(
      key,
      result,
      DndEffectStage.otelLocal,
      journal,
      receipts,
      () => otel.emitDndEvent(telemetry),
    );
    await _runStage(
      key,
      result,
      DndEffectStage.otelSupabase,
      journal,
      receipts,
      () => otel.syncDndEventToSupabase(telemetry, key),
    );
  }
}
