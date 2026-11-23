# Plan: phase 3, Svelte and SvelteKit

This plan turns issue #5 into work for one implementing session. It builds on
the TypeScript adapter. Read `AGENTS.md`, `docs/schema.md`, `docs/adapters.md`
("TypeScript") and issue #5 first.

## Decisions already made

1. TypeScript stays on major version 6, pinned exactly at 6.0.3 in
   `viewer/package.json`, `adapters/typescript/package.json` and
   `package-lock.json`. TypeScript 7 (now `latest` on npm) offers only an
   unstable API, and the Svelte tools do not accept it.
2. The maintainer approves two new runtime dependencies of
   `adapters/typescript`:
   - `svelte` 5.57.3 (26 October 2026). `svelte2tsx` parses with
     `svelte/compiler`.
   - `svelte2tsx` 0.7.63 (1 November 2026). Its peer range accepts
     TypeScript `^6.0.0`.

   Both releases have passed the two-week mark of CONTRIBUTING.md §4.11.3.
   Pin them exactly.
3. No new runtime dependency beyond these two. Write the source-map decoder
   in the adapter (about 40 lines of VLQ); do not import
   `@jridgewell/sourcemap-codec`, which arrives only through `svelte`.
4. The data format stays at version 1 with no new field, kind or enum value.
   A component that renders another one makes a `calls` edge; the
   maintainer chose `calls` over a new `renders` kind.
5. `@sveltejs/kit` stays out of the adapter's dependencies. The plugin reads
   routes from file names; it never runs `svelte.config.js` or
   `vite.config.js`.

## Steps

Each step ends with `scripts/check.sh --quick` passing. Commit after each
step.

### 1. Pins

1. Add `svelte` and `svelte2tsx` to `dependencies` in
   `adapters/typescript/package.json`, exact versions. Run `npm install`.
   Confirm that `node_modules/typescript` in the lock file stays 6.0.3.
2. In `scripts/check-pins.sh`, add a check that the TypeScript version
   starts with `6.`. On failure print
   `pins: TypeScript <v> leaves major version 6; the adapters need the TypeScript 6 API (see docs/adapters.md).`
3. Add the rule to CONTRIBUTING.md §4.11 and to `AGENTS.md` rule 10, and
   list `svelte` and `svelte2tsx` there as pins of one file.

### 2. Components in the program

New file `adapters/typescript/src/svelte.ts`; new file
`adapters/typescript/src/sourcemap.ts`.

1. Find `.svelte` files under the root with the existing walk. Honour
   `exclude` and the `tests` globs, as for `.ts` files.
2. Run each file through `svelte2tsx` with `mode: "ts"`,
   `isTsFile` from the `lang` attribute, `emitOnTemplateError: true` and
   the version from `svelte/compiler`. Keep `{ code, map }`.
3. Serve the generated code to the compiler under a virtual name that no
   real file can take, for example `Button.svelte.acu.tsx`, with
   `ScriptKind.TSX`. Do this in a `CompilerHost` wrapper around the host
   that `extract.ts` builds now.
4. Resolve `./Button.svelte` imports to the virtual file through
   `resolveModuleNameLiterals` on the same host.
5. Add the shims that `svelte2tsx` ships (`svelte-shims-v4.d.ts`,
   `svelte-jsx-v4.d.ts`) as root files, found with `require.resolve` from
   the adapter's own install.
6. Map every location in a virtual file back to the `.svelte` file. A
   position with no mapping takes the nearest earlier mapping on its line,
   else the component's first line. Files, containers and IDs name the
   `.svelte` path, never the virtual one.
7. tsconfig. A SvelteKit project extends a generated file:
   `./.svelte-kit/tsconfig.json` (Kit 2) or `$app/tsconfig` (Kit 3). Neither
   exists before `svelte-kit sync`, and the adapter installs nothing. When an
   `extends` target does not resolve, parse the config without it, apply
   the compiler options that `svelte-check` forces (read its source for the
   list), and add a note.
8. Aliases. When the config gives no `paths` for `$lib`, add `$lib` and
   `$lib/*` to `src/lib`. Kit 3's `#lib` comes from `imports` in
   `package.json`, which `NodeNext` and `Bundler` resolution already read.
   `$app/*`, `$env/*` and `./$types` resolve to nothing; the existing note
   counts them.
