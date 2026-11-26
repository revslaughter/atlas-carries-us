// Unit tests for the editor's pure parts: views to steps, moves, layout
// rows, region IDs, suggested regions, unit picks and the save checks.
import { test } from "node:test";
import assert from "node:assert/strict";
import * as em from "../src/editmodel";

const memberId = (owner: string, m: string) =>
  owner === "go:shop#Service" && m === "Buy" ? "go:shop#Service.Buy" : undefined;

test("a view state becomes an open by ID", () => {
  assert.deepEqual(em.stateToOpen({ v: "overview" }, memberId), { view: "overview" });
  assert.deepEqual(em.stateToOpen({ v: "overview", focusUnit: "u" }, memberId), { view: "overview" });
  assert.deepEqual(em.stateToOpen({ v: "overview", region: "core" }, memberId), { view: "region", id: "core" });
  assert.deepEqual(em.stateToOpen({ v: "unit", id: "go:pkg:shop" }, memberId), { view: "unit", id: "go:pkg:shop" });
  assert.deepEqual(em.stateToOpen({ v: "symbol", id: "go:shop#Service", member: "Buy" }, memberId), {
    view: "symbol",
    id: "go:shop#Service.Buy",
  });
  assert.deepEqual(em.stateToOpen({ v: "symbol", id: "go:shop#Service", member: "Gone" }, memberId), {
    view: "symbol",
    id: "go:shop#Service",
  });
  assert.deepEqual(em.stateToOpen({ v: "trace", entry: "http:POST /buy" }, memberId), {
    view: "trace",
    entryPoint: "http:POST /buy",
  });
  assert.deepEqual(em.stateToOpen({ v: "trace", sym: "go:shop#Service", member: "Buy" }, memberId), {
    view: "trace",
    symbol: "go:shop#Service.Buy",
  });
});

test("an open by ID becomes a view, and an open by name does not", () => {
  assert.deepEqual(em.openToView({ view: "unit", id: "u" }), { view: "unit", id: "u" });
  assert.equal(em.openToView({ view: "unit", path: "internal/x" }), null);
  assert.equal(em.openToView({ view: "symbol", name: "Mux.Get" }), null);
  assert.deepEqual(em.openToView({ view: "trace", entryPoint: "e" }), { view: "trace", entryPoint: "e" });
  assert.deepEqual(em.openToView({ view: "trace", symbol: "s" }), { view: "trace", symbol: "s" });
});

test("opens read as words", () => {
  assert.equal(em.describeOpen({ view: "overview" }), "the overview");
  assert.equal(em.describeOpen({ view: "symbol", name: "chi.Router" }), "the item chi.Router");
  assert.equal(
    em.describeOpen({ view: "unit", id: "go:pkg:x" }, id => id.split(":").pop()!),
    "the unit x",
  );
  assert.equal(em.describeOpen({ view: "trace", entryPoint: "GET /x" }), "the trace from GET /x");
});

test("move shifts one entry and leaves bad positions alone", () => {
  assert.deepEqual(em.move([1, 2, 3, 4], 0, 2), [2, 3, 1, 4]);
  assert.deepEqual(em.move([1, 2, 3, 4], 3, 0), [4, 1, 2, 3]);
  assert.deepEqual(em.move([1, 2], 0, 5), [1, 2]);
  assert.deepEqual(em.move([1, 2], -1, 0), [1, 2]);
});

test("layout rows follow the row numbers and keep the file's order within a row", () => {
  const rows = em.layoutRows([
    { id: "a", row: 2 },
    { id: "b", row: 1 },
    { id: "c", row: null },
    { id: "d", row: 2, col: 0 },
    { id: "e", row: 0 },
  ]);
  assert.deepEqual(rows, [["b"], ["d", "a"]]);
  const back = em.rowsOf({ rows });
  assert.deepEqual(back.get("a"), { row: 2, col: 1 });
  assert.equal(back.get("c"), undefined);
  assert.equal(em.rowsOf(null).size, 0);
});

