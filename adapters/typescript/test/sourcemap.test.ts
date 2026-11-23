// Unit tests of the source-map decoder: Base64 VLQ numbers, the mappings of
// each line, and the lookup that maps a generated position back to its
// source. The expected values come from the reference encoder of
// @jridgewell/sourcemap-codec.

import assert from "node:assert/strict";
import { test } from "node:test";

import { decodeMappings, decodeVLQ, SourceMap } from "../src/sourcemap.js";

test("VLQ numbers decode to their values, with the sign", () => {
  const cases: [string, number][] = [
    ["A", 0],
    ["C", 1],
    ["D", -1],
    ["e", 15],
    ["gB", 16],
    ["hB", -16],
    ["2H", 123],
    ["qxmvrH", 123456789],
    ["//////D", -2147483647],
  ];
  for (const [s, n] of cases) assert.deepEqual(decodeVLQ(s), [n], `decodeVLQ("${s}")`);
  assert.deepEqual(decodeVLQ("AACA"), [0, 0, 1, 0]);
});

test("a VLQ that ends inside a number, or holds a foreign character, fails", () => {
  assert.throws(() => decodeVLQ("g"), /ends inside a number/);
  assert.throws(() => decodeVLQ("A!"), /Base64 VLQ does not use/);
});

test("mappings decode to absolute source positions on each line", () => {
  assert.deepEqual(decodeMappings("AAAA;IACE,MAAM;;AAIRA"), [
    [{ col: 0, srcLine: 0, srcCol: 0 }],
    [
      { col: 4, srcLine: 1, srcCol: 2 },
      { col: 10, srcLine: 1, srcCol: 8 },
    ],
    [],
    [{ col: 0, srcLine: 5, srcCol: 0 }],
  ]);
});

test("a segment without a source drops out, but still moves the column", () => {
  assert.deepEqual(decodeMappings("K,EAAA"), [[{ col: 7, srcLine: 0, srcCol: 0 }]]);
});

test("a position takes its own segment, or the nearest earlier one on its line", () => {
  const map = new SourceMap("AAAA;IACE,MAAM;;AAIRA");
  assert.deepEqual(map.original(1, 4), { line: 1, col: 2 }, "a position with its own segment");
  assert.deepEqual(map.original(1, 7), { line: 1, col: 2 }, "a position between two segments");
  assert.deepEqual(map.original(1, 40), { line: 1, col: 8 }, "a position after the last segment");
  assert.equal(map.original(1, 2), undefined, "a position before the first segment of its line");
  assert.equal(map.original(2, 0), undefined, "a line without segments");
  assert.equal(map.original(9, 0), undefined, "a line after the end of the map");
});
