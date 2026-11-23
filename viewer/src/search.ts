// Search over regions, units, entry points, items and members. Names carry
// the most weight; the first doc sentence shows beside each result.

import { M, go } from "./app";
import { ENTRY_KIND, type State } from "./scene";
import { esc, inlinePlain, summary } from "./text";

/** One thing that the search can find. */
interface Entry {
  /** The kind word shown before the name, such as "region" or the item's own word. */
  k: string;
  /** The name shown, such as "store.Store" or "Store.Get". */
  n: string;
  /** The text shown after the name: the first doc sentence, or the handler of a route. */
  p: string;
  /** The view that a pick opens. */
  st: State;
  /** The name in lower case, which the query matches. */
  l: string;
  /** The ranking group: "region", "unit", "route", "item" or "member". */
  kind: string;
}
/** Every searchable thing, built once at start-up. */
let IDX: Entry[] = [];
/** The highlighted result, or -1. */
let cur = -1,
  /** The results on show. */
  shown: Entry[] = [];

/** The search box. */
const q = document.getElementById("q") as HTMLInputElement;
/** The list of results under the search box. */
const results = document.getElementById("results")!;

/** Builds the index: regions, units, entry points, items and members, each with the view it opens. */
export function buildSearch() {
  const add = (k: string, n: string, p: string, st: State, kind: string) =>
    IDX.push({ k, n, p, st, l: n.toLowerCase(), kind });
  for (const r of M.regions)
    add(
      "region",
      r.name,
      inlinePlain(r.description).split(/(?<=[.!?])\s/)[0] ?? "",
      { v: "overview", region: r.id },
      "region",
    );
  for (const u of M.unitList) add(u.c.languageKind, u.name, summary(u.c.doc), { v: "unit", id: u.id }, "unit");
  for (const r of M.routes)
    add(
      ENTRY_KIND[r.kind]?.toLowerCase() ?? r.kind,
      r.name,
      r.member && !r.inline ? M.items.get(r.item)!.name + "." + r.member : "",
      { v: "trace", entry: r.id },
      "entry",
    );
  for (const it of M.items.values()) {
    const short = (M.units.get(it.unit)?.name ?? "").split("/").pop();
    if (it.kind !== "values")
      add(it.languageKind, short + "." + it.name, summary(it.doc), { v: "symbol", id: it.id }, "item");
    for (const m of it.members) {
      const n = it.kind === "values" ? short + "." + m.name : short + "." + it.name + "." + m.name;
      add(
        m.languageKind,
        n,
        summary(m.doc),
        it.kind === "values" ? { v: "symbol", id: it.id, member: m.name } : { v: "symbol", id: it.id, member: m.name },
        "member",
      );
    }
  }
}

/**
 * Finds the 40 best matches for a query. A name whose last part equals the
 * query ranks first, then one whose last part starts with it, then any name
 * that holds it; regions and units rank a little higher, members a little
 * lower.
 */
export function search(t: string): Entry[] {
  t = t.trim().toLowerCase();
  if (!t) return [];
  const sc: [number, Entry][] = [];
  for (const it of IDX) {
    const i = it.l.indexOf(t);
    if (i < 0) continue;
    const last = it.l.split(/[./ ]/).pop()!;
    let s = 100 - Math.min(i, 50) - it.l.length * 0.2;
    if (last === t) s += 80;
    else if (last.startsWith(t)) s += 40;
    if (it.kind === "unit" || it.kind === "region") s += 10;
    if (it.kind === "member") s -= 8;
    sc.push([s, it]);
  }
  return sc
    .sort((a, b) => b[0] - a[0])
    .slice(0, 40)
    .map(x => x[1]);
}

/** Draws the results on show and keeps the highlighted one in view and announced. */
function render() {
  results.innerHTML = shown
    .map(
      (it, i) =>
        `<button role="option" id="opt${i}" aria-selected="${i === cur}" data-i="${i}"><span class="k">${esc(it.k)}</span><span class="n">${esc(it.n)}<span class="p">${esc(it.p || "")}</span></span></button>`,
    )
    .join("");
  results.hidden = !shown.length;
  q.setAttribute("aria-expanded", String(!results.hidden));
  if (cur >= 0) {
    document.getElementById("opt" + cur)?.scrollIntoView({ block: "nearest" });
    q.setAttribute("aria-activedescendant", "opt" + cur);
  }
}

/** Opens a result's view and clears the search. */
function pick(it: Entry) {
  results.hidden = true;
  q.value = "";
  q.blur();
  go(it.st);
}

/** Connects the search box: typing searches, arrows move, Enter opens, Escape closes. */
export function wireSearch() {
  q.addEventListener("input", () => {
    shown = search(q.value);
    cur = shown.length ? 0 : -1;
    render();
  });
  q.addEventListener("keydown", e => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      cur = Math.min(shown.length - 1, cur + 1);
      render();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      cur = Math.max(0, cur - 1);
      render();
    } else if (e.key === "Enter" && shown[cur]) pick(shown[cur]);
    else if (e.key === "Escape") {
      results.hidden = true;
      q.blur();
    }
  });
  results.addEventListener("mousedown", e => {
    const b = (e.target as HTMLElement).closest("button");
    if (b) {
      e.preventDefault();
      pick(shown[Number(b.dataset.i)]);
    }
  });
  q.addEventListener("blur", () => setTimeout(() => (results.hidden = true), 120));
}
