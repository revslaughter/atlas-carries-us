# Adapters

An adapter reads code in one language and writes an Atlas Carries Us
document (see [`schema.md`](schema.md)). Each adapter lives in its own
language's ecosystem and runs as its own program, so it can use that
language's own compiler or type checker.

## Protocol 1

`acu` starts the adapter, writes one JSON request to its standard input, and
reads one JSON document from its standard output. Messages for people go to
standard error. A non-zero exit code means failure.

The request:

```json
{
  "protocol": 1,
  "repoRoot": "/abs/path/to/repository",
  "root": "/abs/path/to/the/folder/to/read",
  "include": ["internal/**"],
  "exclude": ["**/features/**"],
  "tests": [],
  "options": { "buildTags": ["grafeo"] },
  "entryPoints": [{ "plugin": "go-chi", "options": { "prefixes": [] } }]
}
```

The rules for the reply:

1. Write `version`, `meta`, `containers`, `symbols` and `edges`, and, where
   found, `entryPoints` and `tests`. Leave `project`, `regions`, `layout` and
   `tour` to `acu`.
2. Write every path relative to `repoRoot`, with forward slashes.
3. Build IDs from qualified names, as `schema.md` section 3 describes, so they
   stay stable across runs.
4. Mark each container that the overview should draw as `"unit": true`.
5. Write the language's own words in `languageKind`.
6. Give each edge a `confidence`: `exact` when a type checker resolved it,
   `heuristic` when the adapter matched names, or when a framework's file
   convention implies the edge, such as a layout around a page.
7. Normalise doc comments into blocks and the inline subset. Keep the comment
   as written in `raw`.
8. Fail on an unknown entry-point plugin, with a message that names the known
   ones.
9. Keep the output deterministic: sort lists, so that a golden-file test can
   compare runs.

Each adapter also accepts flags for a run by hand; see its `-help`.

## Testing an adapter

1. A small fixture project inside the adapter's `testdata`, written to use
   every feature, with a golden file that the test compares byte for byte.
   After a deliberate change, `go test ./... -update` rewrites the Go
   adapter's golden file, and `npm run golden -w adapters/typescript` the
   TypeScript adapter's.
2. Schema validation of the fixture output.
3. Named facts: a short list of edges and entry points that must exist,
   written so that a failure reads as a sentence.
4. A public project from `corpus/`, with a count snapshot in
   `examples/<name>/counts.json` that CI compares on every run.

## Decisions per language

### Go (done)

The Go adapter uses `golang.org/x/tools/go/packages` and the type checker
(`go/types`), so every edge counts as `exact`. Doc comments go through
`go/doc/comment`, which already finds paragraphs, headings, lists, code blocks
and doc links such as `[Mux.Get]`. It ports the prototype extractor, and on
chi v5.5.0 and Cobra v1.10.2 its output folds back to the prototype's symbol,
edge, member-call and test counts exactly (`tools/prototype-parity.py`).

Limits, which the help text also lists:

1. Calls through function values, closures passed as arguments, and
   reflection do not appear.
2. A call through an interface points at the interface method. The viewer
   follows `implements` edges to the code that can run.
3. A licence header above the package clause never becomes a doc.

### TypeScript (done)

The TypeScript adapter, `adapters/typescript`, runs in Node and reads the
project with the TypeScript compiler API, pinned to TypeScript 6.0.3.
TypeScript 7, the native port, offers its programmatic API only as "unstable"
so far; the adapter will move once that API settles. The checker resolves
every edge, so every edge counts as `exact`; only the layout edges of the
`sveltekit` plugin, below, count as `heuristic`. On immer v11.2.9 and Hono
v4.13.17 the counts sit in `examples/immer/counts.json` and
`examples/hono/counts.json`.

#### What it reads

1. The program comes from `tsconfig.json` in the root, or from the file that
   `options.tsconfig` names. A solution-style tsconfig (no files, only
   `references`) gives the files of every project it references. Without a
   tsconfig, the adapter reads every `.ts` and `.tsx` file under the root.