9. Turn on Svelte handling only when the root holds a `.svelte` file. A
   project without one gives byte-identical output: the immer and Hono
   count snapshots and the TypeScript golden file must not change.

### 3. Symbols and docs

Follow the tables in `docs/adapters.md`. Kinds come from the existing
`symbolKind` enum.

| Svelte | `kind` | `languageKind` |
| --- | --- | --- |
| A component | `function` | "component" |
| A prop | a parameter in `signature.params` | — |
| A snippet in the markup | `function`, member | "snippet" |
| A dispatched event (legacy) | `event`, member | "event" |
| An export of `<script module>` | its usual kind, member | its usual word |
| A top-level declaration of the instance script | its usual kind, member, not exported | its usual word |

1. IDs. The module part keeps the `.svelte` name, so that `Button.svelte`
   and `Button.ts` in one folder never collide:
   `ts:app/src/lib/Button.svelte#Button`. A `.svelte.ts` file follows the
   usual rule and strips only `.ts`. When the two forms collide, keep the
   component and add a note.
2. The component's name follows the file name, as `svelte2tsx` writes it.
3. Props come from the `$props()` type in Svelte 5, and from `export let`
   in legacy components. A prop's doc comment fills its parameter text.
4. The `<!-- @component -->` comment gives the component's doc. Check that
   `svelte2tsx` carries it as JSDoc; if not, read it from the source.
5. Runes. Look at `$state`, `$derived`, `$effect` and `$props` in the
   output. A variable must show its own type, never a rune's wrapper type.
   Where the generated TypeScript reads poorly, fix the word in the adapter,
   in neutral terms.
6. The viewer must not need the word "svelte". `viewer/test/words.test.ts`
   checks this.

### 4. Edges

1. A component that renders another one: `svelte2tsx` writes the use as a
   call or a `new`. Confirm that the existing call path gives a `calls` edge,
   `exact`, from the outer component to the inner one, with the line of the
   tag in the `.svelte` file.
2. Calls in the markup and in event handlers count for the component, as a
   call in a nested function counts for its declaration.
3. No `renders` kind. The maintainer chose `calls` for every render.

### 5. The `sveltekit` plugin

New file `adapters/typescript/src/sveltekit.ts`. Register it in
`plugins.ts`, and in `plugin_adapter` in `cli/src/adapter.rs` (with its
test). Option `routes`, default `src/routes`.

1. A route path comes from the folders under `routes`. Drop `(group)`
   folders. Keep `[param]`, `[[optional]]`, `[...rest]` and `[id=matcher]`
   as written.
2. `+server.ts` or `+server.js`: each exported `GET`, `POST`, `PUT`,
   `PATCH`, `DELETE`, `OPTIONS` and `HEAD` makes an `http` entry with that
   method. `fallback` makes `ANY`.
3. A page (`+page.svelte`, with optional `+page.ts` and `+page.server.ts`)
   makes one `http` entry, `GET <path>`. Its handler: the `load` of
   `+page.server.ts`, else the `load` of `+page.ts`, else the component.
4. Form actions in `+page.server.ts`: `default` makes `POST <path>`; a
   named action makes `POST <path>?/<name>`. An inline action at the top
   level becomes a function symbol named after its route, as the existing
   plugins do.
5. Layouts make no entry of their own. A layout answers no request alone;
   it wraps the pages below it. The plugin records that wrap as edges
   instead (step 5.6).
6. Layout edges. SvelteKit renders each page inside every `+layout.svelte`
   from `routes` down to the page's folder. Each layout renders its child
   through `{@render children()}` (or `<slot />` in legacy code); the page
   never names its layout. No checker resolves this, so each edge carries
   `confidence: "heuristic"`, and each points from the user to the thing
   used:
   - from each layout component to the nearest layout or page below it, as
     `calls`;
   - from a `load` that calls `parent()` to the `load` of each enclosing
     layout, as `calls`. A `+page.server.ts` or `+layout.server.ts` load
     points at enclosing `+layout.server.ts` loads. A `+page.ts` or
     `+layout.ts` load points at enclosing `+layout.ts` loads, and at a
     layout's `+layout.server.ts` load when that layout has no `+layout.ts`.

   Honour the reset syntax: `+page@.svelte`, `+page@<segment>.svelte` and
   `+layout@<segment>.svelte` skip the layouts between the file and the
   named segment. `(group)` folders count as segments for layouts, but not
   for the path. Read the "Advanced layouts" page of the SvelteKit 3 docs
   before you write this step, and check the reset syntax there.
