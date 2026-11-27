/**
 * The editor: a drawer over the map where a person writes the tour, the
 * regions and the project text of atlas.yaml. It records the view on show
 * as a tour step, reorders steps and regions by drag and drop or by
 * keyboard, previews which units a region's paths match, and saves through
 * the studio's API, which keeps the file's comments and checks the result
 * as `acu extract` does. Only `acu serve -c` and `acu studio` offer it.
 * docs/studio.md describes it; editmodel.ts holds its pure parts.
 *
 * @packageDocumentation
 */

import { M, current, go, stateName } from "./app";
import { call, waitJob, type Job, type ServerState } from "./api";
import * as em from "./editmodel";
import type { OpenCfg, RegionCfg, StepCfg, UnitInfo } from "./editmodel";
import { showSetup } from "./setup";
import { esc, inlineHTML } from "./text";
import { regionColor } from "./theme";
import { viewToState } from "./tour";
import type { Provenance, View } from "./types";

/** One tour step in the editor: its text as atlas.yaml holds it, and the view it opens, when known. */
interface StepItem {
  /** A number that names the entry while the editor lives, for focus and forms. */
  key: number;
  /** The step as atlas.yaml holds it. */
  cfg: StepCfg;
  /** The view by IDs, for Show; null when acu has not resolved the step yet. */
  view: View | null;
  /** The name of the recorded view, which the title field shows as a hint. */
  hint?: string;
  /** The step as the file held it, as JSON; absent for a new step. */
  orig?: string;
}

/** One region in the editor. */
interface RegionItem {
  /** A number that names the entry while the editor lives. */
  key: number;
  /** The region as atlas.yaml holds it. */
  cfg: RegionCfg;
  /** The layout row, from 1, or null to let acu place it. */
  row: number | null;
  /** The region's place within its row in the file, from 0, so a save keeps the order. */
  col?: number | null;
  /** The ID that the file gave the region, so a rename reaches the steps that open it; null for a new region. */
  origId: string | null;
  /** Counts the match requests, so an older reply never overwrites a newer one. */
  seq?: number;
  /** True while a match request waits for its reply. */
  pending?: boolean;
  /** The units that its paths match, as the server reports them. */
  matched?: string[];
  /** Patterns that do not parse. */
  bad?: string[];
  /** True for a region that the person added and whose ID still follows its name. */
  fresh?: boolean;
}

/** The project's text in the editor. */
interface ProjectText {
  /** The project's name. */
  name: string;
  /** The overview text. */
  description: string;
  /** The provenance mark of a description that a program drafted. */
  descriptionDrafted?: Provenance;
}

/** Names the editor's four tabs: the tour, the regions, the project text, and the extraction. */
type Tab = "tour" | "regions" | "project" | "extract";

/** Holds the editor's state: the file's revision, the entries, the open tab and form, and the last message. */
const S = {
  /** The revision of atlas.yaml that the editor read. */
  revision: "",
  /** The path of atlas.yaml. */
  path: "",
  tour: [] as StepItem[],
  regions: [] as RegionItem[],
  project: { name: "", description: "" } as ProjectText,
  units: [] as UnitInfo[],
  /** Whether atlas.yaml had a layout when the editor read it. */
  hadLayout: false,
  /** The IDs of the regions in the file, as the editor read them. */
  fileIds: [] as string[],
  /** Each region's row and place in the layout of the file, by the file's ID. */
  fileRows: new Map<string, { row: number; col: number }>(),
  /** The saved form of each section, to tell what changed. */
  saved: { tour: "", regions: "", project: "" },
  tab: "tour" as Tab,
  /** The key of the entry whose form is open. */
  open: -1,
  /** A message for the reader, and whether it reports a problem. */
  msg: null as { text: string; bad?: boolean } | null,
  /** The last job's state. */
  job: null as Job | null,
  /** True while a save or a job runs. */
  busy: false,
  /** True while a save runs: the fields go inert, so no edit gets lost to the reload. */
  saving: false,
  /** The server's state. */
  server: null as ServerState | null,
};

/** The message that a render last showed. */
let lastShown: { text: string; bad?: boolean } | null = null;

/** Counts keys for new entries. */
let keys = 0;
/** The drawer. */
let root: HTMLElement;
/** The entry that a drag carries: its list and position. */
let dragging: { list: "tour" | "regions"; at: number } | null = null;
/** The pending match request of each region, by key. */
const matchTimers = new Map<number, number>();

/** The session storage key that keeps the drawer open across the reload after a save. */
const KEEP = "acu-editor";

/** Reads session storage; a private window may refuse it. */
function kept(): string | null {
  try {
    return sessionStorage.getItem(KEEP);
  } catch {
    return null;
  }
}

/** Writes session storage, or removes the key for null. */
function keep(v: string | null) {
  try {
    if (v == null) sessionStorage.removeItem(KEEP);
    else sessionStorage.setItem(KEEP, v);
  } catch {
    /* private window */
  }
}

/** Names an ID for the reader: a region's, unit's or entry point's name, or an item's qualified name. */
function label(id: string): string {
  const r = M.regions.find(x => x.id === id);
  if (r) return r.name;
  const u = M.units.get(id);
  if (u) return u.name;
  const e = M.routeById.get(id);
  if (e) return e.name;
  const owner = M.owner.get(id);
  if (owner) {
    const m = M.memberName(id);
    return M.qual(owner) + (m ? "." + m : "");
  }
  return id;
}

