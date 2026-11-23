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
   `heuristic` when the adapter matched names.
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
every edge, so every edge counts as `exact`. On immer v11.2.9 and Hono
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

A plugin knows a receiver by its type (the package's own types, when the
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
