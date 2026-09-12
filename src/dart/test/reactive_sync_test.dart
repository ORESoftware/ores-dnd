import 'dart:convert';
import 'dart:io';

import 'package:ores_dnd/ores_dnd.dart';
import 'package:ores_dnd/reactive_sync.dart';
import 'package:test/test.dart';

final class _OptoFake implements OptoSyncSupabasePort {
  _OptoFake(this.calls, {this.failSupabase = false});

  final List<String> calls;
  final bool failSupabase;

  @override
  Future<void> persistAcceptedDrop(DndEnvelope envelope, DndDropResult result) async {
    calls.add('opto-local');
  }

  @override
  Future<void> syncAcceptedDropToSupabase(DndEnvelope envelope, DndDropResult result) async {
    calls.add('opto-supabase');
    if (failSupabase) {
      throw StateError('sensitive provider detail');
    }
  }
}

final class _OtelFake implements OresOtelSupabasePort {
  _OtelFake(this.calls);

  final List<String> calls;

  @override
  Future<void> emitDndEvent(DndTelemetryEvent event) async {
    expect(jsonEncode(event.toJson()).contains('hello'), isFalse);
    calls.add('otel-local');
  }

  @override
  Future<void> syncDndEventToSupabase(DndTelemetryEvent event) async {
    expect(jsonEncode(event.toJson()).contains('hello'), isFalse);
    calls.add('otel-supabase');
  }
}

final class _FormsFake implements OresFormsPort {
  _FormsFake(this.calls);

  final List<String> calls;

  @override
  Future<void> applyAcceptedDrop(DndEnvelope envelope, DndDropResult result) async {
    calls.add('forms');
  }
}

void main() {
  const codec = OresDndCodec();
  final validText = File('../../contracts/instances/DndEnvelope/valid/text-copy.json').readAsStringSync();
  final envelope = codec.decode(validText);

  test('RxDart commit invokes both Supabase ports without leaking payload data', () async {
    final calls = <String>[];
    final events = <DndReactiveEvent>[];
    final bus = DndReactiveBus();
    final subscription = bus.events.listen(events.add);
    final result = DndDropResult(
      dragId: envelope.dragId,
      accepted: true,
      operation: DndOperation.copy,
      targetId: 'field-1',
    );

    await commitAcceptedDropReactive(
      envelope,
      result,
      forms: _FormsFake(calls),
      optoSync: _OptoFake(calls),
      otel: _OtelFake(calls),
      reactive: bus,
    );

    await subscription.cancel();
    await bus.close();

    expect(calls, [
      'forms',
      'opto-local',
      'opto-supabase',
      'otel-local',
      'otel-supabase',
    ]);
    final serialized = jsonEncode(events.map((event) => event.toJson()).toList(growable: false));
    expect(serialized.contains('hello'), isFalse);
    expect(
      events.whereType<DndSupabaseSyncReceipt>().map((event) => '${event.channel.wire}:${event.ok}'),
      ['opto-sync:true', 'ores-otel:true'],
    );
  });

  test('failed Opto-Sync Supabase write is redacted and stops ORES-OTel', () async {
    final calls = <String>[];
    final receipts = <DndSupabaseSyncReceipt>[];
    final bus = DndReactiveBus();
    final subscription = bus.supabaseSync.listen(receipts.add);
    final result = DndDropResult(
      dragId: envelope.dragId,
      accepted: true,
      operation: DndOperation.copy,
    );

    await expectLater(
      commitAcceptedDropReactive(
        envelope,
        result,
        optoSync: _OptoFake(calls, failSupabase: true),
        otel: _OtelFake(calls),
        reactive: bus,
      ),
      throwsA(isA<StateError>()),
    );

    await subscription.cancel();
    await bus.close();

    expect(calls, ['opto-local', 'opto-supabase']);
    expect(receipts, hasLength(1));
    expect(receipts.single.ok, isFalse);
    expect(receipts.single.errorCode, 'sync-failed');
    final serialized = jsonEncode(receipts.map((event) => event.toJson()).toList(growable: false));
    expect(serialized.contains('sensitive provider detail'), isFalse);
    expect(serialized.contains('hello'), isFalse);
  });
}
