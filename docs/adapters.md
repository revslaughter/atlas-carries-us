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
   every feature, with a golden file that the test compares byte for byte
   (`go test ./... -update` rewrites it after a deliberate change).
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

### TypeScript (next)

The TypeScript compiler API in a Node program, pinned to TypeScript 6.0.3.
TypeScript 7, the native port, offers its programmatic API only as
"unstable" so far; the adapter will move once that API settles. JSDoc and
TSDoc come from the compiler's own comment parser, which also resolves
`{@link}` targets. Planned entry-point plugins: Express and Hono routes.
Planned corpus: `immerjs/immer` and `honojs/hono`.

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
