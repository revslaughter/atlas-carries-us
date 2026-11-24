// The docs panel: reference facts for the current view, as HTML. Every
// relation on the map also appears here as a list of buttons, which gives a
// keyboard and screen-reader path through the same facts.

import { M, go, stateName } from "./app";
import type { Route } from "./model";
import {
  ENTRY_KIND,
  LANES_LEFT,
  LANES_RIGHT,
  memberSections,
  noteFor,
  rankItems,
  symbolRelations,
  traceRoot,
  isEmbedded,
  type Bag,
  type Lane,
  type State,
} from "./scene";
import { EDGE, edgeColor, regionColor } from "./theme";
import {
  docHTML,
  draftedNote,
  esc,
  folderURL,
  inlineHTML,
  inlinePlain,
  paragraphsHTML,
  sourceLink,
  summary,
} from "./text";
import type { Param, Symbol } from "./types";
import { tour } from "./tour";

/** The docs panel beside the canvas. */
const panel = document.getElementById("panel")!;

/** Starts a phrase with a capital letter. */
function cap(s: string): string {
  return s ? s[0].toUpperCase() + s.slice(1) : s;
}

/** A button that opens an item, labelled "unit.Item", with an optional small note such as the members involved. */
function chip(id: string, extra?: string): string {
  if (!M.items.has(id)) return "";
  return `<button class="chip" data-sym="${esc(id)}">${esc(M.qual(id))}${extra ? `<small>${esc(extra)}</small>` : ""}</button>`;
}
/** A button that opens a unit. */
function unitChip(id: string): string {
  const u = M.units.get(id);
  return u ? `<button class="chip" data-unit="${esc(id)}">${esc(u.name)}</button>` : "";
}
/** A small label in a region's colour, with the region's name: colour never works alone. */
function rpill(id: string | undefined): string {
  const r = id ? M.regions.find(x => x.id === id) : undefined;
  return r ? `<span class="kindpill" style="color:${regionColor(r.color)}">${esc(r.name)}</span>` : "";
}
/** The "Put it in the centre" button, for a panel that describes something that the canvas does not centre. */
function topBar(focusBtn: boolean | undefined, st: State): string {
  return focusBtn
    ? `<div class="top"><button class="btn primary" data-go='${esc(JSON.stringify(st))}'>Put it in the centre</button></div>`
    : "";
}

/** A type string with links on the names of known symbols. */
function typeHTML(t: string, refs: string[] | undefined): string {
  if (!refs?.length) return esc(t);
  const names = new Map<string, string>();
  for (const r of refs) {
    const it = M.items.get(M.owner.get(r) ?? r);
    if (!it) continue;
    names.set(it.name, it.id);
    names.set(M.qual(it.id), it.id);
  }
  return esc(t).replace(/[A-Za-z_$][\w$]*(\.[A-Za-z_$][\w$]*)?/g, m => {
    const id = names.get(m) ?? names.get(m.split(".").pop()!);
    return id ? `<span class="tlink" role="link" tabindex="0" data-sym="${esc(id)}">${m}</span>` : m;
  });
}
/** A parameter list with linked types, for a member's signature line. */
function paramsHTML(list: Param[]): string {
  return list
    .map(p => (p.name ? esc(p.name) + " " : "") + `<span class="ty">${typeHTML(p.type, p.refs)}</span>`)
    .join(", ");
}

/** The "Tests that use it" section of an item: up to 12 tests, each linked to its source. */
function testsHTML(id: string): string {
  const t = M.tests.get(id);
  if (!t?.length) return "";
  return `<h3>Tests that use it</h3><p class="note">A test shows the code in use. Read one for a working example.</p><ul class="plain tests">${t
    .slice(0, 12)
    .map(
      x =>
        `<li>${sourceLink(x.name, x.location.path, x.location.line)} <span>${esc(x.location.path.split("/").pop())}</span></li>`,
    )
    .join("")}${t.length > 12 ? `<li><span>+ ${t.length - 12} more</span></li>` : ""}</ul>`;
}

/** A list of entry points: method, path, and handler; each opens its trace. */
function routeList(list: Route[]): string {
  return `<ul class="plain routes">${list
    .map(
      r =>
        `<li><span class="verb">${esc(r.method ?? ENTRY_KIND[r.kind] ?? r.kind)}</span><button class="linkbtn" data-entry="${esc(r.id)}">${esc(r.path ?? r.name)}</button>${!r.inline && r.member ? `<span class="h">${esc(M.items.get(r.item)!.name + "." + r.member)}</span>` : r.externalHandler ? `<span class="h">${esc(r.externalHandler)}</span>` : ""}</li>`,
    )
    .join("")}</ul>`;
}

