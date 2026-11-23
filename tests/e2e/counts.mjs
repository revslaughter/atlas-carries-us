// Compares the counts in an atlas.json with a checked-in snapshot, so a
// change to an adapter that alters the facts shows up in review.
//
//   node tests/e2e/counts.mjs <atlas.json> <counts.json> [--update]

import { readFileSync, writeFileSync } from "node:fs";

const [data, snap] = process.argv.slice(2);
const d = JSON.parse(readFileSync(data, "utf8"));
// The tally starts from an object without a prototype, so a kind named
// "constructor" counts like any other.
const kinds = (list, key) =>
  Object.fromEntries(
    Object.entries(list.reduce((m, x) => ((m[x[key]] = (m[x[key]] ?? 0) + 1), m), Object.create(null))).sort(),
  );
const got = {
  containers: kinds(d.containers, "kind"),
  symbols: kinds(d.symbols, "kind"),
  edges: kinds(d.edges, "kind"),
  entryPoints: (d.entryPoints ?? []).length,
  symbolsWithTests: Object.keys(d.tests ?? {}).length,
  regions: (d.regions ?? []).length,
  tourSteps: (d.tour ?? []).length,
};
if (process.argv.includes("--update")) {
  writeFileSync(snap, JSON.stringify(got, null, 2) + "\n");
  console.log("counts: wrote " + snap);
  process.exit(0);
}
const want = JSON.parse(readFileSync(snap, "utf8"));
const a = JSON.stringify(got, null, 2),
  b = JSON.stringify(want, null, 2);
if (a !== b) {
  console.error(
    `counts: ${data} differs from ${snap}.\n--- got\n${a}\n--- want\n${b}\nRun with --update after a deliberate change.`,
  );
  process.exit(1);
}
console.log("counts: " + snap + " matches");
