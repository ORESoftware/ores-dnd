#!/bin/sh
set -eu
# Compatibility helper only. Zed's canonical checker remains check.mjs until
# the lifecycle wrapper is promoted in a later source-compatible migration.
zed run tjsv check --typespec=contracts/main.tsp --schema=contracts/authored.schema.json --instances=contracts/instances --report=artifacts/zed-lifecycle-schema-parity.json --quiet
node conformance/check.mjs