/** Writes the panel for a view state: the overview, a region, a unit, an item or a trace. */
export function renderFor(st: State) {
  if (st.v === "overview") return st.region ? region(st.region) : overview();
  if (st.v === "unit") return unit(st.id);
  if (st.v === "symbol") return symbol(st.id);
  if (st.v === "trace") return trace(st);
}

/** Writes the overview's panel: the project's description, the ways in (tour, entry points, help) and the regions. */
export function overview() {
  const p = M.doc.project;
  const steps = M.doc.tour?.length ?? 0;
  panel.innerHTML = `<div class="eyebrow">Overview</div><h2 class="sans">${esc(p?.name ?? "This code")}</h2>
  ${p?.description ? `<div class="doc">${paragraphsHTML(p.description)}</div>${draftedNote(p.descriptionDrafted)}` : `<p class="lead">This map groups the code into regions by job. Pick a region, or take the tour.</p>`}
  <h3>Ways in</h3>
  <div class="chips">${steps ? `<button class="chip action" data-tour="1">Take the tour (${steps} steps)</button>` : ""}${M.routes.length ? `<button class="chip action" data-routes="1">Trace an entry point (${M.routes.length})</button>` : ""}<button class="chip action" data-help="1">How to read this map</button></div>
  <h3>Regions</h3><ul class="plain rlist">${M.regions
    .map(
      r =>
        `<li><button class="linkbtn" data-region="${esc(r.id)}" style="font-weight:600">${esc(r.name)}</button><div class="s">${esc(inlinePlain(r.description).split(/(?<=[.!?])\s/)[0] ?? "")}</div></li>`,
    )
    .join("")}</ul>`;
}

/** Writes a region's panel: its description and reading tip, its units, and what it uses and what uses it in other regions. */
export function region(id: string) {
  const r = M.regions.find(x => x.id === id);
  if (!r) return overview();
  const out = new Map<string, Set<string>>(),
    inc = new Set<string>();
  for (const x of M.uses) {
    const a = M.regionOfItem(x.u),
      b = M.regionOfItem(x.t);
    if (!a || !b || a === b) continue;
    if (a.id === id) {
      if (!out.has(b.id)) out.set(b.id, new Set());
      out.get(b.id)!.add(x.t);
    }
    if (b.id === id) inc.add(a.id);
  }
  panel.innerHTML = `<div class="eyebrow">Region</div><h2 class="sans">${esc(r.name)}</h2>
  ${r.description ? `<div class="doc">${paragraphsHTML(r.description)}</div>` : ""}
  ${r.readingTip ? `<div class="about"><h4>Reading tip</h4><p>${inlineHTML(r.readingTip)}</p></div>` : ""}${draftedNote(r.drafted)}
  <h3>${esc(cap(M.allUnits(r.units)))}</h3><ul class="plain rlist">${r.units.map(u => `<li>${unitChip(u)}<div class="s">${esc(summary(M.units.get(u)!.c.doc) || `No doc comment on this ${M.wordOf(u)}.`)}</div></li>`).join("")}</ul>
  <h3>What it uses in other regions</h3>${
    out.size
      ? [...out]
          .map(
            ([rid, t]) =>
              `<div class="rel"><div class="rh">${rpill(rid)}</div><div class="chips">${[...t]
                .slice(0, 10)
                .map(x => chip(x))
                .join("")}${t.size > 10 ? `<span class="tag">+${t.size - 10} more</span>` : ""}</div></div>`,
          )
          .join("")
      : `<p class="nodoc">It uses no other region.</p>`
  }
  <h3>Regions that use it</h3><div class="chips">${[...inc].map(rpill).join(" ") || `<span class="nodoc">No other region uses it.</span>`}</div>`;
}

/**
 * Writes a unit's panel: its doc, the exported items that other code uses
 * most ("Start reading here"), its entry points, its imports both ways, its
 * outside libraries and its files.
 */
