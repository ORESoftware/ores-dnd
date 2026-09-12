// Build the trusted case list every runtime adapter consumes and the corpus
// digest the verifier pins evidence to. Cases come only from the checked-in
// contract corpus; adapters never author expectations.
import { mkdir, readdir, writeFile } from "node:fs/promises";
import { join, relative, resolve } from "node:path";
import { canonicalStringify, sha256 } from "@oresoftware/typespec-json-schema-validator";

export const ROOT = resolve(new URL("../..", import.meta.url).pathname);
export const INSTANCES = join(ROOT, "contracts", "instances");
export const OUT_DIR = join(ROOT, "artifacts", "conformance");
export const NAMESPACE = "OresDnd";

/** [{ id, declaration, expectation, path }] in deterministic order. */
export async function collectCases() {
  const cases = [];
  for (const decl of (await readdir(INSTANCES, { withFileTypes: true })).filter((d) => d.isDirectory()).map((d) => d.name).sort()) {
    for (const [lane, expectation] of [["valid", "accepted"], ["invalid", "rejected"]]) {
      let files = [];
      try {
        files = (await readdir(join(INSTANCES, decl, lane))).filter((f) => f.endsWith(".json")).sort();
      } catch {
        continue;
      }
      for (const file of files) {
        const stem = file.slice(0, -".json".length).toLowerCase().replace(/[^a-z0-9._-]/g, "-");
        cases.push({
          id: `${decl.toLowerCase()}.${lane}.${stem}`,
          declaration: `${NAMESPACE}.${decl}`,
          expectation,
          path: relative(OUT_DIR, join(INSTANCES, decl, lane, file)),
        });
      }
    }
  }
  return cases;
}

/** The trusted expectations (no paths) and their digest. */
export function expectedCasesOf(cases) {
  return cases.map(({ id, declaration, expectation }) => ({ id, declaration, expectation }));
}

export function corpusDigestOf(cases) {
  return sha256(canonicalStringify(expectedCasesOf(cases)));
}

export async function writeCases() {
  const cases = await collectCases();
  await mkdir(OUT_DIR, { recursive: true });
  const digest = corpusDigestOf(cases);
  await writeFile(join(OUT_DIR, "cases.json"), JSON.stringify({ corpusDigest: digest, cases }, null, 2) + "\n");
  return { cases, digest, path: join(OUT_DIR, "cases.json") };
}

if (process.argv[1] && resolve(process.argv[1]) === new URL(import.meta.url).pathname) {
  const { cases, digest, path } = await writeCases();
  console.error(`conformance corpus: ${cases.length} cases, digest ${digest.slice(0, 12)}… -> ${path}`);
}
