#!/usr/bin/env python3
"""Generate the DndSessionTrace conformance corpus.

The traces are the executable specification of the ores.dnd/v1 session state
machine (docs/DESIGN.md §Session). Every runtime replays them; a runtime that
disagrees fails. Regenerate with `python3 scripts/gen-traces.py` and commit the
output under contracts/instances/DndSessionTrace/valid/.
"""
import json, os, sys

ROOT = os.path.join(os.path.dirname(__file__), "..")
OUT = os.path.join(ROOT, "contracts", "instances", "DndSessionTrace", "valid")

def item(kind="text", media="text/plain", data="hello", name=None):
    d = {"kind": kind, "mediaType": media, "data": data}
    if name is not None: d["name"] = name
    return d

def env(drag_id="drag-0001", ops=("copy", "move"), items=None, form=None, runtime="typescript-webview"):
    e = {
        "protocol": "ores.dnd/v1",
        "dragId": drag_id,
        "sourceRuntime": runtime,
        "allowedOperations": list(ops),
        "items": items if items is not None else [item()],
    }
    if form is not None: e["formId"] = form
    return e

def policy(target, ops=("copy", "move"), kinds=("text",), media=None, max_items=None, max_bytes=None, form=None):
    p = {"targetId": target, "allowedOperations": list(ops), "acceptedKinds": list(kinds)}
    if media is not None: p["acceptedMediaTypes"] = list(media)
    if max_items is not None: p["maxItems"] = max_items
    if max_bytes is not None: p["maxTotalBytes"] = max_bytes
    if form is not None: p["formId"] = form
    return p

E1 = env()
E2 = env("drag-0002")
P_OK = policy("zone-a")
P_COPY = policy("zone-b", ops=("copy",))
P_JSON = policy("zone-json", kinds=("json",))
P_MD = policy("zone-md", ops=("copy",), media=("text/markdown",))
P_WILD = policy("zone-any-text", ops=("copy",), media=("text/*",))
P_ONE = policy("zone-one", ops=("copy",), max_items=1)
P_TINY = policy("zone-tiny", ops=("copy",), max_bytes=3)
P_FORM = policy("zone-form", ops=("copy",), form="form-x")
P_LINK = policy("zone-link", ops=("link",))

def start(e): return {"kind": "start", "envelope": e}
def enter(p, preferred=None):
    i = {"kind": "enter", "targetId": p["targetId"], "policy": p}
    if preferred: i["preferredOperation"] = preferred
    return i
def leave(t): return {"kind": "leave", "targetId": t}
def drop(t): return {"kind": "drop", "targetId": t}
CANCEL = {"kind": "cancel"}
END = {"kind": "end"}

def snap(state, drag=None, target=None, op=None, err=None):
    s = {"state": state}
    if drag: s["dragId"] = drag
    if target: s["targetId"] = target
    if op: s["operation"] = op
    if err: s["errorCode"] = err
    return s

IDLE = snap("idle")
D1 = snap("dragging", "drag-0001")

