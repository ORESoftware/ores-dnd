import 'dart:convert';
import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:ores_dnd_flutter/ores_dnd_flutter.dart';

class _Recorder implements OresOtelPort {
  final events = <DndTelemetryEvent>[];
  @override
  Future<void> emitDndEvent(DndTelemetryEvent event) async => events.add(event);
}

Widget testHost(Widget child) => Directionality(
      textDirection: TextDirection.ltr,
      child: Overlay(
        initialEntries: [OverlayEntry(builder: (_) => child)],
      ),
    );

void main() {
  const codec = OresDndCodec();
  final validText = File('../../contracts/instances/DndEnvelope/valid/text-copy.json').readAsStringSync();
  final valid = codec.decode(validText);
  const textPolicy = DndDropPolicy(targetId: 'zone-a', allowedOperations: [DndOperation.copy, DndOperation.move], acceptedKinds: [DndItemKind.text]);
  const jsonPolicy = DndDropPolicy(targetId: 'zone-json', allowedOperations: [DndOperation.copy], acceptedKinds: [DndItemKind.json]);

  Widget harness({
    required OresDndController controller,
    required DndDropPolicy policy,
    required OresDropAccepted onAccepted,
    OresDropRejected? onRejected,
    List<OresZoneState>? states,
  }) =>
      testHost(
        Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            OresDraggable(
              controller: controller,
              envelope: valid,
              feedback: const SizedBox(width: 10, height: 10),
              child: const SizedBox(width: 50, height: 50, child: Text('drag')),
            ),
            const SizedBox(height: 100),
            OresDragTarget(
              controller: controller,
              policy: policy,
              onAccepted: onAccepted,
              onRejected: onRejected,
              builder: (context, state, snapshot) {
                states?.add(state);
                return SizedBox(width: 100, height: 100, child: Text('zone:${state.name}'));
              },
            ),
          ],
        ),
      );

  test('Flutter package re-exports the RxDart reactive lifecycle surface', () async {
    final bus = OresDndReactiveBus();
    final states = <DndReactiveState>[];
    final subscription = bus.state.listen(states.add);

    bus.emit(DndLifecyclePhase.dragStart, valid);
    bus.emit(DndLifecyclePhase.dragEnd, valid);
    await Future<void>.delayed(Duration.zero);

    expect(states.first.active, isFalse);
    expect(states.any((state) => state.active), isTrue);
    expect(states.last.phase, DndLifecyclePhase.dragEnd);
    expect(states.last.active, isFalse);

    await subscription.cancel();
    await bus.dispose();
    expect(bus.isClosed, isTrue);
  });

  testWidgets('Flutter draggable carries the shared JSON envelope', (tester) async {
    final controller = OresDndController();
    await tester.pumpWidget(harness(controller: controller, policy: textPolicy, onAccepted: (_, __) async {}));
    final draggable = tester.widget<Draggable<String>>(find.byType(Draggable<String>));
    expect(codec.decode(draggable.data!).dragId, valid.dragId);
    expect(find.text('zone:idle'), findsOneWidget);
  });

  testWidgets('a real drag onto an accepting target drives the shared state machine to dropped', (tester) async {
    final otel = _Recorder();
    final controller = OresDndController(otel: otel);
    final accepted = <String>[];
    final states = <OresZoneState>[];
    await tester.pumpWidget(harness(
      controller: controller,
      policy: textPolicy,
      states: states,
      onAccepted: (envelope, result) async => accepted.add('${envelope.dragId}:${result.operation?.wire}:${result.targetId}'),
    ));

    final gesture = await tester.startGesture(tester.getCenter(find.text('drag')));
    await tester.pump();
    await gesture.moveBy(const Offset(0, 20));
    await tester.pump();
    expect(controller.snapshot.state, DndSessionState.dragging);
    expect(find.text('zone:dragging'), findsOneWidget);

    await gesture.moveTo(tester.getCenter(find.textContaining('zone:')));
    await tester.pump();
    expect(controller.snapshot.state, DndSessionState.overTarget);
    expect(controller.snapshot.operation, DndOperation.move);
    expect(find.text('zone:accepting'), findsOneWidget);

    await gesture.up();
    await tester.pump();
    expect(controller.snapshot.state, DndSessionState.dropped);
    expect(accepted, ['drag-0001:move:zone-a']);
    expect(find.text('zone:dropped'), findsOneWidget);
    expect(otel.events.map((e) => e.phase), [DndLifecyclePhase.dragStart, DndLifecyclePhase.dragEnter, DndLifecyclePhase.drop]);
    expect(jsonEncode(otel.events.map((e) => e.toJson()).toList()).contains('hello'), isFalse);
  });

  testWidgets('a rejecting target reports the reason and never accepts', (tester) async {
    final controller = OresDndController();
    final accepted = <String>[];
    final rejected = <String?>[];
    await tester.pumpWidget(harness(
      controller: controller,
      policy: jsonPolicy,
      onAccepted: (_, __) async => accepted.add('yes'),
      onRejected: (_, result) => rejected.add(result.errorCode),
    ));

    final gesture = await tester.startGesture(tester.getCenter(find.text('drag')));
    await tester.pump();
    await gesture.moveTo(tester.getCenter(find.textContaining('zone:')));
    await tester.pump();
    expect(controller.snapshot.state, DndSessionState.dragging);
    expect(controller.snapshot.errorCode, DndRejectCode.itemKindNotAccepted);
    expect(find.text('zone:rejecting'), findsOneWidget);

    await gesture.up();
    await tester.pump();
    // Flutter never calls onAccept for a refused candidate; dragEnd cancels the session.
    expect(accepted, isEmpty);
    expect(controller.snapshot.state, DndSessionState.cancelled);
    expect(find.text('zone:idle'), findsOneWidget);
  });

  testWidgets('leaving the target returns to dragging; releasing outside cancels', (tester) async {
    final controller = OresDndController();
    await tester.pumpWidget(harness(controller: controller, policy: textPolicy, onAccepted: (_, __) async {}));
    final gesture = await tester.startGesture(tester.getCenter(find.text('drag')));
    await tester.pump();
    await gesture.moveTo(tester.getCenter(find.textContaining('zone:')));
    await tester.pump();
    expect(controller.snapshot.state, DndSessionState.overTarget);
    await gesture.moveTo(const Offset(5, 5));
    await tester.pump();
    expect(controller.snapshot.state, DndSessionState.dragging);
    expect(controller.snapshot.targetId, isNull);
    await gesture.up();
    await tester.pump();
    expect(controller.snapshot.state, DndSessionState.cancelled);
    expect(controller.result?.errorCode, 'cancelled');
  });

  test('zone state mirrors the browser data-ores-dnd-state attribute', () {
    expect(zoneStateFor(const DndSessionSnapshot(state: DndSessionState.overTarget, targetId: 'a'), 'a'), OresZoneState.accepting);
    expect(zoneStateFor(const DndSessionSnapshot(state: DndSessionState.overTarget, targetId: 'a'), 'b'), OresZoneState.dragging);
    expect(zoneStateFor(const DndSessionSnapshot(state: DndSessionState.dragging, targetId: 'a', errorCode: DndRejectCode.tooManyItems), 'a'), OresZoneState.rejecting);
    expect(zoneStateFor(const DndSessionSnapshot(state: DndSessionState.dropped, targetId: 'a'), 'a'), OresZoneState.dropped);
    expect(zoneStateFor(DndSessionSnapshot.idle, 'a'), OresZoneState.idle);
  });
}
