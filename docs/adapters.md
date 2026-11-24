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
every edge, so in TypeScript every edge counts as `exact`; only the layout
edges of the `sveltekit` plugin, below, count as `heuristic`. JavaScript
marks some calls `heuristic`; see "JavaScript" below. On immer v11.2.9 and Hono
v4.13.17 the counts sit in `examples/immer/counts.json` and
`examples/hono/counts.json`.

#### What it reads

1. The program comes from `tsconfig.json` in the root, or from the file that
   `options.tsconfig` names. A solution-style tsconfig (no files, only
   `references`) gives the files of every project it references. Without a
   tsconfig, it reads `jsconfig.json`; without either, it reads every
   TypeScript and JavaScript file under the root.
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
parses the components, sit pinned in `adapters/typescript/package.json`. Two
public apps hold the count snapshots. sveltejs/realworld at commit df79670
(`examples/realworld/counts.json`) covers pages, form actions, loads that
call `parent()`, and layouts, in JavaScript. WarningImHack3r/svelte-changelog
at commit c100d72 (`examples/svelte-changelog/counts.json`) covers SvelteKit
3, `+server` methods, param matchers, and code with doc comments.

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
   exports, with that method. `fallback` makes `ANY`. An export that a
   factory builds, such as `export const GET = rssHandler(…)`, takes as its
   handler the first project function that its initializer names, as in the
   Express and Hono plugins. The same rule holds for a `load` and a form
   action.
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

### JavaScript (done)

JavaScript runs as a mode of the TypeScript adapter, not as an adapter of
its own. The compiler API binds JavaScript by itself: with `allowJs` it reads
CommonJS exports, `require` calls, prototype assignments, `this.x = …`
properties and the JSDoc type tags, and the checker resolves calls through
all of them. `adapters/typescript/src/javascript.ts` adds the symbols that
JavaScript writes as assignments and in JSDoc. The config keeps
`adapter: typescript`, so `express` and `hono` serve both languages.
`docs/plans/phase-5-javascript.md` records the design; issue #25 holds the
discussion. On commander.js v14.0.3 the counts sit in
`examples/commander/counts.json`; the fixture's sit in
`examples/lending-js/counts.json`.

#### The mode

1. A run reads JavaScript when its map holds no TypeScript file and its root
   holds no `.svelte` file. The option `language` (`js` or `ts`) picks the
   mode by hand.
2. In the JavaScript mode, every ID and container carries the tag `js`:
   `js:lending-js/lib/domain/book#Shelf.add`. `meta.languages` holds
   `["js"]`.
3. A mixed project makes one run in the TypeScript mode. Its IDs start with
   `ts:`. Each file container of a `.js`, `.jsx`, `.mjs` or `.cjs` file
   carries `language: "js"`, and `meta.languages` lists both tags. The
   JavaScript rules below hold for those files too.
4. `checkJs` stays off. It only asks the compiler to report errors; binding
   and inference stay the same. On the fixture and on commander, a run with
   `checkJs` on writes the same bytes in the same time.
5. Without a config, module resolution follows `Bundler`: it reads
   `require` and `import`, paths without extensions, `index` files and the
   `exports` of `package.json`, so it covers Node, bundlers and Bun. The
   option `moduleResolution` (`bundler`, `node16`, `nodenext` or `node10`)
   picks another one. A config's own setting wins.

#### Files

| File | `languageKind` |
| --- | --- |
| A file with an `import` or `export` statement | "module" |
| A file that uses `require`, `module.exports` or `exports` alone | "CommonJS module" |
| Any other file | "script" |

#### Symbols

The TypeScript table holds, with these additions:

| JavaScript | `kind` | `languageKind` |
| --- | --- | --- |
| A function that `@constructor`, `this.x = …` or a prototype assignment marks as a class | `class` | "constructor function" |
| Its body, which `new` reaches | `constructor`, member | "constructor" |
| `F.prototype.m = function …`, or a method of `F.prototype = { … }` | `method`, member | "method" |
| `F.m = function …` | `method`, member, `static` | "function" |
| `this.x = …` in a constructor, `F.x = …` with another value | `property`, member | "property" |
| A `const` object literal that holds a function | `namespace` | "object" |
| A function of such an object | `method`, member | "method", "function" or "arrow function" |
| `exports.f = function …`, or a function in `module.exports = { … }` | `function` | "function" or "arrow function" |
| `module.exports = function …` | `function`, named after the function, else `default` | "function" |
| `exports.x = …` with another value | `variable` | "export" |
| `@typedef`, and each of its `@property` tags | `type_alias`, and `property` members | "typedef", "property" |
| `@callback` | `type_alias`, with a signature | "callback" |