2. The map holds the program's own source files under the root. Declaration
   files (`.d.ts`), `node_modules` and test files stay out.
3. Test files match the request's `tests` globs, by default `**/*.test.*`,
   `**/*.spec.*` and `**/__tests__/**`. The adapter adds them to the program,
   so that their uses resolve, but never maps them. Its search for them
   skips the folders that `exclude` names.
4. The adapter installs nothing. An import of an outside package that the
   project has not installed resolves to nothing, and a note in the output
   counts such imports.

#### Containers and IDs

| Container | ID | `kind`, `languageKind` |
| --- | --- | --- |
| The package that `package.json` names | `ts:pkg:<name>` | `package`, "package" |
| A folder that holds source files: the unit | `ts:dir:<name>/<folder>` | `directory`, "folder" |
| A file | `ts:file:<name>/<path>` | `file`, "module" or "script" |

Each folder that holds a source file becomes a unit. The units stay flat: each
names the package as its parent, as Go packages name their module. A unit
takes its path from the root as its name, for example `src/core`.

A symbol ID joins the module and the symbol's path:
`ts:immer/src/core/immerClass#Immer.produce`. The module joins the package
name and the file's path without its extension. The ID names the file, not the
folder, because two files in one folder may declare the same name. A project
without a `package.json` name drops the package part.

#### Symbols

| Code | `kind` | `languageKind` |
| --- | --- | --- |
| `class` | `class` | "class" |
| `interface` | `interface` | "interface" |
| `type X = …` | `type_alias` | "type alias" |
| `enum`, and its members | `enum`, `constant` | "enum", "enum member" |
| `function` | `function` | "function" |
| `const f = () => …` | `function` | "arrow function" |
| A method, or a method signature | `method` | "method" |
| A property that holds or types a function | `method` | "arrow function" or "property" |
| `constructor` | `constructor` | "constructor" |
| A property, or a parameter property | `property` | "property" |
| `get` and `set` | `property` | "accessor" |
| `const`, and `let` or `var` | `constant`, `variable` | "constant", "variable" |
| `namespace` | `namespace` | "namespace" |

1. Overloads, merged interfaces, and a get and set pair each make one symbol.
2. A type alias of an object type gets its members, as an interface does.
3. A top-level declaration counts as exported when its module exports it,
   under any name. A member counts as exported unless it carries `private`
   or `protected`, or a `#` name.
4. Modifiers name `static`, `async`, `abstract`, `readonly`, `override`,
   `optional`, `private`, `protected`, `declare`, `default`, `generator`,
   `get` and `set`. Decorators go to `annotations`, as written.

#### Docs

1. The adapter reads the JSDoc or TSDoc comment above each declaration. The
   text holds Markdown, which turns into blocks and the inline subset.
2. Block tags fill their own fields: `@param`, `@typeParam` and `@template`,
   `@returns`, `@throws`, `@example`, `@deprecated`, `@since` and `@see`.
   `@remarks` joins the description. Other tags, such as `@beta` or
   `@default`, go to `tags`. Tags that only restate types, such as `@type`,
   drop out.
3. `{@link}`, `{@linkcode}` and `{@linkplain}` resolve through the names in
   the file's scope, then through members, so they become `sym:` links. A
   link that resolves to nothing keeps its text.
4. A doc counts as TSDoc when it uses a tag that only TSDoc defines, such as
   `@remarks` or `@typeParam`. Otherwise it counts as JSDoc.
5. The doc of a file comes from its first comment that no declaration owns: a
   comment with `@file`, `@module` or `@packageDocumentation`, or a comment
   that a blank line parts from the code. Licence headers and tool directives
   never count. A folder takes the doc of the file that carries
   `@packageDocumentation`, or else the doc of its `index` file.
6. A parameter property takes the text of its `@param` tag on the constructor.

#### Edges

