import test from "node:test";
import assert from "node:assert/strict";
import { DECLARATIONS, decodeDeclaration } from "../dist/corpus.js";
import { corpus } from "./helpers.mjs";

test("every corpus instance gets the declared verdict from the TypeScript decoder", async () => {
  const all = await corpus();
  assert.ok(all.length >= 70);
  const failures = [];
  for (const { declaration, expectation, file, json } of all) {
    assert.ok(DECLARATIONS.includes(declaration), `unknown declaration dir ${declaration}`);
    let verdict = "accepted";
    try { decodeDeclaration(declaration, json); } catch { verdict = "rejected"; }
    if (verdict !== expectation) failures.push(`${declaration}/${expectation}/${file}: typescript said ${verdict}`);
  }
  assert.deepEqual(failures, []);
});

test("every declaration has valid and invalid coverage", async () => {
  const all = await corpus();
  for (const declaration of DECLARATIONS) {
    assert.ok(all.some((c) => c.declaration === declaration && c.expectation === "accepted"), `${declaration} valid`);
    assert.ok(all.some((c) => c.declaration === declaration && c.expectation === "rejected"), `${declaration} invalid`);
  }
});

test("qualified declaration names are accepted", () => {
  assert.equal(decodeDeclaration("OresDnd.DndOperation", '"copy"'), "copy");
  assert.throws(() => decodeDeclaration("OresDnd.Nope", "{}"), /unknown declaration/);
});
