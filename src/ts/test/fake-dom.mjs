// A deliberately tiny DOM stand-in for node:test — just what the adapters touch.
export class FakeElement extends EventTarget {
  #attrs = new Map();
  children = [];
  parentElement = null;
  ownerDocument = null;
  style = {};
  innerHTML = "";
  tagName;
  constructor(tagName = "div", doc = null) {
    super();
    this.tagName = tagName.toUpperCase();
    this.ownerDocument = doc;
  }
  setAttribute(name, value) { this.#attrs.set(name, String(value)); }
  getAttribute(name) { return this.#attrs.has(name) ? this.#attrs.get(name) : null; }
  hasAttribute(name) { return this.#attrs.has(name); }
  removeAttribute(name) { this.#attrs.delete(name); }
  get id() { return this.getAttribute("id") ?? ""; }
  appendChild(child) { child.parentElement = this; child.ownerDocument = this.ownerDocument; this.children.push(child); return child; }
  contains(node) { for (let n = node; n; n = n.parentElement) if (n === this) return true; return false; }
  setPointerCapture() {}
  releasePointerCapture() {}
  *descendants() { for (const c of this.children) { yield c; yield* c.descendants(); } }
  matches(selector) {
    return selector.split("][").map((s) => s.replace(/^\[|\]$/g, "")).every((attr) => this.hasAttribute(attr));
  }
  querySelectorAll(selector) { return [...this.descendants()].filter((el) => el.matches(selector)); }
  querySelector(selector) {
    if (selector.startsWith("#")) return [...this.descendants()].find((el) => el.id === selector.slice(1)) ?? null;
    return this.querySelectorAll(selector)[0] ?? null;
  }
}

export class FakeDocument {
  body;
  hits = [];
  constructor() { this.body = new FakeElement("body", this); }
  createElement(tag) { return new FakeElement(tag, this); }
  /** register `el` as covering the rectangle [x1,y1,x2,y2] for elementFromPoint */
  place(el, x1, y1, x2, y2) { this.hits.unshift({ el, x1, y1, x2, y2 }); }
  elementFromPoint(x, y) {
    const hit = this.hits.find((h) => x >= h.x1 && x <= h.x2 && y >= h.y1 && y <= h.y2);
    return hit ? hit.el : null;
  }
  querySelector(selector) { return this.body.querySelector(selector); }
  querySelectorAll(selector) { return this.body.querySelectorAll(selector); }
}

export class FakeDataTransfer {
  #data = new Map();
  effectAllowed = "none";
  dropEffect = "none";
  files = [];
  /** browsers hide data during dragenter/dragover ("protected mode") */
  protectedMode = false;
  setData(type, value) { this.#data.set(type, String(value)); }
  getData(type) { return this.protectedMode ? "" : (this.#data.get(type) ?? ""); }
  get types() {
    const types = [...this.#data.keys()];
    if (this.files.length > 0 && !types.includes("Files")) types.push("Files");
    return types;
  }
  /** DataTransferItem kind/type metadata stays visible in protected mode. */
  get items() {
    const out = this.files.map((file) => ({ kind: "file", type: file.type ?? "" }));
    for (const type of this.#data.keys()) {
      if (type !== "Files") out.push({ kind: "string", type });
    }
    return out;
  }
}

export function dragEvent(type, { dataTransfer = new FakeDataTransfer(), relatedTarget = null, ...keys } = {}) {
  const ev = new Event(type, { bubbles: true, cancelable: true });
  Object.assign(ev, { dataTransfer, relatedTarget, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...keys });
  return ev;
}

export function pointerEvent(type, { pointerId = 1, button = 0, clientX = 0, clientY = 0, ...keys } = {}) {
  const ev = new Event(type, { bubbles: true, cancelable: true });
  Object.assign(ev, { pointerId, button, clientX, clientY, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...keys });
  return ev;
}

export function otelRecorder() {
  const events = [];
  return { events, emitDndEvent(event) { events.push(event); } };
}
