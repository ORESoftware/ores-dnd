import 'package:ores_dnd/ores_dnd.dart';
import 'package:test/test.dart';

import 'helpers.dart';

void main() {
  final env = DndEnvelope.fromJson(
    readJson('instances/DndEnvelope/valid/text-copy.json'),
  );
  const base = DndDropPolicy(
    targetId: 'z',
    allowedOperations: [DndOperation.copy],
    acceptedKinds: [DndItemKind.text],
  );

  DndRejectCode? rejected(PolicyVerdict v) =>
      v is PolicyRejected ? v.errorCode : null;

  test('media type matching is case-insensitive and supports wildcards', () {
    expect(mediaTypeMatches('text/plain', 'TEXT/Plain; charset=utf-8'), isTrue);
    expect(mediaTypeMatches('text/*', 'text/markdown'), isTrue);
    expect(mediaTypeMatches('text/*', 'image/png'), isFalse);
    expect(mediaTypeMatches('text/*', 'text'), isFalse);
  });

  test('policy evaluation order is fixed', () {
    expect(
      rejected(
        evaluatePolicy(
          env,
          const DndDropPolicy(
            targetId: 'z',
            allowedOperations: [DndOperation.link],
            acceptedKinds: [DndItemKind.json],
          ),
        ),
      ),
      DndRejectCode.noCommonOperation,
    );
    expect(
      rejected(
        evaluatePolicy(
          env,
          const DndDropPolicy(
            targetId: 'z',
            allowedOperations: [DndOperation.copy],
            acceptedKinds: [DndItemKind.json],
          ),
        ),
      ),
      DndRejectCode.itemKindNotAccepted,
    );
    expect(
      rejected(
        evaluatePolicy(
          env,
          const DndDropPolicy(
            targetId: 'z',
            allowedOperations: [DndOperation.copy],
            acceptedKinds: [DndItemKind.text],
            acceptedMediaTypes: ['text/markdown'],
            maxTotalBytes: 1,
          ),
        ),
      ),
      DndRejectCode.mediaTypeNotAccepted,
    );
    expect(
      rejected(
        evaluatePolicy(
          env,
          const DndDropPolicy(
            targetId: 'z',
            allowedOperations: [DndOperation.copy],
            acceptedKinds: [DndItemKind.text],
            maxTotalBytes: 4,
          ),
        ),
      ),
      DndRejectCode.payloadTooLarge,
    );
    final accepted = evaluatePolicy(
      env,
      const DndDropPolicy(
        targetId: 'z',
        allowedOperations: [DndOperation.copy],
        acceptedKinds: [DndItemKind.text],
        maxTotalBytes: 5,
      ),
    );
    expect(
      accepted,
      isA<PolicyAccepted>().having(
        (a) => a.operation,
        'operation',
        DndOperation.copy,
      ),
    );
    expect(
      evaluatePolicy(env, base, preferred: DndOperation.move),
      isA<PolicyAccepted>().having(
        (a) => a.operation,
        'operation',
        DndOperation.copy,
      ),
    );
    expect(env.totalDataBytes, 5);
    expect(effectAllowedFor(env.allowedOperations), 'copyMove');
    expect(env.textFallback, 'hello');
  });

  test('policy decoding fails closed', () {
    expect(
      () => DndDropPolicy.fromJson({
        'targetId': 'z',
        'allowedOperations': ['copy'],
        'acceptedKinds': ['text'],
        'maxItems': 0,
      }),
      throwsFormatException,
    );
    expect(
      () => DndDropPolicy.fromJson({
        'targetId': 'z',
        'allowedOperations': ['copy'],
        'acceptedKinds': ['text'],
        'onDrop': 'x',
      }),
      throwsFormatException,
    );
    expect(
      () => DndDropPolicy.fromJson({
        'targetId': 'z',
        'allowedOperations': ['teleport'],
        'acceptedKinds': ['text'],
      }),
      throwsFormatException,
    );
    final policy = DndDropPolicy.fromJson({
      'targetId': 'z',
      'allowedOperations': ['copy'],
      'acceptedKinds': ['text'],
      'maxTotalBytes': 10,
    });
    expect(policy.maxTotalBytes, 10);
    expect(DndDropPolicy.fromJson(policy.toJson()).toJson(), policy.toJson());
    expect(
      const DndDropPolicy(
        targetId: '',
        allowedOperations: [],
        acceptedKinds: [],
      ).isValid,
      isFalse,
    );
  });
}
