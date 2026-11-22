// The running viewer: state and history, the camera, staged transitions,
// drawing, pointer and keyboard input, and the small bits of chrome (hint,
// status line, breadcrumbs, legend) that change with the view.

import type { Model } from "./model";
import {
  buildOverview, buildSymScene, buildTraceScene, buildUnitScene, F, OV_ENTER_AT, OV_ROOMS_AT, traceRoot, wrap,
  type Node, type Row, type Scene, type SEdge, type State, type RegionBox, ENTRY_KIND,
} from "./scene";
import { EDGE, T, color, edgeColor, readTheme, regionColor, rgba, SANS } from "./theme";
import { esc, inlinePlain, summary } from "./text";
import * as panel from "./panel";
import { tour } from "./tour";

/* State. */

export let M: Model;
let OV: Scene;
export let scene: Scene;
let cam = { k: 1, x: 0, y: 0 };
let camAnim: ((now: number) => void) | null = null;
let morph: { prev: Map<string, { x: number; y: number }>; t0: number; ms: number } | null = null;
interface Hit { node?: Node; row?: Row; region?: RegionBox }
let hover: Hit | null = null;
export let picked: Hit | null = null;
let kbd: Hit | null = null;
export let hist: State[] = [];
export let hi = -1;
let outAccum = 0;
let needDraw = true;
let dpr = 1;
const laneShow: Record<string, Record<string, number | boolean>> = {};
const traceOpen: Record<string, Record<string, boolean>> = {};
const cx = { laneShow, traceOpen, rebuild: () => rebuild() };

const canvas = document.getElementById("map") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;
const mini = document.getElementById("minimap") as HTMLCanvasElement;
const mctx = mini.getContext("2d")!;
let miniT: { s: number; ox: number; oy: number } | null = null;
const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
const announce = document.getElementById("announce")!;

export function current(): State { return hist[hi] ?? { v: "overview" }; }

/* Navigation. */

function buildScene(st: State): Scene {
  if (st.v === "unit" && M.units.has(st.id)) return buildUnitScene(M, st.id, cx);
  if (st.v === "symbol" && M.items.has(st.id)) return buildSymScene(M, st.id, cx);
  if (st.v === "trace") return buildTraceScene(M, st, cx) ?? OV;
  return OV;
}

function sameState(a: State | undefined, b: State): boolean {
  return !!a && JSON.stringify(a) === JSON.stringify(b);
}

export function go(st: State, opts: { noPush?: boolean } = {}) {
  if (st.v === "symbol" && !M.items.has(st.id)) {
    const owner = M.owner.get(st.id);
    if (!owner) st = { v: "overview" };
    else st = { v: "symbol", id: owner, member: M.memberName(st.id) || undefined };
  }
  if (!opts.noPush) {
    if (sameState(hist[hi], st)) return;
    hist = hist.slice(0, hi + 1);
    hist.push(st);
    hi = hist.length - 1;
  }
  const prev = screenRects();
  scene = buildScene(st);
  picked = null; hover = null; kbd = null; outAccum = 0;
  if (scene.t === "overview") setOverviewCam(st);
  else { camAnim = null; cam = fitCam(scene); }
  startMorph(prev);
  panel.renderFor(st);
  if (st.v === "symbol" && st.member) panel.symbol(st.id, { member: st.member });
  renderCrumbs(); updateNav(); writeHash(st); hint(); status(null); renderLegend();
  needDraw = true;
}

export function rebuild() {
  const prev = screenRects();
  scene = buildScene(current());
  startMorph(prev, true);
  needDraw = true;
}

export function back() { if (hi > 0) { hi--; go(hist[hi], { noPush: true }); } }
export function fwd() { if (hi < hist.length - 1) { hi++; go(hist[hi], { noPush: true }); } }

export function up() {
  const st = current();
  if (st.v === "symbol") go({ v: "unit", id: M.items.get(st.id)!.unit });
  else if (st.v === "unit") go({ v: "overview", focusUnit: st.id });
  else if (st.v === "trace") { const r = traceRoot(M, st); if (r) go({ v: "symbol", id: r.item }); }
  else if (st.v === "overview" && (st.region || st.focusUnit)) go({ v: "overview" });
}

function updateNav() {
  const b = document.getElementById("back") as HTMLButtonElement, f = document.getElementById("fwd") as HTMLButtonElement;
  b.disabled = hi <= 0; f.disabled = hi >= hist.length - 1;
  b.title = hi > 0 ? "Back to " + stateName(hist[hi - 1]) + " (Backspace)" : "Back";
  f.title = hi < hist.length - 1 ? "Forward to " + stateName(hist[hi + 1]) : "Forward";
}

export function stateName(st: State): string {
  if (st.v === "overview") return st.region ? (M.regions.find(r => r.id === st.region)?.name ?? "the overview") : "the overview";
  if (st.v === "unit") return M.units.get(st.id)?.name ?? st.id;
  if (st.v === "symbol") return M.qual(st.id) + (st.member ? "." + st.member : "");
  const r = traceRoot(M, st);
  return "the trace of " + (r?.route?.name ?? (r ? M.qual(r.item) + (r.member ? "." + r.member : "") : ""));
}

/* Links: every state has a stable address built from IDs. */

function writeHash(st: State) {
  let h = "overview";
  if (st.v === "overview" && st.region) h = "region=" + encodeURIComponent(st.region);
  else if (st.v === "unit") h = "unit=" + encodeURIComponent(st.id);
  else if (st.v === "symbol") {
    const m = st.member ? M.member(st.id, st.member) : undefined;
    h = "symbol=" + encodeURIComponent(m?.id ?? st.id);
  } else if (st.v === "trace") {
    if (st.entry) h = "entry=" + encodeURIComponent(st.entry);
    else if (st.sym) { const m = st.member ? M.member(st.sym, st.member) : undefined; h = "trace=" + encodeURIComponent(m?.id ?? st.sym); }
  }
  try { if (location.hash !== "#" + h) history.replaceState(null, "", "#" + h); } catch { /* sandboxed frames refuse */ }
}

