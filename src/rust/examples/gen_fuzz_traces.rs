//! Generate the differential fuzz corpus: seeded random sessions whose expected
//! snapshots are what the Rust core produced. TypeScript and Dart must replay
//! them identically (`fuzz-*.json` under contracts/instances/DndSessionTrace/valid).
//!
//!   cargo run -p ores-dnd-core --example gen_fuzz_traces -- <out-dir> [count] [steps] [seed-base]
use ores_dnd_core::fuzz::random_sequence;
use ores_dnd_core::{DndSession, DndSessionTrace};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("output directory");
    let count: u32 = args.next().map(|v| v.parse().expect("count")).unwrap_or(40);
    let steps: usize = args.next().map(|v| v.parse().expect("steps")).unwrap_or(14);
    let seed_base: u64 = args
        .next()
        .map(|v| v.parse().expect("seed"))
        .unwrap_or(0x5eed_0000);
    std::fs::create_dir_all(&out).expect("mkdir");
    for i in 0..count {
        let seed = seed_base + u64::from(i);
        let inputs = random_sequence(seed, steps);
        let mut session = DndSession::new();
        let expected = inputs
            .iter()
            .map(|input| session.apply(input).clone())
            .collect();
        let trace = DndSessionTrace {
            id: format!("fuzz-{seed:x}"),
            description: Some(format!("differential fuzz trace: xorshift64* seed {seed:#x}, {steps} steps, expected snapshots recorded by ores-dnd-core")),
            inputs,
            expected,
        };
        trace
            .structural()
            .expect("generated trace is structurally valid");
        let path = format!("{out}/{}.json", trace.id);
        std::fs::write(&path, serde_json::to_string_pretty(&trace).unwrap() + "\n").expect("write");
    }
    eprintln!("wrote {count} fuzz traces to {out}");
}
