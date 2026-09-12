import 'package:rxdart/rxdart.dart';

import 'ores_dnd.dart';
import 'ores_dnd_reactive.dart';

enum DndSyncChannel { optoSync, oresOtel }

extension DndSyncChannelWire on DndSyncChannel {
  String get wire => switch (this) {
        DndSyncChannel.optoSync => 'opto-sync',
        DndSyncChannel.oresOtel => 'ores-otel',
      };
}

final class DndSupabaseSyncReceipt {
  const DndSupabaseSyncReceipt({
    required this.dragId,
    required this.channel,
    required this.ok,
    this.targetId,
    this.errorCode,
  });

  final String dragId;
  final DndSyncChannel channel;
  final bool ok;
  final String? targetId;
  final String? errorCode;

  Map<String, Object?> toJson() => {
        'dragId': dragId,
        'channel': channel.wire,
        'backend': 'supabase',
        'ok': ok,
        if (targetId != null) 'targetId': targetId,
        if (errorCode != null) 'errorCode': errorCode,
      };
}

abstract interface class OptoSyncSupabasePort implements OptoSyncPort {
  Future<void> syncAcceptedDropToSupabase(
    DndEnvelope envelope,
    DndDropResult result,
  );
}

abstract interface class OresOtelSupabasePort implements OresOtelPort {
  Future<void> syncDndEventToSupabase(DndTelemetryEvent event);
}

/// Hot, non-replaying RxDart stream of payload-free Supabase sync receipts.
final class OresDndSupabaseSyncBus {
  OresDndSupabaseSyncBus()
      : _receipts = PublishSubject<DndSupabaseSyncReceipt>(sync: true);

  final PublishSubject<DndSupabaseSyncReceipt> _receipts;

  Stream<DndSupabaseSyncReceipt> get receipts => _receipts.stream;
  Stream<DndSupabaseSyncReceipt> get failures =>
      receipts.where((receipt) => !receipt.ok);

  void publish(DndSupabaseSyncReceipt receipt) => _receipts.add(receipt);

  Future<void> dispose() => _receipts.close();
}

DndSupabaseSyncReceipt _receipt(
  DndDropResult result,
  DndSyncChannel channel,
  bool ok,
) =>
    DndSupabaseSyncReceipt(
      dragId: result.dragId,
      channel: channel,
      ok: ok,
      targetId: result.targetId,
      errorCode: ok ? null : 'sync-failed',
    );

Future<void> _syncOpto(
  OptoSyncSupabasePort port,
  DndEnvelope envelope,
  DndDropResult result,
  OresDndSupabaseSyncBus? bus,
) async {
  try {
    await port.syncAcceptedDropToSupabase(envelope, result);
    bus?.publish(_receipt(result, DndSyncChannel.optoSync, true));
  } catch (_) {
    bus?.publish(_receipt(result, DndSyncChannel.optoSync, false));
    rethrow;
  }
}

Future<void> _syncOtel(
  OresOtelSupabasePort port,
  DndTelemetryEvent event,
  DndDropResult result,
  OresDndSupabaseSyncBus? bus,
) async {
  try {
    await port.syncDndEventToSupabase(event);
    bus?.publish(_receipt(result, DndSyncChannel.oresOtel, true));
  } catch (_) {
    bus?.publish(_receipt(result, DndSyncChannel.oresOtel, false));
    rethrow;
  }
}

/// Runs canonical accepted-drop validation/local effects, then calls the
/// injected Opto-Sync and ORES-OTel Supabase methods.
///
/// ores-dnd owns no Supabase SDK, endpoint, table or credential. Concrete host
/// adapters keep those concerns behind runtime config/secret stores.
Future<void> commitAcceptedDropWithSupabase(
  DndEnvelope envelope,
  DndDropResult result, {
  OresFormsPort? forms,
  OptoSyncSupabasePort? optoSync,
  OresOtelSupabasePort? otel,
  OresDndReactiveBus? lifecycle,
  OresDndSupabaseSyncBus? sync,
}) async {
  await commitAcceptedDrop(
    envelope,
    result,
    forms: forms,
    optoSync: optoSync,
  );
  if (!result.accepted) return;

  final operation = result.operation;
  if (operation == null) {
    throw const FormatException('accepted drop requires an operation');
  }

  if (optoSync != null) {
    await _syncOpto(optoSync, envelope, result, sync);
  }

  lifecycle?.emit(
    DndLifecyclePhase.drop,
    envelope,
    operation: operation,
    targetId: result.targetId,
  );

  final telemetry = telemetryFor(
    DndLifecyclePhase.drop,
    envelope,
    operation: operation,
    targetId: result.targetId,
  );
  if (otel != null) {
    await otel.emitDndEvent(telemetry);
    await _syncOtel(otel, telemetry, result, sync);
  }
}