/** The provenance mark of text that the editor itself makes, such as suggested regions. */
function mark(): Provenance {
  return { by: "acu studio", at: new Date().toISOString().replace(/\.\d+Z$/, "Z") };
}

/** Serializes the parts of the editor that atlas.yaml stores, per section. */
function forms() {
  const regions = S.regions.map(r => em.clean(r.cfg));
  const rows = em.layoutRows(S.regions.map(r => ({ id: r.cfg.id, row: r.row, col: r.col })));
  const layout = rows.length ? { rows } : S.hadLayout ? null : undefined;
  return {
    tour: S.tour.map(s => em.clean(s.cfg)),
    regions,
    layout,
    project: S.project,
    keys: {
      tour: JSON.stringify(S.tour.map(s => em.clean(s.cfg))),
      regions: JSON.stringify({ regions, layout }),
      project: JSON.stringify(em.clean(S.project)),
    },
  };
}

/** Lists the regions whose ID the person changed, as old ID to new ID. */
function renames(): Map<string, string> {
  const m = new Map<string, string>();
  for (const r of S.regions) if (r.origId && r.origId !== r.cfg.id) m.set(r.origId, r.cfg.id);
  return m;
}

/** Lists the IDs of the file's regions that the person deleted. */
function deletedIds(): Set<string> {
  const now = new Set(S.regions.map(r => r.origId));
  return new Set(S.fileIds.filter(id => !now.has(id)));
}

/** Reports which sections differ from the file; nothing differs before the file loads. */
function changed() {
  if (!S.revision) return { tour: false, regions: false, project: false };
  const k = forms().keys;
  return {
    tour: k.tour !== S.saved.tour,
    regions: k.regions !== S.saved.regions,
    project: k.project !== S.saved.project,
  };
}

/** Reports whether any section has unsaved changes. */
function dirty(): boolean {
  const c = changed();
  return c.tour || c.regions || c.project;
}

/** Reads atlas.yaml and the units from the server and fills the editor. */
async function load(): Promise<boolean> {
  const a = await call("config");
  if (!a.ok) {
    S.msg = { text: a.data.error, bad: true };
    return false;
  }
  const d = a.data;
  S.revision = d.revision;
  S.path = d.path ?? "";
  S.units = d.units ?? [];
  S.hadLayout = d.layout != null;
  const rows = em.rowsOf(d.layout);
  S.fileIds = (d.regions as RegionCfg[]).map(r => r.id);
  S.fileRows = rows;
  const resolved: View[] = (d.resolved?.tour ?? []).map((s: any) => s.open);
  S.tour = (d.tour as StepCfg[]).map((cfg, i) => ({
    key: ++keys,
    cfg,
    view: resolved[i] ?? null,
    orig: JSON.stringify(cfg),
  }));
  S.regions = (d.regions as RegionCfg[]).map(cfg => ({
    key: ++keys,
    cfg,
    row: rows.get(cfg.id)?.row ?? null,
    col: rows.get(cfg.id)?.col ?? null,
    origId: cfg.id,
  }));
  S.project = {
    name: d.project?.name ?? "",
    description: d.project?.description ?? "",
    ...(d.project?.descriptionDrafted ? { descriptionDrafted: d.project.descriptionDrafted } : {}),
  };
  markSaved();
  S.regions.forEach(r => queueMatch(r, 0));
  return true;
}

/** Records the current state as the saved one. */
function markSaved() {
  const k = forms().keys;
  S.saved = { ...k };
}

/** Announces a short sentence to screen readers, through the page's live region, which no redraw replaces. */
function announce(text: string) {
  const say = document.getElementById("announce")!;
  say.textContent = "";
  requestAnimationFrame(() => (say.textContent = text));
}

/* Drawing. */

/** Draws a drafted mark with the button that a person presses after checking the text. */
function draftedBox(p: Provenance | undefined, act: string, key: number): string {
  if (!p) return "";
  return `<div class="ed-drafted" role="note"><b>Machine-drafted.</b> ${esc(p.by)} wrote this on ${esc(
    p.at.slice(0, 10),
  )}. Read it against the code, then <button class="btn" data-act="${act}" data-key="${key}">Mark as checked</button></div>`;
}

/** Draws one tour step: its head row, and its form when open. */
function stepHTML(s: StepItem, i: number, n: number): string {
  const openForm = S.open === s.key;
  const c = s.cfg;
  const head = `<div class="ed-head" role="group" draggable="true" data-list="tour" data-at="${i}" tabindex="0" data-key="${s.key}"
    aria-label="Step ${i + 1} of ${n}: ${esc(c.title || "untitled")}" aria-describedby="edMoveHint">
    <span class="ed-grip" aria-hidden="true">⠿</span>
    <span class="ed-num">${i + 1}</span>
    <span class="ed-title"><b>${esc(c.title || "Untitled step")}</b><small>Opens ${esc(em.describeOpen(c.open, label))}${
      c.drafted ? ' · <span class="ed-tag">drafted</span>' : ""
    }</small></span>
    <span class="ed-acts">
      <button class="btn" data-act="up" data-list="tour" data-at="${i}" ${i === 0 ? "disabled" : ""} aria-label="Move step ${i + 1} up">↑</button>
      <button class="btn" data-act="down" data-list="tour" data-at="${i}" ${i === n - 1 ? "disabled" : ""} aria-label="Move step ${i + 1} down">↓</button>
      <button class="btn" data-act="toggle" data-key="${s.key}" aria-expanded="${openForm}">${openForm ? "Close" : "Edit"}</button>
    </span></div>`;
  if (!openForm) return `<li data-key="${s.key}">${head}</li>`;
  const form = `<div class="ed-form">
    <label>Title<input data-field="title" data-key="${s.key}" value="${esc(c.title)}" placeholder="${esc(s.hint ?? "")}" /></label>
    <label>Text<textarea data-field="text" data-key="${s.key}" rows="6">${esc(c.text)}</textarea></label>
    <p class="ed-hint">Write short sentences. Use \`backticks\` for code. Preview:</p>
    <p class="ed-preview" data-preview="${s.key}">${inlineHTML(c.text)}</p>
    <p class="ed-opens">Opens ${esc(em.describeOpen(c.open, label))}.</p>
    <div class="ed-row">
      <button class="btn" data-act="show" data-key="${s.key}" ${s.view ? "" : "disabled"}>Show</button>
      <button class="btn" data-act="reopen" data-key="${s.key}">Use the current view</button>
      <button class="btn" data-act="delete" data-list="tour" data-at="${i}">Delete step</button>
    </div>
    ${draftedBox(c.drafted, "checked-step", s.key)}
  </div>`;
  return `<li data-key="${s.key}" class="open">${head}${form}</li>`;
}

