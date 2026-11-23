// Tests of the adapter on its invented fixtures: the golden files, the
// schema, and the named facts that the viewer depends on. The first fixture
// holds TypeScript only; the second, testdata/svelte, holds a SvelteKit app.
// Run "npm run golden -w adapters/typescript" after a deliberate change, and
// read the diff.

import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import ajvModule from "ajv/dist/2020.js";

import { run } from "../src/extract.js";
import type { Document } from "../src/schema.js";

const here = (p: string) => fileURLToPath(new URL(p, import.meta.url));
const fixture = here("../../testdata/fixture");
const golden = here("../../testdata/golden/fixture.json");
const svelteFixture = here("../../testdata/svelte");
const svelteGolden = here("../../testdata/golden/svelte.json");
const schemaFile = here("../../../../schema/atlas.schema.json");

function runFixture(): { doc: Document; text: string } {
  const doc = run({
    protocol: 1,
    root: fixture,
    exclude: ["**/generated/**"],
    entryPoints: [
      { plugin: "express", options: { prefixes: [{ inFunc: "routes", prefix: "/reports" }] } },
      { plugin: "hono" },
    ],
  });
  doc.meta.generated = "2000-01-01T00:00:00Z";
  return { doc, text: JSON.stringify(doc, null, 1) + "\n" };
}

/** Maps the SvelteKit fixture with the sveltekit plugin, at a fixed time. */
function runSvelte(): { doc: Document; text: string } {
  const doc = run({ protocol: 1, root: svelteFixture, entryPoints: [{ plugin: "sveltekit" }] });
  doc.meta.generated = "2000-01-01T00:00:00Z";
  return { doc, text: JSON.stringify(doc, null, 1) + "\n" };
}

/** Compares an output with its golden file, or rewrites the file when ACU_UPDATE_GOLDEN is set. */
function compareGolden(file: string, text: string) {
  if (process.env.ACU_UPDATE_GOLDEN) {
    writeFileSync(file, text);
    return;
  }
  let want: string;
  try {
    want = readFileSync(file, "utf8");
  } catch {
    assert.fail(`read ${file}; run npm run golden -w adapters/typescript to create it`);
  }
  if (text === want) return;
  const g = text.split("\n");
  const w = want.split("\n");
  const i = g.findIndex((line, n) => line !== w[n]);
  assert.fail(
    `the output differs from ${file} at line ${i + 1}:\n got: ${g[i]}\nwant: ${w[i]}\n(run npm run golden -w adapters/typescript after a deliberate change)`,
  );
}

/** Checks an output against schema/atlas.schema.json. */
function validateSchema(text: string) {
  const Ajv2020 = ajvModule.default;
  const ajv = new Ajv2020({ allErrors: true, strict: false, validateFormats: false });
  const validate = ajv.compile(JSON.parse(readFileSync(schemaFile, "utf8")));
  const ok = validate(JSON.parse(text));
  assert.ok(ok, "the output breaks the schema:\n" + JSON.stringify(validate.errors, null, 1));
}

const { doc, text } = runFixture();
const p = "ts:lending-web/src/";

test("the output matches the golden file", () => {
  compareGolden(golden, text);
});

test("two runs write the same bytes", () => {
  assert.equal(runFixture().text, text);
});

test("the output passes schema/atlas.schema.json", () => {
  validateSchema(text);
});

test("the edges that the viewer draws exist", () => {
  const has = (from: string, to: string, kind: string) =>
    doc.edges.some(e => e.from === from && e.to === to && e.kind === kind);
  const want: [string, string, string][] = [
    [p + "client/api#HttpClient", p + "client/api#LendingApi", "implements"],
    [p + "client/api#HttpClient", p + "client/api#BaseClient", "extends"],
    [p + "client/cache#CachingClient", p + "client/api#HttpClient", "extends"],
    [p + "client/cache#CachingClient.getBook", p + "client/api#HttpClient.getBook", "overrides"],
    [p + "client/api#HttpClient.describe", p + "client/api#BaseClient.describe", "overrides"],
    [p + "client/api#HttpClient.borrow", p + "domain/loan#mayBorrow", "calls"],
    [p + "client/api#HttpClient.borrow", p + "client/api#HttpClient.loans", "calls"],
    [p + "client/api#HttpClient.constructor", p + "client/api#BaseClient.constructor", "calls"],
    [p + "client/cache#CachingClient.getBook", p + "client/api#HttpClient.getBook", "calls"],
    [p + "web/books#GET /books/:id", p + "client/cache#CachingClient.getBook", "calls"],
    [p + "ui/BookCard#BookList", p + "ui/BookCard#BookCard", "calls"],
    [p + "domain/loan#Loan.book", p + "domain/book#Book", "field_type"],
    [p + "client/api#HttpClient.getBook", p + "domain/book#BookID", "param_type"],
    [p + "client/api#HttpClient.getBook", p + "domain/book#Book", "return_type"],
    ["ts:dir:lending-web/src/client", "ts:dir:lending-web/src/domain", "imports"],
  ];
  for (const [from, to, kind] of want) assert.ok(has(from, to, kind), `missing edge ${from} -${kind}-> ${to}`);
  const calls = doc.edges.filter(e => e.kind === "calls" && e.from === p + "client/api#HttpClient.borrow");
  assert.ok(
    calls.every(e => e.line && e.line > 0),
    "every call edge carries the line of its first call",
  );
});

