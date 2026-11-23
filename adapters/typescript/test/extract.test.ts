// Tests of the adapter on its invented fixture: the golden file, the schema,
// and the named facts that the viewer depends on. Run "npm run golden -w
// adapters/typescript" after a deliberate change, and read the diff.

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

const { doc, text } = runFixture();
const p = "ts:lending-web/src/";

test("the output matches the golden file", () => {
  if (process.env.ACU_UPDATE_GOLDEN) {
    writeFileSync(golden, text);
    return;
  }
  let want: string;
  try {
    want = readFileSync(golden, "utf8");
  } catch {
    assert.fail(`read ${golden}; run npm run golden -w adapters/typescript to create it`);
  }
  if (text === want) return;
  const g = text.split("\n");
  const w = want.split("\n");
  const i = g.findIndex((line, n) => line !== w[n]);
  assert.fail(
    `the output differs from ${golden} at line ${i + 1}:\n got: ${g[i]}\nwant: ${w[i]}\n(run npm run golden -w adapters/typescript after a deliberate change)`,
  );
});

test("two runs write the same bytes", () => {
  assert.equal(runFixture().text, text);
});

test("the output passes schema/atlas.schema.json", () => {
  const Ajv2020 = ajvModule.default;
  const ajv = new Ajv2020({ allErrors: true, strict: false, validateFormats: false });
  const validate = ajv.compile(JSON.parse(readFileSync(schemaFile, "utf8")));
  const ok = validate(JSON.parse(text));
  assert.ok(ok, "the output breaks the schema:\n" + JSON.stringify(validate.errors, null, 1));
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
