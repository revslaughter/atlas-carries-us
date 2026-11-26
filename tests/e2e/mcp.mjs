// End-to-end test of acu's MCP server. A scripted client stands in for a
// language model: it speaks JSON-RPC to `acu mcp` over standard input and
// output, calls every tool on the chi and Cobra examples, and checks the
// answers against facts of the code. Then it drafts a tour step through
// `acu serve` over streamable HTTP, opens the served map in Chromium, and
// checks that the step shows the "Machine-drafted" label.
//
//   node tests/e2e/mcp.mjs <acu program> <work folder>
//
// The examples need their corpus submodules (corpus/go/chi, corpus/go/cobra)
// and the Go adapter. The test copies each atlas.yaml into the work folder
// first, so the drafts never touch examples/. The exit code is 1 when a
// check fails.

import { chromium } from "playwright";
import { spawn } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const acu = resolve(process.argv[2] ?? join(repo, "target/release/acu"));
const work = resolve(process.argv[3] ?? join(repo, "out/mcp"));
const failures = [];
const check = (ok, what, detail = "") => {
  if (!ok) failures.push(`${what}${detail ? "\n    " + detail.replace(/\n/g, "\n    ") : ""}`);
};

/** Copies an example's atlas.yaml into the work folder, with its root made absolute. */
function copyConfig(name, folder = name) {
  const src = join(repo, "examples", name, "atlas.yaml");
  const dir = join(work, folder);
  mkdirSync(dir, { recursive: true });
  const text = readFileSync(src, "utf8").replace(/^(\s*root:\s*)(\S+)/m, (_, k, p) => k + resolve(dirname(src), p));
  writeFileSync(join(dir, "atlas.yaml"), text);
  return join(dir, "atlas.yaml");
}

/** Starts `acu mcp` and returns a client that sends requests and waits for their answers. */
function stdioClient(config, extra = []) {
  const child = spawn(acu, ["mcp", "-c", config, ...extra], { stdio: ["pipe", "pipe", "pipe"] });
  let buf = "";
  let stderr = "";
  const waiting = new Map();
  child.stdout.on("data", d => {
    buf += d;
    let i;
    while ((i = buf.indexOf("\n")) >= 0) {
      const line = buf.slice(0, i).trim();
      buf = buf.slice(i + 1);
      if (!line) continue;
      const msg = JSON.parse(line);
      if (msg.id !== undefined && waiting.has(msg.id)) {
        waiting.get(msg.id)(msg);
        waiting.delete(msg.id);
      }
    }
  });
  child.stderr.on("data", d => (stderr += d));
  let next = 1;
  const send = (method, params) => {
    const id = next++;
    child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
    return new Promise((res, rej) => {
      const t = setTimeout(() => rej(new Error(`no answer to ${method}; acu wrote:\n${stderr}`)), 120000);
      waiting.set(id, m => {
        clearTimeout(t);
        res(m);
      });
    });
  };
  const notify = (method, params) => child.stdin.write(JSON.stringify({ jsonrpc: "2.0", method, params }) + "\n");
  const close = () =>
    new Promise(res => {
      child.on("exit", res);
      child.stdin.end();
    });
  return { send, notify, close, stderr: () => stderr };
}

/** Opens a session the way an MCP client does. */
async function initialize(c) {
  const r = await c.send("initialize", {
    protocolVersion: "2025-06-18",
    capabilities: {},
    clientInfo: { name: "acu-e2e", version: "1" },
  });
  c.notify("notifications/initialized", {});
  return r;
}

/** Calls a tool and returns its text and whether it failed. */
async function tool(c, name, args = {}) {
  const r = await c.send("tools/call", { name, arguments: args });
  if (r.error) return { text: `protocol error: ${JSON.stringify(r.error)}`, error: true };
  return { text: r.result.content.map(x => x.text).join("\n"), error: !!r.result.isError };
}

/** Calls a tool and checks that its text holds each expected piece. */
async function expect(c, label, name, args, pieces, wantError = false) {
  const r = await tool(c, name, args);
  check(r.error === wantError, `${label}: ${name} ${wantError ? "fails" : "succeeds"}`, r.text);
  for (const p of pieces) check(r.text.includes(p), `${label}: ${name} answers with ${JSON.stringify(p)}`, r.text);
  return r.text;
}

