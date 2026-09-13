import 'dart:convert';

import 'package:ores_dnd/ores_dnd.dart';
import 'package:test/test.dart';

import 'helpers.dart';

void main() {
  test('xorshift64* reproduces the Rust reference values', () {
    final rng = Xorshift64(7);
    expect(
      [rng.nextU64(), rng.nextU64(), rng.nextU64()],
      [-3315863738710675794, -1322813130528676178, 1648209070578717474],
    );
    final b = Xorshift64(7);
    expect([b.below(100), b.below(100), b.below(100)], [7, 63, 78]);
    expect(
      jsonEncode(Fuzz.randomSequence(42, 10).map((i) => i.toJson()).toList()),
      jsonEncode(Fuzz.randomSequence(42, 10).map((i) => i.toJson()).toList()),
    );
  });

  test(
    'the Dart generator reproduces every Rust-generated fuzz trace input for input',
    () {
      final fuzz = corpus()
          .where(
            (c) =>
                c.declaration == 'DndSessionTrace' &&
                c.expectation == 'accepted' &&
                c.file.startsWith('fuzz-'),
          )
          .map(
            (c) =>
                decodeDeclaration('DndSessionTrace', c.json) as DndSessionTrace,
          )
          .toList();
      expect(fuzz.length, greaterThanOrEqualTo(40));
      for (final trace in fuzz) {
        final seed = int.parse(trace.id.substring('fuzz-'.length), radix: 16);
        final generated = Fuzz.randomSequence(
          seed,
          trace.inputs.length,
        ).map((i) => jsonEncode(i.toJson())).toList();
        expect(
          generated,
          trace.inputs.map((i) => jsonEncode(i.toJson())).toList(),
          reason: trace.id,
        );
      }
    },
  );

  test('session invariants hold over random sequences', () {
    final seeds = Xorshift64(0xc0ffee00);
    for (var run = 0; run < 1500; run++) {
      final seed = seeds.nextU64();
      final session = DndSession();
      var previous = DndSessionSnapshot.idle;
      final inputs = Fuzz.randomSequence(seed, 24);
      for (var step = 0; step < inputs.length; step++) {
        final input = inputs[step];
        final snapshot = session.apply(input);
        final ctx =
            'seed ${seed.toRadixString(16)} step $step ${input.kind.wire} -> $snapshot';
        expect(
          DndSessionSnapshot.fromJson(snapshot.toJson()),
          snapshot,
          reason: ctx,
        );
        if (previous.state.isTerminal &&
            input.kind != DndSessionInputKind.start) {
          expect(snapshot, previous, reason: ctx);
        }
        switch (snapshot.state) {
          case DndSessionState.idle:
            expect(
              snapshot.dragId == null &&
                  snapshot.targetId == null &&
                  snapshot.operation == null,
              isTrue,
              reason: ctx,
            );
            expect(session.envelope, isNull, reason: ctx);
          case DndSessionState.dragging:
            expect(
              snapshot.dragId != null && snapshot.operation == null,
              isTrue,
              reason: ctx,
            );
            expect(
              snapshot.targetId != null,
              snapshot.errorCode != null,
              reason: ctx,
            );
          case DndSessionState.overTarget:
            expect(
              session.envelope!.allowedOperations,
              contains(snapshot.operation),
              reason: ctx,
            );
            expect(
              snapshot.targetId != null && snapshot.errorCode == null,
              isTrue,
              reason: ctx,
            );
            if (input.kind == DndSessionInputKind.enter &&
                snapshot != previous) {
              expect(
                input.policy!.allowedOperations,
                contains(snapshot.operation),
                reason: ctx,
              );
              expect(snapshot.targetId, input.policy!.targetId, reason: ctx);
            }
          case DndSessionState.dropped:
            expect(
              snapshot.operation != null &&
                  snapshot.targetId != null &&
                  snapshot.errorCode == null,
              isTrue,
              reason: ctx,
            );
            expect(session.result!.accepted, isTrue, reason: ctx);
            if (snapshot != previous) {
              expect(input.kind, DndSessionInputKind.drop, reason: ctx);
              expect(previous.state, DndSessionState.overTarget, reason: ctx);
              expect(previous.targetId, snapshot.targetId, reason: ctx);
              expect(input.targetId, snapshot.targetId, reason: ctx);
            }
          case DndSessionState.cancelled:
            expect(
              snapshot.errorCode != null && snapshot.operation == null,
              isTrue,
              reason: ctx,
            );
            expect(session.result!.accepted, isFalse, reason: ctx);
            expect(
              session.result!.errorCode,
              snapshot.errorCode!.wire,
              reason: ctx,
            );
        }
        if (input.kind != DndSessionInputKind.start &&
            !previous.state.isTerminal &&
            previous.state != DndSessionState.idle) {
          expect(snapshot.dragId, previous.dragId, reason: ctx);
        }
        previous = snapshot;
      }
    }
  });
}