test("the excluded folder and the test files stay out of the map", () => {
  assert.ok(!doc.containers.some(c => c.path?.includes("generated")), "the generated folder appears");
  assert.ok(!doc.containers.some(c => /\.(test|spec)\.ts$/.test(c.path ?? "")), "a test file appears as a container");
  assert.ok(!doc.containers.some(c => c.path?.startsWith("types")), "a declaration file appears");
});

test("the plugins find every route, with its handler", () => {
  const eps = new Map((doc.entryPoints ?? []).map(e => [e.id, e]));
  const want: Record<string, string> = {
    "http:GET /admin/books/:id": p + "admin/handlers#showBook",
    "http:GET /admin/books": p + "admin/handlers#listBooks",
    "http:POST /admin/books": p + "admin/handlers#addBook",
    "http:GET /reports/loans": p + "admin/handlers#loanReport",
    "http:GET /health": p + "web/app#GET /health",
    "http:GET /version": p + "web/app#version",
    "http:POST /version": p + "web/app#version",
    "http:GET /books/:id": p + "web/books#GET /books/:id",
    "http:POST /books/:id/loans": p + "web/books#borrowBook",
    "http:GET /api/loans/:member": p + "web/app#GET /api/loans/:member",
  };
  for (const [id, handler] of Object.entries(want)) {
    assert.ok(eps.has(id), `missing entry point ${id}`);
    assert.equal(eps.get(id)!.handler, handler, `entry point ${id}`);
  }
  const status = eps.get("http:GET /status");
  assert.ok(
    status?.inline && status.handler === p + "admin/server#server",
    "the inline GET /status handler names server and counts as inline",
  );
  assert.match(
    eps.get("http:GET /static/*")?.externalHandler ?? "",
    /^serveStatic/,
    "GET /static/* names its outside handler",
  );
});

test("docs resolve links and keep their tags", () => {
  const sym = (id: string) => doc.symbols.find(s => s.id === p + id);
  const may = sym("domain/loan#mayBorrow")?.doc;
  assert.ok(
    may?.returns?.includes("(sym:" + p + "domain/loan#MAX_LOANS)"),
    "the {@link MAX_LOANS} in mayBorrow resolves to its symbol",
  );
  assert.equal(may?.since, "0.2.0");
  assert.ok(may?.deprecated !== undefined, "mayBorrow carries its @deprecated note");
  assert.equal(sym("client/api#LendingApi")?.doc?.format, "tsdoc", "@remarks marks LendingApi's doc as TSDoc");
  assert.equal(sym("client/api#HttpClient")?.doc?.examples?.[0]?.title, "Read one book:");
  assert.equal(sym("client/api#BaseClient.send")?.doc?.throws?.[0]?.type, "HttpError");
  assert.match(
    sym("client/api#BaseClient.base")?.doc?.summary ?? "",
    /address of the Go service/,
    "a parameter property takes its @param text",
  );
  const unit = doc.containers.find(c => c.id === "ts:dir:lending-web/src/domain");
  assert.match(unit?.doc?.summary ?? "", /^Books, members and loans/, "the folder takes the doc of its index file");
  const web = doc.containers.find(c => c.id === "ts:dir:lending-web/src/web");
  assert.match(
    web?.doc?.summary ?? "",
    /^The readers' web app/,
    "the folder takes the doc that @packageDocumentation marks",
  );
});

test("the test index names each test by its titles", () => {
  const uses = doc.tests?.[p + "domain/loan#mayBorrow"] ?? [];
  assert.deepEqual(
    uses.map(u => u.name),
    ["mayBorrow > allows %i loans", "mayBorrow > stops at ${MAX_LOANS}"],
  );
  assert.ok(
    (doc.tests?.[p + "client/api#HttpClient.borrow"] ?? []).some(
      u => u.name === "HttpClient > borrow > refuses a member who holds too many books",
    ),
    "nested describe blocks join their titles",
  );
});

