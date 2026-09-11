import 'dart:convert';
import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:ores_dnd/ores_dnd.dart';

void main() {
  const codec = OresDndCodec();
  final validText = File('../../contracts/instances/DndEnvelope/valid/text-copy.json').readAsStringSync();
  final valid = codec.decode(validText);

  test('shared fixture round-trips', () {
    final roundTrip = codec.decode(codec.encode(valid));
    expect(roundTrip.dragId, valid.dragId);
    expect(roundTrip.items.single.data, 'hello');
  });

  test('unknown operation fails closed', () {
    final invalid = File('../../contracts/instances/DndEnvelope/invalid/unknown-op.json').readAsStringSync();
    expect(() => codec.decode(invalid), throwsFormatException);
  });

  test('unknown property fails closed', () {
    final map = (jsonDecode(validText) as Map<String, Object?>)..['secret'] = 'x';
    expect(() => codec.decode(jsonEncode(map)), throwsFormatException);
  });

  test('payload byte limit is checked', () {
    const tiny = OresDndCodec(maxPayloadBytes: 4);
    expect(() => tiny.decode('12345'), throwsFormatException);
  });

  test('operation negotiation is deterministic', () {
    expect(
      negotiateOperation([DndOperation.copy, DndOperation.move], [DndOperation.copy, DndOperation.move]),
      DndOperation.move,
    );
    expect(negotiateOperation([DndOperation.copy], [DndOperation.move]), isNull);
  });

  test('telemetry never includes item data', () {
    final event = telemetryFor(DndLifecyclePhase.drop, valid, operation: DndOperation.copy);
    expect(jsonEncode(event.toJson()).contains('hello'), isFalse);
    expect(event.itemCount, 1);
  });

  testWidgets('Flutter draggable carries the shared JSON envelope', (tester) async {
    await tester.pumpWidget(Directionality(
      textDirection: TextDirection.ltr,
      child: OresDraggable(
        envelope: valid,
        feedback: const SizedBox(width: 10, height: 10),
        child: const Text('drag'),
      ),
    ));
    expect(find.text('drag'), findsOneWidget);
    expect(find.byType(Draggable<String>), findsOneWidget);
  });
}