export function readHash(): State {
  const h = (location.hash || "").slice(1);
  const i = h.indexOf("=");
  const k = i < 0 ? h : h.slice(0, i), v = i < 0 ? "" : decodeURIComponent(h.slice(i + 1));
  if (k === "region" && M.regions.some(r => r.id === v)) return { v: "overview", region: v };
  if (k === "unit" && M.units.has(v)) return { v: "unit", id: v };
  if (k === "symbol" && M.owner.has(v)) { const o = M.owner.get(v)!; const m = M.memberName(v); return m ? { v: "symbol", id: o, member: m } : { v: "symbol", id: o }; }
  if (k === "entry" && M.routeById.has(v)) return { v: "trace", entry: v };
  if (k === "trace" && M.owner.has(v)) return { v: "trace", sym: M.owner.get(v)!, member: M.memberName(v) };
  return { v: "overview" };
}

/* Camera. */

function vw() { return canvas.width / dpr; }
function vh() { return canvas.height / dpr; }
function fitRect(x: number, y: number, w: number, h: number, pad = 40) {
  const k = Math.min((vw() - pad * 2) / w, (vh() - pad * 2) / h);
  return { k, x: vw() / 2 - (x + w / 2) * k, y: vh() / 2 - (y + h / 2) * k };
}
function toWorld(sx: number, sy: number) { return { x: (sx - cam.x) / cam.k, y: (sy - cam.y) / cam.k }; }
function minK() { const b = scene.bounds; return Math.min(fitRect(b.x, b.y, b.w, b.h, 40).k * 0.7, 0.3); }

function setOverviewCam(st: State) {
  const b = OV.bounds;
  if (st.v === "overview" && st.focusUnit) {
    const n = OV.rooms!.find(r => r.unit === st.focusUnit);
    if (n) {
      const k = Math.min(1.0, fitRect(n.x - 300, n.y - 200, n.w + 600, n.h + 400).k);
      cam = { k, x: vw() / 2 - (n.x + n.w / 2) * k, y: vh() / 2 - (n.y + n.h / 2) * k };
      return;
    }
  }
  if (st.v === "overview" && st.region) {
    const g = OV.regions!.find(g => g.r.id === st.region);
    if (g) { cam = fitRect(g.x, g.y - 60, g.w, g.h + 60, 30); return; }
  }
  cam = fitRect(b.x, b.y - 60, b.w, b.h + 60, 30);
}

/** Focus views start readable: at least 0.8 scale, fitted by width, at the top. */
function fitCam(sc: Scene) {
  const b = sc.bounds, f = fitRect(b.x, b.y, b.w, b.h, 40);
  let k = Math.min(1.1, f.k);
  if (k < 0.88) {
    const c = sc.center!;
    if (sc.t === "trace") { k = 0.88; return { k, x: 24 - b.x * k, y: 64 - b.y * k }; }
    k = Math.max(0.8, Math.min(1.1, (vw() - 32) / b.w));
    const cxw = b.w * k <= vw() ? b.x + b.w / 2 : c.x + c.w / 2;
    return { k, x: vw() / 2 - cxw * k, y: 64 - Math.min(b.y, c.y - 40) * k };
  }
  return { k, x: vw() / 2 - (b.x + b.w / 2) * k, y: vh() / 2 - (b.y + b.h / 2) * k };
}

function zoomAt(sx: number, sy: number, f: number) {
  const k = Math.min(2.5, Math.max(minK(), cam.k * f)), r = k / cam.k;
  cam = { k, x: sx - (sx - cam.x) * r, y: sy - (sy - cam.y) * r };
  camAnim = null; needDraw = true; hint();
}

function animateCam(target: typeof cam, ms = 480) {
  if (reduceMotion) { cam = target; needDraw = true; return; }
  const from = { ...cam }, t0 = performance.now();
  const c0 = { x: (vw() / 2 - from.x) / from.k, y: (vh() / 2 - from.y) / from.k }, c1 = { x: (vw() / 2 - target.x) / target.k, y: (vh() / 2 - target.y) / target.k };
  camAnim = now => {
    let t = Math.min(1, (now - t0) / ms);
    t = t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2;
    const k = Math.exp(Math.log(from.k) + (Math.log(target.k) - Math.log(from.k)) * t);
    cam = { k, x: vw() / 2 - (c0.x + (c1.x - c0.x) * t) * k, y: vh() / 2 - (c0.y + (c1.y - c0.y) * t) * k };
    if (t >= 1) { cam = target; camAnim = null; }
    needDraw = true;
  };
}

/* Staged transitions: a box that exists before and after slides; new boxes fade in after. */

function screenRects() {
  const m = new Map<string, { x: number; y: number }>();
  if (!scene) return m;
  const list = scene.t === "overview" ? (cam.k >= OV_ROOMS_AT ? scene.rooms! : []) : scene.nodes;
  for (const n of list) m.set(n.key.split("@")[0], { x: n.x * cam.k + cam.x, y: n.y * cam.k + cam.y });
  return m;
}
function startMorph(prev: Map<string, { x: number; y: number }>, quick = false) {
  morph = reduceMotion || !scene ? null : { prev, t0: performance.now(), ms: quick ? 320 : 700 };
}
function ease(t: number) { return t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2; }
function morphState(n: Node, now: number) {
  if (!morph) return null;
  const t = Math.min(1, (now - morph.t0) / morph.ms);
  if (t >= 1) return null;
  const p = morph.prev.get(n.key.split("@")[0]);
  if (!p) return { alpha: Math.max(0, (t - 0.4) / 0.6), dx: 0, dy: 0 };
  const e = ease(t), px0 = (p.x - cam.x) / cam.k, py0 = (p.y - cam.y) / cam.k;
  return { alpha: 1, dx: (px0 - n.x) * (1 - e), dy: (py0 - n.y) * (1 - e) };
}
function nodePos(n: Node, now: number) { const m = morphState(n, now); return { x: n.x + (m?.dx || 0), y: n.y + (m?.dy || 0), alpha: m ? m.alpha : 1 }; }

/* Drawing. */

function rr(c: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) { c.beginPath(); c.roundRect(x, y, w, h, r); }
const px = (n: number) => n / cam.k;

