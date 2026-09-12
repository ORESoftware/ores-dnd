part of '../ores_dnd.dart';

/// xorshift64* — the Dart mirror of `ores_dnd_core::fuzz::Xorshift64`
/// (bit-identical on the Dart VM, whose ints are 64-bit two's complement).
/// Not for `dart2js`, where ints are doubles; the fuzz tooling runs on the VM.
final class Xorshift64 {
  Xorshift64(int seed)
    : _state = seed == 0 ? -7046029254386353131 /* 0x9E3779B97F4A7C15 */ : seed;

  int _state;

  int nextU64() {
    var x = _state;
    x ^= x >>> 12;
    x ^= x << 25;
    x ^= x >>> 27;
    _state = x;
    return x * 0x2545F4914F6CDD1D;
  }

  /// Uniform in `0..n` (n > 0), from the high 32 bits.
  int below(int n) => (nextU64() >>> 32) % n;

  bool chance(int percent) => below(100) < percent;

  T pick<T>(List<T> items) => items[below(items.length)];
}

/// The shared fuzz vocabulary (identical to the Rust and TypeScript generators).
abstract final class Fuzz {
  static const ops = DndOperation.values; // copy, move, link
  static const kinds = DndItemKind.values; // text, uri, json, bytes
  static const media = [
    'text/plain',
    'text/markdown',
    'text/uri-list',
    'application/json',
    'image/png',
  ];
  static const mediaPatterns = [
    'text/plain',
    'text/*',
    'application/json',
    'image/*',
    'text/markdown',
    'application/*',
  ];
  static const targets = ['zone-a', 'zone-b', 'zone-c', 'zone-d'];
  static const forms = ['form-x', 'form-y'];

  static List<DndOperation> _opsSubset(Xorshift64 rng) {
    final out = ops.where((_) => rng.chance(55)).toList();
    if (out.isEmpty) out.add(rng.pick(ops));
    return out;
  }

  static DndEnvelope randomEnvelope(Xorshift64 rng, int n) {
    final itemCount = 1 + rng.below(3);
    final items = <DndItem>[];
    for (var i = 0; i < itemCount; i++) {
      final m = rng.pick(media);
      final kind = switch (m) {
        'text/uri-list' => DndItemKind.uri,
        'application/json' => DndItemKind.json,
        'image/png' => DndItemKind.bytes,
        _ => DndItemKind.text,
      };
      items.add(DndItem(kind: kind, mediaType: m, data: 'x' * rng.below(9)));
    }
    final allowed = _opsSubset(rng);
    return DndEnvelope(
      protocol: rng.chance(8) ? 'ores.dnd/v2' : oresDndProtocol,
      dragId: 'drag-${n.toString().padLeft(4, '0')}',
      sourceRuntime: 'fuzz',
      allowedOperations: allowed,
      items: items,
      formId: rng.chance(30) ? rng.pick(forms) : null,
    );
  }

  static DndDropPolicy randomPolicy(Xorshift64 rng) {
    final k = kinds.where((_) => rng.chance(55)).toList();
    if (k.isEmpty) k.add(rng.pick(kinds));
    final targetId = rng.pick(targets);
    final allowed = _opsSubset(rng);
    List<String>? patterns;
    if (rng.chance(35)) {
      patterns = mediaPatterns.where((_) => rng.chance(40)).toList();
      if (patterns.isEmpty) patterns.add(rng.pick(mediaPatterns));
    }
    final maxItems = rng.chance(30) ? 1 + rng.below(3) : null;
    final maxTotalBytes = rng.chance(30) ? 1 + rng.below(16) : null;
    final formId = rng.chance(25) ? rng.pick(forms) : null;
    return DndDropPolicy(
      targetId: targetId,
      allowedOperations: allowed,
      acceptedKinds: k,
      acceptedMediaTypes: patterns,
      maxItems: maxItems,
      maxTotalBytes: maxTotalBytes,
      formId: formId,
    );
  }

  static DndSessionInput randomInput(Xorshift64 rng, List<int> counter) {
    final roll = rng.below(100);
    if (roll <= 17) {
      counter[0] += 1;
      return DndSessionInput.start(randomEnvelope(rng, counter[0]));
    }
    if (roll <= 52) {
      final policy = randomPolicy(rng);
      final preferred = rng.chance(30) ? rng.pick(ops) : null;
      return DndSessionInput.enter(policy, preferred: preferred);
    }
    if (roll <= 67) return DndSessionInput.leave(rng.pick(targets));
    if (roll <= 85) return DndSessionInput.drop(rng.pick(targets));
    if (roll <= 92) return const DndSessionInput.cancel();
    return const DndSessionInput.end();
  }

  /// A full random sequence, always beginning with a valid start.
  static List<DndSessionInput> randomSequence(int seed, int steps) {
    final rng = Xorshift64(seed);
    final counter = [1];
    final first = randomEnvelope(rng, counter[0]);
    final out = <DndSessionInput>[
      DndSessionInput.start(
        DndEnvelope(
          protocol: oresDndProtocol,
          dragId: first.dragId,
          sourceRuntime: first.sourceRuntime,
          allowedOperations: first.allowedOperations,
          items: first.items,
          formId: first.formId,
        ),
      ),
    ];
    while (out.length < steps) {
      out.add(randomInput(rng, counter));
    }
    return out;
  }
}
