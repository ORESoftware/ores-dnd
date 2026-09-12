part of '../ores_dnd.dart';

/// Every declaration the contract admits, in TypeSpec order.
const List<String> declarations = [
  'DndOperation',
  'DndItemKind',
  'DndLifecyclePhase',
  'DndRejectCode',
  'DndSessionState',
  'DndSessionInputKind',
  'SafeId',
  'ProtocolId',
  'MediaType',
  'MediaTypePattern',
  'Traceparent',
  'ErrorCode',
  'DndItem',
  'DndEnvelope',
  'DndDropResult',
  'DndTelemetryEvent',
  'DndDropPolicy',
  'DndSessionInput',
  'DndSessionSnapshot',
  'DndSessionTrace',
];

Object? _scalar(Object? value, String name, bool Function(Object?) ok) {
  if (!ok(value)) throw FormatException('$name rejected: $value');
  return value;
}

Map<String, Object?> _object(Object? value, String label) {
  if (value is! Map) throw FormatException('$label must be an object');
  return value.cast<String, Object?>();
}

/// Structural decode (closed enums, no unknown properties, contract bounds)
/// for the named declaration; `DndEnvelope` additionally runs semantic
/// validation. Throws [FormatException] on rejection.
Object? decodeDeclaration(String declaration, String json) {
  final name = declaration.contains('.')
      ? declaration.substring(declaration.lastIndexOf('.') + 1)
      : declaration;
  final Object? value = jsonDecode(json);
  return switch (name) {
    'DndOperation' => DndOperationWire.parse(value),
    'DndItemKind' => DndItemKindWire.parse(value),
    'DndLifecyclePhase' => DndLifecyclePhaseWire.parse(value),
    'DndRejectCode' => DndRejectCode.parse(value),
    'DndSessionState' => DndSessionState.parse(value),
    'DndSessionInputKind' => DndSessionInputKind.parse(value),
    'SafeId' => _scalar(value, name, Wire.isSafeId),
    'ProtocolId' => _scalar(value, name, Wire.isProtocolId),
    'MediaType' => _scalar(value, name, Wire.isMediaType),
    'MediaTypePattern' => _scalar(value, name, Wire.isMediaTypePattern),
    'Traceparent' => _scalar(value, name, Wire.isTraceparent),
    'ErrorCode' => _scalar(value, name, Wire.isErrorCode),
    'DndItem' => DndItem.fromJson(_object(value, 'drag item')),
    'DndEnvelope' => DndEnvelope.fromJson(_object(value, 'drag envelope')),
    'DndDropResult' => DndDropResult.fromJson(_object(value, 'drop result')),
    'DndTelemetryEvent' => DndTelemetryEvent.fromJson(
      _object(value, 'telemetry event'),
    ),
    'DndDropPolicy' => DndDropPolicy.fromJson(_object(value, 'drop policy')),
    'DndSessionInput' => DndSessionInput.fromJson(
      _object(value, 'session input'),
    ),
    'DndSessionSnapshot' => DndSessionSnapshot.fromJson(
      _object(value, 'session snapshot'),
    ),
    'DndSessionTrace' => DndSessionTrace.fromJson(
      _object(value, 'session trace'),
    ),
    _ => throw FormatException('unknown declaration: $declaration'),
  };
}
