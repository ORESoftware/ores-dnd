import 'package:ores_dnd/ores_dnd.dart';
import 'package:test/test.dart';

import 'helpers.dart';

void main() {
  final traces = corpus()
      .where((c) => c.declaration == 'DndSessionTrace' && c.expectation == 'accepted')
      .map((c) => decodeDeclaration('DndSessionTrace', c.json) as DndSessionTrace)
      .toList();

  test('every shared DndSessionTrace replays identically', () {
    expect(traces.length, greaterThanOrEqualTo(20));
    final divergences = traces.map(replayTrace).whereType<TraceDivergence>().map((d) => d.toString()).toList();
    expect(divergences, isEmpty);
  });

  test('trace replay detects a divergence', () {
    final trace = traces.firstWhere((t) => t.id == 'basic-drop');
    final broken = DndSessionTrace(
      id: trace.id,
      inputs: trace.inputs,
      expected: [
        ...trace.expected.take(2),
        DndSessionSnapshot(state: DndSessionState.dropped, dragId: 'drag-0001', targetId: 'zone-a', operation: DndOperation.link),
      ],
    );
    expect(replayTrace(broken)?.step, 2);
  });

  test('session exposes envelope, result and listeners', () {
    final envelope = DndEnvelope.fromJson(readJson('instances/DndEnvelope/valid/text-copy.json'));
    final session = DndSession();
    final seen = <String>[];
    final unsubscribe = session.subscribe((snapshot, input) => seen.add('${input.kind.wire}:${snapshot.state.wire}'));
    expect(session.result, isNull);
    session.apply(DndSessionInput.start(envelope));
    expect(session.envelope?.dragId, envelope.dragId);
    session.apply(DndSessionInput.enter(const DndDropPolicy(targetId: 'zone-a', allowedOperations: [DndOperation.move], acceptedKinds: [DndItemKind.text])));
    expect(session.snapshot.isOverAcceptingTarget, isTrue);
    session.apply(const DndSessionInput.drop('zone-a'));
    expect(session.result, const DndDropResult(dragId: 'drag-0001', accepted: true, operation: DndOperation.move, targetId: 'zone-a'));
    unsubscribe();
    session.apply(const DndSessionInput.cancel());
    expect(seen, ['start:dragging', 'enter:over-target', 'drop:dropped']);
  });

  test('malformed inputs are ignored', () {
    final envelope = DndEnvelope.fromJson(readJson('instances/DndEnvelope/valid/text-copy.json'));
    final session = DndSession()..apply(DndSessionInput.start(envelope));
    final before = session.snapshot;
    expect(session.apply(const DndSessionInput(kind: DndSessionInputKind.enter, targetId: 'z')), before);
    expect(session.apply(const DndSessionInput(kind: DndSessionInputKind.leave)), before);
    expect(
      session.apply(DndSessionInput.enter(const DndDropPolicy(targetId: 'z', allowedOperations: [DndOperation.copy], acceptedKinds: [DndItemKind.text], maxItems: 0))),
      before,
    );
  });
}
