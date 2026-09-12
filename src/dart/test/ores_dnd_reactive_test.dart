import 'dart:async';
import 'dart:convert';

import 'package:ores_dnd/ores_dnd.dart';
import 'package:ores_dnd/ores_dnd_reactive.dart';
import 'package:test/test.dart';

DndEnvelope envelope() => const DndEnvelope(
      protocol: oresDndProtocol,
      dragId: 'drag-rx-1',
      sourceRuntime: 'dart-test',
      allowedOperations: [DndOperation.copy, DndOperation.move],
      items: [
        DndItem(
          kind: DndItemKind.text,
          mediaType: 'text/plain',
          data: 'TOP-SECRET-DRAG-DATA',
        ),
      ],
    );

void main() {
  test('RxDart bus keeps replayable state and telemetry payload-free', () async {
    final bus = OresDndReactiveBus();
    final states = <DndReactiveState>[];
    final telemetry = <DndTelemetryEvent>[];
    final active = <bool>[];
    final drops = <DndReactiveEvent>[];

    final subscriptions = <StreamSubscription<dynamic>>[
      bus.state.listen(states.add),
      bus.telemetry.listen(telemetry.add),
      bus.active.listen(active.add),
      bus.drops.listen(drops.add),
    ];

    bus.emit(DndLifecyclePhase.dragStart, envelope());
    bus.emit(
      DndLifecyclePhase.dragOver,
      envelope(),
      operation: DndOperation.copy,
      targetId: 'zone-a',
    );
    bus.emit(
      DndLifecyclePhase.drop,
      envelope(),
      operation: DndOperation.copy,
      targetId: 'zone-a',
    );
    bus.emit(
      DndLifecyclePhase.dragEnd,
      envelope(),
      operation: DndOperation.copy,
      targetId: 'zone-a',
    );

    await Future<void>.delayed(Duration.zero);

    expect(active, [false, true, false]);
    expect(drops, hasLength(1));
    expect(states.last.phase, DndLifecyclePhase.dragEnd);
    expect(states.last.active, isFalse);
    expect(telemetry, hasLength(4));
    expect(telemetry[2].phase, DndLifecyclePhase.drop);
    expect(telemetry[2].itemCount, 1);
    expect(
      jsonEncode(states.map((state) => state.toJson()).toList()),
      isNot(contains('TOP-SECRET-DRAG-DATA')),
    );
    expect(
      jsonEncode(telemetry.map((event) => event.toJson()).toList()),
      isNot(contains('TOP-SECRET-DRAG-DATA')),
    );

    for (final subscription in subscriptions) {
      await subscription.cancel();
    }
    await bus.dispose();
    expect(bus.isClosed, isTrue);
  });

  test('raw RxDart event stream is hot and does not replay envelopes', () async {
    final bus = OresDndReactiveBus();
    bus.emit(DndLifecyclePhase.dragStart, envelope());

    final seen = <DndLifecyclePhase>[];
    final subscription = bus.events.listen((event) => seen.add(event.phase));
    await Future<void>.delayed(Duration.zero);
    expect(seen, isEmpty);

    bus.emit(DndLifecyclePhase.dragOver, envelope());
    await Future<void>.delayed(Duration.zero);
    expect(seen, [DndLifecyclePhase.dragOver]);

    await subscription.cancel();
    await bus.dispose();
  });
}