/* The SvelteKit fixture. */

const sv = runSvelte();
const q = "ts:reading-room/src/";
const svSym = (id: string) => sv.doc.symbols.find(s => s.id === q + id);
const svHas = (from: string, to: string, kind: string) =>
  sv.doc.edges.find(e => e.from === q + from && e.to === q + to && e.kind === kind);

/** The line, counting from 1, of the first line of a fixture file that holds a text. */
function lineOf(file: string, needle: string): number {
  const lines = readFileSync(`${svelteFixture}/${file}`, "utf8").split("\n");
  const i = lines.findIndex(l => l.includes(needle));
  assert.ok(i >= 0, `${file} holds "${needle}"`);
  return i + 1;
}

test("the SvelteKit output matches its golden file", () => {
  compareGolden(svelteGolden, sv.text);
});

test("two runs of the SvelteKit fixture write the same bytes", () => {
  assert.equal(runSvelte().text, sv.text);
});

test("the SvelteKit output passes schema/atlas.schema.json", () => {
  validateSchema(sv.text);
});

test("files, containers and IDs name the .svelte file, never the virtual one", () => {
  assert.ok(!sv.text.includes(".acu."), "a virtual file name appears in the output");
  assert.ok(sv.doc.containers.some(c => c.id === "ts:file:reading-room/src/lib/Button.svelte"));
  assert.equal(svSym("lib/Button.svelte#Button")?.location.path, "src/lib/Button.svelte");
});

test("a component counts as a function, with the word component", () => {
  const b = svSym("lib/Button.svelte#Button");
  assert.equal(b?.kind, "function");
  assert.equal(b?.languageKind, "component");
  assert.match(b?.doc?.summary ?? "", /^A button that names its action\./, "the @component comment gives its doc");
  assert.equal(svSym("lib/Button.svelte#Button.badge")?.languageKind, "snippet");
  assert.equal(svSym("lib/Legacy.svelte#Legacy.wave")?.kind, "event");
  assert.equal(
    svSym("lib/Legacy.svelte#Legacy.DEFAULT_NAME")?.exported,
    true,
    "a module script export counts as exported",
  );
  assert.equal(svSym("lib/Button.svelte#Button.press")?.exported, false, "an instance script function stays inside");
});

test("props become the component's parameters, with their docs and defaults", () => {
  const b = svSym("lib/Button.svelte#Button");
  assert.deepEqual(
    b?.signature?.params.map(x => [x.name, x.type, x.optional ?? false, x.default ?? ""]),
    [
      ["label", "string", false, ""],
      ["primary", "boolean", true, "false"],
      ["onclick", "(event: MouseEvent) => void", true, ""],
      ["children", "Snippet", true, ""],
    ],
  );
  assert.equal(b?.doc?.params?.label, "The text on the button.");
  const legacy = svSym("lib/Legacy.svelte#Legacy");
  assert.deepEqual(
    legacy?.signature?.params.map(x => [x.name, x.type, x.default ?? ""]),
    [
      ["name", "string", "DEFAULT_NAME"],
      ["loud", "boolean", "false"],
    ],
    "export let gives the props of a legacy component",
  );
});

test("a rune's variable shows its own type", () => {
  assert.equal(svSym("lib/Button.svelte#Button.presses")?.type?.text, "number");
  assert.equal(svSym("lib/Counter.svelte#Counter.doubled")?.type?.text, "number");
  assert.equal(svSym("lib/Counter.svelte#Counter.over")?.type?.text, "boolean");
});

test("locations in a component take the lines of the .svelte file", () => {
  const press = svSym("lib/Button.svelte#Button.press");
  assert.equal(press?.location.line, lineOf("src/lib/Button.svelte", "function press("));
  const badge = svSym("lib/Button.svelte#Button.badge");
  assert.equal(badge?.location.line, lineOf("src/lib/Button.svelte", "{#snippet badge"));
  assert.equal(badge?.location.endLine, lineOf("src/lib/Button.svelte", "{/snippet}"));
  const render = svHas("lib/Button.svelte#Button", "lib/Button.svelte#Button.badge", "calls");
  assert.equal(render?.line, lineOf("src/lib/Button.svelte", "{@render badge("), "a call in the markup");
  const wave = svSym("lib/Legacy.svelte#Legacy.wave");
  assert.equal(wave?.location.line, lineOf("src/lib/Legacy.svelte", 'dispatch("wave"'));
});