function frame(now: number) {
  if (camAnim) camAnim(now);
  if (morph) { needDraw = true; if (now - morph.t0 > morph.ms) morph = null; }
  if (needDraw) { needDraw = false; draw(now); drawMini(); }
  requestAnimationFrame(frame);
}

function draw(now: number) {
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.fillStyle = T.bg;
  ctx.fillRect(0, 0, vw(), vh());
  if (!scene) return;
  ctx.setTransform(dpr * cam.k, 0, 0, dpr * cam.k, dpr * cam.x, dpr * cam.y);
  if (scene.t === "overview") drawOverview(now); else drawFocus(now);
}

function visRect(x: number, y: number, w: number, h: number) {
  const a = toWorld(0, 0), b = toWorld(vw(), vh());
  return x < b.x && x + w > a.x && y < b.y && y + h > a.y;
}
function drawOps(n: Node, ox: number, oy: number) {
  for (const o of n.ops) { ctx.font = o.f; ctx.fillStyle = color(o.c); ctx.fillText(o.s, ox + o.x, oy + o.y); }
}

function focusRing(x: number, y: number, w: number, h: number, r: number) {
  ctx.save();
  ctx.setLineDash([]);
  rr(ctx, x - px(4), y - px(4), w + px(8), h + px(8), r + px(4));
  ctx.lineWidth = px(3);
  ctx.strokeStyle = T.focus;
  ctx.stroke();
  ctx.restore();
}

function drawOverview(now: number) {
  const showRooms = cam.k >= OV_ROOMS_AT;
  if (!showRooms) for (const l of OV.links!) {
    const hot = hover?.region && (l.from === hover.region || l.to === hover.region), fade = hover?.region && !hot;
    curve(l.from, l.to, edgeColor("calls"), Math.min(8, 1.5 + l.targets.size / 8), fade ? 0.08 : hot ? 0.95 : 0.4, hot ? `uses ${l.targets.size} items` : null);
  }
  for (const g of OV.regions!) {
    const col = regionColor(g.r.color), hot = hover?.region === g || kbd?.region === g;
    // Opaque boxes over the lines: lines never cross text.
    rr(ctx, g.x, g.y, g.w, g.h, 26); ctx.fillStyle = T.bg; ctx.fill(); ctx.fillStyle = rgba(col, 0.06); ctx.fill();
    ctx.lineWidth = px(hot ? 3 : 2); ctx.strokeStyle = rgba(col, hot ? 1 : 0.7); ctx.stroke();
    if (kbd?.region === g) focusRing(g.x, g.y, g.w, g.h, 26);
    if (!showRooms) {
      const name = g.r.name;
      ctx.font = `700 100px ${SANS()}`;
      const fs = Math.min((100 * (g.w - 80)) / ctx.measureText(name).width, px(26));
      ctx.font = `700 ${fs}px ${SANS()}`; ctx.fillStyle = col; ctx.fillText(name, g.x + 40, g.y + 40 + fs);
      const s1 = Math.min(px(13.5), fs * 0.55), f1 = `400 ${s1}px ${SANS()}`;
      const maxLines = Math.max(1, Math.floor((g.h - 80 - fs * 1.6) / (s1 * 1.5)));
      ctx.fillStyle = T.fg;
      let y = g.y + 40 + fs + s1 * 1.9;
      for (const l of wrap(inlinePlain(g.r.description), f1, g.w - 80, maxLines)) { ctx.font = f1; ctx.fillText(l, g.x + 40, y); y += s1 * 1.5; }
      continue;
    }
    ctx.font = g.titleF; ctx.fillStyle = col; ctx.fillText(g.r.name, g.x + 40, g.y + 68);
    ctx.font = g.descF; ctx.fillStyle = T.muted;
    let y = g.y + 103;
    for (const l of g.desc) { ctx.fillText(l, g.x + 40, y); y += 24; }
    for (const n of g.rooms) {
      if (!visRect(n.x, n.y, n.w, n.h)) continue;
      const P = nodePos(n, now), h2 = hover?.node === n;
      ctx.globalAlpha = P.alpha;
      rr(ctx, P.x, P.y, n.w, n.h, 12); ctx.fillStyle = T.surface; ctx.fill();
      ctx.lineWidth = px(h2 ? 2.5 : 1.2); ctx.strokeStyle = h2 ? col : rgba(col, 0.55); ctx.stroke();
      drawOps(n, P.x, P.y);
      if (kbd?.node === n) focusRing(P.x, P.y, n.w, n.h, 12);
      ctx.globalAlpha = 1;
    }
  }
}

function arrowHead(x: number, y: number, ang: number, col: string, L: number, W: number) {
  ctx.beginPath(); ctx.moveTo(x, y);
  ctx.lineTo(x - L * Math.cos(ang) + W * Math.sin(ang), y - L * Math.sin(ang) - W * Math.cos(ang));
  ctx.lineTo(x - L * Math.cos(ang) - W * Math.sin(ang), y - L * Math.sin(ang) + W * Math.cos(ang));
  ctx.closePath(); ctx.fillStyle = col; ctx.fill();
}

function curve(a: RegionBox, b: RegionBox, col: string, w: number, alpha: number, label: string | null) {
  const ac = { x: a.x + a.w / 2, y: a.y + a.h / 2 }, bc = { x: b.x + b.w / 2, y: b.y + b.h / 2 };
  const bp = (r: RegionBox, t: { x: number; y: number }) => { const cx2 = r.x + r.w / 2, cy = r.y + r.h / 2, dx = t.x - cx2, dy = t.y - cy; const s = Math.min(r.w / 2 / Math.abs(dx || 1e-9), r.h / 2 / Math.abs(dy || 1e-9)); return { x: cx2 + dx * s, y: cy + dy * s }; };
  const p0 = bp(a, bc), p1 = bp(b, ac), mx = (p0.x + p1.x) / 2, my = (p0.y + p1.y) / 2, qx = mx - (p1.y - p0.y) * 0.08, qy = my + (p1.x - p0.x) * 0.08;
  ctx.globalAlpha = alpha; ctx.beginPath(); ctx.moveTo(p0.x, p0.y); ctx.quadraticCurveTo(qx, qy, p1.x, p1.y);
  ctx.strokeStyle = col; ctx.lineWidth = px(w); ctx.stroke();
  arrowHead(p1.x, p1.y, Math.atan2(p1.y - qy, p1.x - qx), col, px(10 + w), px(5 + w * 0.6));
  if (label) {
    const lx = 0.25 * p0.x + 0.5 * qx + 0.25 * p1.x, ly = 0.25 * p0.y + 0.5 * qy + 0.25 * p1.y;
    ctx.globalAlpha = 1; ctx.font = `600 ${px(12)}px ${SANS()}`;
    const w2 = ctx.measureText(label).width + px(14);
    rr(ctx, lx - w2 / 2, ly - px(10), w2, px(20), px(10)); ctx.fillStyle = T.surface; ctx.fill(); ctx.lineWidth = px(1); ctx.strokeStyle = col; ctx.stroke();
    ctx.fillStyle = T.fg; ctx.textAlign = "center"; ctx.fillText(label, lx, ly + px(4)); ctx.textAlign = "left";
  }
  ctx.globalAlpha = 1;
}

