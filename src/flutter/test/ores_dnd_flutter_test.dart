import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:ores_dnd_flutter/ores_dnd_flutter.dart';

Widget _testHarness(Widget child) => Directionality(
      textDirection: TextDirection.ltr,
      child: Overlay(
        initialEntries: [
          OverlayEntry(
            builder: (context) => Center(child: child),
          ),
        ],
      ),
    );

void main() {
  const codec = OresDndCodec();
  final validText = File('../../contracts/instances/DndEnvelope/valid/text-copy.json').readAsStringSync();
  final valid = codec.decode(validText);

  testWidgets('Flutter draggable carries the shared JSON envelope', (tester) async {
    await tester.pumpWidget(_testHarness(
      OresDraggable(
        envelope: valid,
        feedback: const SizedBox(width: 10, height: 10),
        child: const Text('drag'),
      ),
    ));
    expect(find.text('drag'), findsOneWidget);
    final draggable = tester.widget<Draggable<String>>(find.byType(Draggable<String>));
    final decoded = codec.decode(draggable.data!);
    expect(decoded.dragId, valid.dragId);
  });

  testWidgets('Flutter target composes with the same pure Dart codec', (tester) async {
    await tester.pumpWidget(_testHarness(
      Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          OresDraggable(
            envelope: valid,
            feedback: const SizedBox(width: 10, height: 10),
            child: const Text('drag'),
          ),
          OresDragTarget(
            targetId: 'target-1',
            allowedOperations: const [DndOperation.copy],
            onAccepted: (envelope, operation) async {},
            builder: (context, hovering) => const SizedBox(width: 40, height: 40),
          ),
        ],
      ),
    ));

    expect(find.byType(Draggable<String>), findsOneWidget);
    expect(find.byType(DragTarget<String>), findsOneWidget);
    final draggable = tester.widget<Draggable<String>>(find.byType(Draggable<String>));
    expect(negotiateOperation(
      codec.decode(draggable.data!).allowedOperations,
      const [DndOperation.copy],
    ), DndOperation.copy);
  });
}
