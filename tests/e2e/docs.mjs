// Checks that a map documents its code: every unit, every file and every
// top-level symbol carries a doc whose first sentence ends as a sentence
// ends, and so does every exported member. The map draws that sentence on
// every box, so a gap here shows as "No doc comment" there. The project's
// own map (atlas.yaml in the repository root) must pass; see CONTRIBUTING.md.
//
//   node tests/e2e/docs.mjs <atlas.json>

import { readFileSync } from "node:fs";

const [data] = process.argv.slice(2);
if (!data) {
  console.error("docs: name an atlas.json: node tests/e2e/docs.mjs <atlas.json>");
  process.exit(2);
}
const d = JSON.parse(readFileSync(data, "utf8"));
const symbols = new Map(d.symbols.map(s => [s.id, s]));

/** Reports why a doc falls short, or returns "" when it serves. */
function gap(doc) {
  if (!doc) return "no doc comment";
  const s = (doc.summary ?? "").trim();
  if (!s) return "a doc without a first sentence";
  if (!/[.!?]$/.test(s.replace(/[`*)\]]+$/, ""))) return "a first sentence without an end: " + JSON.stringify(s);
  return "";
}

const problems = [];
const add = (where, what, why) => problems.push({ where, what, why });
for (const c of d.containers) {
  if (!c.unit && c.kind !== "file") continue;
  const why = gap(c.doc);
  // A file whose doc documents its unit keeps none of its own.
  const ownsUnitDoc =
    c.kind === "file" &&
    !c.doc &&
    d.containers.some(u => u.id === c.parent && u.doc && u.doc.raw && fileHoldsUnitDoc(c, u));
  if (why && !ownsUnitDoc) add(c.path ?? c.id, `${c.languageKind} ${c.name}`, why);
}
for (const s of d.symbols) {
  const member = symbols.has(s.parent);
  if (member && !s.exported) continue;
  const why = gap(s.doc);
  if (why) add(`${s.location.path}:${s.location.line}`, `${s.languageKind} ${member ? qual(s) : s.name}`, why);
}

/** A unit's doc lives in one of its files: the Go package clause's file, or the TypeScript file that carries it. */
function fileHoldsUnitDoc(file, unit) {
  const units = d.containers.filter(x => x.parent === unit.id && x.kind === "file");
  const undocumented = units.filter(x => !x.doc);
  return undocumented.length === 1 && undocumented[0].id === file.id;
}

function qual(s) {
  const p = symbols.get(s.parent);
  return p ? `${p.name}.${s.name}` : s.name;
}

if (problems.length) {
  problems.sort((a, b) => a.where.localeCompare(b.where, "en", { numeric: true }));
  for (const p of problems) console.error(`${p.where}: ${p.what}: ${p.why}`);
  console.error(
    `docs: ${problems.length} places in ${data} lack a doc that the map can show. Write a doc comment whose first sentence says what each one does, then run again.`,
  );
  process.exit(1);
}
console.log(`docs: every unit, file, item and exported member in ${data} carries a doc`);