1. `const x = require("x")` imports, as an `import` statement does, so it
   declares no symbol. Nor does `@typedef {import("./x").Y} Y`: it resolves
   to `Y`.
2. A declaration counts as exported when the module exports it in any form:
   `export`, `exports.x = x`, `module.exports = x`, or a name in
   `module.exports = { x }`.
3. A member counts as exported unless JSDoc marks it `@private` or
   `@protected`, or its name starts with `#` or `_`. A leading `_` marks a
   private member by convention in JavaScript.
4. `this.x = …` adds a member only in a constructor, unless the class
   already declares that name.
5. In a project with `.svelte` files, an object literal keeps the
   TypeScript rule and stays a constant. SvelteKit writes form actions as
   `export const actions = { … }`, and the `sveltekit` plugin names an
   inline action after its route.

#### Types and edges

1. Types come from JSDoc, as written: `@param {T}`, `@returns {T}`,
   `@type {T}`, `@template T`, and the tags of `@typedef` and `@callback`.
   `[name]` and `T=` mark a parameter optional; `...T` marks it rest.
   Without a tag, the checker infers the type.
2. `field_type`, `param_type` and `return_type` edges follow from those
   types. `import("./x").T` and `@import` resolve too.
3. `require("x")` and `import("x")` with a literal add `imports` edges, or
   go to `externalImports`, from anywhere in a file.
4. `new F()` and `F()` on a constructor function call `F.constructor`.
5. `extends` comes from an ES class's clause, and from `@augments` (or
   `@extends`) and `@implements` tags.
6. The adapter never matches names: a call that the checker cannot resolve
   adds no edge, and a note counts the calls that name a symbol of the map
   but resolve to nothing. A call that the checker resolves counts as
   `exact` when declarations fix its callee: a declared function or an
   import, `new`, `this` or `super`, or a method on a receiver whose type a
   class, a `const`, a JSDoc type, an import or a declared result fixes.
   Otherwise it counts as `heuristic`. JavaScript lets a variable take a
   value of another type later, so a callee that the checker finds through
   an inferred type may not be the one that runs: in
   `let x = new A(); if (f) x = new B(); x.m()`, the map shows only
   `A.m`, as `heuristic`. Every other edge counts as `exact`.
7. The `express` and `hono` plugins follow `require`: `const express =
   require("express")` marks `express()` as an app, and
   `app.use("/books", require("./books"))` mounts the router that
   `books.js` exports with `module.exports = router`.

#### Limits

The output's notes list these limits, and the help text repeats them:

1. A call through a value of unknown type does not appear. In JavaScript
   without JSDoc, a parameter's type often stays unknown.
2. Mixins, `util.inherits`, `Object.create` and other prototype changes at
   run time add no members and no edges.
3. A property that code adds outside a constructor does not appear, and
   neither does an export under a computed name (`exports[name] = …`).
4. The checker binds a member that an assignment adds to a non-empty object
   literal (`rules.mayBorrow = function …`) only as a property of that
   literal's file. The map shows the member, but calls to it from elsewhere
   do not resolve.
5. A call through a receiver whose type the checker infers counts as
   `heuristic`, and points at one callee only.
6. A call through a receiver of a union type, such as `@param {A|B} x`,
   points at one member of the union only, as in TypeScript. JSDoc unions
   turn up often in JavaScript.
7. A package whose `package.json` names `types` resolves, for code that
   imports it by package path, to those declarations. Commander's tests
   import `../`, so their uses reach `typings/index.d.ts`, which stays out of
   the map; the test index holds few of them.

### Rust (done)

The Rust adapter, `adapters/rust`, runs as its own program,
`acu-adapter-rust`. Rust has no stable compiler API for this work, so the
adapter combines two tools from one pinned nightly toolchain,
`nightly-2026-11-01`:

1. rustdoc's JSON output gives the crates, modules, items, docs and trait
   implementations. The adapter reads it with `rustdoc-types` 0.61.0, which
   matches that nightly's format 61, and fails with the toolchain to use when
   a file carries another format.
2. rust-analyzer's SCIP index gives the references. A reference to a function
   inside another function's body counts as a call.

The adapter itself builds on the stable toolchain, as part of the Cargo
workspace; it picks the nightly for each child process through
`RUSTUP_TOOLCHAIN`. Its only dependency beyond `serde`, `serde_json` and
`anyhow`, which `acu` already uses, is `rustdoc-types`. A protobuf decoder for
the SCIP index, the Markdown normaliser and a small lexer for the plugins
live in the adapter. Install the toolchain once:

```sh
rustup toolchain install nightly-2026-11-01 --profile minimal --component rust-src,rust-analyzer
```

#### What it reads

1. `cargo metadata` lists the workspace members. The map holds each member's
   library, binaries and procedural macros; examples, tests and benches stay
   out. `include` and `exclude` match a package's folder and each source
   file. The option `packages` names the packages to keep.
2. `cargo rustdoc` runs on each target with `--document-private-items`, so
   the map shows private items too. rustdoc builds each crate's dependencies,
   so cargo may fetch them, as `go list` does for the Go adapter.
3. `rust-analyzer scip` indexes the whole workspace once.
4. Build output goes to `acu` inside the workspace's target folder, so a run
   never disturbs the project's own build and keeps its cache. The option
   `targetDir` names another folder; `features` and `allFeatures` pick the
   Cargo features.
5. rustdoc never sees code under `#[cfg(test)]`, so such code stays out of
   the map. SCIP sees it; the tests come from there.

#### Containers and IDs

| Container | ID | `kind`, `languageKind` |
| --- | --- | --- |
| A crate: one library, binary or procedural-macro target | `rs:crate:<crate>` | `package`, "crate" |
| A module: the unit | `rs:mod:<crate>::<path>`, or `rs:mod:<crate>` for the root | `module`, "module" |
| A file | `rs:file:<path>` | `file`, "file" |

1. A crate takes its target's crate name, such as `grep_searcher`. A binary
   whose crate name a library of the map already takes gains `@bin`, as in
   `rs:crate:lending@bin`.
2. Modules stay flat under their crate, as Go packages sit under their
   module. A module takes its path as its name, such as
   `grep_searcher::sink`, and its file as its path.
3. A file module (`mod sink;`) owns a file container, which shares the
   module's `//!` doc. An inline module (`mod rules { … }`) has no file of its
   own; its items name the module as their parent.
4. A symbol ID joins the module path and the symbol's path:
   `rs:lending::library#Library.borrow`. Two items of one name in one module,
   such as a field and a method, gain `@<file>:<line>`. Methods come first,
   so a getter keeps the plain ID.
5. A member of a trait implementation adds the trait to its path, so that
   `Display::fmt` and `Debug::fmt` stay apart:
   `rs:lending::domain#Book.Display::fmt`. Its `name` stays `fmt`, so the
   viewer's trace finds it under the trait's method.
6. One crate's types link to another crate's items through rustdoc's paths,
   which name each item where the code defines it.

#### Symbols

| Rust | `kind` | `languageKind` |
| --- | --- | --- |
| `struct` | `struct` | "struct" |
| `union` | `struct` | "union" |
| `enum` | `enum` | "enum" |
| A variant | `constant`, member | "variant" |
| `trait` | `trait` | "trait" |
| A trait alias | `type_alias` | "trait alias" |
| `fn` | `function` | "function" |
| A function in an `extern` block | `function` | "foreign function" |
| A method with `self` | `method`, member | "method" |
| An associated function without `self` | `method`, member | "associated function" |
| A field | `field`, member | "field" |
| An associated type | `type_alias`, member | "associated type" |
| An associated constant | `constant`, member | "associated constant" |
| `type X = …` | `type_alias` | "type alias" |
| `const` | `constant` | "constant" |
| `static` | `variable` | "static" |
| `macro_rules!` | `macro` | "macro" |
| A procedural macro | `macro` | "derive macro", "attribute macro" or "function-like macro" |
| An `impl` of a local trait for an outside type | `namespace` | "impl" |

1. An `impl` block adds its items to its type, wherever it sits in the
   crate. An `impl` of a local trait for an outside or generic type has no
   local type to join, so it becomes an item of its own, named as written:
   `impl Shelved for Vec<Book>`.
2. Derived implementations add no members. Their traits go to one
   `#[derive(…)]` annotation and to `satisfies`. Other annotations come from
   rustdoc: `#[repr]`, `#[non_exhaustive]`, `#[must_use]`, `#[macro_export]`.
3. `satisfies` lists the outside traits that a type implements, by name with
   their arguments: `Debug`, `Display`, `From<String>`. Auto traits and
   blanket implementations stay out.
4. `exported` holds `true` for `pub` items. Variants, trait members and
   members of trait implementations take their owner's value. rustdoc writes
   `pub(crate)` and a private item at the crate root alike, so neither counts
   as exported.