function anchorY(n: Node, a?: string) {
  if (a && n.anchors[a] != null) return n.anchors[a];
  if (a === "head" && n.anchors.head != null) return n.anchors.head;
  return n.t === "nbr" || n.t === "trace" || n.t === "more" || n.t === "route" ? Math.min(n.h / 2, 30) : n.h / 2;
}

function edgeHot(e: SEdge, h: Hit | null) {
  const node = h?.node;
  if (!node) return false;
  if (e.a !== node && e.b !== node) return false;
  if (h.row && node.t === "center") { const an = node === e.a ? e.aAnchor : e.bAnchor; return an === h.row.anchor; }
  return true;
}

function drawFocus(now: number) {
  const sc = scene, h = hover ?? kbd;
  let edges = sc.edges;
  if (sc.t === "unit") {
    const extra: SEdge[] = [];
    if (h?.node?.detail) extra.push(...h.node.detail);
    if (h?.node === sc.center && h?.row?.item) for (const n of sc.nodes) for (const d of n.detail ?? []) if (d.aAnchor === h.row.anchor || d.bAnchor === h.row.anchor) extra.push(d);
    if (extra.length) edges = extra;
  }
  const anyHot = edges.some(e => edgeHot(e, h));
  for (const e of edges) {
    const A = nodePos(e.a, now), B = nodePos(e.b, now), hot = edgeHot(e, h);
    let alpha = e.faint ? 0.35 : anyHot ? (hot ? 1 : 0.07) : 0.6;
    alpha *= Math.min(A.alpha, B.alpha);
    if (e.heuristic) alpha *= 0.6;
    const dash = e.heuristic ? [2, 4] : EDGE[e.kind]?.dash ?? [];
    hbez(A.x + e.a.w, A.y + anchorY(e.a, e.aAnchor), B.x, B.y + anchorY(e.b, e.bAnchor), edgeColor(e.kind), dash, hot ? 2.6 : e.heuristic ? 1.2 : e.w || 1.6, alpha, e.label ?? null);
  }
  for (const n of sc.nodes) {
    const P = nodePos(n, now);
    if (!visRect(P.x, P.y, n.w, n.h)) continue;
    ctx.globalAlpha = P.alpha;
    const hot = h?.node === n, sel = picked?.node === n && !picked.row, col = n.region ? regionColor(n.region.color) : T.muted;
    if (n.t === "lane") {
      const k = edgeColor(n.edge!);
      ctx.strokeStyle = k; ctx.lineWidth = 2.2; ctx.setLineDash(EDGE[n.edge!]?.dash ?? []);
      ctx.beginPath(); ctx.moveTo(P.x, P.y + 11); ctx.lineTo(P.x + 22, P.y + 11); ctx.stroke(); ctx.setLineDash([]);
      arrowHead(P.x + 28, P.y + 11, 0, k, 8, 4.5);
      drawOps(n, P.x, P.y); ctx.globalAlpha = 1;
      continue;
    }
    rr(ctx, P.x, P.y, n.w, n.h, n.t === "more" ? 16 : 8);
    ctx.fillStyle = n.t === "more" ? T.bg : n.t === "center" || n.t === "route" ? T.surface2 : T.surface; ctx.fill();
    // A dashed box marks an interface (shape, not only colour).
    const dashed = n.item && M.isInterfaceLike(n.item) && !sel;
    ctx.lineWidth = sel ? 3 : hot ? 2.4 : n.t === "center" ? 2 : 1.3;
    ctx.strokeStyle = sel ? T.focus : hot ? col : n.t === "more" ? T.edge : rgba(col, n.t === "center" ? 0.95 : 0.6);
    ctx.setLineDash(dashed ? [6, 4] : []); ctx.stroke(); ctx.setLineDash([]);
    for (const r of n.rows) {
      const isHot = h?.node === n && h.row === r, isSel = picked?.node === n && picked.row === r;
      const onEdge = anyHot && h?.node !== n && edges.some(e => edgeHot(e, h) && ((e.a === n && e.aAnchor === r.anchor) || (e.b === n && e.bAnchor === r.anchor)));
      if (isHot || isSel || onEdge) { ctx.fillStyle = isSel ? rgba(T.focus, 0.22) : rgba(col, 0.18); ctx.fillRect(P.x + 4, P.y + r.y, n.w - 8, r.h); }
      if (kbd?.node === n && kbd.row === r) { ctx.strokeStyle = T.focus; ctx.lineWidth = 2; ctx.strokeRect(P.x + 4, P.y + r.y, n.w - 8, r.h); }
    }
    drawOps(n, P.x, P.y);
    if (kbd?.node === n && !kbd.row) focusRing(P.x, P.y, n.w, n.h, 8);
    ctx.globalAlpha = 1;
  }
  if (sc.empty && sc.center) {
    ctx.font = F(400, 14); ctx.fillStyle = T.faint;
    ctx.fillText("No other item in this code refers to this one, and it refers to none.", sc.center.x, sc.center.y + sc.center.h + 30);
  }
}

