part of '../ores_dnd.dart';

abstract interface class OresOtelPort {
  Future<void> emitDndEvent(DndTelemetryEvent event);
}

abstract interface class OptoSyncPort {
  Future<void> persistAcceptedDrop(DndEnvelope envelope, DndDropResult result);
}

abstract interface class OresFormsPort {
  Future<void> applyAcceptedDrop(DndEnvelope envelope, DndDropResult result);
}

/// Inject a web/native WASM host without coupling this Dart package to a particular
/// wasm loader. Flutter Web typically wires this to `ores_dnd_wasm` JS glue; desktop
/// hosts may wire it to Wasmtime/Wasmer/FFI.
abstract interface class OresDndWasmPort {
  Future<String> normalizeEnvelopeJson(String payload);
}

Future<void> commitAcceptedDrop(
  DndEnvelope envelope,
  DndDropResult result, {
  OresFormsPort? forms,
  OptoSyncPort? optoSync,
  OresOtelPort? otel,
}) async {
  final safeEnvelope = DndEnvelope.fromJson(envelope.toJson());
  if (result.dragId != safeEnvelope.dragId) {
    throw const FormatException('drop result dragId does not match envelope');
  }
  if (!result.accepted) return;
  final operation = result.operation;
  if (operation == null || !safeEnvelope.allowedOperations.contains(operation)) {
    throw const FormatException('accepted drop must use a source-allowed operation');
  }
  await forms?.applyAcceptedDrop(safeEnvelope, result);
  await optoSync?.persistAcceptedDrop(safeEnvelope, result);
  await otel?.emitDndEvent(telemetryFor(
    DndLifecyclePhase.drop,
    safeEnvelope,
    operation: operation,
    targetId: result.targetId,
  ));
}
