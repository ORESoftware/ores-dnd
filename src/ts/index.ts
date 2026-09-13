// @oresoftware/ores-dnd — browser and webview drag-and-drop for ores.dnd/v1.
//
// - wire:     bounded scalars and array limits both schema authorities check
// - codec:    wire types, validation, DataTransfer read/write, negotiation, telemetry
// - policy:   what a target accepts, evaluated in the fleet-wide order
// - session:  the pure drag session state machine (shared trace corpus)
// - corpus:   decode any contract declaration by name (tjsv runtime evidence)
// - dom:      HTML5 drag-and-drop bindings + autoBind for HTML-first pages
// - external: bounded drop-time materialization of Files / URI-list / JSON / text
// - keyboard: accessible keyboard input adapter over the same DndSession
// - pointer:  pointer-events fallback for touch surfaces / webviews
// - htmx:     MASH commit endpoint wiring (JSON verdict or HTML swap)
// - fuzz:     seeded random session generator (mirror of the Rust one) for differential fuzzing
// - wasm:     typed shim over the ores-dnd-wasm exports + cross-checks
export * from "./wire.js";
export * from "./codec.js";
export * from "./policy.js";
export * from "./session.js";
export * from "./corpus.js";
export * from "./dom.js";
export * from "./external.js";
export * from "./keyboard.js";
export * from "./pointer.js";
export * from "./htmx.js";
export * from "./wasm.js";
export * from "./fuzz.js";
