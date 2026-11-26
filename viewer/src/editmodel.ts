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
  view: "overview" | "region" | "unit" | "symbol" | "trace";
  id?: string;
  path?: string;
  name?: string;
  entryPoint?: string;
  symbol?: string;
}

/** A tour step as atlas.yaml holds it. */
export interface StepCfg {
  title: string;
  text: string;
  open: OpenCfg;
  drafted?: Provenance;
}

/** A region as atlas.yaml holds it. */
export interface RegionCfg {
  id: string;
  name: string;
  color?: number;
  paths?: string[];
  default?: boolean;
  description?: string;
  readingTip?: string;
  drafted?: Provenance;
}

/** One unit as GET /api/config lists it, with the region that holds it now. */
export interface UnitInfo {
  id: string;
  path: string;
  local: string;
  name: string;
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

/** Builds the layout rows from each region's row number; regions without one stay out, and acu places them after. */
export function layoutRows(regions: { id: string; row: number | null }[]): string[][] {
  const rows = new Map<number, string[]>();
  for (const r of regions) {
    if (r.row == null || !(r.row >= 1)) continue;
    const n = Math.floor(r.row);
    if (!rows.has(n)) rows.set(n, []);
    rows.get(n)!.push(r.id);
  }
  return [...rows.keys()].sort((a, b) => a - b).map(k => rows.get(k)!);
}

/** Reads each region's row number (from 1) from the layout rows of atlas.yaml. */
export function rowsOf(layout: { rows?: string[][] } | null | undefined): Map<string, number> {
  const m = new Map<string, number>();
  (layout?.rows ?? []).forEach((row, i) => row.forEach(id => m.has(id) || m.set(id, i + 1)));
  return m;
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

/** Reports whether a list of path patterns names a unit's path literally. */
export function hasPath(paths: string[], u: UnitInfo): boolean {
  return paths.includes(u.local) || paths.includes(u.path);
}

/** Adds or removes a unit's path from a list of patterns, and returns the new list. */
export function togglePath(paths: string[], u: UnitInfo, on: boolean): string[] {
  const rest = paths.filter(p => p !== u.local && p !== u.path);
  return on ? [...rest, u.local] : rest;
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
