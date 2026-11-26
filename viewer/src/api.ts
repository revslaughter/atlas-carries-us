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
  id: number;
  kind: string;
  state: "running" | "done" | "failed";
  log: string[];
  error?: string | null;
  path?: string | null;
}

/** What GET /api/state reports. */
export interface ServerState {
  mode: "serve" | "studio";
  map: boolean;
  writable: boolean;
  why?: string | null;
  project?: { config: string; dir: string; name?: string } | null;
  job?: Job | null;
  setup: boolean;
  browseRoot?: string | null;
  workspace?: string | null;
}

/** The answer of one call: whether it worked, its status, and its JSON body. */
export interface Answer<T = any> {
  ok: boolean;
  status: number;
  data: T;
}

/** Reads the server's state, or returns null when no API answers (a static site). */
export async function loadState(): Promise<ServerState | null> {
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