export function unit(id: string, opt: { focusBtn?: boolean } = {}) {
  const u = M.units.get(id)!,
    r = M.regionOfUnit.get(id),
    kt = M.keyItems(id, 6);
  const routes = M.routes.filter(x => M.items.get(x.item)?.unit === id);
  const folder = u.path ? folderURL(u.path) : null;
  panel.innerHTML = `${topBar(opt.focusBtn, { v: "unit", id })}<div class="eyebrow">${esc(u.c.languageKind)} ${rpill(r?.id)}</div><h2>${esc(u.name)}</h2>
  <div class="where">${folder ? `<a href="${esc(folder)}" target="_blank" rel="noopener">Open the folder</a>` : esc(u.path)}</div>
  <h3>What it does</h3>${docHTML(u.c.doc, `No doc comment on this ${u.c.languageKind}.`)}
  ${kt.length ? `<h3>Start reading here</h3><p class="note">The exported items that other code uses most.</p><div class="chips">${kt.map(s => chip(s.id)).join("")}</div>` : ""}
  ${routes.length ? `<h3>Entry points it serves</h3><p class="note">Click one to trace it through the code.</p>${routeList(routes)}` : ""}
  <h3>Imports from this code</h3><div class="chips">${u.imports.map(unitChip).join("") || `<span class="nodoc">None. It depends on nothing else here.</span>`}</div>
  <h3>Imported by</h3><div class="chips">${u.importedBy.map(unitChip).join("") || `<span class="nodoc">Nothing here imports it.</span>`}</div>
  ${u.c.externalImports?.length ? `<h3>Outside libraries</h3><div class="chips">${u.c.externalImports.map(x => `<span class="chip static">${esc(x)}</span>`).join("")}</div>` : ""}
  ${u.files.length ? `<h3>Files</h3><ul class="plain files">${u.files.map(f => `<li>${sourceLink(f.name, f.path ?? f.name)}${f.doc ? `<div class="s">${esc(summary(f.doc))}</div>` : ""}</li>`).join("")}</ul>` : ""}`;
}

/** One member of an item: its signature or type, its modifiers, its first doc sentence, and links to trace it and to its source. */
function memberHTML(itemId: string, m: Symbol, actions: boolean): string {
  const callable = !!m.signature;
  const sig = callable
    ? `<span class="nm">${esc(m.name)}</span>(${paramsHTML(m.signature!.params)})${m.signature!.results.length ? " " + (m.signature!.results.length > 1 || m.signature!.results[0].name ? "(" + paramsHTML(m.signature!.results) + ")" : paramsHTML(m.signature!.results)) : ""}`
    : isEmbedded(m)
      ? `<span class="nm">${esc(m.type?.text ?? m.name)}</span> <span class="ty">(embedded)</span>`
      : `<span class="nm">${esc(m.name)}</span> <span class="ty">${m.type ? typeHTML(m.type.text, m.type.refs) : ""}</span>${m.value ? ` = ${esc(m.value)}` : ""}`;
  const acts = actions
    ? `<span class="acts">${callable ? `<button class="linkbtn" data-trace="${esc(itemId)}" data-member="${esc(m.name)}">trace</button>` : ""}${sourceLink("source", m.location.path, m.location.line)}</span>`
    : "";
  const mods = [...(m.modifiers ?? []).filter(x => x !== "embedded"), ...(m.exported ? [] : ["not exported"])];
  return `<div class="mem">${acts}<div class="sig">${sig}</div>${mods.length ? `<div class="tag">${esc(mods.join(" · "))}</div>` : ""}${(m.annotations ?? []).map(a => `<div class="tag">${esc(a)}</div>`).join("")}${m.doc ? `<div class="d">${inlineHTML(m.doc.summary)}${m.doc.description.length > 1 || (m.doc.description[0]?.type === "paragraph" && inlinePlain(m.doc.description[0].text) !== inlinePlain(m.doc.summary)) ? ` <button class="linkbtn more" data-memberdoc="${esc(m.id)}">more</button>` : ""}</div>` : ""}</div>`;
}

/** A side of an item's relations as lists: one list per lane, each headed by its line style and verb. */
function lanesHTML(bag: Bag, list: Lane[], title: string): string {
  const parts = list
    .filter(l => bag[l.id]?.size)
    .map(l => {
      const items = rankItems(M, bag[l.id]);
      return `<div class="rel"><div class="rh"><svg width="26" height="8" aria-hidden="true"><line x1="0" y1="4" x2="26" y2="4" stroke="${edgeColor(l.edge)}" stroke-width="2" stroke-dasharray="${EDGE[l.edge].dash.join(" ")}"></line></svg>${esc(l.label)} <span class="c">${items.length}</span></div><div class="chips">${items
        .slice(0, 40)
        .map(it => chip(it.id, [...it.via].slice(0, 2).join(", ") + (it.heuristic ? " (matched by name)" : "")))
        .join("")}${items.length > 40 ? `<span class="tag">+${items.length - 40} more</span>` : ""}</div></div>`;
    });
  return parts.length ? `<h3>${title}</h3>${parts.join("")}` : "";
}

