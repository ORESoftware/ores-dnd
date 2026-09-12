#!/usr/bin/env node
// tjsv runtime-evidence adapter: `node conformance-adapter.mjs <cases.json> <out.json>`
// Reads the trusted case list produced by scripts/conformance/corpus.mjs and
// writes this runtime's adapter block (verdicts only — never expectations).
import { readFile, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, join } from "node:path";
import { createRequire } from "node:module";
import { decodeDeclaration } from "./dist/corpus.js";

const [casesPath, outPath] = process.argv.slice(2);
if (!casesPath || !outPath) {
  console.error("usage: conformance-adapter.mjs <cases.json> <out.json>");
  process.exit(64);
}
const root = dirname(casesPath);
const { cases } = JSON.parse(await readFile(casesPath, "utf8"));
const results = [];
for (const c of cases) {
  const json = await readFile(isAbsolute(c.path) ? c.path : join(root, c.path), "utf8");
  let verdict = "accepted";
  try {
    decodeDeclaration(c.declaration, json);
  } catch {
    verdict = "rejected";
  }
  results.push({ caseId: c.id, declaration: c.declaration, verdict });
}
const tsVersion = createRequire(import.meta.url)("typescript/package.json").version;
const adapter = {
  id: "typescript-ores-dnd",
  language: "typescript",
  runtime: `node@${process.versions.node}`,
  validator: "@oresoftware/ores-dnd@0.1.0",
  toolchain: `typescript@${tsVersion}`,
  status: "passed",
  results,
};
await writeFile(outPath, JSON.stringify(adapter, null, 2) + "\n");
console.error(`typescript adapter: ${results.length} cases -> ${outPath}`);
