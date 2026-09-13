import 'dart:convert';
import 'dart:io';

import 'package:ores_dnd/ores_dnd.dart';
import 'package:ores_dnd/ores_dnd_reactive_effects.dart';
import 'package:test/test.dart';

final class _Journal implements DndEffectJournalPort {
  final Set<String> completed = {};
  String _key(String key, DndEffectStage stage) => '$key|${stage.wire}';

  @override
  Future<bool> hasCompleted(
    String idempotencyKey,
    DndEffectStage stage,
  ) async =>
      completed.contains(_key(idempotencyKey, stage));

  @override
  Future<void> markCompleted(
    String idempotencyKey,
    DndEffectStage stage,
  ) async {
    completed.add(_key(idempotencyKey, stage));
  }
}

final class _Forms implements OresFormsPort {
  _Forms(this.calls);
  final List<String> calls;
  @override
  Future<void> applyAcceptedDrop(
    DndEnvelope envelope,
    DndDropResult result,
  ) async {
    calls.add('forms');
  }
}

final class _Opto implements OptoSyncSupabasePort {
  _Opto(this.calls, this.keys);
  final List<String> calls;
  final List<String> keys;

  @override
  Future<void> persistAcceptedDrop(
    DndEnvelope envelope,
    DndDropResult result,
  ) async {
    calls.add('opto-local');
  }

  @override
  Future<void> syncAcceptedDropToSupabase(
    DndEnvelope envelope,
    DndDropResult result,
    String idempotencyKey,
  ) async {
    calls.add('opto-supabase');
    keys.add(idempotencyKey);
  }
}

final class _Otel implements OresOtelSupabasePort {
  _Otel(this.calls, this.keys, {this.failRemoteOnce = false});
  final List<String> calls;
  final List<String> keys;
  bool failRemoteOnce;

  @override
  Future<void> emitDndEvent(DndTelemetryEvent event) async {
    expect(jsonEncode(event.toJson()).contains('hello'), isFalse);
    calls.add('otel-local');
  }

  @override
  Future<void> syncDndEventToSupabase(
    DndTelemetryEvent event,
    String idempotencyKey,
  ) async {
    expect(jsonEncode(event.toJson()).contains('hello'), isFalse);
    calls.add('otel-supabase');
    keys.add(idempotencyKey);
    if (failRemoteOnce) {
      failRemoteOnce = false;
      throw StateError('provider-token=sensitive-value');
    }
  }
}

void main() {
  const codec = OresDndCodec();
  final envelope = codec.decode(
    File(
      '../../contracts/instances/DndEnvelope/valid/text-copy.json',
    ).readAsStringSync(),
  );

  DndDropResult drop() => DndDropResult(
        dragId: envelope.dragId,
        accepted: true,
        operation: DndOperation.copy,
        targetId: 'field-1',
      );

  test(
    'RxDart effects share a stable idempotency key and redact payloads',
    () async {
      final calls = <String>[];
      final keys = <String>[];
      final events = <DndEffectReceipt>[];
      final journal = _Journal();
      final bus = OresDndEffectBus();
      final subscription = bus.receipts.listen(events.add);
      final result = drop();

      await commitAcceptedDropEffects(
        envelope,
        result,
        forms: _Forms(calls),
        optoSync: _Opto(calls, keys),
        otel: _Otel(calls, keys),
        journal: journal,
        receipts: bus,
      );

      await subscription.cancel();
      await bus.dispose();
      expect(calls, [
        'forms',
        'opto-local',
        'opto-supabase',
        'otel-local',
        'otel-supabase',
      ]);
      expect(keys.toSet(), {dndEffectKey(result)});
      expect(
        events.map((event) => event.status),
        everyElement(DndEffectStatus.completed),
      );
      expect(
        jsonEncode(
          events.map((event) => event.toJson()).toList(),
        ).contains('hello'),
        isFalse,
      );
    },
  );

  test(
    'retry skips completed stages after a later OTel Supabase failure',
    () async {
      final calls = <String>[];
      final keys = <String>[];
      final events = <DndEffectReceipt>[];
      final journal = _Journal();
      final bus = OresDndEffectBus();
      final subscription = bus.receipts.listen(events.add);
      final result = drop();
      final otel = _Otel(calls, keys, failRemoteOnce: true);

      Future<void> attempt() => commitAcceptedDropEffects(
            envelope,
            result,
            forms: _Forms(calls),
            optoSync: _Opto(calls, keys),
            otel: otel,
            journal: journal,
            receipts: bus,
          );

      await expectLater(attempt(), throwsA(isA<StateError>()));
      await attempt();

      await subscription.cancel();
      await bus.dispose();
      expect(calls, [
        'forms',
        'opto-local',
        'opto-supabase',
        'otel-local',
        'otel-supabase',
        'otel-supabase',
      ]);
      final second = events.skip(5).toList(growable: false);
      expect(
        second.map((event) => '${event.stage.wire}:${event.status.wire}'),
        [
          'forms:skipped',
          'opto-local:skipped',
          'opto-supabase:skipped',
          'otel-local:skipped',
          'otel-supabase:completed',
        ],
      );
      final json = jsonEncode(events.map((event) => event.toJson()).toList());
      expect(json.contains('provider-token'), isFalse);
      expect(json.contains('sensitive-value'), isFalse);
      expect(json.contains('hello'), isFalse);
    },
  );
}
