// tjsv runtime-conformance verifier for ores-dnd.
//
// 1. run the parity check (both authorities + the instance corpus) and turn the
//    receipt into a Contract IR;
// 2. load the adapter evidence written by each runtime
//    (artifacts/conformance/evidence-<lang>.json);
// 3. admit it only against the exact Contract IR id, parity run id and corpus
//    digest, with every required adapter present and every verdict matching.
//
//   node scripts/conformance/verify.mjs [--require typescript,rust,dart]
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { buildContractIr, runCheck, writeReport } from "@oresoftware/typespec-json-schema-validator";
import {
  RUNTIME_EVIDENCE_SCHEMA,
  verifyRuntimeEvidenceAgainstCurrentInputs,
} from "@oresoftware/typespec-json-schema-validator/runtime-conformance";
import { OUT_DIR, ROOT, collectCases, corpusDigestOf, expectedCasesOf } from "./corpus.mjs";

export const ADAPTERS = {
  typescript: { id: "typescript-ores-dnd", language: "typescript", validator: "@oresoftware/ores-dnd@0.1.0", file: "evidence-typescript.json" },
  rust: { id: "rust-serde", language: "rust", validator: "serde@1", file: "evidence-rust.json" },
  dart: { id: "dart-ores-dnd", language: "dart", validator: "ores_dnd@0.1.0", file: "evidence-dart.json" },
};

function parseRequire(argv) {
  const flag = argv.find((a) => a.startsWith("--require="))?.slice("--require=".length) ?? argv[argv.indexOf("--require") + 1];
  const names = (flag && !flag.startsWith("--") ? flag : "typescript,rust,dart").split(",").map((s) => s.trim()).filter(Boolean);
  for (const name of names) if (!ADAPTERS[name]) throw new Error(`unknown adapter ${name}; known: ${Object.keys(ADAPTERS).join(", ")}`);
  return names;
}

export async function verify({ require = ["typescript", "rust", "dart"] } = {}) {
  await mkdir(OUT_DIR, { recursive: true });
  const typespec = join(ROOT, "contracts", "main.tsp");
  const authoredSchema = join(ROOT, "contracts", "authored.schema.json");
  const generatedSchema = join(OUT_DIR, "generated");
  const cases = await collectCases();
  const expectedCases = expectedCasesOf(cases);
  const expectedCorpusDigest = corpusDigestOf(cases);

  const parityReport = await runCheck({
    typespec,
    authoredSchema,
    outputDir: generatedSchema,
    instances: join(ROOT, "contracts", "instances"),
    maxFindings: 250,
    maxProbes: 64,
  });
  await writeReport(join(OUT_DIR, "parity-report.json"), parityReport);
  if (parityReport.status !== "passed") {
    return { status: parityReport.status, stage: "parity", findings: parityReport.findings };
  }
  const contractIr = await buildContractIr({ report: parityReport, typespec, generatedSchema, authoredSchema });
  await writeFile(join(OUT_DIR, "contract-ir.json"), JSON.stringify(contractIr, null, 2) + "\n");
  if (contractIr.status !== "passed" || contractIr.admissible !== true) {
    return { status: "stopped_for_evaluation", stage: "contract-ir", findings: contractIr.findings ?? [] };
  }

  const adapters = [];
  const missing = [];
  for (const name of require) {
    const spec = ADAPTERS[name];
    try {
      const block = JSON.parse(await readFile(join(OUT_DIR, spec.file), "utf8"));
      adapters.push(block);
    } catch {
      missing.push(name);
    }
  }
  if (missing.length > 0) {
    return { status: "stopped_for_evaluation", stage: "evidence", findings: missing.map((name) => ({ ruleId: "runtime-adapter-missing", message: `no evidence file for required adapter ${name}` })) };
  }
  const evidence = {
    schema: RUNTIME_EVIDENCE_SCHEMA,
    contractIrId: contractIr.irId,
    inputDigest: parityReport.runId,
    corpusDigest: expectedCorpusDigest,
    adapters,
  };
  await writeFile(join(OUT_DIR, "runtime-evidence.json"), JSON.stringify(evidence, null, 2) + "\n");
  const report = await verifyRuntimeEvidenceAgainstCurrentInputs({
    evidence,
    contractIr,
    parityReport,
    typespec,
    generatedSchema,
    authoredSchema,
    expectedCorpusDigest,
    expectedCases,
    requiredAdapters: require.map((name) => ({ id: ADAPTERS[name].id, language: ADAPTERS[name].language, validator: ADAPTERS[name].validator })),
  });
  await writeFile(join(OUT_DIR, "runtime-conformance-report.json"), JSON.stringify(report, null, 2) + "\n");
  return { status: report.status, stage: "runtime", findings: report.findings ?? [], report, cases: cases.length, adapters: adapters.map((a) => a.id) };
}

if (process.argv[1] && new URL(import.meta.url).pathname.endsWith(process.argv[1].split("/").slice(-2).join("/"))) {
  const result = await verify({ require: parseRequire(process.argv.slice(2)) });
  const summary = result.status === "passed"
    ? `runtime conformance: PASSED — ${result.cases} cases × ${result.adapters.length} adapters (${result.adapters.join(", ")})`
    : `runtime conformance: ${result.status.toUpperCase()} at ${result.stage}`;
  console.log(summary);
  for (const finding of result.findings ?? []) console.log(`- [${finding.ruleId}] ${finding.message}${finding.pointer ? ` at ${finding.pointer}` : ""}`);
  process.exitCode = result.status === "passed" ? 0 : 2;
}
