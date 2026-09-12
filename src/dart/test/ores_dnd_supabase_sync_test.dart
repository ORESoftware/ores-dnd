import 'dart:convert';
import 'dart:io';

import 'package:ores_dnd/ores_dnd.dart';
import 'package:ores_dnd/ores_dnd_reactive.dart';
import 'package:ores_dnd/ores_dnd_supabase_sync.dart';
import 'package:test/test.dart';

final class _Opto implements OptoSyncSupabasePort {
  _Opto(this.calls, {this.fail = false});
  final List<String> calls;
  final bool fail;

  @override
  Future<void> persistAcceptedDrop(DndEnvelope envelope, DndDropResult result) async {
    calls.add('opto-local');
  }

  @override
  Future<void> syncAcceptedDropToSupabase(DndEnvelope envelope, DndDropResult result) async {
    calls.add('opto-supabase');
    if (fail) throw StateError('sensitive provider detail');
  }
}

final class _Otel implements OresOtelSupabasePort {
  _Otel(this.calls);
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

void main() {
  const codec = OresDndCodec();
  final envelope = codec.decode(
    File('../../contracts/instances/DndEnvelope/valid/text-copy.json').readAsStringSync(),
  );

  test('both Supabase hooks execute and receipts stay payload-free', () async {
    final calls = <String>[];
    final receipts = <DndSupabaseSyncReceipt>[];
    final lifecycle = OresDndReactiveBus();
    final sync = OresDndSupabaseSyncBus();
    final sub = sync.receipts.listen(receipts.add);

    await commitAcceptedDropWithSupabase(
      envelope,
      DndDropResult(
        dragId: envelope.dragId,
        accepted: true,
        operation: DndOperation.copy,
        targetId: 'field-1',
      ),
      optoSync: _Opto(calls),
      otel: _Otel(calls),
      lifecycle: lifecycle,
      sync: sync,
    );

    await sub.cancel();
    await lifecycle.dispose();
    await sync.dispose();

    expect(calls, ['opto-local', 'opto-supabase', 'otel-local', 'otel-supabase']);
    expect(receipts.map((value) => '${value.channel.wire}:${value.ok}'),
        ['opto-sync:true', 'ores-otel:true']);
    expect(jsonEncode(receipts.map((value) => value.toJson()).toList()).contains('hello'), isFalse);
  });

  test('failed Opto-Sync Supabase write blocks ORES-OTel and redacts receipt', () async {
    final calls = <String>[];
    final receipts = <DndSupabaseSyncReceipt>[];
    final sync = OresDndSupabaseSyncBus();
    final sub = sync.receipts.listen(receipts.add);

    await expectLater(
      commitAcceptedDropWithSupabase(
        envelope,
        DndDropResult(
          dragId: envelope.dragId,
          accepted: true,
          operation: DndOperation.copy,
        ),
        optoSync: _Opto(calls, fail: true),
        otel: _Otel(calls),
        sync: sync,
      ),
      throwsA(isA<StateError>()),
    );

    await sub.cancel();
    await sync.dispose();

    expect(calls, ['opto-local', 'opto-supabase']);
    expect(receipts, hasLength(1));
    expect(receipts.single.errorCode, 'sync-failed');
    final json = jsonEncode(receipts.single.toJson());
    expect(json.contains('sensitive provider detail'), isFalse);
    expect(json.contains('hello'), isFalse);
  });
}