/** Draws one region: its head row, and its form when open. */
function regionHTML(r: RegionItem, i: number, n: number): string {
  const c = r.cfg;
  const openForm = S.open === r.key;
  const count = r.matched ? `${r.matched.length} unit${r.matched.length === 1 ? "" : "s"}` : "…";
  const swatch = `<span class="ed-swatch" style="background:${regionColor(c.color ?? i)}" aria-hidden="true"></span>`;
  const head = `<div class="ed-head" role="group" draggable="true" data-list="regions" data-at="${i}" tabindex="0" data-key="${r.key}"
    aria-label="Region ${i + 1} of ${n}: ${esc(c.name || "unnamed")}" aria-describedby="edMoveHint">
    <span class="ed-grip" aria-hidden="true">⠿</span>${swatch}
    <span class="ed-title"><b>${esc(c.name || "Unnamed region")}</b><small>${esc(c.id)} · <span data-count="${r.key}">${count}</span>${
      c.default ? " · default" : ""
    }${r.row ? " · row " + r.row : ""}${c.drafted ? ' · <span class="ed-tag">drafted</span>' : ""}</small></span>
    <span class="ed-acts">
      <button class="btn" data-act="up" data-list="regions" data-at="${i}" ${i === 0 ? "disabled" : ""} aria-label="Move region ${esc(c.name)} up">↑</button>
      <button class="btn" data-act="down" data-list="regions" data-at="${i}" ${i === n - 1 ? "disabled" : ""} aria-label="Move region ${esc(c.name)} down">↓</button>
      <button class="btn" data-act="toggle" data-key="${r.key}" aria-expanded="${openForm}">${openForm ? "Close" : "Edit"}</button>
    </span></div>`;
  if (!openForm) return `<li data-key="${r.key}">${head}</li>`;
  const paths = c.paths ?? [];
  const colours = ['<option value="">Automatic</option>']
    .concat(
      Array.from(
        { length: 9 },
        (_, k) => `<option value="${k}" ${c.color === k ? "selected" : ""}>Colour ${k + 1}</option>`,
      ),
    )
    .join("");
  const names = (r.matched ?? []).map(id => S.units.find(u => u.id === id)?.path ?? id);
  const shared = em.sharedLocals(S.units);
  const unitBoxes = S.units
    .map(u => {
      const st = em.pickState(paths, u, shared.has(u.local));
      return st === "fixed"
        ? `<label class="ed-check"><input type="checkbox" checked disabled /> <code>${esc(u.path)}</code> (through <code>${esc(
            u.local,
          )}</code>; edit Paths to change it)</label>`
        : `<label class="ed-check"><input type="checkbox" data-unit="${esc(u.id)}" data-key="${r.key}" ${
            st === "on" ? "checked" : ""
          } /> <code>${esc(u.path)}</code></label>`;
    })
    .join("");
  const form = `<div class="ed-form">
    <label>Name<input data-field="name" data-key="${r.key}" value="${esc(c.name)}" placeholder="Name the region by its job" /></label>
    <label>ID<input data-field="id" data-key="${r.key}" value="${esc(c.id)}" spellcheck="false" /></label>
    <label>What it does<textarea data-field="description" data-key="${r.key}" rows="4">${esc(c.description ?? "")}</textarea></label>
    <label>How to read it<textarea data-field="readingTip" data-key="${r.key}" rows="3">${esc(c.readingTip ?? "")}</textarea></label>
    <label>Paths, one pattern per line<textarea data-field="paths" data-key="${r.key}" rows="3" spellcheck="false">${esc(
      paths.join("\n"),
    )}</textarea></label>
    <p class="ed-hint" data-match="${r.key}">${matchText(r, names)}</p>
    <details class="ed-units"><summary>Pick units</summary>${unitBoxes || "<p>You find no units on this map.</p>"}</details>
    <div class="ed-grid">
      <label>Colour<select data-field="color" data-key="${r.key}">${colours}</select></label>
      <label>Layout row<input type="number" min="1" max="20" data-field="row" data-key="${r.key}" value="${r.row ?? ""}" placeholder="Automatic" /></label>
    </div>
    <label class="ed-check"><input type="checkbox" data-field="default" data-key="${r.key}" ${
      c.default ? "checked" : ""
    } /> Default region: it takes the units that match no other region</label>
    <div class="ed-row"><button class="btn" data-act="delete" data-list="regions" data-at="${i}">Delete region</button></div>
    ${draftedBox(c.drafted, "checked-region", r.key)}
  </div>`;
  return `<li data-key="${r.key}" class="open">${head}${form}</li>`;
}