test("a component that renders another one calls it, from the line of its tag", () => {
  const e = svHas("routes/blog/[slug]/+page.svelte#Page", "lib/Button.svelte#Button", "calls");
  assert.ok(e, "missing edge +page.svelte -calls-> Button");
  assert.equal(e.confidence, "exact");
  assert.equal(e.line, lineOf("src/routes/blog/[slug]/+page.svelte", '<Button label="Like"'));
  assert.ok(svHas("lib/Counter.svelte#Counter", "lib/Button.svelte#Button", "calls"), "Counter renders Button");
  assert.ok(
    svHas("lib/Button.svelte#Button.badge", "lib/format#shout", "calls"),
    "a call inside a snippet counts for the snippet",
  );
});

test("a test records against the component that it renders", () => {
  assert.deepEqual(
    (sv.doc.tests?.[q + "lib/Button.svelte#Button"] ?? []).map(u => u.name),
    ["Button > shows its label"],
  );
});

test("the sveltekit plugin finds every route, with its handler", () => {
  const got = Object.fromEntries((sv.doc.entryPoints ?? []).map(e => [e.id, e.handler.slice(q.length)]));
  assert.deepEqual(got, {
    "http:GET /": "routes/+page.svelte#Page",
    "http:GET /about": "routes/(marketing)/about/+page#load",
    "http:GET /pricing": "routes/(marketing)/pricing/+page@.svelte#Page",
    "http:POST /pricing": "routes/(marketing)/pricing/+page.server#POST /pricing",
    "http:GET /api/items": "routes/api/items/+server#GET",
    "http:POST /api/items": "routes/api/items/+server#POST",
    "http:ANY /api/items": "routes/api/items/+server#fallback",
    "http:GET /blog/[slug]": "routes/blog/[slug]/+page.server#load",
    "http:POST /blog/[slug]?/like": "routes/blog/[slug]/+page.server#like",
    "http:POST /blog/[slug]?/comment": "routes/blog/[slug]/+page.server#POST /blog/[slug]?/comment",
  });
  assert.ok((sv.doc.entryPoints ?? []).every(e => e.plugin === "sveltekit"));
  assert.ok(
    svHas("routes/blog/[slug]/+page.server#POST /blog/[slug]?/comment", "lib/format#slugTitle", "calls"),
    "the calls of an inline action count for the action",
  );
});

test("layouts wrap pages through heuristic edges, and honour groups and resets", () => {
  const heuristic = (from: string, to: string) => {
    const e = svHas(from, to, "calls");
    assert.ok(e, `missing edge ${from} -calls-> ${to}`);
    assert.equal(e.confidence, "heuristic", `${from} -calls-> ${to}`);
  };
  const root = "routes/+layout.svelte#Layout";
  const marketing = "routes/(marketing)/+layout.svelte#Layout";
  // A nested page: the blog folder has a layout module but no component.
  heuristic(root, "routes/blog/[slug]/+page.svelte#Page");
  heuristic("routes/blog/[slug]/+page#load", "routes/blog/+layout#load");
  heuristic("routes/blog/[slug]/+page#load", "routes/+layout.server#load");
  heuristic("routes/blog/[slug]/+page.server#load", "routes/+layout.server#load");
  assert.ok(
    !svHas("routes/blog/[slug]/+page.server#load", "routes/blog/+layout#load", "calls"),
    "a server load never reads a universal layout load",
  );
  // A grouped page: the group's layout sits between the root and the page.
  heuristic(root, marketing);
  heuristic(marketing, "routes/(marketing)/about/+page.svelte#Page");
  // A reset page: +page@.svelte skips the group's layout.
  heuristic(root, "routes/(marketing)/pricing/+page@.svelte#Page");
  assert.ok(
    !svHas(marketing, "routes/(marketing)/pricing/+page@.svelte#Page", "calls"),
    "the reset page skips the (marketing) layout",
  );
  assert.equal(
    svHas(root, marketing, "calls")?.line,
    lineOf("src/routes/+layout.svelte", "{@render children()}"),
    "a layout edge starts at the line that renders the child",
  );
});

test("the notes name the limits that components add", () => {
  const notes = sv.doc.meta.adapters[0].notes ?? [];
  assert.ok(notes.some(n => /svelte2tsx 0\.7\.63/.test(n)));
  assert.ok(
    notes.some(n => /extends \$app\/tsconfig/.test(n)),
    "the missing $app/tsconfig adds a note",
  );
  assert.ok(
    notes.some(n => /\.\/\$types/.test(n)),
    "the unresolved ./$types imports add a note",
  );
});