1. Calls come from the checker's resolved signature at each call, `new`,
   `super(…)`, tagged template, decorator and JSX element. A call inside a
   nested function counts for the declaration around it, as in Go.
2. A call through an interface points at the interface member. The viewer
   follows `implements` edges to the code that can run.
3. `implements` and `extends` come from the heritage clauses as written.
   Structural matches without a clause do not count.
4. `overrides` links a method to the method of the same name in a base class.
5. Field, parameter and result types come from the annotation as written.
   Without one, they come from the type that the checker infers.
6. `imports` joins folders, as Go's edges join packages. Imports of outside
   packages go to `externalImports`, by package name; Node's built-in modules
   stay out.

#### Tests

A call to `it`, `test` or `specify` with a callback makes a test, inside any
number of `describe` blocks. Its name joins the titles with " > ", as Vitest
and Jest print them. `.each`, `.skip` and `.only` forms count too. Each name
that the test's callback uses records the test against its symbol.

#### Entry-point plugins

1. `express` finds `app.get("/x", h)` and the other verbs,
   `app.route("/x").get(h)`, and routers that `app.use("/prefix", router)`
   mounts, across files.
2. `hono` finds `app.get("/x", h)` and the other verbs, `app.on(method,
   path, h)`, apps that `app.route("/prefix", sub)` mounts, and
   `new Hono().basePath("/api")`.
3. `sveltekit` reads a SvelteKit app's file-based routes; see "Svelte and
   SvelteKit" below.

The `express` and `hono` plugins know a receiver by its type (the package's own types, when the
project has them installed) or else by the import that it comes from. The last argument
handles the request; the arguments before it run as middleware. Both plugins
take the `prefixes` option of the Go plugins, for mounts that happen at run
time.

An inline handler inside a function names that function as its handler and
counts as inline, as in Go. An inline handler at the top level of a module
has no function around it. It becomes a function symbol of its own, named
after its route, such as `GET /books/:id`, so that a trace can follow its
calls.

#### Limits

The output's notes list these limits, and the help text repeats them:

1. Calls through function values, callbacks and computed property names do
   not appear.
2. A call through an interface points at the interface member.
3. Declaration files do not appear.

### Svelte and SvelteKit (done)

Svelte rides on the TypeScript adapter. The TypeScript compiler cannot read
a `.svelte` file, so the adapter turns each component into TypeScript with
`svelte2tsx`, the tool behind `svelte-check`, and the compiler does the
rest. Both `svelte2tsx` (0.7.63) and `svelte` (5.57.3), whose compiler
parses the components, sit pinned in `adapters/typescript/package.json`. On
sveltejs/realworld at commit df79670 the counts sit in
`examples/realworld/counts.json`.

#### What it reads

1. The adapter turns Svelte handling on only when the root holds a
   `.svelte` file. A project without one reads exactly as before.
2. It finds `.svelte` files with the walk that finds test files, so it skips
   `node_modules`, folders whose names start with a dot, and the folders that
   `exclude` names. A component that matches a `tests` glob joins the
   program but not the map.
3. `svelte2tsx` writes TypeScript for each component (`mode: "ts"`, with
   `emitOnTemplateError`). The compiler reads that code under a virtual name
   that ends in `.acu.tsx`, or in `.acu.jsx` for a component in JavaScript,
   so that its JSDoc types count. A compiler host wrapper serves these files.
4. An import of a `.svelte` file resolves through TypeScript's own
   resolution, so relative paths, `paths`, package `imports` (`#lib`) and
   every resolution mode behave as usual. The resolver probes for
   `Button.d.svelte.ts` beside `Button.svelte`; the wrapper answers for each
   known component and hands back its virtual name.
5. The shims that `svelte2tsx` ships, `svelte-shims-v4.d.ts` and
   `svelte-jsx-v4.d.ts`, join the program from the adapter's own install.
   They load `svelte`'s own types from there too, so runes such as `$state`
   get their declared types even in a project with nothing installed.
