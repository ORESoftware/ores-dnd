import test from "node:test";
import assert from "node:assert/strict";
import { ATTR_COMMIT, ATTR_SWAP, EVENT_COMMITTED } from "../dist/dom.js";
import { DEFAULT_COMMIT_PATH, commitDrop, commitFromZone } from "../dist/htmx.js";
import { FakeDocument } from "./fake-dom.mjs";
import { validEnvelope } from "./helpers.mjs";

function fakeFetch(reply) {
  const calls = [];
  const fetch = async (url, init) => {
    calls.push({ url, init, body: JSON.parse(init.body) });
    const r = typeof reply === "function" ? reply(calls.length) : reply;
    return {
      ok: r.status < 300,
      status: r.status,
      headers: new Headers({ "content-type": r.type }),
      json: async () => JSON.parse(r.body),
      text: async () => r.body,
    };
  };
  return { fetch, calls };
}

test("a JSON verdict from the server replaces the browser's result", async () => {
  const envelope = await validEnvelope();
  const result = { dragId: envelope.dragId, accepted: true, operation: "copy", targetId: "zone-a" };
  const { fetch, calls } = fakeFetch({ status: 422, type: "application/json", body: JSON.stringify({ dragId: envelope.dragId, accepted: false, errorCode: "item-kind-not-accepted" }) });
  const verdict = await commitDrop("/drops", envelope, result, { fetch });
  assert.equal(verdict.accepted, false);
  assert.equal(verdict.errorCode, "item-kind-not-accepted");
  assert.equal(calls[0].url, "/drops");
  assert.equal(calls[0].init.method, "POST");
  assert.equal(calls[0].init.headers["HX-Request"], "true");
  assert.deepEqual(calls[0].body, { envelope, result });
});

test("a verdict for another drag is refused", async () => {
  const envelope = await validEnvelope();
  const result = { dragId: envelope.dragId, accepted: true, operation: "copy", targetId: "zone-a" };
  const { fetch } = fakeFetch({ status: 200, type: "application/json", body: JSON.stringify({ dragId: "other", accepted: true }) });
  await assert.rejects(commitDrop("/drops", envelope, result, { fetch }), /different drag/);
});

test("an HTML response is swapped into the target and processed by htmx", async () => {
  const doc = new FakeDocument();
  const envelope = await validEnvelope();
  const list = doc.body.appendChild(doc.createElement("ul"));
  list.setAttribute("id", "list");
  const processed = [];
  const htmx = { process: (el) => processed.push(el) };
  const result = { dragId: envelope.dragId, accepted: true, operation: "move", targetId: "zone-a" };
  const { fetch } = fakeFetch({ status: 200, type: "text/html; charset=utf-8", body: "<li>moved</li>" });
  const verdict = await commitDrop("/drops", envelope, result, { fetch, swapTarget: "#list", htmx, doc });
  assert.equal(list.innerHTML, "<li>moved</li>");
  assert.deepEqual(processed, [list]);
  assert.deepEqual(verdict, result);
  const failing = fakeFetch({ status: 500, type: "text/html", body: "<p>nope</p>" });
  const failed = await commitDrop("/drops", envelope, result, { fetch: failing.fetch, swapTarget: list, htmx, doc });
  assert.deepEqual(failed, { dragId: envelope.dragId, accepted: false, errorCode: "http-500" });
});

test("commitFromZone reads the zone's attributes and announces the server verdict", async () => {
  const doc = new FakeDocument();
  const envelope = await validEnvelope();
  const zone = doc.body.appendChild(doc.createElement("div"));
  const list = doc.body.appendChild(doc.createElement("ul"));
  list.setAttribute("id", "list");
  zone.setAttribute(ATTR_SWAP, "#list");
  const committed = [];
  zone.addEventListener(EVENT_COMMITTED, (e) => committed.push(e.detail.result.accepted));
  const result = { dragId: envelope.dragId, accepted: true, operation: "move", targetId: "zone-a" };
  const { fetch, calls } = fakeFetch({ status: 200, type: "text/html", body: "<li>ok</li>" });
  await commitFromZone(zone, envelope, result, { fetch });
  assert.equal(calls[0].url, DEFAULT_COMMIT_PATH, "no data-ores-dnd-commit → default endpoint");
  assert.equal(list.innerHTML, "<li>ok</li>");
  assert.deepEqual(committed, [true]);
  zone.setAttribute(ATTR_COMMIT, "/custom");
  await commitFromZone(zone, envelope, result, { fetch });
  assert.equal(calls[1].url, "/custom");
});