/** The facts of each example that the read tools must find. */
const cases = {
  chi: async c => {
    const mux = "go:github.com/go-chi/chi/v5#Mux";
    await expect(c, "chi", "overview", {}, ["Project: chi", "router: Router", "Write tools: on"]);
    await expect(c, "chi", "search", { query: "ServeHTTP" }, [`${mux}.ServeHTTP`]);
    await expect(c, "chi", "search", { query: "middleware", kind: "unit" }, [
      "unit go:pkg:github.com/go-chi/chi/v5/middleware",
    ]);
    await expect(c, "chi", "search", { query: "profiler", kind: "entry_point" }, ["entry_point http:GET /"]);
    await expect(c, "chi", "symbol", { id: "Mux.Get" }, [
      `ID: ${mux}.Get`,
      "Signature: func (mx *Mux) Get(",
      "region Router",
    ]);
    await expect(c, "chi", "symbol", { id: "chi.Router" }, ["interface Router", "Members ("]);
    await expect(c, "chi", "neighbours", { id: "Mux.Get", direction: "out" }, [`calls ${mux}.handle`]);
    await expect(c, "chi", "neighbours", { id: "chi.Router", direction: "in", kind: "implements" }, [
      `implements ${mux}`,
    ]);
    await expect(c, "chi", "path", { from: "Mux.Get", to: "chi.node.InsertRoute" }, ["A chain of", `${mux}.handle`]);
    await expect(c, "chi", "trace", { start: "Mux.routeHTTP", depth: 2 }, ["FindRoute"]);
    await expect(c, "chi", "read_source", { path: "mux.go", start_line: 1, end_line: 3 }, ["    1  package chi"]);
    await expect(c, "chi", "read_source", { path: "Mux.Get" }, ["func (mx *Mux) Get("]);
    await expect(c, "chi", "read_source", { path: "../../../../etc/passwd" }, ["outside the project"], true);
    await expect(c, "chi", "read_source", { path: "/etc/passwd" }, ["relative"], true);
    await expect(c, "chi", "check", {}, ["passes the check"]);
  },
  cobra: async c => {
    const cmd = "go:github.com/spf13/cobra#Command";
    await expect(c, "cobra", "overview", {}, ["Project:", "Tour:"]);
    await expect(c, "cobra", "search", { query: "ExecuteC" }, [`${cmd}.ExecuteC`]);
    await expect(c, "cobra", "symbol", { id: "Command.Execute" }, [
      `ID: ${cmd}.Execute`,
      "Signature: func (c *Command) Execute() error",
    ]);
    await expect(c, "cobra", "neighbours", { id: "Command.Execute", direction: "out", kind: "calls" }, [
      `calls ${cmd}.ExecuteC`,
    ]);
    await expect(c, "cobra", "path", { from: "Command.Execute", to: "Command.execute" }, [
      "A chain of",
      `${cmd}.ExecuteC`,
    ]);
    await expect(c, "cobra", "trace", { start: "Command.ExecuteC", depth: 1 }, [`${cmd}.execute`]);
    await expect(c, "cobra", "read_source", { path: "command.go", start_line: 1, end_line: 40 }, ["package cobra"]);
    await expect(c, "cobra", "check", {}, ["passes the check"]);
  },
};

/** Runs the read tools and the write tools on one example over standard input and output. */
async function stdioRun(name) {
  const config = copyConfig(name);
  const c = stdioClient(config);
  const init = await initialize(c);
  check(init.result?.serverInfo?.name === "acu", `${name}: initialize names acu`, JSON.stringify(init));
  const list = await c.send("tools/list", {});
  const names = (list.result?.tools ?? []).map(t => t.name);
  check(names.length === 11, `${name}: tools/list offers 11 tools`, names.join(", "));
  await cases[name](c);

  // Write tools: a region, the description and a tour step, each with a mark.
  await expect(
    c,
    name,
    "draft_description",
    { text: "A drafted overview.\n\nIt has two paragraphs.", model: "e2e-model" },
    ["the project description", "drafted by e2e-model"],
  );
  const steps = (await tool(c, "overview")).text.match(/^ {2}\d+\. /gm)?.length ?? 0;
  await expect(
    c,
    name,
    "draft_tour_step",
    { title: "A drafted step", text: "This step comes from a test.", view: "overview", model: "e2e-model" },
    [`tour step ${steps + 1}`],
  );
  await expect(
    c,
    name,
    "draft_tour_step",
    { title: "Gone", text: "x", view: "symbol", target: "No.Such.Symbol" },
    ["acu wrote nothing"],
    true,
  );
  await expect(
    c,
    name,
    "draft_region",
    { id: "drafted", name: "Drafted region", paths: ["no/such/folder"], description: "Nothing yet." },
    ["region drafted", "holds no units", "drafted by a model in acu-e2e"],
  );
  await expect(c, name, "overview", {}, [
    "[drafted by e2e-model, not checked]",
    "A drafted step (overview) [drafted by e2e-model",
  ]);
  await c.close();
  const text = readFileSync(config, "utf8");
  check(
    text.includes('descriptionDrafted: { by: e2e-model, at: "'),
    `${name}: atlas.yaml holds the description's mark`,
    text,
  );
  check(text.includes('  - title: "A drafted step"'), `${name}: atlas.yaml holds the step`, text);
  check(text.startsWith("# "), `${name}: atlas.yaml keeps its opening comment`, text.slice(0, 200));

  // --read-only hides and refuses the write tools.
  const ro = stdioClient(config, ["--read-only"]);
  await initialize(ro);
  const roList = await ro.send("tools/list", {});
  check((roList.result?.tools ?? []).length === 8, `${name}: --read-only offers 8 tools`);
  await expect(ro, name, "draft_description", { text: "x" }, ["--read-only"], true);
  await ro.close();
}