/** Says what a region's paths match. */
function matchText(r: RegionItem, names: string[]): string {
  const bad = r.bad?.length ? ` Fix these patterns: ${esc(r.bad.join("; "))}.` : "";
  if (!r.matched) return "Checking the paths…" + bad;
  if (!r.matched.length) return "The paths match no unit yet." + bad;
  const shown = names
    .slice(0, 12)
    .map(n => `<code>${esc(n)}</code>`)
    .join(", ");
  const more = names.length > 12 ? ` and ${names.length - 12} more` : "";
  return `The paths match ${names.length} unit${names.length === 1 ? "" : "s"}: ${shown}${more}. A unit joins the first region that matches it.${bad}`;
}

/** Draws the tour tab. */
function tourTab(): string {
  const n = S.tour.length;
  return `<p class="ed-lead">Move the map to a view, then record it as a step. Drag steps, or use the arrows, to reorder them.</p>
    <div class="ed-row"><button class="btn primary" data-act="record">Record this view as a step</button></div>
    ${n ? `<ol class="ed-list" data-list="tour">${S.tour.map((s, i) => stepHTML(s, i, n)).join("")}</ol>` : '<p class="ed-empty">The tour has no steps yet.</p>'}`;
}

/** Draws the regions tab. */
function regionsTab(): string {
  const n = S.regions.length;
  // The map on show names regions by the IDs of the file, so a renamed region still counts.
  const ids = new Set(S.regions.flatMap(r => [r.cfg.id, r.origId ?? ""]));
  const loose = S.units.filter(u => !u.region || !ids.has(u.region));
  const looseText = loose.length
    ? `<details class="ed-units"><summary>${loose.length} unit${loose.length === 1 ? "" : "s"} sit in no region of yours</summary><p>${loose
        .slice(0, 60)
        .map(u => `<code>${esc(u.local)}</code>`)
        .join(", ")}${loose.length > 60 ? " …" : ""}</p></details>`
    : "";
  return `<p class="ed-lead">A region groups units by job. The overview draws one box per region.</p>
    <div class="ed-row"><button class="btn primary" data-act="add-region">Add a region</button>${
      n ? "" : '<button class="btn" data-act="suggest">Suggest regions from folders</button>'
    }</div>
    ${looseText}
    ${n ? `<ol class="ed-list" data-list="regions">${S.regions.map((r, i) => regionHTML(r, i, n)).join("")}</ol>` : '<p class="ed-empty">The config names no regions, so acu makes one per top-level folder.</p>'}`;
}

/** Draws the project tab. */
function projectTab(): string {
  const p = S.project;
  return `<div class="ed-form">
    <label>Name<input data-field="pname" value="${esc(p.name)}" /></label>
    <label>Description<textarea data-field="pdesc" rows="10">${esc(p.description)}</textarea></label>
    <p class="ed-hint">A blank line starts a new paragraph.</p>
    ${draftedBox(p.descriptionDrafted, "checked-project", 0)}
  </div>`;
}

/** Joins a job's lines of progress and its error, if any, for the log box. */
function logText(j: Job): string {
  return j.log.join("\n") + (j.error ? "\n" + j.error : "");
}

/** Draws the extraction tab. */
function extractTab(): string {
  const j = S.job;
  const log = j ? `<pre class="ed-log">${esc(logText(j))}</pre>` : "";
  const state = j
    ? `<p class="ed-hint">The last run ${j.state === "running" ? "runs now" : j.state === "done" ? "finished" : "failed"}.</p>`
    : "";
  return `<p class="ed-lead">Run the adapters again after the code changes, or after you edit the languages or entry points in atlas.yaml.</p>
    <div class="ed-row"><button class="btn primary" data-act="extract" ${S.busy || dirty() ? "disabled" : ""}>Run the adapters again</button></div>
    ${dirty() ? '<p class="ed-hint">Save or discard your changes first.</p>' : ""}${state}${log}`;
}

