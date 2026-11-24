#!/usr/bin/env node
/**
 * The TypeScript adapter: acu-adapter-typescript reads a TypeScript or
 * JavaScript project with the TypeScript compiler API and writes an Atlas
 * Carries Us document (data format version 1) as JSON. A project without
 * TypeScript files runs in the JavaScript mode, whose IDs start with "js:".
 *
 * The acu command runs it with a request on standard input (protocol 1, see
 * [docs/adapters.md](https://github.com/revslaughter/atlas-carries-us/blob/main/docs/adapters.md)).
 * {@link run} reads the project in four passes: declarations, entry points,
 * relations, then docs and tests. The type checker resolves every edge. In
 * TypeScript every edge counts as exact, except the layout edges of the
 * sveltekit plugin; in JavaScript, a call through a receiver whose type the
 * checker infers counts as heuristic. Svelte components join the program through svelte2tsx.
 *
 * For a run by hand, pass flags instead:
 *
 * ```sh
 * acu-adapter-typescript --root /path/to/project --exclude '**\/generated/**' --plugin hono --out atlas.json
 * ```
 *
 * @packageDocumentation
 */

import fs from "node:fs";
import { parseArgs } from "node:util";

import { ADAPTER_VERSION, type Request, run } from "./extract.js";

/** The text that --help prints: every flag and the limits of a run. */
const HELP = `acu-adapter-typescript ${ADAPTER_VERSION}: reads a TypeScript or JavaScript project and writes Atlas Carries Us data.

Without --root, the request comes as JSON on standard input (protocol 1).

  --root <dir>        folder to read
  --repo-root <dir>   repository root that file paths are relative to (default: --root)
  --tsconfig <file>   tsconfig file, relative to --root (default: tsconfig.json, else jsconfig.json)
  --language <tag>    js or ts (default: js when the map holds no TypeScript file and the root no .svelte file)
  --module-resolution <name>  without a config: bundler (default), node16, nodenext or node10
  --include <glob>    folder to keep (repeatable)
  --exclude <glob>    folder to leave out (repeatable)
  --tests <glob>      files that hold tests (repeatable; default: **/*.test.*, **/*.spec.*, **/__tests__/**)
  --plugin <name>     entry-point plugin to switch on: express, hono, sveltekit (repeatable)
  --out <file>        output file (default: standard output)
  --indent            indent the JSON
  --version           print the version and exit

The map leaves out these limits; the output's meta.adapters[].notes lists them:
calls through function values and callbacks, and declaration files (.d.ts).

In JavaScript, the types come from JSDoc (@param, @returns, @type, @typedef,
@callback, @template) or from inference. CommonJS exports, constructor
functions and their prototypes, and object literals that act as modules make
symbols. A call through a receiver whose type the checker infers counts as
heuristic. The map leaves out calls through values of unknown type, members that
mixins or other run-time prototype changes add, properties that code adds
outside a constructor, and exports under a computed name.

A project with .svelte files reads its components through svelte2tsx. The map
leaves out renders of a component that a variable holds and of a snippet that
a prop passes in. A component that fails to parse stays out, with a note. The
sveltekit plugin draws layouts around pages, and parent() reads, as heuristic
edges, because no checker resolves them.
`;

/** Writes the error to standard error, with the program's name first, and exits with status 1, which acu reports as a failed adapter. */
function fail(err: unknown): never {
  const msg = err instanceof Error ? err.message : String(err);
  process.stderr.write(`acu-adapter-typescript: ${msg}\n`);
  process.exit(1);
}

/**
 * Reads a request (protocol 1 on standard input, or flags for a run by hand),
 * runs {@link run}, and writes the document as JSON. A summary line goes to
 * standard error, so standard output carries data only.
 */
function main() {
  const { values } = parseArgs({
    options: {
      root: { type: "string" },
      "repo-root": { type: "string" },
      tsconfig: { type: "string" },
      language: { type: "string" },
      "module-resolution": { type: "string" },
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
      options: {
        ...(values.tsconfig ? { tsconfig: values.tsconfig } : {}),
        ...(values.language ? { language: values.language } : {}),
        ...(values["module-resolution"] ? { moduleResolution: values["module-resolution"] } : {}),
      },
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
