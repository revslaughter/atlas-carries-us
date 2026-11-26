/**
 * The setup screen of `acu studio`: pick a folder on this computer, or clone
 * a repository; read the atlas.yaml that acu proposes, change it, and write
 * it; confirm that you trust the code; and extract the map. The page
 * reloads when the map is ready. docs/studio.md describes each step.
 *
 * @packageDocumentation
 */

import { call, waitJob, type ServerState } from "./api";
import { esc } from "./text";

/** A folder as POST /api/folders lists it. */
interface Folder {
  /** The folder's name. */
  name: string;
  /** The folder's absolute path. */
  path: string;
  /** True when the folder holds an atlas.yaml. */
  config: boolean;
  /** True when the folder holds a git repository. */
  git: boolean;
}

/** Holds the setup screen's state: the folder on show, the chosen folder and its proposal, the fields, and the job's progress. */
const S = {
  server: null as ServerState | null,
  /** The folder on show in the picker. */
  at: null as { path: string; parent: string | null; config: boolean; folders: Folder[]; more: boolean } | null,
  /** The proposal for the chosen folder. */
  chosen: null as {
    path: string;
    exists: boolean;
    config: string;
    languages: { adapter: string; root: string }[];
  } | null,
  /** The text of the config field. */
  text: "",
  /** The repository address field. */
  url: "",
  trust: false,
  busy: false,
  log: [] as string[],
  msg: null as { text: string; bad?: boolean } | null,
};

/** The element that holds the setup screen. */
let root: HTMLElement;
/** True after the handlers sit on the screen. */
let wired = false;

/** Lists the folders inside one folder, or inside the browse root. */
async function list(path?: string) {
  const a = await call("folders", path ? { path } : {});
  if (!a.ok) {
    S.msg = { text: a.data.error, bad: true };
    return;
  }
  S.at = a.data;
  S.msg = null;
}

/** Reads the proposal for a folder: its atlas.yaml, or a new one. */
async function choose(path: string) {
  const a = await call("propose", { path });
  if (!a.ok) {
    S.msg = { text: a.data.error, bad: true };
    return;
  }
  S.chosen = a.data;
  S.text = a.data.config;
  S.msg = null;
}

/** Draws the picker's list of folders. */
function pickerHTML(): string {
  const at = S.at;
  if (!at) return "<p>Reading the folders…</p>";
  const items = at.folders
    .map(
      f =>
        `<li><button class="su-folder" data-act="enter" data-path="${esc(f.path)}"><span>${esc(f.name)}</span>${
          f.config ? '<span class="ed-tag">atlas.yaml</span>' : ""
        }${f.git ? '<span class="ed-tag">git</span>' : ""}</button></li>`,
    )
    .join("");
  return `<p class="su-path"><code>${esc(at.path)}</code></p>
    <div class="ed-row">
      <button class="btn" data-act="enter" data-path="${esc(at.parent ?? "")}" ${at.parent ? "" : "disabled"}>Up one folder</button>
      <button class="btn primary" data-act="choose" data-path="${esc(at.path)}">Use this folder</button>
    </div>
    ${items ? `<ul class="su-folders">${items}</ul>` : "<p>This folder holds no other folders.</p>"}
    ${at.more ? '<p class="ed-hint">The list shows the first 500 folders.</p>' : ""}`;
}

/** Draws the proposal and the button that writes it and extracts. */
function chosenHTML(): string {
  const c = S.chosen!;
  const langs = c.languages.length
    ? c.languages
        .map(
          l =>
            `<code>${esc(l.adapter)}</code> in ${l.root === "." ? "the folder itself" : `<code>${esc(l.root)}</code>`}`,
        )
        .join(", ")
    : "none that acu knows";
  const what = c.exists
    ? `<p>This folder holds an atlas.yaml. acu opens it as it stands.</p><pre class="ed-log">${esc(c.config)}</pre>`
    : `<p>acu found these languages: ${langs}. Check the config below and change it as you need. <a href="https://github.com/revslaughter/atlas-carries-us/blob/main/docs/config.md" target="_blank" rel="noopener">The config guide</a> explains each field.</p>
       <label>atlas.yaml<textarea id="suText" rows="16" spellcheck="false">${esc(S.text)}</textarea></label>`;
  return `<h3>2. ${c.exists ? "Open" : "Write"} the config</h3>
    <p class="su-path"><code>${esc(c.path)}</code></p>
    ${what}
    <h3>3. Extract the map</h3>
    <label class="ed-check"><input type="checkbox" id="suTrust" ${S.trust ? "checked" : ""} /> I trust this code. Extraction runs the project's own build tools, and they can run any program.</label>
    <div class="ed-row">
      <button class="btn primary" data-act="go" ${S.trust && !S.busy ? "" : "disabled"}>${c.exists ? "Open and extract" : "Write and extract"}</button>
      <button class="btn" data-act="back" ${S.busy ? "disabled" : ""}>Pick another folder</button>
    </div>`;
}

