part of '../ores_dnd.dart';

/// Standard rejection codes, reported in the fixed evaluation order
/// operation → item kind → media type → item count → total bytes → form.
enum DndRejectCode {
  invalidEnvelope('invalid-envelope'),
  noCommonOperation('no-common-operation'),
  itemKindNotAccepted('item-kind-not-accepted'),
  mediaTypeNotAccepted('media-type-not-accepted'),
  tooManyItems('too-many-items'),
  payloadTooLarge('payload-too-large'),
  formMismatch('form-mismatch'),
  noActiveTarget('no-active-target'),
  targetMismatch('target-mismatch'),
  cancelled('cancelled');

  const DndRejectCode(this.wire);
  final String wire;

  static DndRejectCode parse(Object? value) => DndRejectCode.values.firstWhere(
        (candidate) => candidate.wire == value,
        orElse: () => throw FormatException('unsupported reject code: $value'),
      );
}

const int _int32Max = 2147483647;

int? _optionalPositiveInt(Object? value, String label) {
  if (value == null) return null;
  if (value is! int || value < 1 || value > _int32Max) throw FormatException('$label must be an integer >= 1');
  return value;
}

/// What a drop target accepts. Declared by the target, evaluated by every
/// runtime with the same rules, never mutated by a drag.
final class DndDropPolicy {
  const DndDropPolicy({
    required this.targetId,
    required this.allowedOperations,
    required this.acceptedKinds,
    this.acceptedMediaTypes,
    this.maxItems,
    this.maxTotalBytes,
    this.formId,
  });

  final String targetId;
  final List<DndOperation> allowedOperations;
  final List<DndItemKind> acceptedKinds;

  /// Exact media types or `type/*` wildcards. Null means any media type.
  final List<String>? acceptedMediaTypes;
  final int? maxItems;
  final int? maxTotalBytes;

  /// ores-forms binding; when both the policy and the envelope carry a formId they must match.
  final String? formId;

  factory DndDropPolicy.fromJson(Map<String, Object?> json) {
    _rejectUnknown(
      json,
      const {'targetId', 'allowedOperations', 'acceptedKinds', 'acceptedMediaTypes', 'maxItems', 'maxTotalBytes', 'formId'},
      'drop policy',
    );
    final rawOps = json['allowedOperations'];
    if (rawOps is! List) throw const FormatException('allowedOperations must be an array');
    Wire.checkLength(rawOps.length, 1, Wire.operationsMax, 'allowedOperations');
    final rawKinds = json['acceptedKinds'];
    if (rawKinds is! List) throw const FormatException('acceptedKinds must be an array');
    Wire.checkLength(rawKinds.length, 1, Wire.kindsMax, 'acceptedKinds');
    final rawMedia = json['acceptedMediaTypes'];
    if (rawMedia != null) {
      if (rawMedia is! List || rawMedia.any((v) => !Wire.isMediaTypePattern(v))) {
        throw const FormatException('acceptedMediaTypes must be canonical media types or type/* wildcards');
      }
      Wire.checkLength(rawMedia.length, 1, Wire.mediaPatternsMax, 'acceptedMediaTypes');
    }
    final maxItems = _optionalPositiveInt(json['maxItems'], 'maxItems');
    if (maxItems != null && maxItems > Wire.policyMaxItemsMax) throw const FormatException('maxItems must be an integer in 1..=64');
    return DndDropPolicy(
      targetId: Wire.requireSafeId(json['targetId'], 'targetId'),
      allowedOperations: List.unmodifiable(rawOps.map(DndOperationWire.parse)),
      acceptedKinds: List.unmodifiable(rawKinds.map(DndItemKindWire.parse)),
      acceptedMediaTypes: rawMedia == null ? null : List.unmodifiable((rawMedia as List).cast<String>()),
      maxItems: maxItems,
      maxTotalBytes: _optionalPositiveInt(json['maxTotalBytes'], 'maxTotalBytes'),
      formId: Wire.optionalSafeId(json['formId'], 'formId'),
    );
  }

  Map<String, Object?> toJson() => {
        'targetId': targetId,
        'allowedOperations': allowedOperations.map((op) => op.wire).toList(growable: false),
        'acceptedKinds': acceptedKinds.map((kind) => kind.wire).toList(growable: false),
        if (acceptedMediaTypes != null) 'acceptedMediaTypes': acceptedMediaTypes,
        if (maxItems != null) 'maxItems': maxItems,
        if (maxTotalBytes != null) 'maxTotalBytes': maxTotalBytes,
        if (formId != null) 'formId': formId,
      };

  /// Structural sanity beyond what decoding checks: the contract bounds.
  bool get isValid {
    try {
      DndDropPolicy.fromJson(toJson());
      return true;
    } on FormatException {
      return false;
    }
  }
}

/// `pattern` is an exact media type or a `type/*` wildcard; ASCII
/// case-insensitive, parameters after `;` ignored.
bool mediaTypeMatches(String pattern, String mediaType) {
  final media = mediaType.split(';').first.trim().toLowerCase();
  final p = pattern.trim().toLowerCase();
  if (p.endsWith('/*')) {
    final slash = media.indexOf('/');
    return slash > 0 && media.substring(0, slash) == p.substring(0, p.length - 2);
  }
  return media == p;
}

/// The outcome of evaluating a policy against an envelope.
sealed class PolicyVerdict {
  const PolicyVerdict();
}

final class PolicyAccepted extends PolicyVerdict {
  const PolicyAccepted(this.operation);
  final DndOperation operation;
}

final class PolicyRejected extends PolicyVerdict {
  const PolicyRejected(this.errorCode);
  final DndRejectCode errorCode;
}

/// Evaluate `policy` against `envelope`: operation → kind → media type → count → bytes → form.
PolicyVerdict evaluatePolicy(DndEnvelope envelope, DndDropPolicy policy, {DndOperation? preferred}) {
  final operation = negotiateOperation(envelope.allowedOperations, policy.allowedOperations, preferred: preferred);
  if (operation == null) return const PolicyRejected(DndRejectCode.noCommonOperation);
  if (envelope.items.any((item) => !policy.acceptedKinds.contains(item.kind))) {
    return const PolicyRejected(DndRejectCode.itemKindNotAccepted);
  }
  final patterns = policy.acceptedMediaTypes;
  if (patterns != null && !envelope.items.every((item) => patterns.any((pattern) => mediaTypeMatches(pattern, item.mediaType)))) {
    return const PolicyRejected(DndRejectCode.mediaTypeNotAccepted);
  }
  if (policy.maxItems != null && envelope.items.length > policy.maxItems!) {
    return const PolicyRejected(DndRejectCode.tooManyItems);
  }
  if (policy.maxTotalBytes != null && envelope.totalDataBytes > policy.maxTotalBytes!) {
    return const PolicyRejected(DndRejectCode.payloadTooLarge);
  }
  if (policy.formId != null && envelope.formId != null && policy.formId != envelope.formId) {
    return const PolicyRejected(DndRejectCode.formMismatch);
  }
  return PolicyAccepted(operation);
}
