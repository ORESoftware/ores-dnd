# ores-dnd security model

Drag-and-drop moves attacker-influenced bytes across trust boundaries: from
another application into ours (external drags), from the browser to the server
(MASH commit), and from one runtime to another (WASM hosts, Flutter web). This
document lists the threats the library is designed against and the invariants
every runtime enforces. The invariants are executable: they are the contract
bounds checked by both schema authorities, the runtime decoders proven equal
by the tjsv conformance gate, and the session traces every core replays.

## Threats

| # | Threat | Where it enters | Control |
| --- | --- | --- | --- |
| T1 | Oversized payload (memory/CPU exhaustion) | any `decode` | byte limit checked **before** parsing (`maxPayloadBytes`, default 1 MiB); `items ≤ 64`; `data ≤ 1 048 576` code points; the MASH endpoint refuses bodies over `RouterOptions::body_limit_bytes` (default 2 MiB) with 413 before parsing |
| T2 | Injection through identifiers that reach DOM attributes, logs, telemetry, URLs | `dragId`, `targetId`, `sourceRuntime`, `formId` | `SafeId`: `^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$` in both authorities and all decoders |
| T3 | Media-type confusion (parameters, case tricks, `*/*`) | `DndItem.mediaType`, `acceptedMediaTypes` | canonical lowercase `type/subtype`, no parameters; wildcards only as `type/*`; browser formats that are not media types become `application/octet-stream` |
| T4 | Unknown/extra properties smuggling data or behaviour | every object | `unevaluatedProperties: false`; decoders reject unknown keys; closed enums |
| T5 | Browser reporting a verdict the policy never gave | `POST /ores-dnd/drop` | the server re-decodes the envelope, re-evaluates the zone's policy with the same core and refuses on any disagreement (`verify_drop_commit`); the browser's verdict is advisory only |
| T6 | Internal error text leaking to clients | commit failures | backend errors collapse to `commit-failed` unless they are already wire-safe `ErrorCode`s |
| T7 | Dragged content leaking into observability | ores-otel port | `DndTelemetryEvent` has no `data` field at all; adapters emit counts, ids, phases, operations only |
| T8 | Trace-context spoofing / log injection via `traceparent` | envelope | W3C pattern enforced; propagation is opt-in per app policy |
| T9 | Drop applied to the wrong target or twice | session, `POST /ores-dnd/drop` | a drop must name the active accepting target (`target-mismatch` / `no-active-target` otherwise); terminal states are absorbing; the MASH endpoint refuses a repeated `dragId` with `duplicate-drag` (409) from a bounded in-memory window plus the backend's durable `already_committed` |
| T10 | Policy tampering client-side | `data-ores-dnd-policy` | the attribute is a convenience for the browser adapter; the server keeps its own policy table and never reads the client's copy |
| T12 | Cross-site request forgery against a cookie-authenticated commit endpoint | `POST /ores-dnd/drop` | the endpoint requires the `HX-Request` header the adapters always send (a cross-site form post cannot set custom headers); responses are `Cache-Control: no-store`; `RouterOptions::require_hx_request` to relax for token-authenticated APIs |
| T11 | Protocol downgrade / version confusion | `protocol` | `ProtocolId` shape enforced structurally; each runtime accepts an explicit version list; a foreign major never starts a session |

## Invariants every runtime enforces

1. Structural rules (T2, T3, T4, T8, T11) are checked by the decoder of every
   declaration before any semantic rule runs, in Rust, TypeScript and Dart
   alike; the 136-instance corpus (valid + invalid per declaration) is the
   evidence, and tjsv's runtime-conformance gate refuses a runtime whose
   verdicts diverge.
2. Semantic rules (protocol version, host item limit) sit on top; a
   structurally valid envelope of another protocol major is `invalid-envelope`.
3. Policy evaluation order is fixed (operation → kind → media type → count →
   bytes → form) so the reject code an attacker can observe is the same
   everywhere and never reveals more than the first failed rule.
4. The session state machine is pure and replayed from the same 23 traces by
   all three cores; adapters only translate events.
5. Side effects (ores-forms, opto-sync, ores-otel) run only for a `dropped`
   session and only in that order; a decoded drop never mutates anything by
   itself.

## Out of scope (host responsibilities)

- Authentication and authorization of the commit endpoint (shared-auth) and
  request rate limiting (ores-rate-limit / ores-middleware) — `ores-dnd-mash`
  exposes a router you mount behind them.
- Deciding whether a `uri` or `bytes` item may be opened, fetched or stored.
- CSP / sandboxing of webviews that host the TypeScript adapter.

## Reporting

Open a security advisory on the repository, or contact the ORESoftware
maintainers privately. Do not file public issues for exploitable findings.