function hbez(x0: number, y0: number, x1: number, y1: number, col: string, dash: number[], w: number, alpha: number, label: string | null) {
  ctx.globalAlpha = alpha; ctx.beginPath(); ctx.moveTo(x0, y0);
  let c1x: number, c2x: number, c1y = y0, c2y = y1;
  if (x1 >= x0) { const d = Math.max(40, (x1 - x0) * 0.5); c1x = x0 + d; c2x = x1 - d; }
  else { c1x = x0 + 140; c2x = x1 - 140; c1y = y0 + 90; c2y = y1 + 90; }
  ctx.bezierCurveTo(c1x, c1y, c2x, c2y, x1, y1);
  ctx.strokeStyle = col; ctx.lineWidth = w; ctx.setLineDash(dash); ctx.stroke(); ctx.setLineDash([]);
  arrowHead(x1, y1, Math.atan2(y1 - c2y, x1 - c2x), col, 10 + w, 5 + w * 0.6);
  if (label) {
    const t = 0.16, mt = 1 - t;
    const lx = mt * mt * mt * x0 + 3 * mt * mt * t * c1x + 3 * mt * t * t * c2x + t * t * t * x1;
    const ly = mt * mt * mt * y0 + 3 * mt * mt * t * c1y + 3 * mt * t * t * c2y + t * t * t * y1;
    ctx.font = F(700, 11);
    const w2 = ctx.measureText(label).width + 12;
    rr(ctx, lx - w2 / 2, ly - 9, w2, 18, 9); ctx.fillStyle = T.surface; ctx.fill(); ctx.lineWidth = 1; ctx.strokeStyle = col; ctx.stroke();
    ctx.fillStyle = T.fg; ctx.textAlign = "center"; ctx.fillText(label, lx, ly + 4); ctx.textAlign = "left";
  }
  ctx.globalAlpha = 1;
}

function drawMini() {
  const W = mini.width, H = mini.height;
  mctx.setTransform(1, 0, 0, 1, 0, 0); mctx.clearRect(0, 0, W, H);
  if (!scene || !W) return;
  const b = scene.bounds, s = Math.min((W - 12) / b.w, (H - 12) / b.h), ox = (W - b.w * s) / 2 - b.x * s, oy = (H - b.h * s) / 2 - b.y * s;
  miniT = { s, ox, oy };
  if (scene.t === "overview") for (const g of OV.regions!) { mctx.fillStyle = rgba(regionColor(g.r.color), 0.35); mctx.fillRect(ox + g.x * s, oy + g.y * s, g.w * s, g.h * s); }
  else for (const n of scene.nodes) {
    if (n.t === "lane") continue;
    mctx.fillStyle = n === scene.center ? T.focus : rgba(n.region ? regionColor(n.region.color) : T.muted, 0.6);
    mctx.fillRect(ox + n.x * s, oy + n.y * s, Math.max(1, n.w * s), Math.max(1, n.h * s));
  }
  const a = toWorld(0, 0), c = toWorld(vw(), vh());
  mctx.strokeStyle = T.fg; mctx.lineWidth = 1.5 * dpr;
  mctx.strokeRect(ox + a.x * s, oy + a.y * s, (c.x - a.x) * s, (c.y - a.y) * s);
}

/* Pointer input. */

function hit(sx: number, sy: number): Hit | null {
  const p = toWorld(sx, sy);
  if (!scene) return null;
  const inside = (n: { x: number; y: number; w: number; h: number }) => p.x >= n.x && p.x <= n.x + n.w && p.y >= n.y && p.y <= n.y + n.h;
  if (scene.t === "overview") {
    if (cam.k >= OV_ROOMS_AT) for (const n of OV.rooms!) if (inside(n)) return { node: n };
    for (const g of OV.regions!) if (inside(g)) return { region: g };
    return null;
  }
  for (let i = scene.nodes.length - 1; i >= 0; i--) {
    const n = scene.nodes[i];
    if (n.t === "lane" || !inside(n)) continue;
    const ry = p.y - n.y;
    return { node: n, row: n.rows.find(r => ry >= r.y && ry < r.y + r.h) };
  }
  return null;
}

const pointers = new Map<number, { x: number; y: number }>();
let drag: { x: number; y: number; cx: number; cy: number } | null = null, moved = false, pinch: { d: number; k: number } | null = null;

function onPointerDown(e: PointerEvent) {
  canvas.setPointerCapture(e.pointerId);
  pointers.set(e.pointerId, { x: e.offsetX, y: e.offsetY });
  if (pointers.size === 1) { drag = { x: e.offsetX, y: e.offsetY, cx: cam.x, cy: cam.y }; moved = false; }
  if (pointers.size === 2) { const [a, b] = [...pointers.values()]; pinch = { d: Math.hypot(a.x - b.x, a.y - b.y), k: cam.k }; drag = null; }
  camAnim = null;
}
function onPointerMove(e: PointerEvent) {
  if (pointers.has(e.pointerId)) pointers.set(e.pointerId, { x: e.offsetX, y: e.offsetY });
  if (pinch && pointers.size === 2) { const [a, b] = [...pointers.values()]; zoomAt((a.x + b.x) / 2, (a.y + b.y) / 2, (pinch.k * Math.hypot(a.x - b.x, a.y - b.y)) / pinch.d / cam.k); moved = true; return; }
  if (drag) {
    const dx = e.offsetX - drag.x, dy = e.offsetY - drag.y;
    if (Math.abs(dx) + Math.abs(dy) > 4) { moved = true; canvas.classList.add("dragging"); }
    if (moved) { cam.x = drag.cx + dx; cam.y = drag.cy + dy; needDraw = true; }
    return;
  }
  const h = hit(e.offsetX, e.offsetY);
  if (h?.node !== hover?.node || h?.row !== hover?.row || h?.region !== hover?.region) {
    hover = h; needDraw = true; status(h);
    canvas.classList.toggle("pointer", !!(h && (h.node || h.region)));
  }
}
function endPointer(e: PointerEvent) {
  pointers.delete(e.pointerId);
  canvas.classList.remove("dragging");
  if (pointers.size < 2) pinch = null;
  if (drag && !moved && e.type === "pointerup") click(hit(e.offsetX, e.offsetY));
  if (pointers.size === 0) drag = null;
}

