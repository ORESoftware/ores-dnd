import 'dart:convert';

import 'package:ores_dnd/ores_dnd.dart';
import 'package:test/test.dart';

class _Recorder implements OresOtelPort {
  final events = <DndTelemetryEvent>[];

  @override
  Future<void> emitDndEvent(DndTelemetryEvent event) async => events.add(event);
}

DndEnvelope envelope(String id) => DndEnvelope(
      protocol: oresDndProtocol,
      dragId: id,
      sourceRuntime: 'dart-keyboard-test',
      allowedOperations: const [DndOperation.copy, DndOperation.move],
      items: const [
        DndItem(
          kind: DndItemKind.text,
          mediaType: 'text/plain',
          data: 'SECRET-DRAG-DATA',
        ),
      ],
    );

const textTarget = DndDropPolicy(
  targetId: 'text-zone',
  allowedOperations: [DndOperation.copy, DndOperation.move],
  acceptedKinds: [DndItemKind.text],
);

const jsonTarget = DndDropPolicy(
  targetId: 'json-zone',
  allowedOperations: [DndOperation.copy],
  acceptedKinds: [DndItemKind.json],
);

void main() {
  test('keyboard controller reuses session policy order and wraps targets', () {
    final session = DndSession();
    final announcements = <DndKeyboardAnnouncement>[];
    final targets = <String>[];
    final controller = DndKeyboardController.forSession(
      session: session,
      targets: const [textTarget, jsonTarget],
      announce: announcements.add,
      onTargetChange: (targetId, _) => targets.add(targetId),
    );

    expect(
      controller.start(envelope('drag-1')).state,
      DndSessionState.dragging,
    );
    expect(controller.move(1).state, DndSessionState.overTarget);
    expect(session.snapshot.operation, DndOperation.move);
    expect(controller.move(1).state, DndSessionState.dragging);
    expect(session.snapshot.errorCode, DndRejectCode.itemKindNotAccepted);
    expect(
      controller.move(1, preferred: DndOperation.copy).state,
      DndSessionState.overTarget,
    );
    final result = controller.drop();
    expect(result?.accepted, isTrue);
    expect(result?.operation, DndOperation.copy);
    expect(targets, ['text-zone', 'json-zone', 'text-zone']);
    expect(announcements.map((value) => value.kind), [
      DndKeyboardAnnouncementKind.started,
      DndKeyboardAnnouncementKind.targetAccepted,
      DndKeyboardAnnouncementKind.targetRejected,
      DndKeyboardAnnouncementKind.targetAccepted,
      DndKeyboardAnnouncementKind.dropped,
    ]);
    expect(
      jsonEncode(announcements.map((a) => a.kind.name).toList()),
      isNot(contains('SECRET-DRAG-DATA')),
    );
  });

  test(
    'cancel telemetry and announcements never include dragged data',
    () async {
      final recorder = _Recorder();
      final announcements = <DndKeyboardAnnouncement>[];
      final controller = DndKeyboardController.forSession(
        session: DndSession(),
        targets: const [textTarget],
        otel: recorder,
        announce: announcements.add,
      );
      controller.start(envelope('drag-2'));
      controller.move(1);
      final result = controller.cancel();
      await Future<void>.delayed(Duration.zero);

      expect(result?.accepted, isFalse);
      expect(result?.errorCode, DndRejectCode.cancelled.wire);
      expect(recorder.events.map((e) => e.phase), [
        DndLifecyclePhase.dragStart,
        DndLifecyclePhase.dragEnter,
        DndLifecyclePhase.dragEnd,
      ]);
      expect(
        jsonEncode(recorder.events.map((e) => e.toJson()).toList()),
        isNot(contains('SECRET-DRAG-DATA')),
      );
      expect(announcements.last.kind, DndKeyboardAnnouncementKind.cancelled);
    },
  );

  test('duplicate targets and invalid move step fail closed', () {
    expect(
      () => DndKeyboardController.forSession(
        session: DndSession(),
        targets: const [textTarget, textTarget],
      ),
      throwsFormatException,
    );
    final controller = DndKeyboardController.forSession(
      session: DndSession(),
      targets: const [textTarget],
    )..start(envelope('drag-3'));
    expect(() => controller.move(2), throwsArgumentError);
  });
}