6. A source map from `svelte2tsx` maps every location back to the `.svelte`
   file. A position without a mapping takes the nearest earlier mapping on
   its line, else the component's first line. Files, containers and IDs
   always name the `.svelte` path, never the virtual one.
7. A SvelteKit project's tsconfig extends a file that SvelteKit writes:
   `./.svelte-kit/tsconfig.json` (Kit 2) or `$app/tsconfig` (Kit 3). Neither
   exists before an install and `svelte-kit sync`. When an `extends` target
   does not resolve, the adapter reads the config without it, with
   `allowJs`, and adds a note. Without a `tsconfig.json`, it reads
   `jsconfig.json`, as `svelte-check` does; without either, it reads every
   `.ts`, `.tsx`, `.js` and `.jsx` file under the root.
8. In a project with components, the adapter applies the compiler options
   that `svelte-check` forces: an ES module kind where the config names none
   or an old one, a module resolution where it names none, and a target of
   at least ES2015. Where the config names no module resolution, the adapter
   picks Bundler, as SvelteKit's generated config does, not `svelte-check`'s
   Node10: only the newer resolutions read the `imports` of `package.json`,
   where SvelteKit 3 declares `#lib`.
9. When the config gives no `paths` for `$lib`, the adapter maps `$lib` and
   `$lib/*` to `src/lib`, as SvelteKit 2 does. `$app/*`, `$env/*` and
   `./$types` resolve to nothing; the notes count the unresolved `./$types`
   imports. Imports through `$lib` and `#` never count as outside packages,
   because they name the project's own files.

#### IDs

A component's module keeps its `.svelte` name, so that `Button.svelte` and
`Button.ts` in one folder never collide:
`ts:app/src/lib/Button.svelte#Button`. A `.svelte.ts` module follows the
usual rule and drops only `.ts`. When the two forms collide (`Button.svelte`
beside `Button.svelte.ts`), the component keeps the name, the IDs of the
`.svelte.ts` file keep its extension, and a note says so.

#### Symbols

Kinds come from the schema's neutral list; the words go to `languageKind`.

| Svelte | `kind` | `languageKind` |
| --- | --- | --- |
| A component | `function` | "component" |
| A prop | a parameter in `signature.params` | — |
| A snippet in the markup | `function`, member | "snippet" |
| A dispatched or forwarded event (legacy) | `event`, member | "event" |
| A declaration of `<script module>` | its usual kind, member, exported when the script exports it | its usual word |
| A top-level declaration of the instance script | its usual kind, member, not exported | its usual word |

1. A component takes its name from its file, as `svelte2tsx` writes it:
   `Button.svelte` gives `Button`, `+page.svelte` gives `Page`. Its location
   spans the whole file.
2. Props come from the type of what the component takes: the type of
   `$props()` in Svelte 5, or the `export let` declarations of a legacy
   component. Each prop's type stays as written where its declaration
   annotates it. A default value in the `$props()` destructuring or in
   `export let` fills `default`.
3. A prop's doc comment fills its text in the component's `doc.params`.
4. The `<!-- @component -->` comment gives the component's doc. `svelte2tsx`
   carries it as JSDoc, so it keeps its Markdown, code blocks included.
5. `signature.text` shows the component as a tag: `<Button {label}
   {primary} />`.
6. A variable that a rune declares shows its own type: `let n = $state(0)`
   has the type `number`. Names that start with `$` stay out, because
   Svelte keeps them for itself: store reads and the code that `svelte2tsx`
   adds.
7. A legacy event takes the line of its `dispatch("name")` call, or of the
   `on:name` that forwards it. Its type comes from `svelte2tsx`, and a doc
   comment in the dispatcher's type gives its doc.

The viewer needs no Svelte words: it shows `languageKind` from the data.

#### Edges

1. A component tag renders that component. `svelte2tsx` writes
   `<Button />` as a call of `__sveltets_2_ensureComponent(Button)`, which
   the adapter reads as an exact `calls` edge from the component around the
   tag to `Button`, from the line of the tag. The maintainer chose `calls`
   over a new edge kind for every render.
