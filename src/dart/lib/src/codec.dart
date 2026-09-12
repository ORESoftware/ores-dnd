part of '../ores_dnd.dart';

const String oresDndProtocol = 'ores.dnd/v1';
const String oresDndMime = 'application/vnd.ores.dnd+json';
const int defaultMaxPayloadBytes = 1024 * 1024;
const int defaultMaxItems = 64;

enum DndOperation { copy, move, link }

enum DndItemKind { text, uri, json, bytes }

enum DndLifecyclePhase {
  dragStart,
  dragEnter,
  dragOver,
  dragLeave,
  drop,
  dragEnd,
}

extension DndOperationWire on DndOperation {
  String get wire => name;
  static DndOperation parse(Object? value) => DndOperation.values.firstWhere(
        (candidate) => candidate.wire == value,
        orElse: () =>
            throw FormatException('unsupported drag operation: $value'),
      );
}

extension DndItemKindWire on DndItemKind {
  String get wire => name;
  static DndItemKind parse(Object? value) => DndItemKind.values.firstWhere(
        (candidate) => candidate.wire == value,
        orElse: () =>
            throw FormatException('unsupported drag item kind: $value'),
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
  static DndLifecyclePhase parse(Object? value) =>
      DndLifecyclePhase.values.firstWhere(
        (candidate) => candidate.wire == value,
        orElse: () =>
            throw FormatException('unsupported lifecycle phase: $value'),
      );
}

String _requiredString(Object? value, String label, {bool allowEmpty = false}) {
  if (value is! String || (!allowEmpty && value.isEmpty)) {
    throw FormatException(
      '$label must be a ${allowEmpty ? '' : 'non-empty '}string',
    );
  }
  return value;
}

String? _optionalString(
  Object? value,
  String label, {
  bool allowEmpty = false,
}) {
  if (value == null) return null;
  return _requiredString(value, label, allowEmpty: allowEmpty);
}

void _rejectUnknown(
  Map<String, Object?> value,
  Set<String> allowed,
  String label,
) {
  final unknown =
      value.keys.where((key) => !allowed.contains(key)).toList(growable: false);
  if (unknown.isNotEmpty) {
    throw FormatException(
      '$label contains unsupported properties: ${unknown.join(', ')}',
    );
  }
}

final class DndItem {
  const DndItem({
    required this.kind,
    required this.mediaType,
    required this.data,
    this.name,
  });

  final DndItemKind kind;
  final String mediaType;
  final String data;
  final String? name;

