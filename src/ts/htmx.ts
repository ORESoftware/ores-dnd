// MASH / htmx wiring: report an accepted drop to the server as JSON
// (`DropCommitRequest`, see ores-dnd-mash::server), let the server decide, and
// optionally swap an HTML response into the page the htmx way.
import type { DndDropResult, DndEnvelope } from "./codec.js";
import { decodeDropResult } from "./corpus.js";
import { ATTR_COMMIT, ATTR_SWAP, EVENT_COMMITTED } from "./dom.js";

/** Default endpoint mounted by `ores_dnd_mash::server::router`. */
export const DEFAULT_COMMIT_PATH = "/ores-dnd/drop";

export interface HtmxLike {
  process(el: Element): void;
}

export interface CommitOptions {
  fetch?: typeof fetch;
  /** CSS selector (or element) whose innerHTML receives an HTML response. */
  swapTarget?: string | Element | null;
  /** `window.htmx` when present; used to `process()` swapped markup. */
  htmx?: HtmxLike | null;
  headers?: Record<string, string>;
  /** Document used to resolve `swapTarget` selectors. */
  doc?: Document;
}

export interface DropCommitRequest {
  envelope: DndEnvelope;
  result: DndDropResult;
}

function resolveFetch(options: CommitOptions): typeof fetch {
  if (options.fetch) return options.fetch;
  if (typeof globalThis.fetch === "function") return globalThis.fetch.bind(globalThis);
  throw new Error("fetch is not available; pass options.fetch");
}

/**
 * POST the drop to `url` and return the server's verdict. A JSON response is a
 * `DndDropResult` (the server re-verified the policy); an HTML response is
 * swapped into `swapTarget` and counts as accepted when the status is 2xx.
 */
export async function commitDrop(url: string, envelope: DndEnvelope, result: DndDropResult, options: CommitOptions = {}): Promise<DndDropResult> {
  const body: DropCommitRequest = { envelope, result };
  const response = await resolveFetch(options)(url, {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json, text/html", "HX-Request": "true", ...(options.headers ?? {}) },
    body: JSON.stringify(body),
    credentials: "same-origin",
  });
  const contentType = response.headers.get("content-type") ?? "";
  if (contentType.includes("application/json")) {
    const verdict = decodeDropResult(await response.json());
    if (verdict.dragId !== result.dragId) throw new Error("server verdict is for a different drag");
    return verdict;
  }
  const html = await response.text();
  if (options.swapTarget) {
    const target = typeof options.swapTarget === "string" ? (options.doc ?? document).querySelector(options.swapTarget) : options.swapTarget;
    if (target) {
      target.innerHTML = html;
      options.htmx?.process(target);
    }
  }
  if (!response.ok) return { dragId: result.dragId, accepted: false, errorCode: `http-${response.status}` };
  return result;
}

/** Commit using the zone's own `data-ores-dnd-commit` / `data-ores-dnd-swap` attributes. */
export async function commitFromZone(zone: HTMLElement, envelope: DndEnvelope, result: DndDropResult, options: CommitOptions = {}): Promise<void> {
  const url = zone.getAttribute(ATTR_COMMIT) || DEFAULT_COMMIT_PATH;
  const swap = options.swapTarget ?? zone.getAttribute(ATTR_SWAP);
  const htmx = options.htmx ?? ((globalThis as { htmx?: HtmxLike }).htmx ?? null);
  const verdict = await commitDrop(url, envelope, result, { ...options, swapTarget: swap, htmx, doc: options.doc ?? zone.ownerDocument });
  zone.dispatchEvent(new CustomEvent(EVENT_COMMITTED, { detail: { envelope, result: verdict }, bubbles: true }));
}
