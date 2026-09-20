#!/bin/sh
set -eu
mode=${1:-full}
case "$mode" in full|--full) mode=full ;; structural|--structural-only) mode=structural ;; *) echo "usage: conformance/check.sh [--full|--structural-only]" >&2; exit 2 ;; esac
root=$(git rev-parse --show-toplevel 2>/dev/null || pwd)
cd "$root"
fail(){ echo "[zed-conformance] $*" >&2; exit 1; }
for boundary in contracts conformance; do [ ! -L "$boundary" ] || fail "$boundary must not be a symlink"; [ -d "$boundary" ] || fail "missing $boundary/"; done
escaped=$(find contracts conformance -type l -print -quit 2>/dev/null || true)
[ -z "$escaped" ] || fail "symlink inside contract/conformance boundary: $escaped"
[ "$mode" = full ] || exit 0
command -v node >/dev/null 2>&1 || fail "node is required for conformance/check.mjs"
exec node conformance/check.mjs