  /// Decode an item: closed enums, canonical media type, bounded data/name.
  factory DndItem.fromJson(Map<String, Object?> json) {
    _rejectUnknown(
        json,
        const {
          'kind',
          'mediaType',
          'data',
          'name',
        },
        'drag item');
    final mediaType = json['mediaType'];
    if (!Wire.isMediaType(mediaType)) {
      throw const FormatException(
        'drag item mediaType must be a canonical lowercase type/subtype',
      );
    }
    final data = _requiredString(
      json['data'],
      'drag item data',
      allowEmpty: true,
    );
    if (Wire.codePoints(data) > Wire.itemDataMaxChars) {
      throw const FormatException(
        'drag item data exceeds the contract maximum length',
      );
    }
    final name = _optionalString(json['name'], 'drag item name');
    if (name != null && Wire.codePoints(name) > Wire.itemNameMax) {
      throw const FormatException('drag item name must be 1..=255 characters');
    }
    return DndItem(
      kind: DndItemKindWire.parse(json['kind']),
      mediaType: mediaType as String,
      data: data,
      name: name,
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

  /// Decode an envelope. By default the runtime rules apply (accepted
  /// protocol, non-empty operations/items/strings, item limit) on top of the
  /// structural contract; `structural: true` checks only what the schema
  /// authorities check — used when an envelope is embedded in another
  /// declaration such as `DndSessionInput`.
  factory DndEnvelope.fromJson(
    Map<String, Object?> json, {
    int maxItems = defaultMaxItems,
    bool structural = false,
  }) {
    _rejectUnknown(
        json,
        const {
          'protocol',
          'dragId',
          'sourceRuntime',
          'allowedOperations',
          'items',
          'traceparent',
          'formId',
        },
        'drag envelope');
    final protocol = json['protocol'];
    if (!Wire.isProtocolId(protocol)) {
      throw FormatException('malformed drag protocol tag: $protocol');
    }
    if (!structural && protocol != oresDndProtocol) {
      throw FormatException('unsupported drag protocol: $protocol');
    }

    final rawOperations = json['allowedOperations'];
    if (rawOperations is! List) {
      throw const FormatException('allowedOperations must be an array');
    }
    Wire.checkLength(
      rawOperations.length,
      1,
      Wire.operationsMax,
      'allowedOperations',
    );
    final operations = <DndOperation>[];
    for (final value in rawOperations) {
      final op = DndOperationWire.parse(value);
      if (!operations.contains(op)) operations.add(op);
    }

    final rawItems = json['items'];
    if (rawItems is! List) {
      throw const FormatException('items must be an array');
    }
    Wire.checkLength(rawItems.length, 1, Wire.envelopeItemsMax, 'items');
    if (!structural && rawItems.length > maxItems) {
      throw FormatException(
        'too many drag items: ${rawItems.length} > $maxItems',
      );
    }
    final items = rawItems.map((value) {
      if (value is! Map) {
        throw const FormatException('drag item must be an object');
      }
      return DndItem.fromJson(value.cast<String, Object?>());
    }).toList(growable: false);

    final traceparent = json['traceparent'];
    if (traceparent != null && !Wire.isTraceparent(traceparent)) {
      throw const FormatException(
        'traceparent must be a W3C trace-context value',
      );
    }
    return DndEnvelope(
      protocol: protocol as String,
      dragId: Wire.requireSafeId(json['dragId'], 'dragId'),
      sourceRuntime: Wire.requireSafeId(json['sourceRuntime'], 'sourceRuntime'),
      allowedOperations: List.unmodifiable(operations),
      items: List.unmodifiable(items),
      traceparent: traceparent as String?,
      formId: Wire.optionalSafeId(json['formId'], 'formId'),
    );
  }

  /// Total UTF-8 byte length of all item data (the `maxTotalBytes` measure).
  int get totalDataBytes =>
      items.fold(0, (sum, item) => sum + utf8.encode(item.data).length);

  /// The plain-text fallback emitted next to the ores MIME type, if any.
  String? get textFallback {
    for (final item in items) {
      if (item.kind == DndItemKind.text && item.mediaType == 'text/plain') {
        return item.data;
      }
    }
    return null;
  }

  Map<String, Object?> toJson() => {
        'protocol': protocol,
        'dragId': dragId,
        'sourceRuntime': sourceRuntime,
        'allowedOperations':
            allowedOperations.map((op) => op.wire).toList(growable: false),
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

  factory DndDropResult.fromJson(Map<String, Object?> json) {
    _rejectUnknown(
        json,
        const {
          'dragId',
          'accepted',
          'operation',
          'targetId',
          'errorCode',
        },
        'drop result');
    final accepted = json['accepted'];
    if (accepted is! bool) {
      throw const FormatException('accepted must be a boolean');
    }
    final operation = json['operation'];
    final errorCode = json['errorCode'];
    if (errorCode != null && !Wire.isErrorCode(errorCode)) {
      throw const FormatException(
        'errorCode must be lowercase kebab-case (1..=64)',
      );
    }
    return DndDropResult(
      dragId: Wire.requireSafeId(json['dragId'], 'dragId'),
      accepted: accepted,
      operation: operation == null ? null : DndOperationWire.parse(operation),
      targetId: Wire.optionalSafeId(json['targetId'], 'targetId'),
      errorCode: errorCode as String?,
    );
  }

  Map<String, Object?> toJson() => {
        'dragId': dragId,
        'accepted': accepted,
        if (operation != null) 'operation': operation!.wire,
        if (targetId != null) 'targetId': targetId,
        if (errorCode != null) 'errorCode': errorCode,
      };

  @override
  bool operator ==(Object other) =>
      other is DndDropResult &&
      other.dragId == dragId &&
      other.accepted == accepted &&
      other.operation == operation &&
      other.targetId == targetId &&
      other.errorCode == errorCode;

  @override
  int get hashCode =>
      Object.hash(dragId, accepted, operation, targetId, errorCode);

  @override
  String toString() => 'DndDropResult${toJson()}';
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

  factory DndTelemetryEvent.fromJson(Map<String, Object?> json) {
    _rejectUnknown(
        json,
        const {
          'phase',
          'dragId',
          'sourceRuntime',
          'itemCount',
          'operation',
          'targetId',
        },
        'telemetry event');
    final itemCount = json['itemCount'];
    if (itemCount is! int || itemCount < 0 || itemCount > 2147483647) {
      throw const FormatException('itemCount must be an int32 >= 0');
    }
    final operation = json['operation'];
    return DndTelemetryEvent(
      phase: DndLifecyclePhaseWire.parse(json['phase']),
      dragId: Wire.requireSafeId(json['dragId'], 'dragId'),
      sourceRuntime: Wire.requireSafeId(json['sourceRuntime'], 'sourceRuntime'),
      itemCount: itemCount,
      operation: operation == null ? null : DndOperationWire.parse(operation),
      targetId: Wire.optionalSafeId(json['targetId'], 'targetId'),
    );
  }

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
      throw FormatException(
        'drag payload too large: $bytes > $maxPayloadBytes bytes',
      );
    }
    final decoded = jsonDecode(payload);
    if (decoded is! Map) {
      throw const FormatException('drag envelope must be an object');
    }
    return DndEnvelope.fromJson(
      decoded.cast<String, Object?>(),
      maxItems: maxItems,
    );
  }

  String encode(DndEnvelope envelope) {
    final normalized = DndEnvelope.fromJson(
      envelope.toJson(),
      maxItems: maxItems,
    );
    final payload = jsonEncode(normalized.toJson());
    final bytes = utf8.encode(payload).length;
    if (bytes > maxPayloadBytes) {
      throw FormatException(
        'drag payload too large: $bytes > $maxPayloadBytes bytes',
      );
    }
    return payload;
  }
}

/// Deterministic negotiation order shared by every runtime.
const List<DndOperation> negotiationOrder = [
  DndOperation.move,
  DndOperation.copy,
  DndOperation.link,
];

/// The HTML5 `effectAllowed` keyword for a set of operations.
String effectAllowedFor(Iterable<DndOperation> ops) {
  final copy = ops.contains(DndOperation.copy);
  final move = ops.contains(DndOperation.move);
  final link = ops.contains(DndOperation.link);
  if (copy && move && link) return 'all';
  if (copy && move) return 'copyMove';
  if (copy && link) return 'copyLink';
  if (link && move) return 'linkMove';
  if (copy) return 'copy';
  if (move) return 'move';
  if (link) return 'link';
  return 'none';
}

DndOperation? negotiateOperation(
  List<DndOperation> source,
  List<DndOperation> target, {
  DndOperation? preferred,
}) {
  if (preferred != null &&
      source.contains(preferred) &&
      target.contains(preferred)) {
    return preferred;
  }
  for (final op in negotiationOrder) {
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
