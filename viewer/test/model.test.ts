import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { Model } from "../src/model";

const doc = JSON.parse(
  readFileSync(new URL("../../adapters/go/testdata/golden/fixture.json", import.meta.url), "utf8"),
);
const M = new Model(doc);
const p = "go:example.com/lending/";

test("units come from containers marked unit", () => {
  assert.equal(M.unitList.length, 7);
  assert.ok(M.units.has("go:pkg:example.com/lending/internal/store"));
});

test("members belong to their item, and edges roll up to items", () => {
  assert.equal(M.owner.get(p + "internal/service#Lending.Borrow"), p + "internal/service#Lending");
  const calls = M.mc.get(p + "internal/service#Lending\0Borrow") ?? [];
  const order = calls.map(c => c.b + "." + c.bm);
  assert.equal(order[0], p + "internal/store#Books.Book", "calls sort in source order");
  assert.ok(order.includes(p + "internal/store#Loans.ForMember"));
});

test("constants and variables group per file", () => {
  const g = M.items.get("values:go:file:internal/domain/status.go");
  assert.ok(g, "a value group for status.go");
  assert.deepEqual(
    g!.members.map(m => m.name),
    ["OnShelf", "Borrowed", "Lost"],
  );
  assert.equal(g!.languageKind, "constants");
});

test("entry points find their handler item and member", () => {
  const r = M.routeById.get("http:GET /api/books/{id}")!;
  assert.equal(r.item, p + "internal/httpapi#Handler");
  assert.equal(r.member, "getBook");
});

test("interfaces list their implementations", () => {
  assert.deepEqual((M.implBy.get(p + "internal/httpapi#Lender") ?? []).sort(), [p + "internal/service#Lending"]);
});

test("tests roll up from members to items", () => {
  assert.ok((M.tests.get(p + "internal/service#Lending") ?? []).some(t => t.name === "TestBorrow"));
});