/** Draws the whole drawer, and puts the focus back on the element that `focus` selects. */
function render(focus?: string) {
  // A message speaks once: only the render that first shows it gives it a
  // live role, so later redraws do not repeat it to a screen reader.
  const fresh = S.msg !== lastShown;
  lastShown = S.msg;
  // A status message speaks through the page's own live region, which
  // stays in place; a problem speaks through role=alert on its first show.
  if (fresh && S.msg && !S.msg.bad) announce(S.msg.text);
  const c = changed();
  const tabs: [Tab, string, boolean][] = [
    ["tour", "Tour", c.tour],
    ["regions", "Regions", c.regions],
    ["project", "Project", c.project],
    ["extract", "Extract", false],
  ];
  const body = !S.revision
    ? ""
    : S.tab === "tour"
      ? tourTab()
      : S.tab === "regions"
        ? regionsTab()
        : S.tab === "project"
          ? projectTab()
          : extractTab();
  const isDirty = c.tour || c.regions || c.project;
  root.innerHTML = `<div class="ed-top">
      <h2>Edit the map</h2>
      <button class="btn" data-act="close" aria-label="Close the editor">Close</button>
    </div>
    <p class="ed-file">${esc(S.path)}</p>
    ${S.revision ? "" : `<p class="ed-msg bad" role="alert" tabindex="-1"><b>Problem.</b> ${esc(S.msg?.text ?? "acu could not read atlas.yaml.")} Fix the file, then reload this page.</p>`}
    <div class="ed-tabs" role="tablist" ${S.saving ? "inert" : ""}>${tabs
      .map(
        ([t, name, ch]) =>
          `<button role="tab" class="ed-tab" id="edTab-${t}" aria-controls="edPanel" tabindex="${S.tab === t ? 0 : -1}" data-act="tab" data-tab="${t}" aria-selected="${S.tab === t}">${name}<span class="ed-dot" ${ch ? "" : "hidden"}> •<span class="sr"> (changed)</span></span></button>`,
      )
      .join("")}</div>
    <div class="ed-body" role="tabpanel" tabindex="-1" id="edPanel" aria-labelledby="edTab-${S.tab}" ${S.saving ? "inert" : ""}>${body}</div>
    <div class="ed-foot">
      ${S.msg && S.revision ? `<p class="ed-msg ${S.msg.bad ? "bad" : ""}" tabindex="-1" ${fresh && S.msg.bad ? `role="alert"` : ""}>${S.msg.bad ? "<b>Problem.</b> " : ""}${esc(S.msg.text)}</p>` : ""}
      <div class="ed-row">
        <button class="btn primary" data-act="save" ${isDirty && !S.busy ? "" : "disabled"}>Save to atlas.yaml</button>
        <button class="btn" data-act="discard" ${isDirty && !S.busy ? "" : "disabled"}>Discard changes</button>
        ${S.server?.setup ? '<button class="btn" data-act="projects">Projects</button>' : ""}
      </div>
    </div>
    <p class="sr" id="edMoveHint">Alt and the up or down arrow key move it. Enter opens its form.</p>`;
  if (focus) (root.querySelector(focus) as HTMLElement | null)?.focus();
}

/* Changes. */

/** Finds a step or a region by key. */
function byKey(key: number): StepItem | RegionItem | undefined {
  return S.tour.find(s => s.key === key) ?? S.regions.find(r => r.key === key);
}

/** Moves an entry of a list and keeps the focus on it. */
function moveEntry(list: "tour" | "regions", from: number, to: number, focusOn: "head" | "up" | "down" = "head") {
  const arr = list === "tour" ? S.tour : S.regions;
  if (to < 0 || to >= arr.length) return;
  const key = arr[from].key;
  if (list === "tour") S.tour = em.move(S.tour, from, to);
  else S.regions = em.move(S.regions, from, to);
  const what = list === "tour" ? "Step" : "Region";
  announce(`${what} moved to position ${to + 1} of ${arr.length}.`);
  render(`.ed-head[data-key="${key}"]`);
  if (focusOn !== "head")
    root.querySelector<HTMLElement>(`li[data-key="${key}"] [data-act="${focusOn}"]:not(:disabled)`)?.focus();
}

/** Asks the server which units a region's paths match, after a short pause, and updates the region's text in place. */
function queueMatch(r: RegionItem, delay = 250) {
  clearTimeout(matchTimers.get(r.key));
  matchTimers.set(
    r.key,
    window.setTimeout(async () => {
      matchTimers.delete(r.key);
      const seq = (r.seq = (r.seq ?? 0) + 1);
      r.pending = true;
      const a = await call<{ units: string[]; errors: string[] }>("match", { paths: r.cfg.paths ?? [] });
      if (seq !== r.seq) return;
      r.pending = false;
      if (!a.ok) return;
      r.matched = a.data.units;
      r.bad = a.data.errors;
      const names = r.matched.map(id => S.units.find(u => u.id === id)?.path ?? id);
      const p = root.querySelector(`[data-match="${r.key}"]`);
      if (p) p.innerHTML = matchText(r, names);
      const count = root.querySelector(`[data-count="${r.key}"]`);
      if (count) count.textContent = `${r.matched.length} unit${r.matched.length === 1 ? "" : "s"}`;
    }, delay),
  );
}

/** Adds the view on show as a new step after the open one, or at the end. */
function record() {
  const st = current();
  const open: OpenCfg = em.stateToOpen(st, (o, m) => M.member(o, m)?.id);
  // The title stays empty for the person to write; the view's name shows as a hint.
  const item: StepItem = {
    key: ++keys,
    cfg: { title: "", text: "", open },
    view: em.openToView(open),
    hint: stateName(st),
  };
  const at = S.tour.findIndex(s => s.key === S.open);
  if (at >= 0) S.tour.splice(at + 1, 0, item);
  else S.tour.push(item);
  S.open = item.key;
  S.msg = { text: "You added a step. Write its title and text, then save." };
  render(`[data-field="title"][data-key="${item.key}"]`);
}

