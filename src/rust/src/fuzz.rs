//! Deterministic random session generation shared by the fuzz-trace generator
//! (`examples/gen_fuzz_traces.rs`) and the invariant tests. A tiny xorshift
//! PRNG keeps the corpus reproducible without a `rand` dependency; the same
//! generator is mirrored in TypeScript and Dart so every runtime can fuzz
//! itself with identical sequences.

use crate::envelope::{DndEnvelope, DndItem, DndItemKind, DndOperation, ORES_DND_PROTOCOL};
use crate::policy::DndDropPolicy;
use crate::session::DndSessionInput;

/// xorshift64* — reproducible across languages (64-bit wrapping arithmetic).
#[derive(Debug, Clone)]
pub struct Xorshift64(pub u64);

impl Xorshift64 {
    pub fn new(seed: u64) -> Self {
        Self(if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `0..n` (n > 0); uses the high bits for quality.
    pub fn below(&mut self, n: u32) -> u32 {
        ((self.next_u64() >> 32) % u64::from(n)) as u32
    }

    pub fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u32) as usize]
    }
}

pub const OPS: [DndOperation; 3] = [DndOperation::Copy, DndOperation::Move, DndOperation::Link];
pub const KINDS: [DndItemKind; 4] = [DndItemKind::Text, DndItemKind::Uri, DndItemKind::Json, DndItemKind::Bytes];
pub const MEDIA: [&str; 5] = ["text/plain", "text/markdown", "text/uri-list", "application/json", "image/png"];
pub const MEDIA_PATTERNS: [&str; 6] = ["text/plain", "text/*", "application/json", "image/*", "text/markdown", "application/*"];
pub const TARGETS: [&str; 4] = ["zone-a", "zone-b", "zone-c", "zone-d"];
pub const FORMS: [&str; 2] = ["form-x", "form-y"];

pub fn random_envelope(rng: &mut Xorshift64, n: u32) -> DndEnvelope {
    let item_count = 1 + rng.below(3) as usize;
    let items = (0..item_count)
        .map(|_| {
            let media = *rng.pick(&MEDIA);
            let kind = match media {
                "text/uri-list" => DndItemKind::Uri,
                "application/json" => DndItemKind::Json,
                "image/png" => DndItemKind::Bytes,
                _ => DndItemKind::Text,
            };
            DndItem { kind, media_type: media.to_owned(), data: "x".repeat(rng.below(9) as usize), name: None }
        })
        .collect();
    let ops = ops_subset(rng);
    DndEnvelope {
        protocol: if rng.chance(8) { "ores.dnd/v2".to_owned() } else { ORES_DND_PROTOCOL.to_owned() },
        drag_id: format!("drag-{n:04}"),
        source_runtime: "fuzz".to_owned(),
        allowed_operations: ops,
        items,
        traceparent: None,
        form_id: if rng.chance(30) { Some((*rng.pick(&FORMS)).to_owned()) } else { None },
    }
}

fn ops_subset(rng: &mut Xorshift64) -> Vec<DndOperation> {
    let mut ops: Vec<DndOperation> = OPS.iter().copied().filter(|_| rng.chance(55)).collect();
    if ops.is_empty() {
        ops.push(*rng.pick(&OPS));
    }
    ops
}

pub fn random_policy(rng: &mut Xorshift64) -> DndDropPolicy {
    let mut kinds: Vec<DndItemKind> = KINDS.iter().copied().filter(|_| rng.chance(55)).collect();
    if kinds.is_empty() {
        kinds.push(*rng.pick(&KINDS));
    }
    let mut policy = DndDropPolicy::new(*rng.pick(&TARGETS), &ops_subset(rng), &kinds);
    if rng.chance(35) {
        let mut patterns: Vec<String> = MEDIA_PATTERNS.iter().filter(|_| rng.chance(40)).map(|p| (*p).to_owned()).collect();
        if patterns.is_empty() {
            patterns.push((*rng.pick(&MEDIA_PATTERNS)).to_owned());
        }
        policy = policy.with_media_types(patterns);
    }
    if rng.chance(30) {
        policy = policy.with_max_items(1 + rng.below(3) as i32);
    }
    if rng.chance(30) {
        policy = policy.with_max_total_bytes(1 + rng.below(16) as i32);
    }
    if rng.chance(25) {
        policy = policy.with_form_id(*rng.pick(&FORMS));
    }
    policy
}

/// One random input; `n` numbers envelopes so drag ids stay unique per trace.
pub fn random_input(rng: &mut Xorshift64, n: &mut u32) -> DndSessionInput {
    match rng.below(100) {
        0..=17 => {
            *n += 1;
            DndSessionInput::start(random_envelope(rng, *n))
        }
        18..=52 => {
            let policy = random_policy(rng);
            let preferred = if rng.chance(30) { Some(*rng.pick(&OPS)) } else { None };
            DndSessionInput::enter(policy, preferred)
        }
        53..=67 => DndSessionInput::leave(*rng.pick(&TARGETS)),
        68..=85 => DndSessionInput::drop(*rng.pick(&TARGETS)),
        86..=92 => DndSessionInput::cancel(),
        _ => DndSessionInput::end(),
    }
}

/// A full random sequence, always beginning with a valid `start`.
pub fn random_sequence(seed: u64, steps: usize) -> Vec<DndSessionInput> {
    let mut rng = Xorshift64::new(seed);
    let mut n = 0;
    let mut inputs = Vec::with_capacity(steps);
    n += 1;
    let mut first = random_envelope(&mut rng, n);
    first.protocol = ORES_DND_PROTOCOL.to_owned();
    inputs.push(DndSessionInput::start(first));
    while inputs.len() < steps {
        inputs.push(random_input(&mut rng, &mut n));
    }
    inputs
}