2. Calls in the markup and in event handlers count for the component, as a
   call in a nested function counts for its declaration. A call inside a
   snippet counts for the snippet, and `{@render badge()}` calls the
   snippet.
3. Every other edge follows the TypeScript rules above.

#### The `sveltekit` plugin

The plugin reads routes from file names under `src/routes`; its option
`routes` names another folder. It never runs `svelte.config.js` or
`vite.config.js`.

1. A route's path joins its folders under `routes`. `(group)` folders drop
   out. `[param]`, `[[optional]]`, `[...rest]` and `[id=matcher]` stay as
   written: `src/routes/(app)/blog/[slug]` gives `/blog/[slug]`.
2. A `+server.ts` or `+server.js` module makes an `http` entry for each
   `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `OPTIONS` and `HEAD` that it
   exports, with that method. `fallback` makes `ANY`.
3. A page (`+page.svelte`, with an optional `+page.ts` and
   `+page.server.ts`) makes one entry, `GET <path>`. Its handler: the `load`
   of `+page.server.ts`, else the `load` of `+page.ts`, else the component.
4. The form actions of `+page.server.ts`: `default` makes `POST <path>`;
   a named action makes `POST <path>?/<name>`. An inline action at the top
   level becomes a function symbol named after its route, such as
   `POST /blog/[slug]?/comment`, as in the Express and Hono plugins.
5. Layouts make no entry of their own: a layout answers no request alone.
   SvelteKit renders each page inside every `+layout.svelte` from `routes`
   down to the page's folder, and the page never names its layout. No
   checker resolves that use, so the plugin records it as edges with
   `confidence: "heuristic"`, each from the user to the thing used:
   - a `calls` edge from each layout component to the nearest layout or
     page below it, from the line of its `{@render children()}` or
     `<slot>`;
   - a `calls` edge from a `load` that calls `parent()` to the `load` of
     each layout around it. A `+page.server.ts` or `+layout.server.ts` load
     points at the `+layout.server.ts` loads. A `+page.ts` or `+layout.ts`
     load points at the `+layout.ts` loads, and at a layout's
     `+layout.server.ts` load where that layout has no `+layout.ts` load.
6. The layouts around a node follow SvelteKit's own rule, including its
   reset syntax: `+page@<segment>.svelte` and `+layout@<segment>.svelte`
   skip the layouts up to the folder of that name, and `+page@.svelte`
   resets to the root layout. A `(group)` folder counts as a segment for a
   reset, but not for the path.
7. `hooks.server.ts`, `+error.svelte` and param matchers make no entries.

#### Limits

The output's notes list these limits, and the help text repeats them:

1. The components come from the adapter's `svelte2tsx` and `svelte`, not
   from the project's own Svelte version. A component that a newer syntax
   breaks adds a note and stays out of the map; the others carry on.
2. A render of a component that a variable holds, such as
   `<svelte:component this={x}>`, and a render of a snippet that a prop
   passes in, such as `{@render children()}`, do not appear.
3. The layout and `parent()` edges count as heuristic: they follow
   SvelteKit's file conventions, not a checker.
4. Without an install and `svelte-kit sync`, `./$types`, `$app/*` and
   `$env/*` resolve to nothing, so the types they bring count as unknown.

### Rust (after TypeScript)

Rust has no stable compiler API for this work, so the adapter will combine
two tools: rustdoc's JSON output (items, docs as Markdown, trait
implementations; it needs a pinned nightly toolchain, and its format carries a
version number) and rust-analyzer's SCIP index (references, from which calls
follow: a reference inside a function's body counts as a call). A Rust
program will hold the adapter itself.

### Other languages

The plan names Python and C# after these. Where no native adapter exists, a
tree-sitter adapter written in Rust can give symbols and names, with calls
marked `heuristic`.
