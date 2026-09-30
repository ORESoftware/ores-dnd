# WIT admission boundary

Tracking: #38.

`wit/ores-dnd.wit` is downstream component evidence. It is not a third authored structural authority beside TypeSpec and authored Draft 2020-12 JSON Schema.

## Artifact classes

- **legacy component ABI** — the currently published JSON-string-oriented component surface. Preserve its package/world/version identity and compare it strictly against an immutable baseline.
- **typed downstream projection** — may be introduced only from admitted Contract IR once the shared `ores-wit` emitter can represent the semantics without loss. Give it a distinct reviewed identity or explicit migration/version plan.

## Release evidence closure

One source revision must bind: peer-authority/Contract-IR admission, raw WIT source digest, resolved/canonical WIT digest, exact `wasm-tools` identity, normalized WIT projection, immutable baseline, and TJSV compatibility receipt. Syntax-only success is necessary but not sufficient.

## Fail-closed cases

Promotion must stop on stale WIT, Contract-IR mismatch, package/version drift, world/resource removal or signature drift, or an incompatible change to the legacy JSON-string ABI. Durable admission logic belongs in Rust/shared tooling rather than growing shell grep/sed policy.