test("renamed and deleted regions reach the steps that open them", () => {
  const tour: em.StepCfg[] = [
    { title: "a", text: "x", open: { view: "region", id: "old" } },
    { title: "b", text: "x", open: { view: "unit", id: "old" } },
    { title: "c", text: "x", open: { view: "region", id: "gone" } },
  ];
  const out = em.renameSteps(tour, new Map([["old", "new"]]));
  assert.equal(out[0].open.id, "new");
  assert.equal(out[1].open.id, "old");
  assert.equal(tour[0].open.id, "old");
  assert.deepEqual(em.orphanSteps(out, new Set(["gone"])), [
    "Step 3 opens the region gone, which you deleted; point the step at another view.",
  ]);
});

test("slugs make unique, valid region IDs", () => {
  assert.equal(em.slug("Storage & Cache"), "storage-cache");
  assert.equal(em.slug("Storage", ["storage", "storage-2"]), "storage-3");
  assert.equal(em.slug("!!!"), "region");
  assert.ok(em.validId(em.slug("Ünïcode name")));
  assert.equal(em.validId("has space"), false);
});

test("suggested regions follow top-level folders and carry a drafted mark", () => {
  const units = [
    { id: "1", path: "x/api", local: "api", name: "api" },
    { id: "2", path: "x/api/v2", local: "api/v2", name: "v2" },
    { id: "3", path: "x", local: ".", name: "root" },
    { id: "4", path: "x/store", local: "store", name: "store" },
  ];
  const mark = { by: "acu studio", at: "2026-11-26T00:00:00Z" };
  const rs = em.suggestRegions(units, mark);
  assert.deepEqual(
    rs.map(r => [r.id, r.name, r.paths]),
    [
      ["root", "Root", ["."]],
      ["api", "api", ["api", "api/**"]],
      ["store", "store", ["store", "store/**"]],
    ],
  );
  assert.deepEqual(rs[0].drafted, mark);
  assert.notEqual(rs[0].drafted, mark);
});

test("picking a unit adds or removes its literal path", () => {
  const u = { id: "1", path: "repo/api", local: "api", name: "api" };
  assert.equal(em.hasPath(["api"], u), true);
  assert.equal(em.hasPath(["repo/api"], u), true);
  assert.equal(em.hasPath(["api/**"], u), false);
  assert.deepEqual(em.togglePath(["x"], u, true), ["x", "repo/api"]);
  assert.deepEqual(em.togglePath(["x", "api", "repo/api"], u, false), ["x"]);
  assert.deepEqual(em.parsePaths(" a \n\n b/** \n"), ["a", "b/**"]);
  // A twin unit in another language shares "api"; ticks never touch it.
  assert.equal(em.hasPath(["api"], u, true), false);
  assert.deepEqual(em.togglePath(["api", "repo/api"], u, false, true), ["api"]);
  const twin = { id: "2", path: "web/api", local: "api", name: "api" };
  assert.deepEqual([...em.sharedLocals([u, twin])], ["api"]);
});

test("problems name what a save still needs", () => {
  const p = em.problems(
    [{ title: " ", text: "x", open: { view: "overview" } }],
    [
      { id: "a", name: "A" },
      { id: "a", name: "" },
      { id: "bad id", name: "B" },
    ],
    "",
  );
  assert.deepEqual(p, [
    "Give the project a name.",
    "Give step 1 a title.",
    "Give region 2 a name.",
    "Give region 2 an ID that no other region uses.",
    'Give region 3 an ID of letters, digits, "_" and "-".',
  ]);
});

test("clean drops empty fields", () => {
  assert.deepEqual(em.clean({ id: "a", name: "A", paths: [], description: "", color: 0, default: undefined }), {
    id: "a",
    name: "A",
    color: 0,
  });
});