/** Saves the changed sections in order (project, regions, tour), then reloads the page to show the new map. */
async function save() {
  const f = forms();
  const cfgs = S.tour.map(s => s.cfg);
  const issues = em
    .problems(
      cfgs,
      S.regions.map(r => r.cfg),
      S.project.name,
      i => JSON.stringify(S.tour[i].cfg) !== S.tour[i].orig,
    )
    .concat(em.orphanSteps(cfgs, deletedIds()))
    .concat(
      S.regions.flatMap((r, i) =>
        // A reply still on its way may clear the problem, so only a settled one counts.
        r.bad?.length && !matchTimers.has(r.key) && !r.pending
          ? [`Fix the path patterns of region ${i + 1}: ${r.bad.join("; ")}.`]
          : [],
      ),
    );
  if (issues.length) {
    S.msg = { text: issues.join(" "), bad: true };
    render(".ed-msg");
    return;
  }
  // A renamed region and the steps that open it save in one write. The
  // rename reaches the request only; the editor's steps take it after the
  // save works, so a failed save leaves them as the person left them.
  const ren = renames();
  if (ren.size) {
    Object.assign(f, { tour: em.renameSteps(cfgs, ren).map(c => em.clean(c)) });
    f.keys.tour = JSON.stringify(f.tour);
  }
  const c = { ...changed(), tour: f.keys.tour !== S.saved.tour };
  const withTour = c.regions && c.tour;
  S.busy = true;
  S.saving = true;
  S.msg = { text: "Saving…" };
  render();
  const steps: [boolean, string, () => object][] = [
    [
      c.project,
      "project",
      () => ({
        name: S.project.name,
        description: S.project.description,
        // A description that the person cleared keeps no mark.
        descriptionDrafted: S.project.description.trim() ? (S.project.descriptionDrafted ?? null) : null,
      }),
    ],
    [
      c.regions,
      "regions",
      () => ({
        regions: f.regions,
        ...(f.layout === undefined ? {} : { layout: f.layout }),
        ...(withTour ? { tour: f.tour } : {}),
      }),
    ],
    [c.tour && !withTour, "tour", () => ({ tour: f.tour })],
  ];
  for (const [need, path, body] of steps) {
    if (!need) continue;
    const a = await call(path, { revision: S.revision, ...body() });
    if (!a.ok) {
      S.busy = false;
      S.saving = false;
      const said = String(a.data?.error ?? "The save failed").replace(/\.?$/, ".");
      S.msg = {
        text: a.status === 409 ? said + " Discard changes reads the file again." : said,
        bad: true,
      };
      render(".ed-msg");
      return;
    }
    S.revision = a.data.revision;
    S.saved[path as "tour" | "regions" | "project"] = f.keys[path as "tour" | "regions" | "project"];
    if ((path === "regions" && withTour) || path === "tour") {
      S.tour.forEach(x => (x.cfg = em.renameSteps([x.cfg], ren)[0]));
    }
    if (path === "regions") {
      if (withTour) S.saved.tour = f.keys.tour;
      // The file now names the regions by their new IDs, in the new layout.
      S.regions.forEach(r => (r.origId = r.cfg.id));
      S.fileIds = S.regions.map(r => r.cfg.id);
      S.fileRows = em.rowsOf(f.layout ?? null);
      if (f.layout !== undefined) S.hadLayout = f.layout != null;
    }
  }
  keep(S.tab);
  location.reload();
}

/** Runs the adapters again, shows the progress, and reloads the page when they finish. */
async function extract() {
  S.busy = true;
  render("#edPanel");
  const a = await call("extract", {});
  if (!a.ok) {
    S.busy = false;
    S.msg = { text: a.data.error, bad: true };
    render(".ed-msg");
    return;
  }
  S.job = a.data.job;
  render();
  const j = await waitJob(job => {
    S.job = job;
    const pre = root.querySelector(".ed-log");
    if (pre) pre.textContent = logText(job);
    else render();
  });
  S.busy = false;
  announce(j?.state === "done" ? "The adapters finished. The page reloads." : "The adapters failed.");
  if (j?.state === "done") {
    keep("extract");
    location.reload();
    return;
  }
  S.job = j;
  S.msg = { text: j?.error ?? "The run failed.", bad: true };
  render(".ed-msg");
}

/** Handles a click on any button of the drawer. */
async function onClick(e: Event) {
  const b = (e.target as HTMLElement).closest<HTMLElement>("[data-act]");
  if (!b || (b as HTMLButtonElement).disabled) return;
  const act = b.dataset.act!;
  const key = Number(b.dataset.key);
  const at = Number(b.dataset.at);
  const list = b.dataset.list as "tour" | "regions";
  switch (act) {
    case "close":
      closeEditor();
      return;
    case "tab":
      S.tab = b.dataset.tab as Tab;
      S.open = -1;
      render(`[data-tab="${S.tab}"]`);
      return;
    case "toggle":
      S.open = S.open === key ? -1 : key;
      render(S.open === key ? `.ed-form [data-key="${key}"]` : `[data-act="toggle"][data-key="${key}"]`);
      return;
    case "up":
      moveEntry(list, at, at - 1, "up");
      return;
    case "down":
      moveEntry(list, at, at + 1, "down");
      return;
    case "delete": {
      const arr = list === "tour" ? S.tour : S.regions;
      arr.splice(at, 1);
      S.open = -1;
      S.msg = {
        text: `You deleted ${list === "tour" ? "a step" : "a region"}. Discard your changes to bring it back.`,
      };
      // Focus goes to the entry that took its place, else the one above,
      // else the button that adds one.
      const next = arr[at] ?? arr[at - 1];
      render(
        next ? `.ed-head[data-key="${next.key}"]` : list === "tour" ? "[data-act='record']" : "[data-act='add-region']",
      );
      return;
    }
    case "record":
      record();
      return;
    case "show": {
      const s = S.tour.find(x => x.key === key);
      if (s?.view) go(viewToState(s.view));
      return;
    }
    case "reopen": {
      const s = S.tour.find(x => x.key === key);
      if (!s) return;
      s.cfg = { ...s.cfg, open: em.stateToOpen(current(), (o, m) => M.member(o, m)?.id) };
      s.view = em.openToView(s.cfg.open);
      render(`[data-act="reopen"][data-key="${key}"]`);
      return;
    }
    case "checked-step":
    case "checked-region": {
      const it = byKey(key);
      if (it) {
        const { drafted: _, ...rest } = it.cfg as StepCfg & RegionCfg;
        it.cfg = rest as any;
      }
      S.msg = { text: "You marked the text as checked. Save to keep it." };
      render(`[data-act="toggle"][data-key="${key}"]`);
      return;
    }
    case "checked-project":
      delete S.project.descriptionDrafted;
      S.msg = { text: "You marked the description as checked. Save to keep it." };
      render('[data-field="pdesc"]');
      return;
    case "add-region": {
      const id = em.slug(
        "region",
        S.regions.map(r => r.cfg.id),
      );
      const r: RegionItem = { key: ++keys, cfg: { id, name: "", paths: [] }, row: null, origId: null, fresh: true };
      S.regions.push(r);
      S.open = r.key;
      queueMatch(r, 0);
      render(`[data-field="name"][data-key="${r.key}"]`);
      return;
    }
    case "suggest": {
      const m = mark();
      S.regions = em.suggestRegions(S.units, m).map(cfg => ({ key: ++keys, cfg, row: null, origId: null }));
      S.regions.forEach(r => queueMatch(r, 0));
      S.msg = {
        text: `acu suggested ${S.regions.length} regions from the folders, marked as drafted. Rename them by job, then save.`,
      };
      render(".ed-list .ed-head");
      return;
    }
    case "save":
      await save();
      return;
    case "discard":
      S.open = -1;
      S.msg = (await load()) ? { text: "You discarded your changes." } : S.msg;
      render(".ed-tab[aria-selected='true']");
      return;
    case "extract":
      await extract();
      return;
    case "projects":
      if (dirty() && !confirm("You have unsaved changes. Leave them?")) return;
      closeEditor();
      showSetup(S.server!);
      return;
  }
}

