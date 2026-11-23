// Scenes: what the canvas draws for each view. A node holds draw operations
// relative to its corner, plus rows that react to hover and click.

import { andList, plural, type Item, type Model, type RegionInfo, type Route } from "./model";
import { regionColor, MONO, SANS } from "./theme";
import { firstPara, inlinePlain, summary } from "./text";
import type { Symbol } from "./types";

/**
 * Where the reader stands: the overview (perhaps zoomed to a region or a
 * unit), a unit focus, an item focus (perhaps with a member picked), or a
 * trace from an entry point or a callable. The address bar and the history
 * hold states.
 */
export type State =
  | { v: "overview"; region?: string; focusUnit?: string }
  | { v: "unit"; id: string }
  | { v: "symbol"; id: string; member?: string }
  | { v: "trace"; entry?: string; sym?: string; member?: string };

/** A band of a node that reacts to hover and click: a member's row, or a unit's line in a neighbour box. */
export interface Row {
  /** The band's top, from the node's top. */
  y: number;
  /** The band's height. */
  h: number;
  /** The anchor name that lines to this row end at, such as a member's name. */
  anchor: string;
  /** The text that the status line shows on hover, when it differs from the item's doc. */
  doc?: string;
  /** The item that the row stands for. */
  item?: string;
  /** The member that the row stands for. */
  member?: string;
  /** What a click on the row does. */
  onClick?: () => void;
}

/** One draw operation: a run of text at a place, in a font and a colour. */
export interface Op {
  /** The left edge, from the node's corner. */
  x: number;
  /** The baseline, from the node's corner. */
  y: number;
  /** The text. */
  s: string;
  /** The CSS font. */
  f: string;
  /** A theme token, such as "fg" or "muted", or a colour. */
  c: string;
}

/** One box on the canvas, with its text already laid out as draw operations. */
export interface Node {
  /** The box's role: a unit in the overview ("room"), the focus ("center"), a neighbour, a "+N more" box, a lane heading, a trace step or a route. */
  t: "room" | "center" | "nbr" | "more" | "lane" | "trace" | "route";
  /** A stable name for the box, so a box that stays between views moves instead of reappearing. */
  key: string;
  /** The width. */
  w: number;
  /** The height, set once the content is laid out. */
  h: number;
  /** The left edge, in world units. */
  x: number;
  /** The top edge, in world units. */
  y: number;
  /** The text to draw. */
  ops: Op[];
  /** The bands that react to the pointer. */
  rows: Row[];
  /** The layout cursor: where the next line of text goes while the box is built. */
  cy: number;
  /** The heights, from the top, where lines end: one per member, and "" for the box itself. */
  anchors: Record<string, number>;
  /** The item that the box shows. */
  item?: string;
  /** The member that the box shows, in a trace. */
  member?: string;
  /** The unit that the box shows. */
  unit?: string;
  /** The entry point that the box shows. */
  route?: Route;
  /** The region of the box, which gives its colour. */
  region?: RegionInfo;
  /** The relation of a lane heading, which gives its line style. */
  edge?: string;
  /** -1 for the left side (what uses the focus), 1 for the right (what it uses). */
  side?: number;
  /** The lane that holds the box. */
  lane?: string;
  /** The call depth of a trace step. */
  depth?: number;
  /** A room's left edge inside its region. */
  rx?: number;
  /** A room's top edge inside its region. */
  ry?: number;
  /** The lines to exact members that a hover on the box reveals, in a unit focus. */
  detail?: SEdge[];
  /** What a click on the box does. */
  onClick?: () => void;
  /** The text of a lane heading or a "+N more" box. */
  label?: string;
}

/** One line on the canvas, from the user to the thing used. */
export interface SEdge {
  /** The box that uses. */
  a: Node;
  /** The box used. */
  b: Node;
  /** The anchor in `a` where the line starts. */
  aAnchor?: string;
  /** The anchor in `b` where the line ends. */
  bAnchor?: string;
  /** The relation, which gives the line's colour, dash and verb. */
  kind: string;
  /** The line's width; thicker lines carry more uses. */
  w?: number;
  /** A text on the line, such as a call's number in a trace. */
  label?: string | null;
  /** Draws the line fainter, as context. */
  faint?: boolean;
  /** Draws a name-matched relation with a dotted line. */
  heuristic?: boolean;
}

/** One region in the overview: its box, its rooms and its wrapped description. */
export interface RegionBox {
  /** The region. */
  r: RegionInfo;
  /** The boxes of its units. */
  rooms: Node[];
  /** The width. */
  w: number;
  /** The height. */
  h: number;
  /** The left edge, in world units. */
  x: number;
  /** The top edge, in world units. */
  y: number;
  /** The font of the title. */
  titleF: string;
  /** The font of the description. */
  descF: string;
  /** The description, wrapped into lines. */
  desc: string[];
}

/** What the canvas draws for one view: boxes, lines and their bounds. */
export interface Scene {
  /** The kind of view. */
  t: "overview" | "unit" | "symbol" | "trace";
  /** A name for the view, so a rebuild of the same view keeps the camera. */
  key: string;
  /** The boxes. */
  nodes: Node[];
  /** The lines. */
  edges: SEdge[];
  /** The focus box of a focus view. */
  center?: Node;
  /** The box around every node, in world units, for the camera. */
  bounds: { x: number; y: number; w: number; h: number };
  /** Marks an item focus with no relations, which the viewer explains. */
  empty?: boolean;
  /** The overview's regions. */
  regions?: RegionBox[];
  /** The overview's unit boxes. */
  rooms?: Node[];
  /** The overview's lines between regions, each with the items it reaches. */
  links?: { from: RegionBox; to: RegionBox; targets: Set<string> }[];
  /** The state that built the scene. */
  st?: State;
  /** The ID of the focus. */
  id?: string;
}

/* Text measurement and node building. */

