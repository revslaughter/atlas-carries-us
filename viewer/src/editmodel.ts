/**
 * The editor's pure parts: turn a view state into a tour step's `open`,
 * describe a step's target in words, move list entries, build the layout
 * rows, make region IDs, and suggest regions from folders. Nothing here
 * touches the page, so the unit tests load this file directly. editor.ts
 * draws the editor; docs/studio.md describes it.
 *
 * @packageDocumentation
 */

import type { State } from "./scene";
import type { Provenance, View } from "./types";

/** A tour step's `open` as atlas.yaml holds it: by ID, or by a name that acu resolves. */
export interface OpenCfg {
  /** The kind of view that the step opens. */
  view: "overview" | "region" | "unit" | "symbol" | "trace";
  /** The exact ID of a region, unit or symbol. */
  id?: string;
  /** A unit's path from the repository root. */
  path?: string;
  /** A symbol's qualified name, which acu resolves. */
  name?: string;
  /** The ID or name of the entry point that a trace starts from. */
  entryPoint?: string;
  /** The ID or name of the function that a trace starts from. */
  symbol?: string;
}

/** A tour step as atlas.yaml holds it. */
export interface StepCfg {
  /** The step's title. */
  title: string;
  /** The step's text, in the inline subset. */
  text: string;
  /** The view that the step opens. */
  open: OpenCfg;
  /** The provenance mark of text that a program drafted. */
  drafted?: Provenance;
}

/** A region as atlas.yaml holds it. */
export interface RegionCfg {
  /** The region's ID: letters, digits, "_" and "-". */
  id: string;
  /** The region's name, by job. */
  name: string;
  /** An index from 0 to 8 into the palette; acu picks one when it lacks. */
  color?: number;
  /** Glob patterns over unit paths; a unit joins the first region that matches. */
  paths?: string[];
  /** True when the region takes the units that match no other region. */
  default?: boolean;
  /** What the region does. */
  description?: string;
  /** How to read the region. */
  readingTip?: string;
  /** The provenance mark of text that a program drafted. */
  drafted?: Provenance;
}

/** One unit as GET /api/config lists it, with the region that holds it now. */
export interface UnitInfo {
  /** The unit's ID. */
  id: string;
  /** The unit's path from the repository root. */
  path: string;
  /** The unit's path from its language's root. */
  local: string;
  /** The unit's name. */
  name: string;
  /** The ID of the region that holds the unit on the map on show. */
  region?: string | null;
}

/**
 * Turns the view on show into a step's `open`, by ID so the step keeps
 * working after a rebuild. `memberId` finds a member's ID from its owner
 * and its name.
 */
export function stateToOpen(st: State, memberId: (owner: string, member: string) => string | undefined): OpenCfg {
  switch (st.v) {
    case "overview":
      return st.region ? { view: "region", id: st.region } : { view: "overview" };
    case "unit":
      return { view: "unit", id: st.id };
    case "symbol":
      return { view: "symbol", id: (st.member && memberId(st.id, st.member)) || st.id };
    case "trace":
      if (st.entry) return { view: "trace", entryPoint: st.entry };
      return { view: "trace", symbol: (st.sym && st.member && memberId(st.sym, st.member)) || st.sym || "" };
  }
}

/**
 * Turns an `open` that the editor recorded, which holds IDs, into the view
 * type of the data; an open without a target gives null. Call it only on
 * recorded opens: a hand-written open may name its target by name.
 */
export function openToView(o: OpenCfg): View | null {
  switch (o.view) {
    case "overview":
      return { view: "overview" };
    case "region":
      return o.id ? { view: "region", id: o.id } : null;
    case "unit":
      return o.id ? { view: "unit", id: o.id } : null;
    case "symbol":
      return o.id ? { view: "symbol", id: o.id } : null;
    case "trace":
      if (o.entryPoint) return { view: "trace", entryPoint: o.entryPoint };
      return o.symbol ? { view: "trace", symbol: o.symbol } : null;
  }
}

/** Describes what a step opens, in words, such as "the unit internal/store". */
export function describeOpen(o: OpenCfg, label: (id: string) => string = id => id): string {
  const t = o.id ? label(o.id) : (o.name ?? o.path ?? o.entryPoint ?? (o.symbol ? label(o.symbol) : ""));
  switch (o.view) {
    case "overview":
      return "the overview";
    case "region":
      return "the region " + t;
    case "unit":
      return "the unit " + t;
    case "symbol":
      return "the item " + t;
    case "trace":
      return "the trace from " + (o.entryPoint ? label(o.entryPoint) : t);
  }
}

/** Returns a copy of a list with the entry at `from` moved to `to`; positions out of range give an unchanged copy. */
export function move<T>(list: readonly T[], from: number, to: number): T[] {
  const out = list.slice();
  if (from < 0 || from >= out.length || to < 0 || to >= out.length || from === to) return out;
  const [x] = out.splice(from, 1);
  out.splice(to, 0, x);
  return out;
}

/**
 * Builds the layout rows from each region's row number; regions without one
 * stay out, and acu places them after. Within a row, regions keep their
 * place from the file (`col`), and new ones follow in list order.
 */