function click(h: Hit | null) {
  if (!h) { if (picked) { picked = null; panel.renderFor(current()); needDraw = true; } return; }
  if (scene.t === "overview") {
    if (h.node) go({ v: "unit", id: h.node.unit! });
    else if (h.region) { animateCam(fitRect(h.region.x, h.region.y, h.region.w, h.region.h, 30)); panel.region(h.region.r.id); }
    return;
  }
  const n = h.node!;
  if (h.row?.onClick) { h.row.onClick(); return; }
  if (n.onClick) { n.onClick(); return; }
  if (n.t === "route") { picked = { node: n }; panel.route(n.route!.id); needDraw = true; return; }
  if (n.t === "center") {
    if (h.row?.item) { go({ v: "symbol", id: h.row.item }); return; }
    if (h.row) { picked = { node: n, row: h.row }; if (h.row.member) panel.symbol(n.item!, { member: h.row.member }); needDraw = true; return; }
    picked = null; panel.renderFor(current()); needDraw = true;
    return;
  }
  // The first click shows the docs; the second puts the box in the centre.
  if (picked?.node === n) { openNode(n); return; }
  picked = { node: n }; needDraw = true;
  if (n.item) panel.symbol(n.item, { focusBtn: true, member: n.member }); else if (n.unit) panel.unit(n.unit, { focusBtn: true });
}

function openNode(n: Node) {
  if (n.item) go({ v: "symbol", id: n.item, member: n.t === "trace" && n.member ? n.member : undefined });
  else if (n.unit) go({ v: "unit", id: n.unit });
}

function onWheel(e: WheelEvent) {
  e.preventDefault();
  const delta = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
  const f = Math.exp(-delta * (e.ctrlKey ? 0.01 : 0.0018));
  if (scene.t === "overview" && f > 1 && cam.k * f >= OV_ENTER_AT) { const h = hit(e.offsetX, e.offsetY); if (h?.node) { go({ v: "unit", id: h.node.unit! }); return; } }
  if (scene.t !== "overview" && f < 1 && cam.k <= minK() * 1.01) { outAccum++; if (outAccum > 6) { outAccum = 0; up(); } else hint(); return; }
  outAccum = 0;
  zoomAt(e.offsetX, e.offsetY, f);
}

function moveMini(e: PointerEvent) {
  if (!miniT) return;
  const { s, ox, oy } = miniT, wx = (e.offsetX * dpr - ox) / s, wy = (e.offsetY * dpr - oy) / s;
  cam.x = vw() / 2 - wx * cam.k; cam.y = vh() / 2 - wy * cam.k; camAnim = null; needDraw = true;
}

/* Keyboard path through the map: browse, then commit. */

interface NavTarget { node?: Node; region?: RegionBox; x: number; y: number; w: number; h: number }

function navTargets(): NavTarget[] {
  if (scene.t === "overview") {
    if (cam.k >= OV_ROOMS_AT) return OV.rooms!.map(n => ({ node: n, x: n.x, y: n.y, w: n.w, h: n.h }));
    return OV.regions!.map(g => ({ region: g, x: g.x, y: g.y, w: g.w, h: g.h }));
  }
  return scene.nodes.filter(n => n.t !== "lane").map(n => ({ node: n, x: n.x, y: n.y, w: n.w, h: n.h }));
}

function targetOf(h: Hit | null): NavTarget | undefined {
  return navTargets().find(t => (h?.node && t.node === h.node) || (h?.region && t.region === h.region));
}

function step(dx: number, dy: number) {
  const all = navTargets();
  if (!all.length) return;
  let cur = targetOf(kbd);
  if (!cur) { setKbd(scene.center ? { node: scene.center } : all[0].node ? { node: all[0].node } : { region: all[0].region }); return; }
  // Rows inside a box come first for up and down.
  const n = cur.node;
  if (n && n.rows.length && dy !== 0) {
    const i = kbd?.row ? n.rows.indexOf(kbd.row) : -1;
    const j = i + dy;
    if (j >= 0 && j < n.rows.length && (i >= 0 || dy > 0)) { setKbd({ node: n, row: n.rows[j] }); return; }
    if (i >= 0 && j < 0) { setKbd({ node: n }); return; }
  }
  const c = { x: cur.x + cur.w / 2, y: cur.y + cur.h / 2 };
  let best: NavTarget | null = null, bestScore = Infinity;
  for (const t of all) {
    if (t === cur) continue;
    const tx = t.x + t.w / 2, ty = t.y + t.h / 2, ddx = tx - c.x, ddy = ty - c.y;
    const along = dx ? ddx * dx : ddy * dy, across = dx ? Math.abs(ddy) : Math.abs(ddx);
    if (along <= 1) continue;
    const score = along + across * 2;
    if (score < bestScore) { bestScore = score; best = t; }
  }
  if (best) setKbd(best.node ? { node: best.node } : { region: best.region });
}

function setKbd(h: Hit) {
  kbd = h;
  const t = targetOf(h);
  if (t) {
    const sx = t.x * cam.k + cam.x, sy = t.y * cam.k + cam.y, sw = t.w * cam.k, sh = t.h * cam.k;
    if (sx < 0 || sy < 0 || sx + Math.min(sw, vw()) > vw() || sy + Math.min(sh, vh()) > vh()) {
      const k = cam.k;
      animateCam({ k, x: vw() / 2 - (t.x + t.w / 2) * k, y: Math.min(vh() / 2, 80) - t.y * k + (sh < vh() / 2 ? vh() / 3 : 0) });
    }
  }
  announce.textContent = describe(h);
  status(h);
  needDraw = true;
}

