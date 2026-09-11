import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";

export const CONTRACTS = new URL("../../../contracts/", import.meta.url).pathname;

export async function readJson(rel) {
  return JSON.parse(await readFile(join(CONTRACTS, rel), "utf8"));
}

/** [{ declaration, expectation, file, json }] for every corpus instance */
export async function corpus() {
  const out = [];
  const root = join(CONTRACTS, "instances");
  for (const declaration of (await readdir(root)).sort()) {
    for (const [lane, expectation] of [["valid", "accepted"], ["invalid", "rejected"]]) {
      let files = [];
      try { files = (await readdir(join(root, declaration, lane))).filter((f) => f.endsWith(".json")).sort(); } catch { continue; }
      for (const file of files) out.push({ declaration, expectation, file, json: await readFile(join(root, declaration, lane, file), "utf8") });
    }
  }
  return out;
}

export async function traces() {
  return (await corpus()).filter((c) => c.declaration === "DndSessionTrace" && c.expectation === "accepted").map((c) => JSON.parse(c.json));
}

export const validEnvelope = () => readJson("instances/DndEnvelope/valid/text-copy.json");
