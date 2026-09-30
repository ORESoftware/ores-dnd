# WIT admission

Driver: `ORESoftware/ores-dnd#38`

This document is a bounded review contract for one slice of the driver issue. It is intentionally mergeable independently and does **not** claim the parent issue is fully implemented.

## Invariants

- WIT remains a downstream projection, not a third authored semantic authority.
- The shared ores-wit toolchain must parse/normalize the emitted package before release.
- TJSV evidence must bind the exact source and generated projection revisions.
- Compatibility failures block promotion instead of being downgraded to warnings.

## Verification

- Review the exact PR head, not a synthetic or stale revision.
- Run the repository's normal format/lint/test/conformance gates that touch this boundary.
- Treat skipped or zero-step CI as missing evidence.
- Preserve existing public behavior unless the driver issue explicitly authorizes a breaking change.

## Non-goals

This slice does not add credentials, bypass branch protection, or declare the broader fleet rollout complete.