/** Reads one JSON-RPC answer from a streamable HTTP response, which may come as an event stream. */
async function httpAnswer(res) {
  const body = await res.text();
  if ((res.headers.get("content-type") ?? "").includes("text/event-stream")) {
    for (const block of body.split(/\n\n/)) {
      const data = block
        .split("\n")
        .filter(l => l.startsWith("data:"))
        .map(l => l.slice(5).trim())
        .join("");
      if (data) {
        const m = JSON.parse(data);
        if (m.id !== undefined) return m;
      }
    }
    throw new Error("no answer in the event stream: " + body);
  }
  return JSON.parse(body);
}

/** Drafts a tour step through `acu serve` and checks that the viewer labels it. */
async function httpRun(name) {
  const config = copyConfig(name, name + "-http");
  const port = 20000 + Math.floor(Math.random() * 20000);
  const addr = `127.0.0.1:${port}`;
  const child = spawn(acu, ["serve", "-c", config, "--addr", addr], { stdio: ["ignore", "ignore", "pipe"] });
  let stderr = "";
  await new Promise((res, rej) => {
    const t = setTimeout(() => rej(new Error("acu serve did not start:\n" + stderr)), 120000);
    child.stderr.on("data", d => {
      stderr += d;
      if (stderr.includes("serving the map")) {
        clearTimeout(t);
        res();
      }
    });
    child.on("exit", () => rej(new Error("acu serve stopped:\n" + stderr)));
  });
  const url = `http://${addr}/mcp`;
  let session = null;
  const post = async (msg, answer = true) => {
    const headers = { "content-type": "application/json", accept: "application/json, text/event-stream" };
    if (session) headers["mcp-session-id"] = session;
    headers["mcp-protocol-version"] = "2025-06-18";
    const res = await fetch(url, { method: "POST", headers, body: JSON.stringify(msg) });
    session = res.headers.get("mcp-session-id") ?? session;
    if (!answer) return res.status;
    return httpAnswer(res);
  };
  const init = await post({
    jsonrpc: "2.0",
    id: 1,
    method: "initialize",
    params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "acu-e2e-http", version: "1" } },
  });
  check(init.result?.serverInfo?.name === "acu", `${name} over HTTP: initialize names acu`, JSON.stringify(init));
  await post({ jsonrpc: "2.0", method: "notifications/initialized", params: {} }, false);

  // A page on another site must not reach the server.
  const foreign = await fetch(url, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      accept: "application/json, text/event-stream",
      origin: "http://evil.example",
    },
    body: JSON.stringify({ jsonrpc: "2.0", id: 9, method: "tools/list", params: {} }),
  });
  check(foreign.status >= 400, `${name} over HTTP: a foreign origin gets refused (status ${foreign.status})`);

  const draft = await post({
    jsonrpc: "2.0",
    id: 2,
    method: "tools/call",
    params: {
      name: "draft_tour_step",
      arguments: {
        title: "Drafted over HTTP",
        text: "Mux.Get registers a GET route.",
        view: "symbol",
        target: "Mux.Get",
        position: 1,
        model: "e2e-http-model",
      },
    },
  });
  const text = draft.result?.content?.map(x => x.text).join("\n") ?? JSON.stringify(draft);
  check(draft.result && !draft.result.isError, `${name} over HTTP: draft_tour_step succeeds`, text);

  // The served map shows the step at once, with the label.
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const errors = [];
  page.on("console", m => m.type() === "error" && errors.push(m.text()));
  page.on("pageerror", e => errors.push(String(e)));
  await page.goto(`http://${addr}/`);
  await page.waitForFunction(() => window.atlas, null, { timeout: 15000 });
  await page.click("#tourBtn");
  await page.waitForTimeout(300);
  const card = (await page.locator("#tour").textContent()) ?? "";
  check(card.includes("Drafted over HTTP"), `${name} over HTTP: the first tour step shows the drafted step`, card);
  check(card.includes("Machine-drafted."), `${name} over HTTP: the drafted step shows the Machine-drafted label`, card);
  check(card.includes("e2e-http-model"), `${name} over HTTP: the label names the model`, card);
  await page.click("#tNext");
  await page.waitForTimeout(300);
  const second = (await page.locator("#tour").textContent()) ?? "";
  check(!second.includes("Machine-drafted."), `${name} over HTTP: a step that a person wrote shows no label`, second);
  check(errors.length === 0, `${name} over HTTP: the page logs no errors`, errors.join("\n"));
  await browser.close();
  child.kill("SIGINT");
}

mkdirSync(work, { recursive: true });
for (const name of ["chi", "cobra"]) {
  await stdioRun(name);
  console.log(`mcp: ${name} over standard input and output: done`);
}
await httpRun("chi");
console.log("mcp: chi over streamable HTTP and the viewer: done");
if (failures.length) {
  console.error("mcp test failed:\n  " + failures.join("\n  "));
  process.exit(1);
}
console.log("mcp test passed");
