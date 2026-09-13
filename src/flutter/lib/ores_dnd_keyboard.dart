/// Flutter bridge for the pure-Dart keyboard DnD controller.
///
/// The bridge delegates every transition to [OresDndController], preserving
/// ChangeNotifier updates while the pure-Dart [DndKeyboardController] remains
/// the only keyboard navigation implementation. Focus, key bindings and
/// accessibility wording remain widget/application responsibilities.
library;

import 'package:ores_dnd/ores_dnd.dart';

import 'ores_dnd_flutter.dart';

export 'package:ores_dnd/ores_dnd.dart'
    show
        DndKeyboardAnnouncement,
        DndKeyboardAnnouncementKind,
        DndKeyboardController,
        DndKeyboardDropAccepted,
        DndKeyboardDropRejected,
        DndKeyboardTargetChanged,
        DndSessionDriver;

final class OresDndKeyboardDriver implements DndSessionDriver {
  OresDndKeyboardDriver(this.controller);

  final OresDndController controller;

  @override
  DndSessionSnapshot get snapshot => controller.snapshot;

  @override
  DndEnvelope? get envelope => controller.envelope;

  @override
  DndDropResult? get result => controller.result;

  @override
  DndSessionSnapshot apply(DndSessionInput input) => controller.apply(input);
}

DndKeyboardController createOresKeyboardController({
  required OresDndController controller,
  required List<DndDropPolicy> targets,
  OresOtelPort? otel,
  DndKeyboardAnnounce? announce,
  DndKeyboardTargetChanged? onTargetChange,
  DndKeyboardDropAccepted? onDrop,
  DndKeyboardDropRejected? onReject,
}) =>
    DndKeyboardController(
      driver: OresDndKeyboardDriver(controller),
      targets: targets,
      otel: otel,
      announce: announce,
      onTargetChange: onTargetChange,
      onDrop: onDrop,
      onReject: onReject,
    );