TRACES = [
    ("basic-drop", "start → enter accepting target (move wins by default order) → drop",
     [start(E1), enter(P_OK), drop("zone-a")],
     [D1, snap("over-target", "drag-0001", "zone-a", "move"), snap("dropped", "drag-0001", "zone-a", "move")]),
    ("preferred-copy", "a preferred operation wins when both sides allow it",
     [start(E1), enter(P_OK, "copy"), drop("zone-a")],
     [D1, snap("over-target", "drag-0001", "zone-a", "copy"), snap("dropped", "drag-0001", "zone-a", "copy")]),
    ("preferred-unavailable-falls-back", "an unavailable preferred operation falls back to move → copy → link",
     [start(E1), enter(P_COPY, "move")],
     [D1, snap("over-target", "drag-0001", "zone-b", "copy")]),
    ("leave-then-cancel", "leaving the current target returns to dragging; cancel is terminal",
     [start(E1), enter(P_OK), leave("zone-a"), CANCEL],
     [D1, snap("over-target", "drag-0001", "zone-a", "move"), D1, snap("cancelled", "drag-0001", err="cancelled")]),
    ("end-without-drop", "dragend with no drop cancels",
     [start(E1), END],
     [D1, snap("cancelled", "drag-0001", err="cancelled")]),
    ("drop-without-target", "a drop with no active target fails closed",
     [start(E1), drop("zone-a")],
     [D1, snap("cancelled", "drag-0001", "zone-a", err="no-active-target")]),
    ("drop-on-other-target", "a drop naming a target other than the active one fails closed",
     [start(E1), enter(P_OK), drop("zone-b")],
     [D1, snap("over-target", "drag-0001", "zone-a", "move"), snap("cancelled", "drag-0001", "zone-b", err="target-mismatch")]),
    ("reject-item-kind", "a rejecting target is recorded with its reason; leaving clears it; dropping on it cancels with that reason",
     [start(E1), enter(P_JSON), leave("zone-json"), enter(P_JSON), drop("zone-json")],
     [D1, snap("dragging", "drag-0001", "zone-json", err="item-kind-not-accepted"), D1,
      snap("dragging", "drag-0001", "zone-json", err="item-kind-not-accepted"),
      snap("cancelled", "drag-0001", "zone-json", err="item-kind-not-accepted")]),
    ("reject-no-common-operation", "operation negotiation is evaluated before item rules",
     [start(E1), enter(P_LINK)],
     [D1, snap("dragging", "drag-0001", "zone-link", err="no-common-operation")]),
    ("reject-media-type-then-wildcard", "exact media types reject text/plain; a type/* wildcard accepts it",
     [start(E1), enter(P_MD), enter(P_WILD)],
     [D1, snap("dragging", "drag-0001", "zone-md", err="media-type-not-accepted"),
      snap("over-target", "drag-0001", "zone-any-text", "copy")]),
    ("reject-too-many-items", "maxItems is enforced against the envelope item count",
     [start(env(items=[item(), item(data="world")])), enter(P_ONE)],
     [D1, snap("dragging", "drag-0001", "zone-one", err="too-many-items")]),
    ("reject-payload-too-large", "maxTotalBytes is enforced against the UTF-8 byte length of all item data",
     [start(E1), enter(P_TINY)],
     [D1, snap("dragging", "drag-0001", "zone-tiny", err="payload-too-large")]),
    ("form-mismatch", "a policy formId must match an envelope formId when both are present",
     [start(env(form="form-y")), enter(P_FORM)],
     [D1, snap("dragging", "drag-0001", "zone-form", err="form-mismatch")]),
    ("form-match", "matching formIds accept; an envelope without formId is accepted by a form-bound policy",
     [start(env(form="form-x")), enter(P_FORM), drop("zone-form"), start(E2), enter(P_FORM)],
     [D1, snap("over-target", "drag-0001", "zone-form", "copy"), snap("dropped", "drag-0001", "zone-form", "copy"),
      snap("dragging", "drag-0002"), snap("over-target", "drag-0002", "zone-form", "copy")]),
    ("invalid-start-protocol", "an envelope with an unsupported protocol never starts a session",
     [start({**E1, "protocol": "ores.dnd/v9"}), enter(P_OK)],
     [snap("idle", err="invalid-envelope"), snap("idle", err="invalid-envelope")]),
    ("invalid-start-wrong-major", "a structurally valid envelope of another protocol major never starts a session",
     [start({**E1, "protocol": "ores.dnd/v2"}), enter(P_OK), drop("zone-a")],
     [snap("idle", err="invalid-envelope"), snap("idle", err="invalid-envelope"), snap("idle", err="invalid-envelope")]),
    ("restart-after-terminal", "start always begins a fresh session, even from a terminal state",
     [start(E1), CANCEL, start(E2)],
     [D1, snap("cancelled", "drag-0001", err="cancelled"), snap("dragging", "drag-0002")]),
    ("inputs-ignored-when-idle", "non-start inputs are ignored while idle",
     [enter(P_OK), leave("zone-a"), drop("zone-a"), CANCEL, END],
     [IDLE, IDLE, IDLE, IDLE, IDLE]),
    ("terminal-ignores-inputs", "dropped and cancelled are terminal for everything except start",
     [start(E1), enter(P_OK), drop("zone-a"), enter(P_COPY), leave("zone-a"), CANCEL, END],
     [D1, snap("over-target", "drag-0001", "zone-a", "move"), snap("dropped", "drag-0001", "zone-a", "move"),
      snap("dropped", "drag-0001", "zone-a", "move"), snap("dropped", "drag-0001", "zone-a", "move"),
      snap("dropped", "drag-0001", "zone-a", "move"), snap("dropped", "drag-0001", "zone-a", "move")]),
    ("reenter-switches-target", "entering another target implicitly leaves the current one; a stale leave is ignored",
     [start(E1), enter(P_OK), enter(P_COPY), leave("zone-a"), leave("zone-b")],
     [D1, snap("over-target", "drag-0001", "zone-a", "move"), snap("over-target", "drag-0001", "zone-b", "copy"),
      snap("over-target", "drag-0001", "zone-b", "copy"), D1]),
    ("enter-rejecting-clears-accepting", "moving from an accepting target onto a rejecting one drops the negotiated operation",
     [start(E1), enter(P_OK), enter(P_LINK)],
     [D1, snap("over-target", "drag-0001", "zone-a", "move"), snap("dragging", "drag-0001", "zone-link", err="no-common-operation")]),
    ("start-while-dragging", "a second start replaces the running session (hosts may miss dragend)",
     [start(E1), enter(P_OK), start(E2)],
     [D1, snap("over-target", "drag-0001", "zone-a", "move"), snap("dragging", "drag-0002")]),
    ("stale-leave-ignored", "a leave for a target that is not the current one changes nothing",
     [start(E1), leave("zone-a"), enter(P_OK), leave("zone-zzz")],
     [D1, D1, snap("over-target", "drag-0001", "zone-a", "move"), snap("over-target", "drag-0001", "zone-a", "move")]),
]

def main():
    os.makedirs(OUT, exist_ok=True)
    ids = set()
    for tid, desc, inputs, expected in TRACES:
        assert tid not in ids, tid; ids.add(tid)
        assert len(inputs) == len(expected), tid
        trace = {"id": tid, "description": desc, "inputs": inputs, "expected": expected}
        with open(os.path.join(OUT, f"{tid}.json"), "w") as f:
            json.dump(trace, f, indent=2); f.write("\n")
    print(f"wrote {len(TRACES)} traces to {os.path.relpath(OUT, ROOT)}")

if __name__ == "__main__":
    main()
