library ores_dnd;

import 'dart:convert';
import 'package:flutter/widgets.dart';

const String oresDndProtocol = 'ores.dnd/v1';
const String oresDndMime = 'application/vnd.ores.dnd+json';
const int defaultMaxPayloadBytes = 1024 * 1024;
const int defaultMaxItems = 64;

enum DndOperation { copy, move, link }
enum DndItemKind { text, uri, json, bytes }
enum DndLifecyclePhase { dragStart, dragEnter, dragOver, dragLeave, drop, dragEnd }

extension DndOperationWire on DndOperation {
  String get wire => name;
  static DndOperation parse(Object? value) => DndOperation.values.firstWhere(
        (candidate) => candidate.wire == value,
        orElse: () => throw FormatException('unsupported drag operation: $value'),
      );
}

extension DndItemKindWire on DndItemKind {
  String get wire => name;
  static DndItemKind parse(Object? value) => DndItemKind.values.firstWhere(
        (candidate) => candidate.wire == value,
        orElse: () => throw FormatException('unsupported drag item kind: $value'),
      );
}

extension DndLifecyclePhaseWire on DndLifecyclePhase {
  String get wire => switch (this) {
        DndLifecyclePhase.dragStart => 'drag-start',
        DndLifecyclePhase.dragEnter => 'drag-enter',
        DndLifecyclePhase.dragOver => 'drag-over',
        DndLifecyclePhase.dragLeave => 'drag-leave',
        DndLifecyclePhase.drop => 'drop',
        DndLifecyclePhase.dragEnd => 'drag-end',
      };
}

String _requiredString(Object? value, String label, {bool allowEmpty = false}) {
  if (value is! String || (!allowEmpty && value.isEmpty)) {
    throw FormatException('$label must be a ${allowEmpty ? '' : 'non-empty '}string');
  }
  return value;
}

String? _optionalString(Object? value, String label) {
  if (value == null) return null;
  return _requiredString(value, label);
}

void _rejectUnknown(Map<String, Object?> value, Set<String> allowed, String label) {
  final unknown = value.keys.where((key) => !allowed.contains(key)).toList(growable: false);
  if (unknown.isNotEmpty) {
    throw FormatException('$label contains unsupported properties: ${unknown.join(', ')}');
  }
}

final class DndItem {
  const DndItem({required this.kind, required this.mediaType, required this.data, this.name});

  final DndItemKind kind;
  final String mediaType;
  final String data;
  final String? name;

  factory DndItem.fromJson(Map<String, Object?> json) {
    _rejectUnknown(json, const {'kind', 'mediaType', 'data', 'name'}, 'drag item');
    return DndItem(
      kind: DndItemKindWire.parse(json['kind']),
      mediaType: _requiredString(json['mediaType'], 'drag item mediaType'),
      data: _requiredString(json['data'], 'drag item data', allowEmpty: true),
      name: _optionalString(json['name'], 'drag item name'),
    );
  }

  Map<String, Object?> toJson() => {
        'kind': kind.wire,
        'mediaType': mediaType,
        'data': data,
        if (name != null) 'name': name,
      };
}

final class DndEnvelope {
  const DndEnvelope({
    required this.protocol,
    required this.dragId,
    required this.sourceRuntime,
    required this.allowedOperations,
    required this.items,
    this.traceparent,
    this.formId,
  });

  final String protocol;
  final String dragId;
  final String sourceRuntime;
  final List<DndOperation> allowedOperations;
  final List<DndItem> items;
  final String? traceparent;
  final String? formId;

  factory DndEnvelope.fromJson(Map<String, Object?> json, {int maxItems = defaultMaxItems}) {
    _rejectUnknown(
      json,
      const {'protocol', 'dragId', 'sourceRuntime', 'allowedOperations', 'items', 'traceparent', 'formId'},
      'drag envelope',
    );
    final protocol = _requiredString(json['protocol'], 'protocol');
    if (protocol != oresDndProtocol) throw FormatException('unsupported drag protocol: $protocol');

    final rawOperations = json['allowedOperations'];
    if (rawOperations is! List || rawOperations.isEmpty) {
      throw const FormatException('allowedOperations must contain at least one operation');
    }
    final operations = <DndOperation>[];
    for (final value in rawOperations) {
      final op = DndOperationWire.parse(value);
      if (!operations.contains(op)) operations.add(op);
    }

    final rawItems = json['items'];
    if (rawItems is! List || rawItems.isEmpty) {
      throw const FormatException('items must contain at least one drag item');
    }
    if (rawItems.length > maxItems) {
      throw FormatException('too many drag items: ${rawItems.length} > $maxItems');
    }
    final items = rawItems.map((value) {
      if (value is! Map) throw const FormatException('drag item must be an object');
      return DndItem.fromJson(value.cast<String, Object?>());
    }).toList(growable: false);

    return DndEnvelope(
      protocol: protocol,
      dragId: _requiredString(json['dragId'], 'dragId'),
      sourceRuntime: _requiredString(json['sourceRuntime'], 'sourceRuntime'),
      allowedOperations: List.unmodifiable(operations),
      items: List.unmodifiable(items),
      traceparent: _optionalString(json['traceparent'], 'traceparent'),
      formId: _optionalString(json['formId'], 'formId'),
    );
  }