export function layoutRows(regions: { id: string; row: number | null; col?: number | null }[]): string[][] {
  const rows = new Map<number, { id: string; col: number; i: number }[]>();
  regions.forEach((r, i) => {
    if (r.row == null || !(r.row >= 1)) return;
    const n = Math.floor(r.row);
    if (!rows.has(n)) rows.set(n, []);
    rows.get(n)!.push({ id: r.id, col: r.col ?? Infinity, i });
  });
  return [...rows.keys()]
    .sort((a, b) => a - b)
    .map(k =>
      rows
        .get(k)!
        .sort((a, b) => a.col - b.col || a.i - b.i)
        .map(x => x.id),
    );
}

/** Reads each region's row (from 1) and place in that row (from 0) from the layout rows of atlas.yaml. */
export function rowsOf(layout: { rows?: string[][] } | null | undefined): Map<string, { row: number; col: number }> {
  const m = new Map<string, { row: number; col: number }>();
  (layout?.rows ?? []).forEach((row, i) => row.forEach((id, c) => m.has(id) || m.set(id, { row: i + 1, col: c })));
  return m;
}

/** Points the steps that open a renamed region at its new ID, and returns the new list. */
export function renameSteps(tour: StepCfg[], renames: Map<string, string>): StepCfg[] {
  return tour.map(s =>
    s.open.view === "region" && s.open.id && renames.has(s.open.id)
      ? { ...s, open: { ...s.open, id: renames.get(s.open.id)! } }
      : s,
  );
}

/** Names the steps that open a region that the person deleted, as sentences for the reader. */
export function orphanSteps(tour: StepCfg[], deleted: Set<string>): string[] {
  const out: string[] = [];
  tour.forEach((s, i) => {
    if (s.open.view === "region" && s.open.id && deleted.has(s.open.id))
      out.push(`Step ${i + 1} opens the region ${s.open.id}, which you deleted; point the step at another view.`);
  });
  return out;
}

/** Makes a region ID from a name: lower case letters, digits and "-", unique among `taken`. */
export function slug(name: string, taken: Iterable<string> = []): string {
  const base =
    name
      .toLowerCase()
      .normalize("NFKD")
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "region";
  const used = new Set(taken);
  let id = base,
    n = 2;
  while (used.has(id)) id = `${base}-${n++}`;
  return id;
}

/** Reports whether a region ID has the shape that the config schema asks for. */
export function validId(id: string): boolean {
  return /^[A-Za-z0-9_-]+$/.test(id);
}

/**
 * Suggests one region per top-level folder of the units' paths (from the
 * language root), as acu does when the config names no regions. A program
 * writes these names, so each carries a drafted mark.
 */
export function suggestRegions(units: UnitInfo[], mark: Provenance): RegionCfg[] {
  const tops = new Map<string, number>();
  for (const u of units) {
    const top = u.local === "." ? "." : u.local.split("/")[0];
    tops.set(top, (tops.get(top) ?? 0) + 1);
  }
  const out: RegionCfg[] = [];
  const taken: string[] = [];
  [...tops.keys()].sort().forEach((top, i) => {
    const name = top === "." ? "Root" : top;
    const id = slug(name, taken);
    taken.push(id);
    out.push({
      id,
      name,
      color: i % 9,
      paths: top === "." ? ["."] : [top, top + "/**"],
      drafted: { ...mark },
    });
  });
  return out;
}

/** Reports whether a list of path patterns names a unit's path literally, from the repository root or from its language's root. */
export function hasPath(paths: string[], u: UnitInfo): boolean {
  return paths.includes(u.local) || paths.includes(u.path);
}

/**
 * Adds or removes a unit's path from a list of patterns, and returns the new
 * list. It adds the path from the repository root, which no other unit
 * shares; it removes both forms of the path.
 */
export function togglePath(paths: string[], u: UnitInfo, on: boolean): string[] {
  const rest = paths.filter(p => p !== u.local && p !== u.path);
  return on ? [...rest, u.path] : rest;
}

/** Splits the text of the paths field into patterns: one per line, blank lines dropped. */
export function parsePaths(text: string): string[] {
  return text
    .split("\n")
    .map(s => s.trim())
    .filter(Boolean);
}

/** Lists what a step or region still needs before a save, as sentences for the reader. */
export function problems(tour: StepCfg[], regions: RegionCfg[], projectName: string): string[] {
  const out: string[] = [];
  if (!projectName.trim()) out.push("Give the project a name.");
  tour.forEach((s, i) => {
    if (!s.title.trim()) out.push(`Give step ${i + 1} a title.`);
    if (!s.text.trim()) out.push(`Give step ${i + 1} a text.`);
  });
  const seen = new Set<string>();
  regions.forEach((r, i) => {
    if (!r.name.trim()) out.push(`Give region ${i + 1} a name.`);
    if (!validId(r.id)) out.push(`Give region ${i + 1} an ID of letters, digits, "_" and "-".`);
    else if (seen.has(r.id)) out.push(`Give region ${i + 1} an ID that no other region uses.`);
    seen.add(r.id);
  });
  return out;
}

/** Removes the keys whose value is empty, so a save writes no empty fields into atlas.yaml. */
export function clean<T extends object>(o: T): T {
  const out: any = {};
  for (const [k, v] of Object.entries(o)) {
    if (v === undefined || v === null || v === "" || (Array.isArray(v) && v.length === 0)) continue;
    out[k] = v;
  }
  return out;
}
