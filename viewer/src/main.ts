/**
 * The viewer: one static page that draws atlas.json as a map, with a docs
 * panel beside it. The acu command embeds the built viewer and serves it
 * beside the data.
 *
 * The map answers the questions that readers of code ask most: "how does
 * control get from here to the data?" and "what does this do, and why?". It
 * does so in four views on one canvas:
 *
 * 1. The overview: regions of units, and the uses between regions.
 * 2. A unit focus: one unit, what uses it on the left, what it uses on the right.
 * 3. An item focus: one item and its direct relations; each line ends at the exact member.
 * 4. A trace: the calls from an entry point or a callable, in order, through
 *    each interface to the code that can run.
 *
 * This file starts the page: {@link boot} loads the fonts and the data and
 * builds the {@link Model}. app.ts holds the canvas, the camera and the
 * history; scene.ts lays out each view; panel.ts writes the docs panel;
 * tour.ts runs the guided tour. The research behind the design sits in
 * [docs/research/report.md](https://github.com/revslaughter/atlas-carries-us/blob/main/docs/research/report.md).
 *
 * @packageDocumentation
 */

import { back, debug, fwd, go, start } from "./app";
import { Model } from "./model";
import * as panel from "./panel";
import { buildSearch, wireSearch } from "./search";
import { setTextEnv } from "./text";
import { tour } from "./tour";
import type { AtlasDoc } from "./types";

/** The "Loading the map" message, which also carries the reason when the data does not load. */
const loading = document.getElementById("loading")!;

/** Applies a theme ("light", "dark", or null to follow the system) and labels the theme button with the other one. */
function setTheme(t: string | null) {
  if (t === "light" || t === "dark") document.documentElement.dataset.theme = t;
  else delete document.documentElement.dataset.theme;
  const dark = t
    ? t === "dark"
    : matchMedia("(prefers-color-scheme: dark)").matches || !matchMedia("(prefers-color-scheme: light)").matches;
  const b = document.getElementById("themeBtn")!;
  b.textContent = dark ? "Light theme" : "Dark theme";
  b.setAttribute("aria-label", dark ? "Switch to the light theme" : "Switch to the dark theme");
}

/** Returns the theme that the reader chose last time, or null; a private window may refuse the storage. */
function storedTheme(): string | null {
  try {
    return localStorage.getItem("acu-theme");
  } catch {
    return null;
  }
}

/**
 * Starts the viewer: sets the theme, waits for the fonts (at most 2.5
 * seconds), loads data/atlas.json, checks its version, builds the
 * {@link Model}, wires the header's buttons, and opens the view that the
 * address names.
 */
async function boot() {
  setTheme(storedTheme());
  document.getElementById("themeBtn")!.addEventListener("click", () => {
    const now =
      document.documentElement.dataset.theme ??
      (matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark");
    const next = now === "dark" ? "light" : "dark";
    setTheme(next);
    try {
      localStorage.setItem("acu-theme", next);
    } catch {
      /* private window */
    }
  });
  try {
    await Promise.race([
      Promise.all(
        [
          '700 20px "JetBrains Mono"',
          '600 12px "JetBrains Mono"',
          '400 12px "JetBrains Mono"',
          '400 13px "Inter"',
          '600 13px "Inter"',
          '700 13px "Inter"',
        ].map(f => document.fonts.load(f)),
      ),
      new Promise(r => setTimeout(r, 2500)),
    ]);
  } catch {
    /* fonts fall back to the system */
  }
  let doc: AtlasDoc;
  try {
    const res = await fetch("data/atlas.json");
    if (!res.ok) throw new Error(String(res.status));
    doc = await res.json();
  } catch {
    loading.textContent =
      "You find no map data here. The file data/atlas.json did not load. Run acu build, then reload this page.";
    return;
  }
  if (doc.version !== 1) {
    loading.textContent = `This viewer reads data version 1, and data/atlas.json holds version ${doc.version}. Rebuild the site with a matching acu.`;
    return;
  }
  const name = doc.project?.name ?? "Code map";
  document.title = name + " · Atlas Carries Us";
  document.getElementById("title")!.textContent = name;
  const M = new Model(doc);
  setTextEnv({
    project: doc.project ?? { name },
    commit: doc.meta.commit ?? "HEAD",
    knows: id => M.owner.has(id) || M.units.has(id) || M.containers.has(id),
  });
  document.getElementById("back")!.addEventListener("click", back);
  document.getElementById("fwd")!.addEventListener("click", fwd);
  document.getElementById("home")!.addEventListener("click", () => go({ v: "overview" }));
  const tourBtn = document.getElementById("tourBtn") as HTMLButtonElement;
  if (doc.tour?.length) tourBtn.addEventListener("click", () => tour.start());
  else tourBtn.hidden = true;
  document.getElementById("routesBtn")!.addEventListener("click", () => panel.routesIndex());
  document.getElementById("aboutBtn")!.addEventListener("click", () => panel.howTo());
  document.getElementById("legendBtn")!.addEventListener("click", e => {
    const l = document.getElementById("legend")!;
    l.hidden = !l.hidden;
    (e.currentTarget as HTMLElement).setAttribute("aria-pressed", String(!l.hidden));
  });
  loading.remove();
  start(M);
  buildSearch();
  wireSearch();
  (window as any).atlas = { model: M, go, tour, debug };
}

boot();
