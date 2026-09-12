import 'package:flutter/widgets.dart';
import 'package:ores_dnd/ores_dnd.dart';

export 'package:ores_dnd/ores_dnd.dart';
export 'package:ores_dnd/ores_dnd_reactive.dart';
export 'package:ores_dnd/ores_dnd_reactive_effects.dart';

class OresDraggable extends StatelessWidget {
  const OresDraggable({
    required this.envelope,
    required this.child,
    required this.feedback,
    this.childWhenDragging,
    this.codec = const OresDndCodec(),
    super.key,
  });

  final DndEnvelope envelope;
  final Widget child;
  final Widget feedback;
  final Widget? childWhenDragging;
  final OresDndCodec codec;

  @override
  Widget build(BuildContext context) => Draggable<String>(
        data: codec.encode(envelope),
        feedback: feedback,
        childWhenDragging: childWhenDragging,
        child: child,
      );
}

typedef OresDropAccepted = Future<void> Function(DndEnvelope envelope, DndOperation operation);

class OresDragTarget extends StatelessWidget {
  const OresDragTarget({
    required this.targetId,
    required this.allowedOperations,
    required this.builder,
    required this.onAccepted,
    this.codec = const OresDndCodec(),
    super.key,
  });

  final String targetId;
  final List<DndOperation> allowedOperations;
  final Widget Function(BuildContext context, bool hovering) builder;
  final OresDropAccepted onAccepted;
  final OresDndCodec codec;

  @override
  Widget build(BuildContext context) => DragTarget<String>(
        onWillAcceptWithDetails: (details) {
          try {
            final envelope = codec.decode(details.data);
            return negotiateOperation(envelope.allowedOperations, allowedOperations) != null;
          } on FormatException {
            return false;
          }
        },
        onAcceptWithDetails: (details) async {
          final envelope = codec.decode(details.data);
          final operation = negotiateOperation(envelope.allowedOperations, allowedOperations);
          if (operation == null) return;
          await onAccepted(envelope, operation);
        },
        builder: (context, candidates, rejected) => builder(context, candidates.isNotEmpty),
      );
}