/** Draws the screen. */
function render(focus?: string) {
  const back = S.server?.map ? '<button class="btn" data-act="map">Back to the map</button>' : "";
  const pick = S.chosen
    ? chosenHTML()
    : `<h3>1. Pick a folder</h3>${pickerHTML()}
       <h3>Or clone a repository</h3>
       <p class="ed-hint">acu clones over HTTPS into <code>${esc(S.server?.workspace ?? "")}</code>, one commit deep.</p>
       <div class="su-clone"><label class="sr" for="suUrl">Repository address</label>
         <input id="suUrl" type="url" placeholder="https://github.com/owner/name" value="${esc(S.url)}" spellcheck="false" />
         <button class="btn primary" data-act="clone" ${S.busy ? "disabled" : ""}>Clone</button></div>`;
  root.innerHTML = `<div class="su-box">
    <div class="ed-top"><h2>Set up a map</h2>${back}</div>
    <p class="ed-lead">Pick the code to map. acu proposes an atlas.yaml, writes it into that folder, and runs the adapters. Then you edit the regions and the tour on the map.</p>
    ${pick}
    ${S.msg ? `<p class="ed-msg ${S.msg.bad ? "bad" : ""}" tabindex="-1" role="${S.msg.bad ? "alert" : "status"}">${S.msg.bad ? "<b>Problem.</b> " : ""}${esc(S.msg.text)}</p>` : ""}
    ${S.log.length ? `<pre class="ed-log">${esc(S.log.join("\n"))}</pre>` : ""}
  </div>`;
  if (focus) (root.querySelector(focus) as HTMLElement | null)?.focus();
}

/** Runs a job and shows its progress; returns the finished job, or null after a failure that it reports. */
async function runJob(path: string, body: object) {
  S.busy = true;
  S.log = [];
  S.msg = null;
  render();
  const a = await call(path, body);
  if (!a.ok) {
    S.busy = false;
    S.msg = { text: a.data.error, bad: true };
    render(".ed-msg");
    return null;
  }
  const j = await waitJob(job => {
    S.log = job.log;
    const pre = root.querySelector(".ed-log");
    if (pre) pre.textContent = S.log.join("\n");
    else render();
  });
  S.busy = false;
  if (j?.state !== "done") {
    S.msg = { text: j?.error ?? "The job failed.", bad: true };
    render(".ed-msg");
    return null;
  }
  return j;
}

/** Handles a click on any button of the screen. */
async function onClick(e: Event) {
  const b = (e.target as HTMLElement).closest<HTMLElement>("[data-act]");
  if (!b || (b as HTMLButtonElement).disabled) return;
  switch (b.dataset.act) {
    case "enter":
      await list(b.dataset.path || undefined);
      render(".su-folder, [data-act='choose']");
      return;
    case "choose":
      await choose(b.dataset.path!);
      render(S.chosen ? "#suText, #suTrust" : ".ed-msg");
      return;
    case "back":
      S.chosen = null;
      S.log = [];
      render("[data-act='choose']");
      return;
    case "map":
      hideSetup();
      return;
    case "clone": {
      const j = await runJob("clone", { url: S.url });
      if (j?.path) {
        await list(undefined);
        await choose(j.path);
        render("#suText, #suTrust");
      }
      return;
    }
    case "go": {
      const c = S.chosen!;
      const body = c.exists ? { path: c.path, trust: true } : { path: c.path, config: S.text, trust: true };
      const j = await runJob("open", body);
      if (j) location.reload();
      return;
    }
  }
}

/** Handles typing in the fields. */
function onInput(e: Event) {
  const t = e.target as HTMLInputElement;
  if (t.id === "suText") S.text = t.value;
  else if (t.id === "suUrl") S.url = t.value;
  else if (t.id === "suTrust") {
    S.trust = t.checked;
    const go = root.querySelector<HTMLButtonElement>("[data-act='go']");
    if (go) go.disabled = !S.trust || S.busy;
  }
}

/** Takes the page behind the setup screen out of the tab order and the accessibility tree, or puts it back. */
function setInert(on: boolean) {
  for (const id of ["stage", "panel", "status"]) document.getElementById(id)?.toggleAttribute("inert", on);
  document.querySelector("#app > header")?.toggleAttribute("inert", on);
}

/** Shows the setup screen over the page. */
export async function showSetup(server: ServerState) {
  S.server = server;
  root = document.getElementById("setup")!;
  if (!wired) {
    wired = true;
    root.addEventListener("click", onClick);
    root.addEventListener("input", onInput);
    root.addEventListener("change", onInput);
    root.addEventListener("keydown", e => {
      if (e.key === "Escape" && S.server?.map && !S.busy) {
        e.preventDefault();
        hideSetup();
      } else if (e.key === "Enter" && (e.target as HTMLElement).id === "suUrl") {
        e.preventDefault();
        root.querySelector<HTMLButtonElement>("[data-act='clone']")?.click();
      }
    });
  }
  root.hidden = false;
  setInert(true);
  document.getElementById("loading")?.remove();
  if (!S.at) await list(undefined);
  render(".su-folder, [data-act='choose']");
}

/** Hides the setup screen. */
export function hideSetup() {
  root.hidden = true;
  setInert(false);
  document.getElementById("projectsBtn")?.focus();
}