/**
 * Writes an item's panel: its doc, its signature or type, its members, its
 * relations on both sides as lists (which mirror the canvas for screen
 * readers), its entry points and its tests.
 */
export function symbol(id: string, opt: { focusBtn?: boolean; member?: string } = {}) {
  const it = M.items.get(id);
  if (!it) return;
  const u = M.units.get(it.unit)!,
    r = M.regionOfUnit.get(it.unit),
    s = it.sym;
  let h = `${topBar(opt.focusBtn, { v: "symbol", id })}<div class="eyebrow"><span>${esc(it.languageKind || it.kind)}</span>${rpill(r?.id)}</div>
  <h2>${esc(it.name)}${esc(s?.typeParams ?? s?.signature?.typeParams ?? "")}</h2>
  <div class="where">in ${unitChip(it.unit)} · ${sourceLink(it.location.path.split("/").pop() + ":" + it.location.line, it.location.path, it.location.line, it.location.endLine)}${it.exported ? "" : ` · not exported (used only inside its ${esc(u.c.languageKind)})`}</div>`;
  if (opt.member) {
    const m = M.member(id, opt.member);
    if (m) {
      h += `<h3>${esc(m.languageKind)} ${esc(m.name)}</h3>${m.signature?.text ? `<pre class="sig">${esc(m.signature.text)}</pre>` : ""}${docHTML(m.doc)}
      <div class="chips" style="margin-top:8px">${m.signature ? `<button class="chip action" data-trace="${esc(id)}" data-member="${esc(m.name)}">Trace the calls from ${esc(m.name)}</button>` : ""}${sourceLink("Open it in the source", m.location.path, m.location.line, m.location.endLine)}</div>${testsOfMember(m.id)}`;
    }
  }
  h += `<h3>What it is</h3>${docHTML(it.doc)}`;
  if (it.doc?.raw)
    h += `<details class="raw"><summary>Show the comment as written</summary><pre>${esc(it.doc.raw)}</pre></details>`;
  const routes = M.routesByHandler.get(id) ?? [];
  if (routes.length)
    h += `<h3>Entry points it serves</h3><p class="note">Click one to trace it through the code.</p>${routeList(routes)}`;
  if (s?.signature && it.kind !== "type" && it.kind !== "type_alias") {
    if (s.signature.text) h += `<h3>Signature</h3><pre class="sig">${esc(s.signature.text)}</pre>`;
    h += `<h3>Parameters</h3>${s.signature.params.length ? s.signature.params.map(p => `<div class="mem"><div class="sig"><span class="nm">${esc(p.name || "_")}</span> <span class="ty">${typeHTML(p.type, p.refs)}</span></div>${it.doc?.params?.[p.name ?? ""] ? `<div class="d">${inlineHTML(it.doc.params[p.name!])}</div>` : ""}</div>`).join("") : `<p class="nodoc">None.</p>`}`;
    h += `<h3>Results</h3>${s.signature.results.length ? s.signature.results.map(p => `<div class="mem"><div class="sig">${p.name ? `<span class="nm">${esc(p.name)}</span> ` : ""}<span class="ty">${typeHTML(p.type, p.refs)}</span></div></div>`).join("") : `<p class="nodoc">None.</p>`}`;
    h += `<div class="chips" style="margin-top:10px"><button class="chip action" data-trace="${esc(id)}" data-member="">Trace the calls from ${esc(it.name)}</button></div>`;
  }
  if ((it.kind === "type" || it.kind === "type_alias") && s?.type)
    h += `<h3>Definition</h3><pre class="sig">${it.kind === "type_alias" ? "alias of " : ""}${typeHTML(s.type.text, s.type.refs)}</pre>`;
  for (const g of memberSections(it))
    h += `<h3>${esc(g.title)}</h3>` + g.members.map(m => memberHTML(id, m, true)).join("");
  if (s?.satisfies?.length)
    h += `<h3>Satisfies well-known interfaces</h3><div class="chips">${s.satisfies.map(x => `<span class="chip static">${esc(x)}</span>`).join("")}</div>`;
  const { L, R } = symbolRelations(M, id);
  h += lanesHTML(L, LANES_LEFT, "What uses it") + lanesHTML(R, LANES_RIGHT, "What it uses");
  h += testsHTML(id);
  panel.innerHTML = h;
}