function describe(h: Hit): string {
  if (h.region) return `Region ${h.region.r.name}. ${inlinePlain(h.region.r.description)} Press Enter to zoom to it.`;
  const n = h.node!;
  if (h.row) {
    if (h.row.item) { const it = M.items.get(h.row.item)!; return `${it.name}, ${it.languageKind}. ${summary(it.doc)} Press Enter to open it.`; }
    if (h.row.member) { const m = M.member(n.item!, h.row.member); return `${h.row.member}, ${m?.languageKind ?? "member"}. ${summary(m?.doc)}`; }
    return "A row. Press Enter to use it.";
  }
  if (n.t === "more") return `${n.label}. Press Enter to show them.`;
  if (n.t === "route") return `${ENTRY_KIND[n.route!.kind] ?? "Entry point"} ${n.route!.name}.`;
  const lane = n.lane ? scene.nodes.find(x => x.t === "lane" && x.key.endsWith(":" + n.lane))?.label : undefined;
  if (n.item) {
    const it = M.items.get(n.item)!;
    const where = n.t === "center" ? "In the centre." : lane ? `In the lane ${lane}.` : "";
    return `${n.member ? it.name + "." + n.member : it.name}, ${it.languageKind}. ${where} ${summary(it.doc)} ${n.t === "center" ? "" : "Press Enter to put it in the centre, Space to read its docs."}`;
  }
  if (n.unit) { const u = M.units.get(n.unit)!; return `${u.name}, ${u.c.languageKind}. Press Enter to open it.`; }
  return "";
}

function activate(open: boolean) {
  if (!kbd) return;
  if (kbd.region) { animateCam(fitRect(kbd.region.x, kbd.region.y, kbd.region.w, kbd.region.h, 30)); panel.region(kbd.region.r.id); return; }
  const n = kbd.node!;
  if (kbd.row) { click({ node: n, row: kbd.row }); return; }
  if (scene.t === "overview") { go({ v: "unit", id: n.unit! }); return; }
  if (n.onClick) { n.onClick(); return; }
  if (open && n.t !== "center" && n.t !== "route") { openNode(n); return; }
  picked = { node: n }; needDraw = true;
  if (n.route) panel.route(n.route.id);
  else if (n.item) panel.symbol(n.item, { focusBtn: n.t !== "center", member: n.member });
  else if (n.unit) panel.unit(n.unit, { focusBtn: n.t !== "center" });
}

function onCanvasKey(e: KeyboardEvent) {
  const keys: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
  if (keys[e.key] && !e.altKey && !(tour.active() && !kbd && (e.key === "ArrowLeft" || e.key === "ArrowRight"))) {
    e.preventDefault(); e.stopPropagation();
    step(...keys[e.key]);
  } else if (e.key === "Enter") { e.preventDefault(); e.stopPropagation(); activate(true); }
  else if (e.key === " ") { e.preventDefault(); e.stopPropagation(); activate(false); }
}

function onGlobalKey(e: KeyboardEvent) {
  const t = e.target as HTMLElement;
  if (t.tagName === "INPUT" || t.tagName === "TEXTAREA") return;
  if (e.key === "/") { e.preventDefault(); (document.getElementById("q") as HTMLInputElement).focus(); }
  else if (e.key === "Backspace" || (e.altKey && e.key === "ArrowLeft")) { e.preventDefault(); back(); }
  else if (e.altKey && e.key === "ArrowRight") { e.preventDefault(); fwd(); }
  else if (e.key === "Escape") { if (tour.active()) tour.end(); else if (kbd) { kbd = null; needDraw = true; } else up(); }
  else if (e.key === "0") go({ v: "overview" });
  else if (e.key === "+" || e.key === "=") zoomAt(vw() / 2, vh() / 2, 1.3);
  else if (e.key === "-") zoomAt(vw() / 2, vh() / 2, 1 / 1.3);
  else if (e.key === "t" || e.key === "T") tour.start();
  else if (tour.active() && e.key === "ArrowRight") tour.next();
  else if (tour.active() && e.key === "ArrowLeft") tour.prev();
}

/* Chrome. */

function resize() {
  const r = canvas.getBoundingClientRect();
  dpr = Math.min(window.devicePixelRatio || 1, 2);
  canvas.width = Math.max(1, Math.round(r.width * dpr)); canvas.height = Math.max(1, Math.round(r.height * dpr));
  const m = mini.getBoundingClientRect();
  mini.width = Math.round(m.width * dpr); mini.height = Math.round(m.height * dpr);
  needDraw = true;
}

export function status(h: Hit | null) {
  const st = document.getElementById("statustext")!;
  if (!h) {
    st.textContent = !scene ? "" : scene.t === "overview"
      ? `Click a region to zoom to it. Click a ${M.unitList[0]?.c.languageKind ?? "unit"}, or zoom into it, to see what it uses and what uses it.`
      : scene.t === "trace" ? "Numbers give the call order inside each function. Dashed lines lead from an interface to the code that runs. Click a box twice to open it."
      : "Click a box to read its docs. Click it again to put it in the centre. Zoom far out to go up a level.";
    return;
  }
  let name = "", sub = "";
  if (h.region) { name = h.region.r.name; sub = inlinePlain(h.region.r.description).split(/(?<=[.!?])\s/)[0]; }
  else if (h.node?.t === "more") { name = "More"; sub = "Click to show more."; }
  else if (h.row?.item) { name = M.qual(h.row.item); sub = summary(M.items.get(h.row.item)!.doc) || "No doc comment."; }
  else if (h.node?.item) { const it = M.items.get(h.node.item)!; name = h.node.member ? it.name + "." + h.node.member : M.qual(it.id); sub = (h.row?.doc || summary(h.node.member ? M.member(it.id, h.node.member)?.doc : it.doc)) || "No doc comment."; }
  else if (h.node?.unit) { const u = M.units.get(h.node.unit)!; name = u.name; sub = summary(u.c.doc); }
  else if (h.node?.route) { name = h.node.route.name; sub = "Click it to see where the code registers it."; }
  st.innerHTML = `<b>${esc(name)}</b>${sub ? " — " + esc(sub) : ""}`;
}

export function hint() {
  const el = document.getElementById("hint")!;
  if (!scene) return;
  const word = M.unitList[0]?.c.languageKind ?? "unit";
  let s: string;
  if (scene.t === "overview") s = cam.k < OV_ROOMS_AT ? `Zoom in to see the ${word}s of each region.` : `Zoom into a ${word}, or click it, to open its focus view.`;
  else if (outAccum > 0) { const v = current().v; s = `Keep zooming out to go up to ${v === "symbol" ? "the " + word : v === "unit" ? "the overview" : "the starting item"}.`; }
  else s = scene.t === "symbol" ? "Left: what uses it.   Right: what it uses.   Arrows point at the thing used."
    : scene.t === "unit" ? `Hover a ${word} to see the exact items it uses. Hover a row to see who uses it.`
    : "Read left to right. Numbers give the call order.";
  el.textContent = s;
  el.hidden = false;
}