/** Handles typing in a field of the drawer: it changes the entry and redraws only what depends on it. */
function onInput(e: Event) {
  const t = e.target as HTMLInputElement;
  const field = t.dataset.field;
  if (!field) {
    if (t.dataset.unit) {
      const r = byKey(Number(t.dataset.key)) as RegionItem | undefined;
      const u = S.units.find(x => x.id === t.dataset.unit);
      if (!r || !u) return;
      const shared = em.sharedLocals(S.units).has(u.local);
      r.cfg = { ...r.cfg, paths: em.togglePath(r.cfg.paths ?? [], u, t.checked, shared) };
      const ta = root.querySelector<HTMLTextAreaElement>(`textarea[data-field="paths"][data-key="${r.key}"]`);
      if (ta) ta.value = (r.cfg.paths ?? []).join("\n");
      queueMatch(r);
      refreshFoot();
    }
    return;
  }
  if (field === "pname") S.project.name = t.value;
  else if (field === "pdesc") S.project.description = t.value;
  else {
    const it = byKey(Number(t.dataset.key));
    if (!it) return;
    if ("view" in it) {
      (it.cfg as any)[field] = t.value;
      if (field === "text") {
        const p = root.querySelector(`[data-preview="${it.key}"]`);
        if (p) p.innerHTML = inlineHTML(t.value);
      }
    } else {
      const r = it;
      if (field === "paths") {
        r.cfg = { ...r.cfg, paths: em.parsePaths(t.value) };
        queueMatch(r);
      } else if (field === "color") r.cfg = { ...r.cfg, color: t.value === "" ? undefined : Number(t.value) };
      else if (field === "row") {
        const row = t.value === "" ? null : Math.max(1, Math.floor(Number(t.value)) || 1);
        // A region in a new row goes to that row's end; back in its row of
        // the file, it takes its place from the file again.
        const home = S.fileRows.get(r.origId ?? "");
        r.col = home && home.row === row ? home.col : null;
        r.row = row;
      } else if (field === "default") r.cfg = { ...r.cfg, default: t.checked || undefined };
      else if (field === "id") {
        r.cfg = { ...r.cfg, id: t.value.trim() };
        r.fresh = false;
      } else if (field === "name") {
        r.cfg = { ...r.cfg, name: t.value };
        if (r.fresh) {
          const id = em.slug(
            t.value,
            S.regions.filter(x => x !== r).map(x => x.cfg.id),
          );
          r.cfg.id = id;
          const idBox = root.querySelector<HTMLInputElement>(`input[data-field="id"][data-key="${r.key}"]`);
          if (idBox) idBox.value = id;
        }
      } else (r.cfg as any)[field] = t.value;
    }
  }
  refreshFoot();
}

/** Updates the Save and Discard buttons and the tab marks without redrawing the fields. */
function refreshFoot() {
  const c = changed();
  const d = c.tour || c.regions || c.project;
  root
    .querySelectorAll<HTMLButtonElement>('[data-act="save"], [data-act="discard"]')
    .forEach(b => (b.disabled = !d || S.busy));
  root.querySelectorAll<HTMLElement>(".ed-tab").forEach(b => {
    const t = b.dataset.tab as Tab;
    const ch = t !== "extract" && c[t];
    b.querySelector<HTMLElement>(".ed-dot")?.toggleAttribute("hidden", !ch);
  });
}

