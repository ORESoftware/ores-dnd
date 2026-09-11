import test from "node:test";
import assert from "node:assert/strict";
import { evaluatePolicy, mediaTypeMatches, totalDataBytes, validatePolicy } from "../dist/policy.js";
import { validEnvelope } from "./helpers.mjs";

test("media type matching is case-insensitive and supports wildcards", () => {
  assert.equal(mediaTypeMatches("text/plain", "TEXT/Plain; charset=utf-8"), true);
  assert.equal(mediaTypeMatches("text/*", "text/markdown"), true);
  assert.equal(mediaTypeMatches("text/*", "image/png"), false);
  assert.equal(mediaTypeMatches("text/*", "text"), false);
});

test("policy evaluation order is fixed", async () => {
  const env = await validEnvelope();
  const base = { targetId: "z", allowedOperations: ["copy"], acceptedKinds: ["text"] };
  assert.deepEqual(evaluatePolicy(env, { ...base, allowedOperations: ["link"], acceptedKinds: ["json"] }), { accepted: false, errorCode: "no-common-operation" });
  assert.deepEqual(evaluatePolicy(env, { ...base, acceptedKinds: ["json"] }), { accepted: false, errorCode: "item-kind-not-accepted" });
  assert.deepEqual(evaluatePolicy(env, { ...base, acceptedMediaTypes: ["text/markdown"], maxTotalBytes: 1 }), { accepted: false, errorCode: "media-type-not-accepted" });
  assert.deepEqual(evaluatePolicy(env, { ...base, maxTotalBytes: 4 }), { accepted: false, errorCode: "payload-too-large" });
  assert.deepEqual(evaluatePolicy(env, { ...base, maxTotalBytes: 5 }), { accepted: true, operation: "copy" });
  assert.deepEqual(evaluatePolicy({ ...env, formId: "a" }, { ...base, formId: "b" }), { accepted: false, errorCode: "form-mismatch" });
  assert.equal(totalDataBytes({ ...env, items: [{ kind: "text", mediaType: "text/plain", data: "héllo" }] }), 6);
});

test("policy bounds and unknown properties fail closed", () => {
  assert.throws(() => validatePolicy({ targetId: "z", allowedOperations: ["copy"], acceptedKinds: ["text"], maxItems: 0 }), /maxItems/);
  assert.throws(() => validatePolicy({ targetId: "z", allowedOperations: ["copy"], acceptedKinds: ["text"], onDrop: "x" }), /unsupported properties/);
  assert.throws(() => validatePolicy({ targetId: "z", allowedOperations: ["teleport"], acceptedKinds: ["text"] }), /allowedOperations/);
  assert.deepEqual(validatePolicy({ targetId: "z", allowedOperations: ["copy"], acceptedKinds: ["text"], maxTotalBytes: 10 }).maxTotalBytes, 10);
});