  Map<String, Object?> toJson() => {
        'protocol': protocol,
        'dragId': dragId,
        'sourceRuntime': sourceRuntime,
        'allowedOperations': allowedOperations.map((op) => op.wire).toList(growable: false),
        'items': items.map((item) => item.toJson()).toList(growable: false),
        if (traceparent != null) 'traceparent': traceparent,
        if (formId != null) 'formId': formId,
      };
}

final class DndDropResult {
  const DndDropResult({
    required this.dragId,
    required this.accepted,
    this.operation,
    this.targetId,
    this.errorCode,
  });

  final String dragId;
  final bool accepted;
  final DndOperation? operation;
  final String? targetId;
  final String? errorCode;
}

final class DndTelemetryEvent {
  const DndTelemetryEvent({
    required this.phase,
    required this.dragId,
    required this.sourceRuntime,
    required this.itemCount,
    this.operation,
    this.targetId,
  });

  final DndLifecyclePhase phase;
  final String dragId;
  final String sourceRuntime;
  final int itemCount;
  final DndOperation? operation;
  final String? targetId;

  Map<String, Object?> toJson() => {
        'phase': phase.wire,
        'dragId': dragId,
        'sourceRuntime': sourceRuntime,
        'itemCount': itemCount,
        if (operation != null) 'operation': operation!.wire,
        if (targetId != null) 'targetId': targetId,
      };
}

final class OresDndCodec {
  const OresDndCodec({
    this.maxPayloadBytes = defaultMaxPayloadBytes,
    this.maxItems = defaultMaxItems,
  });

  final int maxPayloadBytes;
  final int maxItems;

  DndEnvelope decode(String payload) {
    final bytes = utf8.encode(payload).length;
    if (bytes > maxPayloadBytes) {
      throw FormatException('drag payload too large: $bytes > $maxPayloadBytes bytes');
    }
    final decoded = jsonDecode(payload);
    if (decoded is! Map) throw const FormatException('drag envelope must be an object');
    return DndEnvelope.fromJson(decoded.cast<String, Object?>(), maxItems: maxItems);
  }

  String encode(DndEnvelope envelope) {
    final normalized = DndEnvelope.fromJson(envelope.toJson(), maxItems: maxItems);
    final payload = jsonEncode(normalized.toJson());
    final bytes = utf8.encode(payload).length;
    if (bytes > maxPayloadBytes) {
      throw FormatException('drag payload too large: $bytes > $maxPayloadBytes bytes');
    }
    return payload;
  }
}

DndOperation? negotiateOperation(
  List<DndOperation> source,
  List<DndOperation> target, {
  DndOperation? preferred,
}) {
  if (preferred != null && source.contains(preferred) && target.contains(preferred)) return preferred;
  for (final op in const [DndOperation.move, DndOperation.copy, DndOperation.link]) {
    if (source.contains(op) && target.contains(op)) return op;
  }
  return null;
}

DndTelemetryEvent telemetryFor(
  DndLifecyclePhase phase,
  DndEnvelope envelope, {
  DndOperation? operation,
  String? targetId,
}) =>
    DndTelemetryEvent(
      phase: phase,
      dragId: envelope.dragId,
      sourceRuntime: envelope.sourceRuntime,
      itemCount: envelope.items.length,
      operation: operation,
      targetId: targetId,
    );

abstract interface class OresOtelPort {
  Future<void> emitDndEvent(DndTelemetryEvent event);
}

abstract interface class OptoSyncPort {
  Future<void> persistAcceptedDrop(DndEnvelope envelope, DndDropResult result);
}

abstract interface class OresFormsPort {
  Future<void> applyAcceptedDrop(DndEnvelope envelope, DndDropResult result);
}

/// Inject a web/native WASM host without coupling this Dart package to a particular
/// wasm loader. Flutter Web typically wires this to `ores_dnd_wasm` JS glue; desktop
/// hosts may wire it to Wasmtime/Wasmer/FFI.
abstract interface class OresDndWasmPort {
  Future<String> normalizeEnvelopeJson(String payload);
}

Future<void> commitAcceptedDrop(
  DndEnvelope envelope,
  DndDropResult result, {
  OresFormsPort? forms,
  OptoSyncPort? optoSync,
  OresOtelPort? otel,
}) async {
  final safeEnvelope = DndEnvelope.fromJson(envelope.toJson());
  if (result.dragId != safeEnvelope.dragId) {
    throw const FormatException('drop result dragId does not match envelope');
  }
  if (!result.accepted) return;
  final operation = result.operation;
  if (operation == null || !safeEnvelope.allowedOperations.contains(operation)) {
    throw const FormatException('accepted drop must use a source-allowed operation');
  }
  await forms?.applyAcceptedDrop(safeEnvelope, result);
  await optoSync?.persistAcceptedDrop(safeEnvelope, result);
  await otel?.emitDndEvent(telemetryFor(
    DndLifecyclePhase.drop,
    safeEnvelope,
    operation: operation,
    targetId: result.targetId,
  ));
}

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
