part of '../ores_dnd.dart';

/// Wire-level (structural) rules shared by every declaration: the bounded
/// scalars of contracts/main.tsp and the array/length bounds — exactly what
/// both schema authorities check. Mirrors src/rust/src/wire.rs.
abstract final class Wire {
  static const int safeIdMax = 128;
  static const int mediaTypeMax = 255;
  static const int errorCodeMax = 64;
  static const int itemNameMax = 255;
  static const int itemDataMaxChars = 1048576;
  static const int envelopeItemsMax = 64;
  static const int operationsMax = 3;
  static const int kindsMax = 4;
  static const int mediaPatternsMax = 64;
  static const int policyMaxItemsMax = 64;
  static const int traceIdMax = 128;
  static const int traceDescriptionMax = 512;
  static const int traceStepsMax = 256;

  static final RegExp _safeId = RegExp(r'^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$');
  static final RegExp _protocolId = RegExp(r'^ores\.dnd/v[1-9][0-9]{0,2}$');
  static final RegExp _mediaType = RegExp(r'^[a-z0-9][a-z0-9!#$&^_.+-]{0,126}/[a-z0-9][a-z0-9!#$&^_.+-]{0,126}$');
  static final RegExp _mediaTypePattern = RegExp(r'^[a-z0-9][a-z0-9!#$&^_.+-]{0,126}/(\*|[a-z0-9][a-z0-9!#$&^_.+-]{0,126})$');
  static final RegExp _traceparent = RegExp(r'^[0-9a-f]{2}-[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{2}$');
  static final RegExp _errorCode = RegExp(r'^[a-z0-9][a-z0-9-]{0,63}$');
  static final RegExp _traceId = RegExp(r'^[a-z0-9][a-z0-9._-]{0,127}$');

  /// JSON Schema `maxLength` counts code points (runes), not UTF-16 units.
  static int codePoints(String value) => value.runes.length;

  static bool isSafeId(Object? v) => v is String && _safeId.hasMatch(v);
  static bool isProtocolId(Object? v) => v is String && _protocolId.hasMatch(v);
  static bool isMediaType(Object? v) => v is String && v.length <= mediaTypeMax && _mediaType.hasMatch(v);
  static bool isMediaTypePattern(Object? v) => v is String && v.length <= mediaTypeMax && _mediaTypePattern.hasMatch(v);
  static bool isTraceparent(Object? v) => v is String && _traceparent.hasMatch(v);
  static bool isErrorCode(Object? v) => v is String && _errorCode.hasMatch(v);
  static bool isTraceId(Object? v) => v is String && _traceId.hasMatch(v);

  static String requireSafeId(Object? value, String label) {
    if (!isSafeId(value)) throw FormatException('$label must match ^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}\$');
    return value as String;
  }

  static String? optionalSafeId(Object? value, String label) => value == null ? null : requireSafeId(value, label);

  static void checkLength(int length, int min, int max, String label) {
    if (length < min || length > max) throw FormatException('$label must have between $min and $max entries');
  }
}
