// The model: lookup tables over one atlas.json, shaped for the views.
//
// The views work with "items": top-level symbols (types and functions) and,
// for each file, one group of its constants and variables. A member (method,
// field, property) belongs to its item and appears as a row inside it.
// Edges between members roll up to their items, and keep the member names,
// so lines can end at the exact row.

import type { AtlasDoc, Container, Doc, EdgeKind, EntryPoint, Location, Region, Symbol, TestUse } from "./types";

export interface Item {
  id: string;
  sym?: Symbol;
  kind: string;
  languageKind: string;
  name: string;
  unit: string;
  exported: boolean;
  doc?: Doc;
  location: Location;
  members: Symbol[];
}

export interface Unit {
  id: string;
  c: Container;
  name: string;
  path: string;
  files: Container[];
  items: Item[];
  imports: string[];
  importedBy: string[];
}

export interface RegionInfo extends Region {
  color: number;
  units: string[];
}

/** One relation between two items, with the members at each end. */
export interface Rel {
  a: string;
  am: string;
  b: string;
  bm: string;
  kind: EdgeKind;
  line: number;
  heuristic: boolean;
}

export interface Use {
  u: string;
  t: string;
  kind: string;
}

export interface Route extends EntryPoint {
  item: string;
  member: string;
}

function push<K, V>(m: Map<K, V[]>, k: K, v: V) {
  let a = m.get(k);
  if (!a) m.set(k, (a = []));
  a.push(v);
}

const VALUE_KINDS = new Set(["constant", "variable"]);

export function plural(word: string): string {
  if (/(s|x|z|ch|sh)$/.test(word)) return word + "es";
  if (/[^aeiou]y$/.test(word)) return word.slice(0, -1) + "ies";
  return word + "s";
}

/** Joins words as "a", "a and b", "a, b and c". */
export function andList(words: string[]): string {
  if (words.length <= 1) return words.join("");
  return words.slice(0, -1).join(", ") + " and " + words[words.length - 1];
}

/** Joins words as "a", "a or b", "a, b or c". */
export function orList(words: string[]): string {
  if (words.length <= 1) return words.join("");
  return words.slice(0, -1).join(", ") + " or " + words[words.length - 1];
}

export class Model {
  doc: AtlasDoc;
  containers = new Map<string, Container>();
  symbols = new Map<string, Symbol>();
  units = new Map<string, Unit>();
  unitList: Unit[] = [];
  items = new Map<string, Item>();
  /** Any symbol ID to the ID of its item. */
  owner = new Map<string, string>();
  regions: RegionInfo[] = [];
  regionOfUnit = new Map<string, RegionInfo>();
  rows: string[][] = [];
  out = new Map<string, Rel[]>();
  inc = new Map<string, Rel[]>();
  /** Calls, keyed by "item\0member" of the caller (mc) or callee (mcIn). */
  mc = new Map<string, Rel[]>();
  mcIn = new Map<string, Rel[]>();
  implBy = new Map<string, string[]>();
  overriddenBy = new Map<string, Rel[]>();
  uses: Use[] = [];
  users = new Map<string, Use[]>();
  useds = new Map<string, Use[]>();
  routes: Route[] = [];
  routeById = new Map<string, Route>();
  routesByHandler = new Map<string, Route[]>();
  routeCountByUnit = new Map<string, number>();
  tests = new Map<string, TestUse[]>();

  constructor(doc: AtlasDoc) {
    this.doc = doc;
    for (const c of doc.containers) this.containers.set(c.id, c);
    for (const s of doc.symbols) this.symbols.set(s.id, s);
    this.buildUnits();
    this.buildItems();
    this.buildRelations();
    this.buildRoutes();
    this.buildTests();
    this.buildRegions();
  }

