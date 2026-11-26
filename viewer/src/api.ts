/**
 * Calls to the studio's API under `/api/`, which `acu serve -c` and
 * `acu studio` answer. A static site built with `acu build` has no API;
 * {@link loadState} then returns null and the viewer shows no editor.
 * docs/studio.md lists the calls.
 *
 * @packageDocumentation
 */

/** A job of the server: an extraction, the opening of a project, or a clone. */
export interface Job {
  /** The job's number, from 1. */
  id: number;
  /** What the job does: extract, open or clone. */
  kind: string;
  /** Whether the job runs, finished, or failed. */
  state: "running" | "done" | "failed";
  /** The lines of progress so far. */
  log: string[];
  /** Why the job failed. */
  error?: string | null;
  /** The folder that the job opened or cloned. */
  path?: string | null;
}

/** What GET /api/state reports. */
export interface ServerState {
  /** The command that runs the server. */
  mode: "serve" | "studio";
  /** True when a map shows. */
  map: boolean;
  /** True when the editor may change atlas.yaml. */
  writable: boolean;
  /** Why the editor may not change atlas.yaml. */
  why?: string | null;
  /** The open project: its atlas.yaml, its folder and its name. */
  project?: { config: string; dir: string; name?: string } | null;
  /** The last job. */
  job?: Job | null;
  /** True when the server offers the setup screen (acu studio). */
  setup: boolean;
  /** The folder that the folder picker shows. */
  browseRoot?: string | null;
  /** The folder that receives clones. */
  workspace?: string | null;
}

/** The answer of one call: whether it worked, its status, and its JSON body. */
export interface Answer<T = any> {
  /** True for a status from 200 to 299. */
  ok: boolean;
  /** The HTTP status, or 0 when the server does not answer. */
  status: number;
  /** The JSON body; on failure it holds `error`. */
  data: T;
}

/** Reads the server's state, or returns null when no API answers, as on a static site that lacks the page mark. */
export async function loadState(): Promise<ServerState | null> {
  // acu serve and acu studio mark the page; a static site has no API to ask.
  if (!document.querySelector('meta[name="acu-api"]')) return null;
  try {
    const res = await fetch("api/state", { headers: { Accept: "application/json" } });
    if (!res.ok || !(res.headers.get("content-type") ?? "").includes("application/json")) return null;
    return (await res.json()) as ServerState;
  } catch {
    return null;
  }
}

/** Sends one call. A changing call carries the header that the API demands of the studio's own page. */
export async function call<T = any>(path: string, body?: unknown): Promise<Answer<T>> {
  const init: RequestInit =
    body === undefined
      ? {}
      : {
          method: "POST",
          headers: { "Content-Type": "application/json", "X-Acu-Studio": "1" },
          body: JSON.stringify(body),
        };
  try {
    const res = await fetch("api/" + path, init);
    let data: any = null;
    try {
      data = await res.json();
    } catch {
      data = { error: `acu: the server answered ${res.status} without JSON` };
    }
    return { ok: res.ok, status: res.status, data };
  } catch {
    return {
      ok: false,
      status: 0,
      data: { error: "acu: the server does not answer; check that acu still runs" } as any,
    };
  }
}

/** Waits until a job ends, and hands each state to `onTick` on the way. */
export async function waitJob(onTick: (j: Job) => void, everyMs = 400): Promise<Job | null> {
  for (;;) {
    const a = await call<{ job: Job | null }>("job");
    const j = a.data?.job ?? null;
    if (!a.ok || !j) return j;
    onTick(j);
    if (j.state !== "running") return j;
    await new Promise(r => setTimeout(r, everyMs));
  }
}
