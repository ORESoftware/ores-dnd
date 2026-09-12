// Deterministic random session generation — the TypeScript mirror of
// src/rust/src/fuzz.rs (same xorshift64*, same vocabulary, same probabilities)
// so every runtime fuzzes itself with identical sequences.
import { ORES_DND_PROTOCOL, type DndEnvelope, type DndItem, type DndItemKind, type DndOperation } from "./codec.js";
import type { DndDropPolicy } from "./policy.js";
import { inputs, type DndSessionInput } from "./session.js";

const MASK64 = (1n << 64n) - 1n;

/** xorshift64* over BigInt; reproduces the Rust generator bit for bit. */
export class Xorshift64 {
  #state: bigint;

  constructor(seed: bigint | number) {
    const s = BigInt(seed) & MASK64;
    this.#state = s === 0n ? 0x9e3779b97f4a7c15n : s;
  }

  nextU64(): bigint {
    let x = this.#state;
    x ^= x >> 12n;
    x ^= (x << 25n) & MASK64;
    x ^= x >> 27n;
    this.#state = x;
    return (x * 0x2545f4914f6cdd1dn) & MASK64;
  }

  below(n: number): number {
    return Number((this.nextU64() >> 32n) % BigInt(n));
  }

  chance(percent: number): boolean {
    return this.below(100) < percent;
  }

  pick<T>(items: readonly T[]): T {
    return items[this.below(items.length)]!;
  }
}

export const OPS: readonly DndOperation[] = ["copy", "move", "link"];
export const KINDS: readonly DndItemKind[] = ["text", "uri", "json", "bytes"];
export const MEDIA = ["text/plain", "text/markdown", "text/uri-list", "application/json", "image/png"] as const;
export const MEDIA_PATTERNS = ["text/plain", "text/*", "application/json", "image/*", "text/markdown", "application/*"] as const;
export const TARGETS = ["zone-a", "zone-b", "zone-c", "zone-d"] as const;
export const FORMS = ["form-x", "form-y"] as const;

function opsSubset(rng: Xorshift64): DndOperation[] {
  const ops = OPS.filter(() => rng.chance(55));
  if (ops.length === 0) ops.push(rng.pick(OPS));
  return ops;
}

export function randomEnvelope(rng: Xorshift64, n: number): DndEnvelope {
  const itemCount = 1 + rng.below(3);
  const items: DndItem[] = [];
  for (let i = 0; i < itemCount; i += 1) {
    const media = rng.pick(MEDIA);
    const kind: DndItemKind = media === "text/uri-list" ? "uri" : media === "application/json" ? "json" : media === "image/png" ? "bytes" : "text";
    items.push({ kind, mediaType: media, data: "x".repeat(rng.below(9)) });
  }
  const allowedOperations = opsSubset(rng);
  const envelope: DndEnvelope = {
    protocol: rng.chance(8) ? "ores.dnd/v2" : ORES_DND_PROTOCOL,
    dragId: `drag-${String(n).padStart(4, "0")}`,
    sourceRuntime: "fuzz",
    allowedOperations,
    items,
  };
  if (rng.chance(30)) envelope.formId = rng.pick(FORMS);
  return envelope;
}

export function randomPolicy(rng: Xorshift64): DndDropPolicy {
  const kinds = KINDS.filter(() => rng.chance(55));
  if (kinds.length === 0) kinds.push(rng.pick(KINDS));
  const policy: DndDropPolicy = { targetId: rng.pick(TARGETS), allowedOperations: opsSubset(rng), acceptedKinds: kinds };
  if (rng.chance(35)) {
    const patterns = MEDIA_PATTERNS.filter(() => rng.chance(40)).map(String);
    if (patterns.length === 0) patterns.push(rng.pick(MEDIA_PATTERNS));
    policy.acceptedMediaTypes = patterns;
  }
  if (rng.chance(30)) policy.maxItems = 1 + rng.below(3);
  if (rng.chance(30)) policy.maxTotalBytes = 1 + rng.below(16);
  if (rng.chance(25)) policy.formId = rng.pick(FORMS);
  return policy;
}

export function randomInput(rng: Xorshift64, counter: { n: number }): DndSessionInput {
  const roll = rng.below(100);
  if (roll <= 17) {
    counter.n += 1;
    return inputs.start(randomEnvelope(rng, counter.n));
  }
  if (roll <= 52) {
    const policy = randomPolicy(rng);
    const preferred = rng.chance(30) ? rng.pick(OPS) : undefined;
    return inputs.enter(policy, preferred);
  }
  if (roll <= 67) return inputs.leave(rng.pick(TARGETS));
  if (roll <= 85) return inputs.drop(rng.pick(TARGETS));
  if (roll <= 92) return inputs.cancel();
  return inputs.end();
}

/** A full random sequence, always beginning with a valid start. */
export function randomSequence(seed: bigint | number, steps: number): DndSessionInput[] {
  const rng = new Xorshift64(seed);
  const counter = { n: 1 };
  const first = randomEnvelope(rng, counter.n);
  first.protocol = ORES_DND_PROTOCOL;
  const out: DndSessionInput[] = [inputs.start(first)];
  while (out.length < steps) out.push(randomInput(rng, counter));
  return out;
}