/** A hidden canvas that measures text. */
const measure = document.createElement("canvas").getContext("2d")!;

/** Measures the width of a text in a font. */
export function tw(s: string, f: string): number {
  measure.font = f;
  return measure.measureText(s).width;
}
/** Cuts a text with "…" so that it fits a width. */
export function fit(s: string, f: string, max: number): string {
  if (tw(s, f) <= max) return s;
  let lo = 0,
    hi = s.length;
  while (lo < hi) {
    const m = (lo + hi + 1) >> 1;
    if (tw(s.slice(0, m) + "…", f) <= max) lo = m;
    else hi = m - 1;
  }
  return s.slice(0, lo) + "…";
}
/** Wraps a text into lines that fit a width, at most maxLines, the last cut with "…" when the text runs on. */
export function wrap(s: string, f: string, max: number, maxLines = 3): string[] {
  const words = (s || "").split(/\s+/).filter(Boolean),
    lines: string[] = [];
  let cur = "";
  for (const w of words) {
    const t = cur ? cur + " " + w : w;
    if (tw(t, f) <= max) {
      cur = t;
      continue;
    }
    if (cur) lines.push(cur);
    cur = w;
    if (lines.length === maxLines) break;
  }
  if (lines.length < maxLines && cur) lines.push(cur);
  if (lines.length === maxLines && words.join(" ").length > lines.join(" ").length)
    lines[maxLines - 1] = fit(lines[maxLines - 1] + " …", f, max);
  return lines.map(l => fit(l, f, max));
}

/** The padding inside a box. */
const PAD = 12;

/** Writes a CSS font: weight, size, and the sans-serif or monospace stack. */
export function F(w: number, size: number, mono?: boolean): string {
  return `${w} ${size}px ${mono ? MONO() : SANS()}`;
}
/** Makes an empty box of a width, with its cursor at the top padding. */
function newNode(t: Node["t"], key: string, w: number, extra: Partial<Node> = {}): Node {
  return Object.assign({ t, key, w, h: 0, x: 0, y: 0, ops: [], rows: [], cy: PAD, anchors: {} }, extra) as Node;
}
/** How {@link text} lays out a line or a paragraph. */
interface TextOpt {
  /** The left edge; the default is the padding. */
  x?: number;
  /** The widest a line may run; the default is the box's width less padding. */
  maxW?: number;
  /** The line height; the default is 1.45 times the font size. */
  lh?: number;
  /** Wraps the text over lines instead of cutting it. */
  wrap?: boolean;
  /** The most lines that a wrapped text may take. */
  maxLines?: number;
}

/** Adds text to a box at its cursor and moves the cursor down; returns the number of lines. */
function text(n: Node, s: string, f: string, c: string, opt: TextOpt = {}): number {
  const x = opt.x ?? PAD,
    max = opt.maxW ?? n.w - x - PAD,
    lh = opt.lh ?? Math.round(parseFloat(f.split(" ")[1]) * 1.45);
  const lines = opt.wrap ? wrap(s, f, max, opt.maxLines ?? 3) : [fit(s, f, max)];
  for (const l of lines) {
    n.ops.push({ x, y: n.cy + lh * 0.72, s: l, f, c });
    n.cy += lh;
  }
  return lines.length;
}
/** Adds one line made of runs in different fonts and colours, such as a name then its type. */
function segs(n: Node, parts: { s: string; f: string; c: string }[], lh: number, x = PAD) {
  let cx = x;
  const max = n.w - PAD;
  for (const p of parts) {
    const room = max - cx;
    if (room < 10) break;
    const s = fit(p.s, p.f, room);
    n.ops.push({ x: cx, y: n.cy + lh * 0.72, s, f: p.f, c: p.c });
    cx += tw(s, p.f);
  }
  n.cy += lh;
}
/** Moves a box's cursor down by g. */
function gap(n: Node, g: number) {
  n.cy += g;
}
/** Closes a box: its height follows its content. */
function done(n: Node): Node {
  n.h = n.cy + PAD - 4;
  return n;
}

