"""Fold schema-v1 output back into the prototype codemap shape and compare counts.

Usage: python3 tools/prototype-parity.py <prototype codemap.json> <v1 atlas.json>

The prototype extractor (the "Backend Atlas" codemap format) recorded type-level
symbols and edges. This script maps member symbols to their owners and v1 edge
kinds to the old kinds, so the two outputs can be compared count for count.
"""
import json, sys
from collections import defaultdict
proto, v1 = json.load(open(sys.argv[1])), json.load(open(sys.argv[2]))
for k in ("routes", "mcalls", "edges", "symbols"): proto[k] = proto.get(k) or []
proto["tests"] = proto.get("tests") or {}
mod = proto["meta"]["module"]
syms = {s["id"]: s for s in v1["symbols"]}
conts = {c["id"]: c for c in v1["containers"]}
def short(ip): return ip[len(mod):].lstrip("/") if ip.startswith(mod) else ip
def top(id):
    s = syms.get(id)
    while s and s["parent"] in syms: s = syms[s["parent"]]
    return s
def pid(s):
    pkg, path = s["id"][3:].split("#", 1)
    if s["kind"] in ("constant", "variable"):
        return short(pkg) + "#values:" + s["location"]["path"].split("/")[-1]
    return short(pkg) + "." + path
tops = {pid(s) for s in v1["symbols"] if s["parent"] not in syms}
edges = set()
for e in v1["edges"]:
    if e["kind"] == "imports": continue
    a, b = top(e["from"]), top(e["to"])
    if not a or not b: continue
    A, B = pid(a), pid(b)
    k = e["kind"]
    if k == "field_type": k = "embeds" if "embedded" in syms[e["from"]].get("modifiers", []) else "field"
    elif k == "extends": k = "embeds"
    elif k == "param_type": k, A, B = "in", B, A
    elif k == "return_type": k = "out"
    if A != B: edges.add((A, B, k))
pedges = {(e["from"], e["to"], e["kind"]) for e in proto["edges"]}
mcalls = [e for e in v1["edges"] if e["kind"] == "calls"]
tests = defaultdict(set)
for k, uses in (v1.get("tests") or {}).items():
    t = top(k)
    if t: tests[pid(t)] |= {u["name"] for u in uses}
http = [e for e in v1.get("entryPoints", []) if e["kind"] == "http"]
rows = [("symbols", len(proto["symbols"]), len(tops)), ("edges", len(pedges), len(edges)),
        ("member calls", len(proto["mcalls"]), len(mcalls)), ("routes", len(proto["routes"]), len(http)),
        ("symbols with tests", len(proto["tests"]), len(tests))]
print(f"{'':20s}{'prototype':>10s}{'v1 folded':>11s}")
for n, a, b in rows: print(f"{n:20s}{a:>10d}{b:>11d}  {'same' if a == b else 'DIFF'}")
psyms = {s["id"] for s in proto["symbols"]}
for name, x, y in [("symbols only in prototype", psyms, tops), ("symbols only in v1", tops, psyms),
                   ("edges only in prototype", pedges, edges), ("edges only in v1", edges, pedges)]:
    d = sorted(x - y)
    if d: print(name, len(d), d[:8])
