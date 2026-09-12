import 'dart:async';
import 'dart:convert';

import 'package:ores_dnd/ores_dnd.dart';
import 'package:ores_dnd/ores_dnd_reactive.dart';
import 'package:test/test.dart';

DndEnvelope envelope([String dragId = 'drag-rx-1']) => DndEnvelope(
      protocol: oresDndProtocol,
      dragId: dragId,
      sourceRuntime: 'dart-test',
      allowedOperations: const [DndOperation.copy, DndOperation.move],
      items: const [
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
    final lossless = <DndLifecyclePhase>[];
    final dragOvers = <DndLifecyclePhase>[];

    final subscriptions = <StreamSubscription<dynamic>>[
      bus.state.listen(states.add),
      bus.telemetry.listen(telemetry.add),
      bus.active.listen(active.add),
      bus.drops.listen(drops.add),
      bus.lossless.listen((event) => lossless.add(event.phase)),
      bus.dragOvers.listen((event) => dragOvers.add(event.phase)),
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
    expect(lossless, [DndLifecyclePhase.drop, DndLifecyclePhase.dragEnd]);
    expect(dragOvers, [DndLifecyclePhase.dragOver]);
    expect(states.last.phase, DndLifecyclePhase.dragEnd);
    expect(states.last.active, isFalse);
    expect(telemetry, hasLength(4));
    expect(telemetry[2].phase, DndLifecyclePhase.drop);
    expect(telemetry[2].itemCount, 1);
    expect(isHighFrequencyLifecyclePhase(DndLifecyclePhase.dragOver), isTrue);
    expect(isLosslessLifecyclePhase(DndLifecyclePhase.drop), isTrue);
    expect(isLosslessLifecyclePhase(DndLifecyclePhase.dragEnd), isTrue);
    expect(isLosslessLifecyclePhase(DndLifecyclePhase.dragOver), isFalse);
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

  test('external drag lifecycle may begin at drag-enter', () async {
    final bus = OresDndReactiveBus();
    bus.emit(
      DndLifecyclePhase.dragEnter,
      envelope('external-1'),
      targetId: 'zone-a',
    );
    bus.emit(
      DndLifecyclePhase.dragOver,
      envelope('external-1'),
      targetId: 'zone-a',
    );
    bus.emit(
      DndLifecyclePhase.drop,
      envelope('external-1'),
      operation: DndOperation.copy,
      targetId: 'zone-a',
    );
    bus.emit(
      DndLifecyclePhase.dragEnd,
      envelope('external-1'),
      operation: DndOperation.copy,
      targetId: 'zone-a',
    );
    await bus.dispose();
  });

  test('impossible transitions and drag id switches fail closed', () async {
    final dropFirst = OresDndReactiveBus();
    expect(
      () => dropFirst.emit(
        DndLifecyclePhase.drop,
        envelope(),
        operation: DndOperation.copy,
        targetId: 'zone-a',
      ),
      throwsFormatException,
    );
    await dropFirst.dispose();

    final switched = OresDndReactiveBus();
    switched.emit(DndLifecyclePhase.dragStart, envelope('drag-a'));
    expect(
      () => switched.emit(DndLifecyclePhase.dragOver, envelope('drag-b')),
      throwsFormatException,
    );
    await switched.dispose();
  });

  test('drop requires source-allowed operation and target', () async {
    final missingOperation = OresDndReactiveBus();
    missingOperation.emit(DndLifecyclePhase.dragStart, envelope());
    expect(
      () => missingOperation.emit(
        DndLifecyclePhase.drop,
        envelope(),
        targetId: 'zone-a',
      ),
      throwsFormatException,
    );
    await missingOperation.dispose();

    final missingTarget = OresDndReactiveBus();
    missingTarget.emit(DndLifecyclePhase.dragStart, envelope());
    expect(
      () => missingTarget.emit(
        DndLifecyclePhase.drop,
        envelope(),
        operation: DndOperation.copy,
      ),
      throwsFormatException,
    );
    await missingTarget.dispose();

    final disallowed = OresDndReactiveBus();
    disallowed.emit(DndLifecyclePhase.dragStart, envelope());
    expect(
      () => disallowed.emit(
        DndLifecyclePhase.dragOver,
        envelope(),
        operation: DndOperation.link,
      ),
      throwsFormatException,
    );
    await disallowed.dispose();
  });

  test('emit after disposal fails explicitly', () async {
    final bus = OresDndReactiveBus();
    await bus.dispose();
    expect(
      () => bus.emit(DndLifecyclePhase.dragStart, envelope()),
      throwsStateError,
    );
  });
}
