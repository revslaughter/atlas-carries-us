// Unit tests for the doc normaliser and the path globs.

import assert from "node:assert/strict";
import { test } from "node:test";

import { blocks, commentText, firstSentence, inline, normalise } from "../src/docs.js";
import { compileGlobs } from "../src/glob.js";
import { joinPath } from "../src/plugins.js";

const none = () => undefined;
const known = (t: string) => (t === "Store.get" ? "sym:ts:x#Store.get" : undefined);

test("inline text escapes the characters that the inline subset uses", () => {
  assert.equal(inline("a [b] * c \\ d", none), "a \\[b\\] \\* c \\\\ d");
  assert.equal(inline("use `x * y` here", none), "use `x * y` here");
  assert.equal(inline("**bold** and _soft_ and *em*", none), "**bold** and *soft* and *em*");
  assert.equal(inline("snake_case_name stays", none), "snake_case_name stays");
});

test("links become link targets, and unknown symbols stay plain text", () => {
  assert.equal(inline("see {@link Store.get}", known), "see [Store.get](sym:ts:x#Store.get)");
  assert.equal(inline("see {@link Store.get | the getter}", known), "see [the getter](sym:ts:x#Store.get)");
  assert.equal(inline("see {@linkcode Store.get}", known), "see [`Store.get`](sym:ts:x#Store.get)");
  assert.equal(inline("see {@link Gone}", known), "see Gone");
  assert.equal(inline("[docs](https://x.dev/a(b))", none), "[docs](https://x.dev/a%28b%29)");
  assert.equal(inline("at https://x.dev/page.", none), "at [https://x.dev/page](https://x.dev/page).");
});

test("the summary ends at the first sentence, not at an abbreviation or inside code", () => {
  assert.equal(firstSentence("Reads a file, e.g. a config. Then more."), "Reads a file, e.g. a config.");
  assert.equal(firstSentence("Calls `a.b()` first. Then more."), "Calls `a.b()` first.");
  assert.equal(firstSentence("No end at all"), "No end at all");
});

test("Markdown turns into blocks", () => {
  const b = blocks("One\nline.\n\n- a\n  wrapped\n- b\n\n```ts\nx();\n  y();\n```\n\n# Head\n\n> quoted", none);
  assert.deepEqual(b, [
    { type: "paragraph", text: "One line." },
    { type: "list", ordered: false, items: ["a wrapped", "b"] },
    { type: "code", code: "x();\n  y();", language: "ts" },
    { type: "heading", level: 1, text: "Head" },
    { type: "quote", text: "quoted" },
  ]);
});

test("comment markers come off, and indentation inside stays", () => {
  assert.equal(commentText("/**\n * Hello.\n *\n *     code();\n */"), "Hello.\n\n    code();");
  assert.equal(commentText("// one\n// two"), "one\ntwo");
});

test("block tags fill their own fields", () => {
  const d = normalise(
    "Body.\n\n@param {string} id - the ID\n@param [opt=1] an option\n@template T, U the types\n@returns the thing\n@see Store.get\n@beta",
    known,
    "ts",
  )!;
  assert.deepEqual(d.params, { id: "the ID", opt: "an option" });
  assert.deepEqual(d.typeParams, { T: "the types", U: "the types" });
  assert.equal(d.returns, "the thing");
  assert.deepEqual(d.seeAlso, ["[Store.get](sym:ts:x#Store.get)"]);
  assert.deepEqual(d.tags, [{ name: "beta", text: "" }]);
  assert.equal(d.format, "jsdoc");
});

test("an example without a fence keeps its code as written", () => {
  const d = normalise("@example <caption>Short</caption>\nproduce(x, d => {\n  d.a = 1\n})", none, "js")!;
  assert.deepEqual(d.examples, [{ code: "produce(x, d => {\n  d.a = 1\n})", language: "js", title: "Short" }]);
});

test("globs match folders as the Go adapter's do", () => {
  const g = compileGlobs(["**/generated/**", "src/*"]);
  const cases: [string, boolean][] = [
    ["src/generated", true],
    ["generated", true],
    ["a/generated/b", true],
    ["src/web", true],
    ["src", false],
    ["src/generate", true],
    ["lib/generate", false],
  ];
  for (const [dir, want] of cases) assert.equal(g.match(dir), want, dir);
});

test("route paths join with one slash", () => {
  assert.equal(joinPath("", "/x"), "/x");
  assert.equal(joinPath("/api", "/"), "/api");
  assert.equal(joinPath("/api/", "/x"), "/api/x");
  assert.equal(joinPath("/", "/x"), "/x");
  assert.equal(joinPath("", ""), "/");
});
