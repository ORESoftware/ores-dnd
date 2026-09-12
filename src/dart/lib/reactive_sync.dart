import 'package:rxdart/rxdart.dart';

import 'ores_dnd.dart';

enum DndSyncChannel { optoSync, oresOtel }

extension DndSyncChannelWire on DndSyncChannel {
  String get wire => switch (this) {
        DndSyncChannel.optoSync => 'opto-sync',
        DndSyncChannel.oresOtel => 'ores-otel',
      };
}

sealed class DndReactiveEvent {
  const DndReactiveEvent();

  Map<String, Object?> toJson();
}

final class DndAcceptedDropEvent extends DndReactiveEvent {
  const DndAcceptedDropEvent({
    required this.dragId,
    required this.operation,
    this.targetId,
  });

  final String dragId;
  final DndOperation operation;
  final String? targetId;

  @override
  Map<String, Object?> toJson() => {
        'kind': 'accepted-drop',
        'dragId': dragId,
        'operation': operation.wire,
        if (targetId != null) 'targetId': targetId,
      };
}

final class DndLifecycleReactiveEvent extends DndReactiveEvent {
  const DndLifecycleReactiveEvent(this.event);

  final DndTelemetryEvent event;

  @override
  Map<String, Object?> toJson() => {
        'kind': 'lifecycle',
        'event': event.toJson(),
      };
}

final class DndSupabaseSyncReceipt extends DndReactiveEvent {
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

  @override
  Map<String, Object?> toJson() => {
        'kind': 'supabase-sync',
        'dragId': dragId,
        'channel': channel.wire,
        'backend': 'supabase',
        'ok': ok,
        if (targetId != null) 'targetId': targetId,
        if (errorCode != null) 'errorCode': errorCode,
      };
}

/// Opto-Sync implementation supplied by the host application.
///
/// The concrete adapter owns local-first queuing, Supabase configuration and
/// credentials. ores-dnd only calls this method after ores.dnd/v1 validation.
abstract interface class OptoSyncSupabasePort implements OptoSyncPort {
  Future<void> syncAcceptedDropToSupabase(
    DndEnvelope envelope,
    DndDropResult result,
  );
}

/// ORES-OTel implementation supplied by the host application.
///
/// Only sanitized telemetry metadata reaches this boundary. Dragged item data
/// is intentionally unavailable to the method.
abstract interface class OresOtelSupabasePort implements OresOtelPort {
  Future<void> syncDndEventToSupabase(DndTelemetryEvent event);
}

/// RxDart-backed payload-free stream surface for app/domain composition.
final class DndReactiveBus {
  DndReactiveBus() : _events = PublishSubject<DndReactiveEvent>(sync: true);

  final PublishSubject<DndReactiveEvent> _events;

  Stream<DndReactiveEvent> get events => _events.stream;
  Stream<DndAcceptedDropEvent> get acceptedDrops =>
      _events.where((event) => event is DndAcceptedDropEvent).cast<DndAcceptedDropEvent>();
  Stream<DndLifecycleReactiveEvent> get lifecycle =>
      _events.where((event) => event is DndLifecycleReactiveEvent).cast<DndLifecycleReactiveEvent>();
  Stream<DndSupabaseSyncReceipt> get supabaseSync =>
      _events.where((event) => event is DndSupabaseSyncReceipt).cast<DndSupabaseSyncReceipt>();

  void publish(DndReactiveEvent event) => _events.add(event);

  Future<void> close() => _events.close();
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
  DndReactiveBus? bus,
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
  DndReactiveBus? bus,
) async {
  try {
    await port.syncDndEventToSupabase(event);
    bus?.publish(_receipt(result, DndSyncChannel.oresOtel, true));
  } catch (_) {
    bus?.publish(_receipt(result, DndSyncChannel.oresOtel, false));
    rethrow;
  }
}

/// Runs the canonical accepted-drop path, then explicitly invokes the injected
/// Opto-Sync and ORES-OTel Supabase methods.
///
/// No Supabase SDK, endpoint, table name or credential is owned by ores-dnd.
/// Reactive receipts contain metadata only and never contain [DndItem.data].
Future<void> commitAcceptedDropReactive(
  DndEnvelope envelope,
  DndDropResult result, {
  OresFormsPort? forms,
  OptoSyncSupabasePort? optoSync,
  OresOtelSupabasePort? otel,
  DndReactiveBus? reactive,
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

  reactive?.publish(DndAcceptedDropEvent(
    dragId: result.dragId,
    operation: operation,
    targetId: result.targetId,
  ));

  if (optoSync != null) {
    await _syncOpto(optoSync, envelope, result, reactive);
  }

  final telemetry = telemetryFor(
    DndLifecyclePhase.drop,
    envelope,
    operation: operation,
    targetId: result.targetId,
  );
  reactive?.publish(DndLifecycleReactiveEvent(telemetry));

  if (otel != null) {
    await otel.emitDndEvent(telemetry);
    await _syncOtel(otel, telemetry, result, reactive);
  }
}