/** A line that names the tests of one member, up to six. */
function testsOfMember(memberId: string): string {
  const t = M.doc.tests?.[memberId];
  if (!t?.length) return "";
  return `<p class="note">Tests that use it: ${t
    .slice(0, 6)
    .map(x => sourceLink(x.name, x.location.path, x.location.line))
    .join(", ")}${t.length > 6 ? ` and ${t.length - 6} more` : ""}.</p>`;
}

/** Writes a trace's panel: how to read a trace, the entry point's facts, and the doc of the starting code. */
export function trace(st: Extract<State, { v: "trace" }>) {
  const root = traceRoot(M, st);
  if (!root) return;
  const it = M.items.get(root.item)!,
    label = root.member ? it.name + "." + root.member : it.name;
  let h = `<div class="eyebrow">Trace</div><h2>${esc(root.route?.name ?? M.qual(root.item) + (root.member ? "." + root.member : ""))}</h2>
  <p class="lead">This view answers “how does control get from here to the data?” It follows the calls from ${esc(label)} through the code.</p>
  <div class="doc"><ul><li>Columns run left to right by call depth.</li><li>Numbers on the lines give the order of the calls inside each function.</li><li>A dashed line leads from an interface to each implementation that can run.</li><li>Click a box to read its docs. Click it again to put it in the centre.</li></ul></div>`;
  if (root.route) h += routeFacts(root.route);
  const m = root.member ? M.member(root.item, root.member) : undefined;
  h += `<h3>${esc(label)}</h3>${docHTML(m ? m.doc : it.doc)}`;
  panel.innerHTML = h;
}

/** Where an entry point registers, which note from atlas.yaml applies, and whether its handler sits inline or outside the code. */
function routeFacts(r: Route): string {
  const note = noteFor(M, r);
  return `<h3>Registered at</h3><div class="where">${sourceLink(r.registeredAt.path + ":" + r.registeredAt.line, r.registeredAt.path, r.registeredAt.line)}${r.registrar && M.owner.has(r.registrar) ? " by " + chip(M.owner.get(r.registrar)!, M.memberName(r.registrar)) : ""}</div>
  ${note ? `<div class="about"><h4>${esc(note.title ?? "Note")}</h4><p>${inlineHTML(note.text)}</p></div>` : ""}
  ${r.externalHandler ? `<p class="note">The handler, ${esc(r.externalHandler)}, lives outside this code.</p>` : r.inline ? `<p class="note">The handler sits inline in the code that registers it.</p>` : ""}`;
}

/** Writes an entry point's panel: its facts and its handler. */
export function route(id: string) {
  const r = M.routeById.get(id);
  if (!r) return;
  panel.innerHTML = `<div class="eyebrow">${esc(ENTRY_KIND[r.kind] ?? r.kind)}</div><h2>${esc(r.name)}</h2>${routeFacts(r)}
  ${!r.inline && r.member !== undefined ? `<h3>Handler</h3><div class="chips">${chip(r.item, r.member)}</div>` : ""}`;
}

/** Writes the index of entry points, grouped by the first part of each path. */
export function routesIndex() {
  const groups = new Map<string, Route[]>();
  for (const r of M.routes) {
    const g =
      r.kind === "http" ? "/" + ((r.path ?? "").split("/").filter(Boolean)[0] ?? "") : (ENTRY_KIND[r.kind] ?? r.kind);
    if (!groups.has(g)) groups.set(g, []);
    groups.get(g)!.push(r);
  }
  panel.innerHTML = `<div class="eyebrow">Index</div><h2 class="sans">Entry points</h2>
  <p class="lead">Every way into the code that the adapters found, grouped by the first part of its path. Click one to trace it from the handler down.</p>
  ${
    M.routes.length
      ? [...groups]
          .sort((a, b) => a[0].localeCompare(b[0]))
          .map(([g, list]) => `<h3>${esc(g)}</h3>${routeList(list)}`)
          .join("")
      : `<p class="nodoc">The adapters found no entry points. Switch on an entry-point plugin in atlas.yaml.</p>`
  }`;
}