7. `hooks.server.ts`, `+error.svelte` and param matchers make no entries.
   Say so in the limits.
8. `plugin: "sveltekit"`. Sort entries and edges as every other list.

### 6. Fixture and tests

1. New invented app in `adapters/typescript/testdata/svelte/`, in the
   Kit 3 layout (`$app/tsconfig`, `#lib`), with:
   - `src/lib/Button.svelte`: props with docs, a `@component` comment, a
     snippet;
   - `src/lib/Counter.svelte`: runes;
   - `src/lib/Legacy.svelte`: `export let` and `createEventDispatcher`;
   - `src/lib/counter.svelte.ts` and `src/lib/format.ts`;
   - `src/routes/+layout.svelte` and `src/routes/+page.svelte`;
   - `src/routes/blog/[slug]/+page.svelte` and `+page.server.ts` with a
     `load` and two actions, one inline;
   - `src/routes/(marketing)/+layout.svelte` and
     `src/routes/(marketing)/about/+page.ts`;
   - `src/routes/blog/+layout.ts` with a `load`, and a page `load` that
     calls `parent()`;
   - one page that resets its layout with `@`;
   - `src/routes/api/items/+server.ts` with `GET` and `POST`;
   - a `.test.ts` file, so that tests record against components.
2. A golden file `testdata/golden/svelte.json`, and an
   `npm run golden` path for it.
3. Tests in `test/extract.test.ts`: schema validation; a location inside
   the markup maps to the right `.svelte` line; the component's
   `languageKind`; the props in `signature.params`; the `calls` edge from
   `+page.svelte` to `Button`; the full list of entries; the layout edges,
   with `heuristic` confidence, for a nested page, a grouped page and a
   reset page.
4. Unit tests for `sourcemap.ts`.
5. Check that `npm run format:check` passes with `.svelte` files present
   (Prettier has no Svelte plugin here and must skip them).

### 7. Corpus project

1. Choose a public SvelteKit app with a permissive licence that uses
   `+server.ts`, `load` and form actions, at roughly Hono's size or smaller.
   Check `sveltejs/realworld` first. Record its licence and commit.
2. Add it as a shallow submodule with `update = none` under
   `corpus/svelte/<name>`, as `.gitmodules` does for the others.
3. Write `examples/<name>/atlas.yaml` after `examples/hono/atlas.yaml`:
   regions, layout, a tour of five to seven stops, and a `drafted` mark on
   each text you write. Check each claim against the source.
4. Make `examples/<name>/counts.json` with `tests/e2e/counts.mjs`. Wire it
   into `scripts/check.sh` where the Hono snapshot runs.
5. Run `acu check` on every example.

### 8. Viewer check

No viewer code should change. Build a site from the new example, run
`node tests/e2e/smoke.mjs out/site out/shot` and the same with `--light`,
and look at each screenshot: components, the routes list, a trace from a
form action, the docs panel of a component.

### 9. Docs

1. `docs/adapters.md`: a "Svelte and SvelteKit" section under TypeScript,
   with what it reads, IDs, the symbol table, edges, the plugin and limits.
2. `docs/schema.md` and the `confidence` description in the schema: say
   that `heuristic` also covers an edge that a framework's file convention
   implies, such as a layout around a page. The enum stays as it stands.
3. The adapter's help text and output notes: the new limits.
4. `README.md`: the status row for phase 3.
5. Each new file opens with a comment that says what it holds; each item
   carries a doc comment. Run `cargo run -- extract -c atlas.yaml -o
   out/self.json` and `node tests/e2e/docs.mjs out/self.json`.

## Done when

1. `scripts/check.sh` passes in full, with its output in the pull request.
2. The acceptance criteria of issue #5 hold.
3. The immer and Hono snapshots and the TypeScript golden file show no
   change.
4. A symbol view of a page shows its layouts on the left, drawn in the
   heuristic style, in both themes.

## Risks

1. `svelte2tsx` output changes between releases. The exact pin keeps the
   golden file stable; a later bump will need a golden update, with the
   reason in the commit message.
2. The adapter parses with its own `svelte` version, not the project's. A
   project on a newer syntax may fail to parse. Add a note per file that
   fails, and keep going.
3. Generated code without a mapping can give a wrong line. Step 2.6 and the
   location tests guard this.
