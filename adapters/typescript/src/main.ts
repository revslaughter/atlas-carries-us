#!/usr/bin/env node
// acu-adapter-typescript reads a TypeScript project with the compiler API and
// writes an Atlas Carries Us document (schema version 1) as JSON.
//
// The acu command runs it with a request on standard input (protocol 1, see
// docs/adapters.md). For a run by hand, pass flags instead:
//
//   acu-adapter-typescript --root /path/to/project --exclude '**/generated/**' --plugin hono --out atlas.json

import fs from "node:fs";
import { parseArgs } from "node:util";

import { ADAPTER_VERSION, type Request, run } from "./extract.js";

const HELP = `acu-adapter-typescript ${ADAPTER_VERSION}: reads a TypeScript project and writes Atlas Carries Us data.

Without --root, the request comes as JSON on standard input (protocol 1).

  --root <dir>        folder to read
  --repo-root <dir>   repository root that file paths are relative to (default: --root)
  --tsconfig <file>   tsconfig file, relative to --root (default: tsconfig.json)
  --include <glob>    folder to keep (repeatable)
  --exclude <glob>    folder to leave out (repeatable)
  --tests <glob>      files that hold tests (repeatable; default: **/*.test.*, **/*.spec.*, **/__tests__/**)
  --plugin <name>     entry-point plugin to switch on: express, hono (repeatable)
  --out <file>        output file (default: standard output)
  --indent            indent the JSON
  --version           print the version and exit

The map leaves out these limits; the output's meta.adapters[].notes lists them:
calls through function values and callbacks, and declaration files (.d.ts).
`;

function fail(err: unknown): never {
  const msg = err instanceof Error ? err.message : String(err);
  process.stderr.write(`acu-adapter-typescript: ${msg}\n`);
  process.exit(1);
}

function main() {
  const { values } = parseArgs({
    options: {
      root: { type: "string" },
      "repo-root": { type: "string" },
      tsconfig: { type: "string" },
      include: { type: "string", multiple: true },
      exclude: { type: "string", multiple: true },
      tests: { type: "string", multiple: true },
      plugin: { type: "string", multiple: true },
      out: { type: "string" },
      indent: { type: "boolean" },
      version: { type: "boolean" },
      help: { type: "boolean", short: "h" },
    },
  });
  if (values.help) {
    process.stdout.write(HELP);
    return;
  }
  if (values.version) {
    process.stdout.write(`acu-adapter-typescript ${ADAPTER_VERSION}\n`);
    return;
  }
  let req: Request;
  if (!values.root) {
    let parsed: Request;
    try {
      parsed = JSON.parse(fs.readFileSync(0, "utf8")) as Request;
    } catch (e) {
      fail(
        `read the request from standard input: ${e instanceof Error ? e.message : e}; pass --root for a run by hand`,
      );
    }
    if (parsed.protocol !== 1) fail(`request protocol ${parsed.protocol}: this adapter speaks protocol 1`);
    req = parsed;
  } else {
    req = {
      protocol: 1,
      root: values.root,
      repoRoot: values["repo-root"],
      include: values.include,
      exclude: values.exclude,
      tests: values.tests,
      options: values.tsconfig ? { tsconfig: values.tsconfig } : {},
      entryPoints: (values.plugin ?? []).map(plugin => ({ plugin })),
    };
  }
  const doc = run(req);
  doc.meta.generated = new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
  const text = JSON.stringify(doc, null, values.indent ? 1 : undefined) + "\n";
  if (values.out) fs.writeFileSync(values.out, text);
  else process.stdout.write(text);
  process.stderr.write(
    `acu-adapter-typescript: ${doc.containers.length} containers, ${doc.symbols.length} symbols, ${doc.edges.length} edges, ${doc.entryPoints?.length ?? 0} entry points\n`,
  );
}

try {
  main();
} catch (e) {
  fail(e);
}
