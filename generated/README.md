# Generated evidence

Generated artifacts are read-only evidence derived during CI/builds. They are never contract authorities.

The authoritative sources are:

- `contracts/main.tsp`
- `contracts/authored.schema.json`

`tjsv` writes its comparison witness under `.typespec-json-schema-validator/generated/` by default; that directory is intentionally ignored.
