// The viewer must stay language-neutral: it shows the words that the data
// carries (languageKind) and never names one language's constructs itself.
// The schema's neutral kind names (class, struct, interface, trait, enum and
// so on) may appear, because the schema defines them for every language.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";

const FORBIDDEN = /\b(funcs?|goroutines?|godoc|golang|chi|cobra|jsdoc|tsdoc|crates?|npm|packages?|typescript|rust)\b/i;

test("the viewer source names no language-specific constructs", () => {
  const dir = new URL("../src/", import.meta.url);
  const hits: string[] = [];
  for (const f of readdirSync(dir)) {
    if (!/\.(ts|html|css)$/.test(f) || f === "types.ts") continue;
    readFileSync(new URL(f, dir), "utf8").split("\n").forEach((line, i) => {
      const m = line.match(FORBIDDEN);
      if (m) hits.push(`${f}:${i + 1}: ${m[0]}`);
    });
  }
  assert.deepEqual(hits, []);
});