/** Closes a scene: its bounds take in every node. */
function finish(sc: Omit<Scene, "bounds"> & { bounds?: Scene["bounds"] }): Scene {
  let x0 = Infinity,
    y0 = Infinity,
    x1 = -Infinity,
    y1 = -Infinity;
  for (const n of sc.nodes) {
    x0 = Math.min(x0, n.x);
    y0 = Math.min(y0, n.y);
    x1 = Math.max(x1, n.x + n.w);
    y1 = Math.max(y1, n.y + n.h);
  }
  sc.bounds = { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
  return sc as Scene;
}

/** The text that a box shows for code without a doc comment. */
const NO_DOC = "No doc comment.";

/* Shared helpers over the model. */

/** Writes a callable's types in short: "(string, number) Book". */
export function sigTypes(s: Symbol | undefined): string {
  const sig = s?.signature;
  if (!sig) return "";
  const res =
    sig.results.length === 0
      ? ""
      : sig.results.length === 1 && !sig.results[0].name
        ? " " + sig.results[0].type
        : " (" + sig.results.map(r => (r.name ? r.name + " " : "") + r.type).join(", ") + ")";
  return "(" + sig.params.map(p => p.type).join(", ") + ")" + res;
}

/** The kinds that a call can reach; a member of one of them shows its signature. */
const CALLABLE = new Set(["method", "constructor", "function", "macro"]);
/** The order of kinds in a list of items: contracts first, then types, then functions, then values. */
const KIND_ORDER = [
  "interface",
  "trait",
  "class",
  "struct",
  "enum",
  "type",
  "type_alias",
  "function",
  "macro",
  "namespace",
  "event",
  "values",
];

/** Reports whether a member embeds another type, such as an embedded struct field or interface. */
export function isEmbedded(m: Symbol): boolean {
  return (m.modifiers ?? []).includes("embedded");
}

/** Groups an item's members into sections: data members, then callables. */
export function memberSections(it: Item): { title: string; members: Symbol[] }[] {
  if (it.kind === "values") return [{ title: it.languageKind || "values", members: it.members }];
  const data = it.members.filter(m => !CALLABLE.has(m.kind));
  const calls = it.members.filter(m => CALLABLE.has(m.kind));
  const title = (ms: Symbol[]) => andList([...new Set(ms.map(m => plural(m.languageKind)))]);
  const out = [];
  if (data.length) out.push({ title: title(data), members: data });
  if (calls.length) out.push({ title: title(calls), members: calls });
  return out;
}

/** Writes what follows a member's name on its row: its signature's types, its value, or its type. */
export function memberRest(m: Symbol): string {
  if (CALLABLE.has(m.kind)) return sigTypes(m);
  if (m.kind === "constant" || m.kind === "variable")
    return m.value ? " = " + m.value : m.type ? " " + m.type.text : "";
  return m.type ? " " + m.type.text : "";
}

/* Overview: regions, then units. */

/** The width of a unit's box in the overview. */
const ROOM_W = 440,
  /** The space between unit boxes. */
  ROOM_GAP = 36,
  /** The padding inside a region's box. */
  REG_PAD = 40,
  /** The space between regions in a row. */
  REG_GAPX = 200,
  /** The space between rows of regions. */
  ROW_GAP = 240;
/** The zoom at which the overview starts to show the unit boxes inside each region. */
export const OV_ROOMS_AT = 0.42,
  /** The zoom at which a zoom into a unit box opens its unit focus. */
  OV_ENTER_AT = 1.5;

/**
 * Lays out the overview: each region as a box of unit boxes (name, first doc
 * paragraph, most-used items, entry points), the regions in the layout's
 * rows, and one line from region to region for each pair that uses the other.
 */
export function buildOverview(M: Model): Scene {
  const regions: RegionBox[] = [],
    rooms: Node[] = [];
  for (const r of M.regions) {
    const rs: Node[] = [];
    for (const id of r.units) {
      const u = M.units.get(id)!;
      const n = newNode("room", "u:" + id, ROOM_W, { unit: id, region: r });
      n.cy = 16;
      text(n, u.name, F(700, 19, true), "fg", { lh: 26 });
      gap(n, 4);
      const doc = firstPara(u.c.doc);
      text(n, doc || `No doc comment on this ${u.c.languageKind}.`, F(400, 14), doc ? "muted" : "faint", {
        wrap: true,
        maxLines: 5,
        lh: 21,
      });
      const kt = M.keyItems(id);
      if (kt.length) {
        gap(n, 8);
        text(n, "MOST USED", F(600, 10.5), "faint", { lh: 16 });
        text(n, kt.map(s => s.name).join("  ·  "), F(600, 13.5, true), "fg", { wrap: true, maxLines: 2, lh: 20 });
      }
      const rc = M.routeCountByUnit.get(id);
      if (rc) {
        gap(n, 6);
        text(n, `${rc} entry point${rc > 1 ? "s" : ""}`, F(500, 12.5), "muted", { lh: 18 });
      }
      done(n);
      rs.push(n);
      rooms.push(n);
    }
    const cols = rs.length <= 2 ? Math.max(1, rs.length) : rs.length <= 4 ? 2 : 3;
    const regW = Math.max(cols * ROOM_W + (cols - 1) * ROOM_GAP + REG_PAD * 2, 760);
    const g: RegionBox = { r, rooms: rs, w: regW, x: 0, y: 0, h: 0, titleF: F(700, 34), descF: F(400, 16), desc: [] };
    g.desc = wrap(inlinePlain(r.description), g.descF, regW - REG_PAD * 2, 6);
    const head = 40 + 34 + 12 + g.desc.length * 24 + 20;
    const colH = new Array(cols).fill(head);
    rs.forEach(n => {
      const c = colH.indexOf(Math.min(...colH));
      n.rx = REG_PAD + c * (ROOM_W + ROOM_GAP);
      n.ry = colH[c];
      colH[c] += n.h + ROOM_GAP;
    });
    g.h = rs.length ? Math.max(...colH) - ROOM_GAP + REG_PAD : head + REG_PAD;
    regions.push(g);
  }
  const byId = new Map(regions.map(g => [g.r.id, g]));
  const rows = M.rows.map(row => row.map(id => byId.get(id)!).filter(Boolean));
  const widths = rows.map(row => row.reduce((a, g) => a + g.w, 0) + REG_GAPX * (row.length - 1));
  const W = Math.max(...widths, 1);
  let y = 0;
  rows.forEach((row, i) => {
    let x = (W - widths[i]) / 2,
      rh = 0;
    for (const g of row) {
      g.x = x;
      g.y = y;
      x += g.w + REG_GAPX;
      rh = Math.max(rh, g.h);
    }
    y += rh + ROW_GAP;
  });
  for (const g of regions)
    for (const n of g.rooms) {
      n.x = g.x + n.rx!;
      n.y = g.y + n.ry!;
    }
  const links = new Map<string, { from: RegionBox; to: RegionBox; targets: Set<string> }>();
  for (const x of M.uses) {
    const a = M.regionOfItem(x.u),
      b = M.regionOfItem(x.t);
    if (!a || !b || a === b) continue;
    const k = a.id + ">" + b.id;
    if (!links.has(k)) links.set(k, { from: byId.get(a.id)!, to: byId.get(b.id)!, targets: new Set() });
    links.get(k)!.targets.add(x.t);
  }
  return {
    t: "overview",
    key: "overview",
    nodes: [],
    edges: [],
    regions,
    rooms,
    links: [...links.values()],
    bounds: { x: 0, y: 0, w: W, h: Math.max(1, y - ROW_GAP) },
  };
}

/* Symbol focus. */

/** The width of the focus box. */
const CW = 440,
  /** The width of a neighbour box. */
  NW = 290,
  /** The space between the focus box and the lanes on each side. */
  LANE_GAP = 120,
  /** The space between boxes in a lane. */
  NODE_GAP = 10,
  /** The height that a lane's heading takes. */
  LANE_HEAD = 30,
  /** The space between lanes. */
  LANE_SEP = 26;

/** One lane of an item focus: a column of neighbours that share one relation. */
export interface Lane {
  /** The lane's ID, also its key in a {@link Bag}. */
  id: string;
  /** The heading, as a reader says it: "Called by", "Field types". */
  label: string;
  /** The relation, which gives the line style. */
  edge: string;
}

/** The lanes on the left of an item focus: what uses the item, by relation. */
export const LANES_LEFT: Lane[] = [
  { id: "calledBy", label: "Called by", edge: "calls" },
  { id: "implBy", label: "Implemented by", edge: "implements" },
  { id: "extBy", label: "Extended by", edge: "extends" },
  { id: "takesIt", label: "Take it as a parameter", edge: "param" },
  { id: "returnsIt", label: "Return it", edge: "result" },
  { id: "holdsIt", label: "Have a field of this type", edge: "field" },
  { id: "overriddenBy", label: "Overridden by", edge: "overrides" },
];
/** The lanes on the right of an item focus: what the item uses, by relation. */
export const LANES_RIGHT: Lane[] = [
  { id: "calls", label: "Calls", edge: "calls" },
  { id: "impl", label: "Implements", edge: "implements" },
  { id: "ext", label: "Extends", edge: "extends" },
  { id: "params", label: "Parameter types", edge: "param" },
  { id: "results", label: "Result types", edge: "result" },
  { id: "fields", label: "Field types", edge: "field" },
  { id: "overrides", label: "Overrides", edge: "overrides" },
];

/** One neighbour of an item focus, in one lane. */
export interface RelItem {
  /** The neighbour item. */
  id: string;
  /** The members through which the relation runs, shown as "via …". */
  via: Set<string>;
  /** The anchors in the focus box where its lines end: members, parameters or the head. */
  anchors: Set<string>;
  /** Marks a neighbour that only name-matched edges join. */
  heuristic: boolean;
  /** The rank inside its lane; more members, exported, and contracts rank higher. */
  score?: number;
}

/** The neighbours of an item focus, lane by lane: lane ID to neighbour ID to neighbour. */
export type Bag = Record<string, Map<string, RelItem>>;

/**
 * Sorts every relation of an item into lanes: callers, implementers,
 * subtypes and users of its type on the left; callees, contracts, base types
 * and the types it names on the right. Each neighbour keeps the members on
 * both ends, so that lines end at the exact row.
 */
export function symbolRelations(M: Model, id: string): { L: Bag; R: Bag } {
  const it = M.items.get(id)!,
    L: Bag = {},
    R: Bag = {};
  const add = (bag: Bag, lane: string, other: string, via: string[], anchors: string[], heuristic: boolean) => {
    if (other === id || !M.items.has(other)) return;
    const l = bag[lane] || (bag[lane] = new Map());
    const x = l.get(other) || { id: other, via: new Set<string>(), anchors: new Set<string>(), heuristic: true };
    for (const v of via) if (v) x.via.add(v);
    for (const a of anchors) x.anchors.add(a);
    x.heuristic = x.heuristic && heuristic;
    l.set(other, x);
  };
  const anchor = (m: string) => (m && it.members.some(x => x.name === m) ? "m:" + m : "head");
  const name = (i: string) => M.items.get(i)?.name ?? "";
  for (const m of ["", ...it.members.map(x => x.name)]) {
    for (const c of M.mcIn.get(id + "\0" + m) ?? [])
      add(L, "calledBy", c.a, [c.am || name(c.a)], [anchor(m)], c.heuristic);
    for (const c of M.mc.get(id + "\0" + m) ?? []) add(R, "calls", c.b, [c.bm || name(c.b)], [anchor(m)], c.heuristic);
  }
  const paramAnchors = (b: string, which: "params" | "results", am: string) => {
    if (am) return [anchor(am)];
    const out: string[] = [];
    (it.sym?.signature?.[which] ?? []).forEach((p, i) => {
      if ((p.refs ?? []).includes(b)) out.push((which === "params" ? "p:" : "r:") + i);
    });
    return out.length ? out : ["head"];
  };
  for (const r of M.inc.get(id) ?? []) {
    const via = [r.am || name(r.a)];
    if (r.kind === "implements") add(L, "implBy", r.a, [], ["head"], r.heuristic);
    if (r.kind === "extends") add(L, "extBy", r.a, [], ["head"], r.heuristic);
    if (r.kind === "return_type") add(L, "returnsIt", r.a, via, ["head"], r.heuristic);
    if (r.kind === "field_type") add(L, "holdsIt", r.a, [r.am], ["head"], r.heuristic);
    if (r.kind === "param_type") add(L, "takesIt", r.a, via, ["head"], r.heuristic);
    if (r.kind === "overrides") add(L, "overriddenBy", r.a, [r.am], [anchor(r.bm)], r.heuristic);
  }
  for (const r of M.out.get(id) ?? []) {
    if (r.kind === "implements") add(R, "impl", r.b, [], ["head"], r.heuristic);
    if (r.kind === "extends") add(R, "ext", r.b, [], ["head"], r.heuristic);
    if (r.kind === "param_type") add(R, "params", r.b, [], paramAnchors(r.b, "params", r.am), r.heuristic);
    if (r.kind === "return_type") add(R, "results", r.b, [], paramAnchors(r.b, "results", r.am), r.heuristic);
    if (r.kind === "field_type") add(R, "fields", r.b, [], [anchor(r.am)], r.heuristic);
    if (r.kind === "overrides") add(R, "overrides", r.b, [r.bm], [anchor(r.am)], r.heuristic);
  }
  return { L, R };
}

/** Ranks a lane's neighbours: more members involved first, then exported ones, then contracts, then by name. */
export function rankItems(M: Model, map: Map<string, RelItem>): RelItem[] {
  return [...map.values()]
    .map(x => {
      const s = M.items.get(x.id)!;
      return { ...x, score: x.via.size * 3 + (s.exported ? 2 : 0) + (M.isInterfaceLike(x.id) ? 1 : 0) };
    })
    .sort((a, b) => b.score! - a.score! || M.qual(a.id).localeCompare(M.qual(b.id)));
}

/** The line above an item's name: its word and its unit, such as "STRUCT · internal/store". */
function kindLabel(M: Model, id: string): string {
  const it = M.items.get(id)!;
  return `${(it.languageKind || it.kind).toUpperCase()}  ·  ${M.units.get(it.unit)?.name ?? ""}`;
}

/**
 * Builds the focus box of an item focus: its kind and unit, its name, its
 * first doc paragraph, then a row per parameter, result and member (up to
 * 34), each an anchor that lines end at.
 */
function centerSymNode(M: Model, id: string): Node {
  const it = M.items.get(id)!,
    r = M.regionOfItem(id),
    n = newNode("center", "s:" + id, CW, { item: id, region: r });
  n.cy = 14;
  text(n, kindLabel(M, id), F(600, 10.5), r ? regionColor(r.color) : "muted", { lh: 16 });
  text(
    n,
    it.name + (it.sym?.typeParams ?? it.sym?.signature?.typeParams ?? ""),
    F(700, 20, true),
    it.exported ? "fg" : "muted",
    { lh: 28 },
  );
  n.anchors.head = n.cy - 14;
  const doc = firstPara(it.doc);
  text(n, doc || NO_DOC, F(400, 13.5), doc ? "muted" : "faint", { wrap: true, maxLines: 5, lh: 20 });
  const sec = (name: string) => {
    gap(n, 8);
    text(n, name.toUpperCase(), F(600, 10.5), "faint", { lh: 16 });
  };
  const row = (anchor: string, name: string, rest: string, doc?: string, member?: string) => {
    const y0 = n.cy;
    segs(
      n,
      [
        { s: name, f: F(600, 12.5, true), c: "fg" },
        { s: rest, f: F(400, 12.5, true), c: "muted" },
      ],
      20,
    );
    n.rows.push({ y: y0, h: 20, anchor, doc, member });
    n.anchors[anchor] = y0 + 10;
  };
  let shown = 0,
    hidden = 0;
  const CAP = 34;
  const lim = (fn: () => void) => {
    if (shown < CAP) {
      fn();
      shown++;
    } else hidden++;
  };
  const s = it.sym;
  if ((it.kind === "type" || it.kind === "type_alias") && s?.type) {
    sec("definition");
    row("def", it.kind === "type_alias" ? "alias of" : "of", " " + s.type.text);
  }
  if (s?.signature && it.kind !== "type" && it.kind !== "type_alias") {
    sec("parameters");
    if (!s.signature.params.length) text(n, "none", F(400, 12.5), "faint", { lh: 20 });
    s.signature.params.forEach((p, i) => row("p:" + i, p.name || "_", " " + p.type));
    sec("results");
    if (!s.signature.results.length) text(n, "none", F(400, 12.5), "faint", { lh: 20 });
    s.signature.results.forEach((p, i) => row("r:" + i, p.name || "", (p.name ? " " : "") + p.type));
  }
  for (const g of memberSections(it)) {
    sec(g.title);
    for (const m of g.members)
      lim(() =>
        isEmbedded(m)
          ? row("m:" + m.name, m.type?.text ?? m.name, "  (embedded)", summary(m.doc), m.name)
          : row("m:" + m.name, m.name, memberRest(m), summary(m.doc), m.name),
      );
  }
  if (hidden) text(n, `+ ${hidden} more in the panel`, F(400, 12), "faint", { lh: 18 });
  return done(n);
}

/** Builds a neighbour box: kind and unit, name, the members it goes through, and its first doc sentence. */
function nbrSymNode(M: Model, id: string, via: Set<string>, edge: string): Node {
  const it = M.items.get(id)!,
    r = M.regionOfItem(id),
    n = newNode("nbr", "s:" + id, NW, { item: id, region: r, edge });
  n.cy = 10;
  text(n, kindLabel(M, id), F(600, 10), r ? regionColor(r.color) : "muted", { lh: 15 });
  text(n, it.name, F(700, 14.5, true), it.exported ? "fg" : "muted", { lh: 21 });
  const v = [...via].filter(Boolean);
  if (v.length)
    text(
      n,
      "via " + v.slice(0, 4).join(", ") + (v.length > 4 ? ` +${v.length - 4}` : ""),
      F(400, 11.5, true),
      "muted",
      { wrap: true, maxLines: 2, lh: 16 },
    );
  const d = summary(it.doc);
  text(n, d || NO_DOC, F(400, 12), "faint", { wrap: true, maxLines: 2, lh: 17 });
  return done(n);
}

/** Builds a "+N more" box that opens the rest of a lane or a trace step. */
function moreNode(key: string, count: number, onClick: () => void, label?: string): Node {
  const n = newNode("more", key, NW, { onClick, label: label || `+ ${count} more` });
  n.cy = 8;
  text(n, n.label!, F(600, 12.5), "fg", { lh: 18 });
  return done(n);
}

/** Builds a lane's heading: its line style, its verb and its count. */
function laneHead(key: string, lane: Lane, total: number, side: number, w = NW): Node {
  const n = newNode("lane", key, w, { edge: lane.edge, side, label: lane.label });
  n.cy = 2;
  text(n, `${lane.label}  (${total})`, F(600, 12.5), "fg", { lh: 18, x: 34 });
  n.h = 22;
  return n;
}

/** What the scene builders need from the app: what the reader opened, and a way to rebuild after a click. */
export interface Ctx {
  /** How many boxes each lane shows, and whether a unit shows its unexported items, per view. */
  laneShow: Record<string, Record<string, number | boolean>>;
  /** The trace steps that the reader expanded, per trace. */
  traceOpen: Record<string, Record<string, boolean>>;
  /** Rebuilds the scene on show. */
  rebuild: () => void;
}

/**
 * Lays out an item focus: the item in the centre, what uses it in lanes on
 * the left, what it uses in lanes on the right. Each lane shows three to five
 * boxes at first, to keep the view near twenty boxes.
 */
export function buildSymScene(M: Model, id: string, cx: Ctx): Scene {
  const key = "sym:" + id;
  const show = (cx.laneShow[key] ||= {});
  const c = centerSymNode(M, id);
  const { L, R } = symbolRelations(M, id);
  const nodes: Node[] = [c],
    edges: SEdge[] = [];
  const place = (lanes: Lane[], bag: Bag, side: number) => {
    let y = 0;
    const x = side < 0 ? -LANE_GAP - NW : CW + LANE_GAP;
    const active = lanes.filter(l => bag[l.id]?.size);
    const def = active.length >= 3 ? 3 : 5; // keep the view near twenty boxes
    for (const lane of active) {
      const items = rankItems(M, bag[lane.id]);
      const h = laneHead(key + ":" + lane.id, lane, items.length, side);
      h.x = x;
      h.y = y;
      nodes.push(h);
      y += LANE_HEAD;
      const n0 = (show[lane.id] as number) || def;
      for (const x2 of items.slice(0, n0)) {
        const n = nbrSymNode(M, x2.id, x2.via, lane.edge);
        n.x = x;
        n.y = y;
        n.lane = lane.id;
        if (nodes.some(o => o.key === n.key)) n.key += "@" + lane.id;
        nodes.push(n);
        y += n.h + NODE_GAP;
        for (const a of x2.anchors)
          edges.push(
            side < 0
              ? { a: n, b: c, bAnchor: a, kind: lane.edge, heuristic: x2.heuristic }
              : { a: c, aAnchor: a, b: n, kind: lane.edge, heuristic: x2.heuristic },
          );
      }
      if (items.length > n0) {
        const m = moreNode(key + ":more:" + lane.id, items.length - n0, () => {
          show[lane.id] = n0 + 10;
          cx.rebuild();
        });
        m.x = x;
        m.y = y;
        nodes.push(m);
        y += m.h + NODE_GAP;
      }
      y += LANE_SEP;
    }
    return y;
  };
  const hl = place(LANES_LEFT, L, -1),
    hr = place(LANES_RIGHT, R, 1);
  for (const [side, h] of [
    [-1, hl],
    [1, hr],
  ] as const) {
    const dy = Math.max(-200, Math.min(0, (c.h - h) / 2));
    for (const n of nodes) if (n !== c && Math.sign(n.x) === side) n.y += dy;
  }
  return finish({ t: "symbol", key, id, nodes, edges, center: c, empty: hl + hr === 0 });
}

/* Unit focus. */

/**
 * Lays out a unit focus: the unit in the centre with a row per exported item,
 * the units that use it on the left and the units it uses on the right. A
 * hover on a neighbour shows the exact items involved; a line's width grows
 * with the items it carries.
 */
export function buildUnitScene(M: Model, uid: string, cx: Ctx): Scene {
  const key = "unit:" + uid;
  const show = (cx.laneShow[key] ||= {});
  const u = M.units.get(uid)!,
    r = M.regionOfUnit.get(uid);
  const c = newNode("center", "u:" + uid, CW + 40, { unit: uid, region: r });
  c.cy = 14;
  text(c, `${u.c.languageKind.toUpperCase()}  ·  ${r?.name ?? ""}`, F(600, 10.5), regionColor(r?.color), { lh: 16 });
  text(c, u.name, F(700, 20, true), "fg", { lh: 28 });
  c.anchors.head = c.cy - 14;
  const d = firstPara(u.c.doc);
  text(c, d || `No doc comment on this ${u.c.languageKind}.`, F(400, 13.5), d ? "muted" : "faint", {
    wrap: true,
    maxLines: 6,
    lh: 20,
  });
  const order = (k: string) => {
    const i = KIND_ORDER.indexOf(k);
    return i < 0 ? KIND_ORDER.length : i;
  };
  const sorter = (a: Item, b: Item) =>
    order(a.kind) - order(b.kind) || a.languageKind.localeCompare(b.languageKind) || a.name.localeCompare(b.name);
  const exp = u.items.filter(s => s.exported).sort(sorter),
    unexp = u.items.filter(s => !s.exported).sort(sorter);
  let lastKind: string | null = null;
  const listRow = (s: Item) => {
    const k = s.kind === "values" ? "values" : s.languageKind;
    if (k !== lastKind) {
      gap(c, 8);
      const label =
        s.kind === "values"
          ? andList([
              ...new Set(u.items.filter(x => x.kind === "values").flatMap(x => x.languageKind.split(/, | and /))),
            ])
          : plural(s.languageKind);
      text(c, label.toUpperCase(), F(600, 10.5), "faint", { lh: 16 });
      lastKind = k;
    }
    const y0 = c.cy;
    segs(
      c,
      [
        { s: s.name, f: F(600, 12.5, true), c: s.exported ? "fg" : "muted" },
        { s: "  " + summary(s.doc), f: F(400, 12), c: "faint" },
      ],
      20,
    );
    c.rows.push({ y: y0, h: 20, anchor: "s:" + s.id, item: s.id });
    c.anchors["s:" + s.id] = y0 + 10;
  };
  exp.forEach(listRow);
  if (unexp.length) {
    if (show.unexported) {
      lastKind = null;
      gap(c, 10);
      text(c, `NOT EXPORTED (USED ONLY INSIDE THE ${u.c.languageKind.toUpperCase()})`, F(600, 10.5), "faint", {
        lh: 16,
      });
      unexp.forEach(listRow);
    } else {
      gap(c, 8);
      const y0 = c.cy;
      text(c, `Show ${unexp.length} items that only this ${u.c.languageKind} uses`, F(600, 12.5), "link", { lh: 20 });
      c.rows.push({
        y: y0,
        h: 20,
        anchor: "un",
        onClick: () => {
          show.unexported = true;
          cx.rebuild();
        },
      });
    }
  }
  done(c);
  const usedBy = new Map<string, { unit: string; targets: Set<string>; users: Set<string> }>();
  const usesU = new Map<string, { unit: string; targets: Set<string>; users: Set<string> }>();
  for (const s of u.items) {
    for (const x of M.users.get(s.id) ?? []) {
      const q = M.items.get(x.u)!.unit;
      if (q === uid) continue;
      const e = usedBy.get(q) || { unit: q, targets: new Set(), users: new Set() };
      e.targets.add(s.id);
      e.users.add(x.u);
      usedBy.set(q, e);
    }
    for (const x of M.useds.get(s.id) ?? []) {
      const q = M.items.get(x.t)!.unit;
      if (q === uid) continue;
      const e = usesU.get(q) || { unit: q, targets: new Set(), users: new Set() };
      e.targets.add(x.t);
      e.users.add(s.id);
      usesU.set(q, e);
    }
  }
  const nodes: Node[] = [c],
    edges: SEdge[] = [];
  const word = plural(u.c.languageKind);
  const place = (map: typeof usedBy, side: number, lbl: string) => {
    const items = [...map.values()].sort((a, b) => b.targets.size - a.targets.size || a.unit.localeCompare(b.unit));
    let y = 0;
    const w = NW + 40,
      x = side < 0 ? -LANE_GAP - w : c.w + LANE_GAP;
    if (!items.length) return 0;
    const h = laneHead(key + ":" + lbl, { id: lbl, label: lbl, edge: "uses" }, items.length, side, w);
    h.x = x;
    h.y = y;
    nodes.push(h);
    y += LANE_HEAD;
    for (const e of items) {
      const ou = M.units.get(e.unit)!,
        rg = M.regionOfUnit.get(e.unit);
      const n = newNode("nbr", "u:" + e.unit, w, { unit: e.unit, region: rg, edge: "uses" });
      n.cy = 10;
      text(n, `${ou.c.languageKind.toUpperCase()}  ·  ${rg?.name ?? ""}`, F(600, 10), regionColor(rg?.color), {
        lh: 15,
      });
      text(n, ou.name, F(700, 14, true), "fg", { lh: 21 });
      const names = [...e.targets].map(t => M.items.get(t)!.name);
      text(
        n,
        "uses " + names.slice(0, 8).join(", ") + (names.length > 8 ? ` +${names.length - 8} more` : ""),
        F(400, 11.5, true),
        "muted",
        { wrap: true, maxLines: 3, lh: 16 },
      );
      done(n);
      n.x = x;
      n.y = y;
      nodes.push(n);
      y += n.h + NODE_GAP;
      n.detail =
        side < 0
          ? [...e.targets].map(t => ({ a: n, b: c, bAnchor: "s:" + t, kind: "uses" }))
          : [...e.users].map(us => ({ a: c, aAnchor: "s:" + us, b: n, kind: "uses" }));
      const wgt = Math.min(5, 1 + e.targets.size / 3);
      edges.push(
        side < 0
          ? { a: n, b: c, bAnchor: "head", kind: "uses", w: wgt }
          : { a: c, aAnchor: "head", b: n, kind: "uses", w: wgt },
      );
    }
    return y;
  };
  const hl = place(usedBy, -1, `Used by these ${word}`),
    hr = place(usesU, 1, `Uses these ${word}`);
  for (const [side, h] of [
    [-1, hl],
    [1, hr],
  ] as const) {
    const dy = Math.max(-200, Math.min(0, (c.h - h) / 2));
    for (const n of nodes) if (n !== c && Math.sign(n.x) === side) n.y += dy;
  }
  return finish({ t: "unit", key, id: uid, nodes, edges, center: c });
}

/* Request trace. */

/** The width of a trace step. */
const TW = 320,
  /** The space between the columns of a trace. */
  TCOL = 150;

/** Names each kind of entry point for readers: "HTTP route", "Command" and so on. */
export const ENTRY_KIND: Record<string, string> = {
  http: "HTTP route",
  cli: "Command",
  message: "Message handler",
  job: "Scheduled job",
  grpc: "RPC method",
};

/** Finds where a trace starts: the handler of an entry point (the whole item for an inline handler), or a callable. */
export function traceRoot(
  M: Model,
  st: Extract<State, { v: "trace" }>,
): { item: string; member: string; route?: Route } | null {
  if (st.entry) {
    const r = M.routeById.get(st.entry);
    if (!r) return null;
    return { item: r.item, member: r.inline ? "" : r.member, route: r };
  }
  if (!st.sym || !M.items.has(st.sym)) return null;
  return { item: st.sym, member: st.member || "" };
}

/**
 * Lists the next steps of a trace from one callable: its calls in source
 * order, then, for an interface member, each implementation that has the
 * member, and each override; those last two draw as "runs as".
 */
export function traceChildren(M: Model, item: string, member: string) {
  const out: { item: string; member: string; line: number; kind: string }[] = [];
  for (const c of M.mc.get(item + "\0" + member) ?? [])
    out.push({ item: c.b, member: c.bm, line: c.line, kind: "calls" });
  if (M.isInterfaceLike(item) && member)
    for (const impl of M.implBy.get(item) ?? [])
      if (M.member(impl, member)) out.push({ item: impl, member, line: 0, kind: "runs" });
  for (const o of M.overriddenBy.get(item + "\0" + member) ?? [])
    out.push({ item: o.a, member: o.am, line: 0, kind: "runs" });
  out.sort((a, b) => Number(a.kind === "runs") - Number(b.kind === "runs") || a.line - b.line);
  return out;
}

/** Finds the note from atlas.yaml that applies to an entry point, by kind and path prefix. */
export function noteFor(M: Model, r: Route): { title?: string; text: string } | undefined {
  return (M.doc.project?.entryPointNotes ?? []).find(
    n => (!n.kind || n.kind === r.kind) && (!n.pathPrefix || (r.path ?? "").startsWith(n.pathPrefix)),
  );
}

/** Builds one step of a trace: the code's kind, name and first doc sentence. */
function traceNode(M: Model, key: string, item: string, member: string, depth: number): Node {
  const it = M.items.get(item)!,
    r = M.regionOfItem(item),
    n = newNode("trace", key, TW, { item, member, region: r, depth });
  n.cy = 10;
  const m = member ? M.member(item, member) : undefined;
  const lk = m ? `${it.languageKind} ${m.languageKind}` : it.languageKind;
  text(n, `${lk.toUpperCase()}  ·  ${M.units.get(it.unit)?.name ?? ""}`, F(600, 10), regionColor(r?.color), { lh: 15 });
  text(n, member ? `${it.name}.${member}` : it.name, F(700, 14, true), "fg", { lh: 21 });
  const d = summary(m ? m.doc : it.doc);
  text(n, d || NO_DOC, F(400, 12), d ? "muted" : "faint", { wrap: true, maxLines: 3, lh: 17 });
  return done(n);
}

/**
 * Lays out a trace: the entry point, then each call in order, one column per
 * call depth, through each interface to the code that can run. It shows six
 * calls per step and four levels at first, at most 70 boxes; "Expand" opens
 * the rest. A step that the trace already shows gets a line, not a second box.
 */
export function buildTraceScene(M: Model, st: Extract<State, { v: "trace" }>, cx: Ctx): Scene | null {
  const root = traceRoot(M, st);
  if (!root) return null;
  const key = "trace:" + (st.entry || root.item + "#" + root.member);
  const open = (cx.traceOpen[key] ||= {});
  const nodes: Node[] = [],
    edges: SEdge[] = [],
    byKey = new Map<string, Node>();
  let x0 = 0;
  if (root.route) {
    const r = root.route;
    const rn = newNode("route", "route:" + r.id, 260, { route: r });
    rn.cy = 12;
    text(rn, (ENTRY_KIND[r.kind] ?? r.kind).toUpperCase(), F(600, 10), "muted", { lh: 15 });
    if (r.method) text(rn, r.method, F(700, 13, true), "fg", { lh: 19 });
    text(rn, r.path ?? r.name, F(700, 14, true), "fg", { wrap: true, maxLines: 3, lh: 20 });
    const note = noteFor(M, r);
    if (note) {
      gap(rn, 4);
      text(rn, inlinePlain(note.text), F(400, 11.5), "faint", { wrap: true, maxLines: 3, lh: 16 });
    }
    if (r.externalHandler) {
      gap(rn, 4);
      text(rn, `Served by ${r.externalHandler}, outside this code.`, F(400, 11.5), "faint", {
        wrap: true,
        maxLines: 3,
        lh: 16,
      });
    } else if (r.inline) {
      gap(rn, 4);
      text(rn, "The handler sits inline in the code that registers it.", F(400, 11.5), "faint", {
        wrap: true,
        maxLines: 3,
        lh: 16,
      });
    }
    done(rn);
    nodes.push(rn);
    x0 = 260 + TCOL;
  }
  const colY: number[] = [];
  let budget = 70;
  const visit = (
    item: string,
    member: string,
    depth: number,
    parent: Node | null,
    kind: string,
    order: number,
  ): Node | null => {
    const k = "m:" + item + "#" + member;
    if (byKey.has(k)) {
      if (parent) edges.push({ a: parent, b: byKey.get(k)!, kind, label: kind === "calls" ? String(order) : null });
      return null;
    }
    if (budget-- <= 0) return null;
    const n = traceNode(M, k, item, member, depth);
    n.x = x0 + depth * (TW + TCOL);
    n.y = Math.max(colY[depth] || 0, parent ? parent.y : 0);
    colY[depth] = n.y + n.h + 14;
    nodes.push(n);
    byKey.set(k, n);
    if (parent) edges.push({ a: parent, b: n, kind, label: kind === "calls" ? String(order) : null });
    const kids = traceChildren(M, item, member);
    const lim = open[k] ? kids.length : depth >= 4 ? 0 : 6;
    let i = 0;
    for (const ch of kids.slice(0, lim)) {
      if (ch.kind === "calls") i++;
      visit(ch.item, ch.member, depth + 1, n, ch.kind, i);
    }
    if (kids.length > lim) {
      const lbl =
        lim === 0
          ? `Expand: ${kids.length} more step${kids.length > 1 ? "s" : ""}`
          : `+ ${kids.length - lim} more calls`;
      const m = moreNode(
        "more:" + k,
        kids.length - lim,
        () => {
          open[k] = true;
          cx.rebuild();
        },
        lbl,
      );
      m.w = 220;
      m.x = x0 + (depth + 1) * (TW + TCOL);
      m.y = Math.max(colY[depth + 1] || 0, n.y);
      colY[depth + 1] = m.y + m.h + 14;
      nodes.push(m);
      edges.push({ a: n, b: m, kind: "calls", faint: true });
    }
    return n;
  };
  const first = visit(root.item, root.member, 0, null, "calls", 0);
  if (root.route && first) edges.unshift({ a: nodes[0], b: first, kind: "calls", label: "serves" });
  return finish({ t: "trace", key, st, nodes, edges, center: nodes[0] });
}