5. Modifiers name `async`, `const`, `unsafe`, `extern "<abi>"` and `mut`.
6. A variant's fields give its type, as written: `(BookId)` or
   `{ book: BookId, due: u32 }`. They make no symbols of their own.
7. Types show each path by its last segment, as rustdoc's pages do:
   `BTreeMap<BookId, Book>`. `refs` link the rest.

#### Docs

1. The format is `rustdoc`. The text holds Markdown, which turns into blocks
   and the inline subset, as in the TypeScript adapter.
2. Intra-doc links (`[Store]`, `` [`Store::find`] ``, `[text](Store)`, and
   reference links) resolve through rustdoc's own link table, so they become
   `sym:` links to symbols and to modules.
3. A code block without a language, or with only rustdoc's test words
   (`ignore`, `no_run`, `should_panic` and so on), counts as Rust. Its hidden
   lines (`# use …;`) drop out, as rustdoc hides them.
4. `#[deprecated]` fills `deprecated`.
5. A member of a trait implementation without a doc of its own takes the
   doc of the trait's member, as rustdoc's pages show it. That works for the
   map's own traits; an outside trait's docs are not in the JSON.

#### Edges

1. `calls`: a reference to a function or method inside the body of another
   one, from the SCIP index. The innermost function or method whose span
   holds the reference makes the call. A reference that does not call, such
   as `map(Self::check)`, counts as well, as issue #6 decided.
2. A call through a trait object or a generic bound points at the trait's
   method. The viewer follows `implements` edges to the code that can run.
3. `implements`: a type, or an `impl` item, to a local trait.
4. `extends`: a trait to a local supertrait.
5. `field_type`, `param_type`, `return_type`: from the types in rustdoc's
   signatures, which name each item by ID. `Self` adds no edge.
6. `imports`: a module to each other module whose items its code names,
   from the SCIP references. Outside packages go to `externalImports`; the
   standard library stays out.

Every edge counts as `exact`: rustdoc or rust-analyzer resolves each one.

#### Tests

A function with an attribute named `test`, such as `#[test]` or
`#[tokio::test]`, makes a test. SCIP finds it, with the attribute inside the
function's range. Its name joins its module path and its own name, as
`cargo test` prints it: `store::memory::tests::finds_a_saved_book`. Each
reference in its body records the test against the symbol it names.

#### Entry-point plugins

Neither rustdoc nor SCIP writes a syntax tree, so the plugins read tokens
inside each function, and SCIP resolves the names among them.

1. `axum` finds `.route("/books/{id}", get(show).delete(remove))`: one
   `http` entry per method. `route` must resolve to axum's `Router`, and each
   method helper to axum's. `any(h)` and `route_service` give `ANY`. The
   handler: the first function of the map that the handler expression names;
   a closure keeps the registering function and counts as inline; anything
   else fills `externalHandler`. `.nest("/api", api())` puts `/api` in front
   of the routes that `api` registers; the argument may also name a local
   variable or hold a router inline. `.merge(other())` adds no prefix. The
   option `prefixes` covers a mount that the plugin cannot see.
2. `clap` reads the derive API. A struct that derives `Parser` names the
   program: `#[command(name = "…")]`, else the binary's name. Each variant of
   a `Subcommand` enum that a `#[command(subcommand)]` field names makes a
   command: `#[command(name = "…")]`, else the variant's name in kebab case.
   A variant whose arguments hold subcommands adds those instead, so names
   join: `lendd report count`. A command's handler is the function that its
   `match` arm (or `if let` block) calls, when the arm holds that one call
   alone; an arm that does more serves the command inline, in the function
   that matches. A `Parser` without subcommands
   makes one entry, served by the function that calls `parse`.

The fixture holds stand-ins for axum and clap in `testdata/fixture/third_party`,
with the shapes of their APIs, so the tests need no network.

#### Limits

The output's notes list these limits, and the help text repeats them:

1. A reference to a function inside another function's body counts as a
   call, even where the code passes the function on without calling it.
2. A call through a trait object or a generic bound points at the trait's
   method.
3. Calls inside code that a macro writes may count for the wrong function,
   or not at all.
4. Items under `#[cfg(test)]`, and code for other platforms or features, do
   not appear.

The plugins read tokens, so an unusual registration may escape them. clap's
builder API (`Command::new("x")`) stays out, and so do axum's `fallback` and
`nest_service`.

### Other languages

The plan names Python and C# after these. Where no native adapter exists, a
tree-sitter adapter written in Rust can give symbols and names, with calls
marked `heuristic`.
