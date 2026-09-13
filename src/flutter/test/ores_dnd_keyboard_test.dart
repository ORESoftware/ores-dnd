import 'package:flutter_test/flutter_test.dart';
import 'package:ores_dnd_flutter/ores_dnd_flutter.dart';
import 'package:ores_dnd_flutter/ores_dnd_keyboard.dart';

DndEnvelope envelope() => const DndEnvelope(
  protocol: oresDndProtocol,
  dragId: 'flutter-keyboard-1',
  sourceRuntime: 'flutter-keyboard-test',
  allowedOperations: [DndOperation.copy, DndOperation.move],
  items: [
    DndItem(
      kind: DndItemKind.text,
      mediaType: 'text/plain',
      data: 'private-drag-data',
    ),
  ],
);

const target = DndDropPolicy(
  targetId: 'timeline',
  allowedOperations: [DndOperation.copy, DndOperation.move],
  acceptedKinds: [DndItemKind.text],
);

void main() {
  test('Flutter driver preserves notifier updates through keyboard lifecycle', () {
    final flutter = OresDndController();
    var notifications = 0;
    flutter.addListener(() => notifications++);
    final announcements = <DndKeyboardAnnouncement>[];
    final keyboard = createOresKeyboardController(
      controller: flutter,
      targets: const [target],
      announce: announcements.add,
    );

    expect(keyboard.start(envelope()).state, DndSessionState.dragging);
    expect(
      keyboard.move(1, preferred: DndOperation.copy).state,
      DndSessionState.overTarget,
    );
    final result = keyboard.drop();

    expect(result?.accepted, isTrue);
    expect(result?.operation, DndOperation.copy);
    expect(flutter.snapshot.state, DndSessionState.dropped);
    expect(notifications, 3);
    expect(
      announcements.map((value) => value.kind),
      [
        DndKeyboardAnnouncementKind.started,
        DndKeyboardAnnouncementKind.targetAccepted,
        DndKeyboardAnnouncementKind.dropped,
      ],
    );
    flutter.dispose();
  });

  test('Flutter keyboard driver cancellation uses the same session result', () {
    final flutter = OresDndController();
    final keyboard = createOresKeyboardController(
      controller: flutter,
      targets: const [target],
    );

    keyboard.start(envelope());
    keyboard.move(1);
    final result = keyboard.cancel();

    expect(result?.accepted, isFalse);
    expect(result?.errorCode, DndRejectCode.cancelled.wire);
    expect(flutter.snapshot.state, DndSessionState.cancelled);
    flutter.dispose();
  });
}