  /** The words that a set of units uses for itself, such as "folder" or "module", the most common first. */
  unitWords(units: Iterable<string> = this.units.keys()): string[] {
    const n = new Map<string, number>();
    for (const id of units) {
      const w = this.wordOf(id);
      n.set(w, (n.get(w) ?? 0) + 1);
    }
    return [...n.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(e => e[0]);
  }

  /** The word for one unit, as its adapter wrote it in `languageKind`. */
  wordOf(unit: string): string {
    return this.units.get(unit)?.c.languageKind || "unit";
  }

  /** Names any one unit of the map: "folder", or "module or folder" in a map of two languages. */
  anyUnit(): string {
    return orList(this.unitWords()) || "unit";
  }

  /** Names many units: "folders", or "modules and folders" in a map of two languages. */
  allUnits(units?: Iterable<string>): string {
    return andList(this.unitWords(units).map(plural)) || "units";
  }

  /** The unit that holds a symbol or container. */
  unitOf(id: string): string | undefined {
    let cur: string | undefined = id;
    for (let i = 0; cur && i < 64; i++) {
      if (this.units.has(cur)) return cur;
      cur = this.symbols.get(cur)?.parent ?? this.containers.get(cur)?.parent;
    }
    return undefined;
  }

  private buildUnits() {
    for (const c of this.doc.containers) {
      if (!c.unit) continue;
      const u: Unit = {
        id: c.id,
        c,
        name: c.name,
        path: c.path ?? "",
        files: [],
        items: [],
        imports: [],
        importedBy: [],
      };
      this.units.set(c.id, u);
      this.unitList.push(u);
    }
    for (const c of this.doc.containers) {
      if (c.kind !== "file" || !c.parent) continue;
      const u = this.unitOf(c.parent);
      if (u) this.units.get(u)!.files.push(c);
    }
    for (const u of this.unitList) u.files.sort((a, b) => a.name.localeCompare(b.name));
    this.unitList.sort((a, b) => a.path.localeCompare(b.path) || a.name.localeCompare(b.name));
  }

  private buildItems() {
    const groups = new Map<string, Item>();
    for (const s of this.doc.symbols) {
      const parentIsSymbol = this.symbols.has(s.parent);
      if (parentIsSymbol) continue;
      const unit = this.unitOf(s.id);
      if (!unit) continue;
      if (VALUE_KINDS.has(s.kind)) {
        const file = this.containers.get(s.parent);
        const gid = "values:" + (file?.id ?? s.parent);
        let g = groups.get(gid);
        if (!g) {
          const fname = file?.name ?? s.location.path.split("/").pop() ?? "";
          g = {
            id: gid,
            kind: "values",
            languageKind: "",
            name: "values in " + fname,
            unit,
            exported: false,
            location: { path: s.location.path, line: s.location.line },
            members: [],
          };
          groups.set(gid, g);
          this.items.set(gid, g);
          this.units.get(unit)!.items.push(g);
        }
        g.members.push(s);
        g.exported ||= s.exported;
        if (s.location.line < g.location.line) g.location = { path: s.location.path, line: s.location.line };
        this.owner.set(s.id, gid);
        continue;
      }
      const it: Item = {
        id: s.id,
        sym: s,
        kind: s.kind,
        languageKind: s.languageKind,
        name: s.name,
        unit,
        exported: s.exported,
        doc: s.doc,
        location: s.location,
        members: [],
      };
      this.items.set(s.id, it);
      this.owner.set(s.id, s.id);
      this.units.get(unit)!.items.push(it);
    }
    for (const g of groups.values()) {
      const kinds = [...new Set(g.members.map(m => plural(m.languageKind)))];
      g.languageKind = andList(kinds);
      g.members.sort((a, b) => a.location.line - b.location.line);
    }
    // Members: walk up to the nearest item.
    for (const s of this.doc.symbols) {
      if (this.owner.has(s.id)) continue;
      let cur: string | undefined = s.parent;
      for (let i = 0; cur && i < 16 && !this.items.has(cur); i++) cur = this.symbols.get(cur)?.parent;
      if (cur && this.items.has(cur)) {
        this.owner.set(s.id, cur);
        this.items.get(cur)!.members.push(s);
      }
    }
    for (const it of this.items.values()) {
      if (it.kind === "values") continue;
      it.members.sort(
        (a, b) =>
          Number(b.exported) - Number(a.exported) ||
          a.location.path.localeCompare(b.location.path) ||
          a.location.line - b.location.line,
      );
    }
  }

  /** The member name of a symbol inside its item, or "" for the item itself. */
  memberName(id: string): string {
    const o = this.owner.get(id);
    if (!o || o === id) return "";
    return this.symbols.get(id)?.name ?? "";
  }

  member(itemId: string, name: string): Symbol | undefined {
    return this.items.get(itemId)?.members.find(m => m.name === name);
  }

  private buildRelations() {
    for (const e of this.doc.edges) {
      if (e.kind === "imports") {
        const a = this.unitOf(e.from),
          b = this.unitOf(e.to);
        if (a && b && a !== b) {
          this.units.get(a)!.imports.push(b);
          this.units.get(b)!.importedBy.push(a);
        }
        continue;
      }
      const a = this.owner.get(e.from),
        b = this.owner.get(e.to);
      if (!a || !b) continue;
      const r: Rel = {
        a,
        am: this.memberName(e.from),
        b,
        bm: this.memberName(e.to),
        kind: e.kind,
        line: e.line ?? 0,
        heuristic: e.confidence === "heuristic",
      };
      push(this.out, a, r);
      push(this.inc, b, r);
      if (e.kind === "calls") {
        push(this.mc, a + "\0" + r.am, r);
        push(this.mcIn, b + "\0" + r.bm, r);
      }
      if (e.kind === "implements") push(this.implBy, b, a);
      if (e.kind === "overrides") push(this.overriddenBy, b + "\0" + r.bm, r);
      if (a !== b) {
        const kind =
          e.kind === "field_type"
            ? "field"
            : e.kind === "param_type"
              ? "param"
              : e.kind === "return_type"
                ? "result"
                : e.kind;
        this.uses.push({ u: a, t: b, kind });
      }
    }
    for (const list of this.mc.values()) list.sort((x, y) => x.line - y.line);
    for (const x of this.uses) {
      push(this.users, x.t, x);
      push(this.useds, x.u, x);
    }
    for (const u of this.unitList) {
      u.imports = [...new Set(u.imports)].sort();
      u.importedBy = [...new Set(u.importedBy)].sort();
    }
  }

  private buildRoutes() {
    for (const e of this.doc.entryPoints ?? []) {
      const item = this.owner.get(e.handler);
      if (!item) continue;
      const r: Route = { ...e, item, member: this.memberName(e.handler) };
      this.routes.push(r);
      this.routeById.set(r.id, r);
      push(this.routesByHandler, item, r);
      const u = this.items.get(item)!.unit;
      this.routeCountByUnit.set(u, (this.routeCountByUnit.get(u) ?? 0) + 1);
    }
  }

  private buildTests() {
    for (const [id, uses] of Object.entries(this.doc.tests ?? {})) {
      const it = this.owner.get(id);
      if (!it) continue;
      const list = this.tests.get(it) ?? [];
      for (const t of uses) if (!list.some(x => x.name === t.name && x.location.path === t.location.path)) list.push(t);
      this.tests.set(it, list);
    }
    for (const l of this.tests.values())
      l.sort((a, b) => a.location.path.localeCompare(b.location.path) || a.location.line - b.location.line);
  }

  private buildRegions() {
    const given = this.doc.regions?.length
      ? this.doc.regions
      : [{ id: "all", name: this.doc.project?.name ?? "All code", containers: this.unitList.map(u => u.id) }];
    const placed = new Set<string>();
    given.forEach((r, i) => {
      const units = r.containers.filter(id => this.units.has(id) && !placed.has(id));
      units.forEach(id => placed.add(id));
      if (!units.length) return;
      const ri: RegionInfo = { ...r, color: r.color ?? i % 9, units };
      this.regions.push(ri);
      for (const id of units) this.regionOfUnit.set(id, ri);
    });
    const rest = this.unitList.filter(u => !placed.has(u.id)).map(u => u.id);
    if (rest.length) {
      const ri: RegionInfo = {
        id: "other",
        name: "Other code",
        containers: rest,
        color: this.regions.length % 9,
        units: rest,
      };
      this.regions.push(ri);
      for (const id of rest) this.regionOfUnit.set(id, ri);
    }
    const ids = new Set(this.regions.map(r => r.id));
    const rows = (this.doc.layout?.rows ?? []).map(row => row.filter(id => ids.has(id))).filter(row => row.length);
    const inRows = new Set(rows.flat());
    const left = this.regions.map(r => r.id).filter(id => !inRows.has(id));
    for (let i = 0; i < left.length; i += 3) rows.push(left.slice(i, i + 3));
    this.rows = rows;
  }

  regionOfItem(id: string): RegionInfo | undefined {
    const it = this.items.get(id);
    return it ? this.regionOfUnit.get(it.unit) : undefined;
  }

  /** Users of an item from outside its unit. */
  outsideUsers(id: string): number {
    const it = this.items.get(id)!;
    return new Set((this.users.get(id) ?? []).filter(x => this.items.get(x.u)?.unit !== it.unit).map(x => x.u)).size;
  }

  /** The exported items that other code uses most: where to start reading. */
  keyItems(unit: string, n = 4): Item[] {
    const u = this.units.get(unit);
    if (!u) return [];
    return u.items
      .filter(s => s.exported && s.kind !== "values")
      .map(s => [this.outsideUsers(s.id) * 2 + new Set((this.users.get(s.id) ?? []).map(x => x.u)).size, s] as const)
      .filter(x => x[0] > 0)
      .sort((a, b) => b[0] - a[0])
      .slice(0, n)
      .map(x => x[1]);
  }

  /** "unitname.Item" for display. */
  qual(id: string): string {
    const it = this.items.get(id);
    if (!it) return id;
    const u = this.units.get(it.unit);
    const short = (u?.name ?? "").split("/").pop();
    return short ? short + "." + it.name : it.name;
  }

  isInterfaceLike(id: string): boolean {
    const k = this.items.get(id)?.kind;
    return k === "interface" || k === "trait";
  }
}
