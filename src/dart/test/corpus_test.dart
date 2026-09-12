import 'package:ores_dnd/ores_dnd.dart';
import 'package:test/test.dart';

import 'helpers.dart';

void main() {
  final all = corpus();

  test(
    'every corpus instance gets the declared verdict from the Dart decoder',
    () {
      expect(all.length, greaterThanOrEqualTo(70));
      final failures = <String>[];
      for (final c in all) {
        expect(
          declarations,
          contains(c.declaration),
          reason: 'unknown declaration dir ${c.declaration}',
        );
        var verdict = 'accepted';
        try {
          decodeDeclaration(c.declaration, c.json);
        } on FormatException {
          verdict = 'rejected';
        }
        if (verdict != c.expectation)
          failures.add(
            '${c.declaration}/${c.expectation}/${c.file}: dart said $verdict',
          );
      }
      expect(failures, isEmpty);
    },
  );

  test('every declaration has valid and invalid coverage', () {
    for (final declaration in declarations) {
      expect(
        all.any(
          (c) => c.declaration == declaration && c.expectation == 'accepted',
        ),
        isTrue,
        reason: '$declaration valid',
      );
      expect(
        all.any(
          (c) => c.declaration == declaration && c.expectation == 'rejected',
        ),
        isTrue,
        reason: '$declaration invalid',
      );
    }
  });

  test('qualified declaration names are accepted', () {
    expect(
      decodeDeclaration('OresDnd.DndOperation', '"copy"'),
      DndOperation.copy,
    );
    expect(
      () => decodeDeclaration('OresDnd.Nope', '{}'),
      throwsFormatException,
    );
  });
}
