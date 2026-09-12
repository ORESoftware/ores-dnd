#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

fail() {
  printf 'release-admission: %s\n' "$*" >&2
  exit 1
}

zpkg_version="$(awk '$1 == "version" && $2 == "=" { gsub(/\"/, "", $3); print $3; exit }' .zpkg.toml)"
cargo_version="$(awk '/^\[workspace\.package\]$/ { in_package=1; next } /^\[/ { in_package=0 } in_package && $1 == "version" && $2 == "=" { gsub(/\"/, "", $3); print $3; exit }' Cargo.toml)"
ts_version="$(node -p 'JSON.parse(require("fs").readFileSync("src/ts/package.json", "utf8")).version')"
dart_version="$(awk '$1 == "version:" { print $2; exit }' src/dart/pubspec.yaml)"
flutter_version="$(awk '$1 == "version:" { print $2; exit }' src/flutter/pubspec.yaml)"
wit_version="$(sed -n '1s/^package oresoftware:ores-dnd@\([^;]*\);$/\1/p' wit/ores-dnd.wit)"

[[ -n "$zpkg_version" ]] || fail "cannot resolve .zpkg.toml version"
for pair in \
  "Cargo.toml:$cargo_version" \
  "src/ts/package.json:$ts_version" \
  "src/dart/pubspec.yaml:$dart_version" \
  "src/flutter/pubspec.yaml:$flutter_version" \
  "wit/ores-dnd.wit:$wit_version"
do
  file="${pair%%:*}"
  version="${pair#*:}"
  [[ "$version" == "$zpkg_version" ]] || fail "$file version '$version' != Zed package '$zpkg_version'"
done

# Release artifacts must not admit local state, generated caches, build output,
# plaintext environments, or vendored dependency trees.
for required in \
  '.vendor/.zed/**' \
  '**/node_modules/**' \
  '**/target/**' \
  '**/.dart_tool/**' \
  '**/build/**' \
  'artifacts/**' \
  '.typespec-json-schema-validator/**'
do
  grep -Fq "\"$required\"" .zpkg.toml || fail "publish.exclude is missing $required"
done

for target in rust wasm typescript dart flutter contracts repository; do
  grep -Fq "[targets.$target]" .zpkg.toml || fail "missing Zed target: $target"
done

# Public artifacts must not carry accidental plaintext env files.
if git ls-files | grep -E '(^|/)(\.env($|\.)|env/dec/|.*\.env\.dec$)' >/dev/null; then
  fail "tracked plaintext environment material detected"
fi

# Generated/build trees must not be tracked as source authorities.
if git ls-files | grep -E '(^|/)(node_modules|target|build|\.dart_tool|\.typespec-json-schema-validator)(/|$)' >/dev/null; then
  fail "tracked generated/build tree detected"
fi

printf 'release-admission: version parity %s across Zed/Cargo/TypeScript/Dart/Flutter/WIT\n' "$zpkg_version"
printf 'release-admission: target/exclusion/source-tree checks passed\n'