function renderCrumbs() {
  const el = document.getElementById("crumbs")!;
  el.innerHTML = "";
  const st = current(), parts: { label: string; st: State }[] = [];
  const m = document.createElement("span");
  m.className = "mode";
  const word = M.unitList[0]?.c.languageKind ?? "unit";
  m.textContent = { overview: "Overview", unit: `${word[0].toUpperCase() + word.slice(1)} focus`, symbol: "Item focus", trace: "Trace" }[st.v];
  el.appendChild(m);
  let unit: string | undefined, item: string | undefined;
  if (st.v === "unit") unit = st.id;
  if (st.v === "symbol") { item = st.id; unit = M.items.get(st.id)?.unit; }
  if (st.v === "trace") { const r = traceRoot(M, st); if (r) { item = r.item; unit = M.items.get(r.item)?.unit; } }
  parts.push({ label: "All regions", st: { v: "overview" } });
  if (st.v === "overview" && st.region) parts.push({ label: M.regions.find(r => r.id === st.region)?.name ?? st.region, st });
  if (unit) {
    const r = M.regionOfUnit.get(unit);
    if (r) parts.push({ label: r.name, st: { v: "overview", region: r.id } });
    parts.push({ label: M.units.get(unit)!.name, st: { v: "unit", id: unit } });
  }
  if (item) parts.push({ label: M.items.get(item)!.name, st: { v: "symbol", id: item } });
  if (st.v === "trace") { const r = traceRoot(M, st); parts.push({ label: r?.route?.name ?? "trace of " + (st.member || M.items.get(st.sym!)?.name), st }); }
  parts.forEach((p, i) => {
    if (i) { const s = document.createElement("span"); s.className = "sep"; s.textContent = "›"; s.setAttribute("aria-hidden", "true"); el.appendChild(s); }
    const b = document.createElement("button");
    b.textContent = p.label;
    if (i === parts.length - 1) b.setAttribute("aria-current", "page");
    b.addEventListener("click", () => go(p.st));
    el.appendChild(b);
  });
}

function legendLine(k: string, label: string, heuristic = false) {
  const dash = heuristic ? "2 4" : EDGE[k].dash.join(" ");
  return `<div class="row"><svg width="36" height="10" aria-hidden="true"><line x1="0" y1="5" x2="28" y2="5" stroke="${edgeColor(k)}" stroke-width="2.2" stroke-dasharray="${dash}" ${heuristic ? 'opacity=".6"' : ""}></line><path d="M28 1 L36 5 L28 9 Z" fill="${edgeColor(k)}"></path></svg><span>${esc(label)}</span></div>`;
}

export function renderLegend() {
  const el = document.getElementById("legend")!, st = current();
  const word = M.unitList[0]?.c.languageKind ?? "unit";
  const anyHeuristic = M.doc.edges.some(e => e.confidence === "heuristic");
  const guess = anyHeuristic ? legendLine("uses", "matched by name, not checked by a compiler", true) : "";
  if (st.v === "overview") el.innerHTML = `<h2>Overview</h2>${legendLine("calls", "uses code in")}<p>Each region groups ${word}s by their job. Hover a region to see which regions it uses.</p>`;
  else if (st.v === "unit") el.innerHTML = `<h2>${esc(word[0].toUpperCase() + word.slice(1))} focus</h2>${legendLine("uses", "uses")}<p>Thicker lines carry more items. Hover a box to see the exact items.</p>`;
  else if (st.v === "symbol") el.innerHTML = `<h2>Item focus</h2>${legendLine("calls", "calls")}${legendLine("implements", "implements")}${legendLine("extends", "extends")}${legendLine("param", "takes as a parameter")}${legendLine("result", "returns")}${legendLine("field", "has a field of")}${legendLine("overrides", "overrides")}${guess}<p>Arrows point at the thing used. A dashed box marks an interface.</p>`;
  else el.innerHTML = `<h2>Trace</h2>${legendLine("calls", "calls (number = order)")}${legendLine("runs", "runs as (interface to the code that runs)")}${guess}<p>Steps deeper than four calls start closed. Click “Expand” to open them.</p>`;
}

/* Start. */

export function start(model: Model) {
  M = model;
  readTheme();
  OV = buildOverview(M);
  resize();
  canvas.addEventListener("pointerdown", onPointerDown);
  canvas.addEventListener("pointermove", onPointerMove);
  canvas.addEventListener("pointerup", endPointer);
  canvas.addEventListener("pointercancel", endPointer);
  canvas.addEventListener("pointerleave", () => { if (!drag) { hover = null; needDraw = true; status(kbd); } });
  canvas.addEventListener("wheel", onWheel, { passive: false });
  canvas.addEventListener("keydown", onCanvasKey);
  canvas.addEventListener("focus", () => { if (!kbd) step(0, 0); });
  mini.addEventListener("pointerdown", e => { moveMini(e); mini.setPointerCapture(e.pointerId); (mini as any)._d = true; });
  mini.addEventListener("pointermove", e => { if ((mini as any)._d) moveMini(e); });
  mini.addEventListener("pointerup", () => { (mini as any)._d = false; });
  addEventListener("keydown", onGlobalKey);
  addEventListener("resize", resize);
  addEventListener("hashchange", () => { const st = readHash(); if (!sameState(current(), st)) go(st); });
  go(readHash());
  requestAnimationFrame(frame);
  const retheme = () => { readTheme(); OV = buildOverview(M); scene = buildScene(current()); renderLegend(); needDraw = true; };
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", retheme);
  new MutationObserver(retheme).observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
}

/** Hooks for tests and the tour. */
export const debug = {
  get scene() { return scene; },
  get cam() { return cam; },
  set cam(c) { cam = c; needDraw = true; },
};