/** Handles keys: Escape closes the drawer, the arrows move between tabs, and Alt and the arrows move the focused entry. */
function onKey(e: KeyboardEvent) {
  const t = e.target as HTMLElement;
  // Escape in a field or during a save stays with the field.
  const typing =
    t.tagName === "TEXTAREA" ||
    t.tagName === "SELECT" ||
    (t.tagName === "INPUT" && !["checkbox", "radio"].includes((t as HTMLInputElement).type));
  if (e.key === "Escape" && !S.busy && !typing) {
    e.preventDefault();
    closeEditor();
    return;
  }
  const tabKeys = ["ArrowLeft", "ArrowRight", "Home", "End"];
  if (t.classList.contains("ed-tab") && tabKeys.includes(e.key)) {
    e.preventDefault();
    const order: Tab[] = ["tour", "regions", "project", "extract"];
    const i = order.indexOf(t.dataset.tab as Tab);
    const next =
      e.key === "Home" ? 0 : e.key === "End" ? order.length - 1 : i + (e.key === "ArrowRight" ? 1 : order.length - 1);
    S.tab = order[next % order.length];
    S.open = -1;
    render(`[data-tab="${S.tab}"]`);
    return;
  }
  const h = (e.target as HTMLElement).closest<HTMLElement>(".ed-head");
  if (!h || e.target !== h) return;
  const at = Number(h.dataset.at);
  const list = h.dataset.list as "tour" | "regions";
  if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
    e.preventDefault();
    moveEntry(list, at, at + (e.key === "ArrowUp" ? -1 : 1));
  } else if (e.key === "Enter" || e.key === " ") {
    e.preventDefault();
    const key = Number(h.dataset.key);
    S.open = S.open === key ? -1 : key;
    render(S.open === key ? `.ed-form [data-key="${key}"]` : `.ed-head[data-key="${key}"]`);
  }
}

/** Starts a drag of an entry's head row. */
function onDragStart(e: DragEvent) {
  const h = (e.target as HTMLElement).closest<HTMLElement>(".ed-head");
  if (!h) return;
  dragging = { list: h.dataset.list as "tour" | "regions", at: Number(h.dataset.at) };
  e.dataTransfer?.setData("text/plain", String(dragging.at));
  if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
  h.classList.add("dragging");
}

/** Reports whether the pointer sits in the lower half of a row, where a drop lands after the row. */
function lowerHalf(e: DragEvent, h: HTMLElement): boolean {
  const r = h.getBoundingClientRect();
  return e.clientY > r.top + r.height / 2;
}

/** Marks the place that a drop would take: before the row under the pointer, or after it in its lower half. */
function onDragOver(e: DragEvent) {
  const h = (e.target as HTMLElement).closest<HTMLElement>(".ed-head");
  if (!dragging || !h || h.dataset.list !== dragging.list) return;
  e.preventDefault();
  root.querySelectorAll(".drop, .drop-after").forEach(x => x.classList.remove("drop", "drop-after"));
  h.classList.add(lowerHalf(e, h) ? "drop-after" : "drop");
}

/** Drops the dragged entry at the position of the entry under the pointer. */
function onDrop(e: DragEvent) {
  const h = (e.target as HTMLElement).closest<HTMLElement>(".ed-head");
  const d = dragging;
  dragging = null;
  root
    .querySelectorAll(".drop, .drop-after, .dragging")
    .forEach(x => x.classList.remove("drop", "drop-after", "dragging"));
  if (!d || !h || h.dataset.list !== d.list) return;
  e.preventDefault();
  // The place before (or after) the target; moving down, it sits one lower once the entry leaves.
  const target = Number(h.dataset.at) + (lowerHalf(e, h) ? 1 : 0);
  moveEntry(d.list, d.at, d.at < target ? target - 1 : target);
}

/** Opens the drawer, reading atlas.yaml first. */
export async function openEditor() {
  root.hidden = false;
  // On a narrow screen the docs panel would cover the drawer.
  document.body.classList.add("editing");
  document.getElementById("editBtn")?.setAttribute("aria-pressed", "true");
  keep(S.tab);
  if (!S.revision) await load();
  render(".ed-tab[aria-selected='true']");
}

/** Closes the drawer; unsaved changes stay until the page reloads. */
export function closeEditor() {
  root.hidden = true;
  document.body.classList.remove("editing");
  keep(null);
  const b = document.getElementById("editBtn");
  b?.setAttribute("aria-pressed", "false");
  b?.focus();
}

/** Wires the Edit button when the server lets this page edit atlas.yaml, and reopens the drawer after a save. */
export function wireEditor(server: ServerState) {
  S.server = server;
  root = document.getElementById("editor")!;
  const btn = document.getElementById("editBtn") as HTMLButtonElement;
  const projects = document.getElementById("projectsBtn") as HTMLButtonElement;
  if (server.setup) {
    projects.hidden = false;
    projects.addEventListener("click", () => {
      if (dirty() && !confirm("You have unsaved changes. Leave them?")) return;
      closeEditor();
      showSetup(server);
    });
  }
  if (!server.writable || !server.project) return;
  btn.hidden = false;
  btn.addEventListener("click", () => (root.hidden ? openEditor() : closeEditor()));
  root.addEventListener("click", onClick);
  root.addEventListener("input", onInput);
  root.addEventListener("change", onInput);
  root.addEventListener("keydown", onKey);
  root.addEventListener("dragstart", onDragStart);
  root.addEventListener("dragover", onDragOver);
  root.addEventListener("drop", onDrop);
  root.addEventListener("dragend", () => {
    dragging = null;
    root
      .querySelectorAll(".drop, .drop-after, .dragging")
      .forEach(x => x.classList.remove("drop", "drop-after", "dragging"));
  });
  addEventListener("beforeunload", e => {
    if (dirty() && !S.busy) e.preventDefault();
  });
  const tab = kept();
  if (tab) {
    S.tab = (["tour", "regions", "project", "extract"].includes(tab) ? tab : "tour") as Tab;
    void openEditor();
  }
}