/** Writes the help: how to read each view, how to move around, what the colours and lines mean, and the limits that each adapter reported. */
export function howTo() {
  const w = M.anyUnit(),
    ws = M.allUnits(),
    p = M.doc.project,
    meta = M.doc.meta;
  const notes = meta.adapters.flatMap(a => a.notes ?? []);
  panel.innerHTML = `<div class="eyebrow">Help</div><h2 class="sans">How to read this map</h2>
  <div class="doc">
  <p>The map comes straight from the source. ${esc(meta.adapters.map(a => a.name).join(", "))} adapter${meta.adapters.length > 1 ? "s" : ""} read the code${meta.commit ? ` at commit <code>${esc(meta.commit.slice(0, 10))}</code>` : ""}. Every box links to its code.</p>
  <p><b>Overview.</b> Regions group ${ws} by their job. Zoom in to see the ${ws}, with their docs and most-used items.</p>
  <p><b>${esc(cap(w))} focus.</b> Click a ${w}, or zoom into it. The map redraws with it in the centre. The left side lists the ${ws} that use it. The right side lists the ${ws} it uses. Each box names the exact items involved. Hover a ${w} to see the exact items it uses. Hover a row to see who uses it.</p>
  <p><b>Item focus.</b> Click a row in a ${w}. The centre shows its members. The left lists what uses it, by kind of relation. The right lists what it uses. Lines end at the exact member. Every arrow points at the thing used.</p>
  <p><b>Trace.</b> Pick an entry point, or click “trace” beside a callable. The trace follows the calls in order, through each interface to the code that runs. Columns run left to right by call depth. The number on a line gives the order of that call inside its caller.</p>
  <p><b>Moving around.</b> Scroll or pinch to zoom in and out. Zoom far out in a focus view to go up one level. Back and Forward retrace your path. The breadcrumbs at the top show where you are.</p>
  </div>
  <h3>Keys</h3><dl class="keys"><dt><kbd>/</kbd></dt><dd>search</dd><dt><kbd>Tab</kbd> to the map, then arrows</dt><dd>move between boxes and rows</dd><dt><kbd>Enter</kbd></dt><dd>open the selected box</dd><dt><kbd>Space</kbd></dt><dd>read its docs</dd><dt><kbd>Backspace</kbd></dt><dd>back</dd><dt><kbd>Esc</kbd></dt><dd>up one level</dd><dt><kbd>0</kbd></dt><dd>overview</dd><dt><kbd>T</kbd></dt><dd>tour</dd><dt><kbd>+</kbd> <kbd>−</kbd></dt><dd>zoom</dd></dl>
  <h3>What the map leaves out</h3><div class="doc"><ul>${notes.map(n => `<li>${esc(n)}</li>`).join("")}<li>Test code appears only under “Tests that use it”.</li>${p?.excluded?.length ? `<li>These paths were left out: ${p.excluded.map(x => `<code>${esc(x)}</code>`).join(", ")}.</li>` : ""}</ul></div>
  <h3>Rebuild the map</h3><div class="doc"><p>Run <code>acu extract</code> beside <code>atlas.yaml</code>, then <code>acu build</code>. The project README explains the config.</p></div>`;
}

panel.addEventListener("click", e => {
  const t = (e.target as HTMLElement).closest<HTMLElement>(
    "[data-sym],[data-unit],[data-go],[data-entry],[data-trace],[data-region],[data-tour],[data-routes],[data-help],[data-memberdoc]",
  );
  if (!t) return;
  const d = t.dataset;
  if (d.go) go(JSON.parse(d.go));
  else if (d.trace !== undefined) go({ v: "trace", sym: d.trace, member: d.member || "" });
  else if (d.memberdoc) {
    const o = M.owner.get(d.memberdoc)!;
    symbol(o, { member: M.memberName(d.memberdoc) });
    panel.scrollTop = 0;
  } else if (d.sym) go({ v: "symbol", id: d.sym });
  else if (d.unit) go({ v: "unit", id: d.unit });
  else if (d.entry) go({ v: "trace", entry: d.entry });
  else if (d.region) go({ v: "overview", region: d.region });
  else if (d.tour) tour.start();
  else if (d.routes) routesIndex();
  else if (d.help) howTo();
});
panel.addEventListener("keydown", e => {
  if (e.key === "Enter" && (e.target as HTMLElement).classList.contains("tlink")) (e.target as HTMLElement).click();
});

export { stateName };
