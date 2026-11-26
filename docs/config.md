# atlas.yaml

`atlas.yaml` tells `acu` what to read and how to present it. It lives beside
the code it describes, or anywhere else: every relative path in it resolves
against the folder that holds it. `schema/config.schema.json` holds the same
rules for editors and for `acu check`.

The files in [`examples/`](../examples) show complete configs.

## Top level

| Field | Required | Holds |
| --- | --- | --- |
| `version` | yes | The number `1`. |
| `project` | yes | Name, repository and overview text. |
| `languages` | yes | One entry per adapter run. |
| `entryPoints` | no | Plugins that find HTTP routes, CLI commands and other ways in. |
| `regions` | no | Groups of units for the overview. |
| `layout` | no | Where the regions sit. |
| `tour` | no | Guided steps. |

## project

```yaml
project:
  name: chi, an HTTP router for Go
  repo: go-chi/chi
  links: github          # github, gitlab, none, or templates (below)
  description: |
    Overview text. A blank line starts a new paragraph.
  autolinks:
    - pattern: "(?<![\\w/&])#(\\d+)\\b"
      url: "https://github.com/{repo}/issues/$1"
```

1. `links` turns locations into source links. The presets cover GitHub and
   GitLab. For another host, give templates with the placeholders `{repo}`,
   `{commit}`, `{path}`, `{line}` and `{endLine}`:

   ```yaml
   links:
     file: "https://dev.azure.com/org/proj/_git/{repo}?version=GC{commit}&path=/{path}"
     line: "https://dev.azure.com/org/proj/_git/{repo}?version=GC{commit}&path=/{path}&line={line}"
   ```

2. `autolinks` turn plain text in doc comments into links. `pattern` holds a
   JavaScript regular expression; `url` may use `$1` to `$9` and `{repo}`.
3. `descriptionDrafted` marks a description that a program wrote (see
   "Drafted text" below).

## languages

```yaml
languages:
  - adapter: go
    root: ../../corpus/go/chi   # default: this file's folder
    include: ["internal/**"]    # package folders to keep
    exclude: ["**/features/**"] # package folders to leave out
    options:
      buildTags: [grafeo]       # Go: build tags
  - adapter: typescript
    root: ../web
    exclude: ["**/generated/**"] # folders to leave out
    tests: ["**/*.test.ts"]      # files that hold tests (default below)
    options:
      tsconfig: tsconfig.build.json # TypeScript: the tsconfig to read
  - adapter: rust
    root: ../server              # a Cargo workspace or package
    exclude: ["examples/**"]     # package folders and files to leave out
    options:
      packages: [server, server-core] # Rust: the packages to map (default: every member)
      features: [postgres]       # Rust: Cargo features to switch on
      allFeatures: false         # Rust: switch on every feature
      toolchain: nightly-2026-11-01 # Rust: the toolchain that runs rustdoc (default: the pinned nightly)
      targetDir: target/acu      # Rust: the folder for build output (default: acu in the target folder)
```

1. `include` and `exclude` match folders: a Go package's folder, or a
   TypeScript folder that holds source files. For Rust they match a
   package's folder, and each source file.
2. `tests` matters to the TypeScript adapter only. Its default,
   `["**/*.test.*", "**/*.spec.*", "**/__tests__/**"]`, covers Vitest and
   Jest. Test files never appear on the map; they fill "Tests that use it".
3. `options.tsconfig` names the TypeScript project file, relative to `root`.
   It defaults to `tsconfig.json`. A tsconfig that only lists `references`
   reads every project it references: this repository's own `atlas.yaml`
   reads the viewer and the TypeScript adapter that way, through
   `tsconfig.atlas.json`.
4. One config may hold several languages. `acu` merges their output into one
   map; [`examples/mixed`](../examples/mixed) holds Go and TypeScript
   together.
5. The Rust adapter runs rustdoc and rust-analyzer from a pinned nightly
   toolchain. Install it once:
   `rustup toolchain install nightly-2026-11-01 --profile minimal --component rust-src,rust-analyzer`.
   rustdoc builds each crate's dependencies, so cargo may fetch them.

`acu` finds the program for each adapter in this order: `command` (a list:
program, then arguments), the variable `ACU_ADAPTER_<NAME>`, a program called
`acu-adapter-<name>` on the `PATH`, and, inside this repository, the adapter's
source. A program called `acu-adapter-<name>` beside `acu` itself also
counts, so `cargo build --release --workspace` gives `acu` its Rust adapter.
For TypeScript, the source needs a build first:
`npm run build -w adapters/typescript`. [`adapters.md`](adapters.md) describes
the protocol.

## entryPoints

```yaml
entryPoints:
  - plugin: go-chi
    options:
      prefixes:
        - { inFunc: Routes, prefix: /api }   # routes registered inside Routes() sit under /api
    notes:
      - pathPrefix: /api
        title: Before the handler runs
        text: Every route under /api passes the Authenticate middleware first.
  - plugin: go-nethttp
  - plugin: cobra
```

The Go adapter offers three plugins, the TypeScript adapter three, and the
Rust adapter two:

1. `go-chi` finds `r.Get("/x", h)`, the other verbs, `Method`, `Handle` and
   `HandleFunc`, and follows `r.Route("/prefix", func(r chi.Router) { … })`.
   A router that the code mounts under a path (with `Mount` or
   `http.StripPrefix`) needs a `prefixes` rule, because the mount happens at
   run time.
2. `go-nethttp` finds `mux.HandleFunc("GET /x", h)` and `http.Handle("/x", h)`.
3. `cobra` finds `&cobra.Command{Use: "serve", RunE: run}`.
4. `express` finds `app.get("/x", h)` and the other verbs,
   `app.route("/x").get(h)`, and follows `app.use("/prefix", router)` across
   files.
5. `hono` finds `app.get("/x", h)` and the other verbs, `app.on(method, path,
   h)`, and follows `app.route("/prefix", sub)` and `basePath("/api")`.
6. `sveltekit` reads the file-based routes of a SvelteKit app: pages,
   `+server` methods and form actions. Its option `routes` names the routes
   folder, by default `src/routes`.
7. `axum` finds `.route("/x", get(h).post(g))` on axum's `Router`, and
   follows `.nest("/prefix", router)` and `.merge(router)` across functions.
8. `clap` reads clap's derive API: a `Parser` struct names the program, and
   each variant of a `Subcommand` enum names a command, as in
   `lendd import-books`.

Every route plugin except `sveltekit` takes `prefixes` for a mount that
happens at run time: the routes that the function `inFunc` registers gain
`prefix`.

`notes` show beside every entry point whose path starts with `pathPrefix`.

## regions

```yaml
regions:
  - id: storage
    name: Storage
    color: 2                 # 0-8, a colour-blind-safe palette; default: by order
    paths: ["internal/store", "internal/store/**"]
    description: One storage contract and two stores that implement it.
    readingTip: Read the contract first; both stores follow it.
  - id: other
    name: Everything else
    default: true            # units that match no region go here
```

1. Name regions by job, not by folder. Readers navigate by these names.
2. `paths` match unit paths (a Go package's folder, a TypeScript folder),
   relative to the repository root or to the language's `root`. A unit joins
   the first region that matches. `src/router/**` matches the folders below
   `src/router`, not `src/router` itself; list both when the region needs
   both.
3. Without any regions, `acu` makes one per top-level folder.
4. In a glob, `[...]` matches one character from a set. To match a folder
   whose name holds `[`, such as a SvelteKit route parameter, write `[[]`
   for the `[`: `src/routes/[[]slug]/**` matches the folders below
   `src/routes/[slug]`.

## layout

```yaml
layout:
  rows:
    - [entry, http]
    - [rules, domain]
    - [storage]
```

Rows run top to bottom. Regions that no row names go into extra rows below.

## tour

```yaml
tour:
  - title: Follow one request
    open: { view: trace, entryPoint: "GET /api/books/{id}" }
    text: The handler calls the service, and the service reads through the store.
```

Each step opens one view:

| `open` | Opens |
| --- | --- |
| `{ view: overview }` | The overview. |
| `{ view: region, id: storage }` | One region. |
| `{ view: unit, path: internal/store }` | A unit focus. `id` works too. |
| `{ view: symbol, name: store.Store }` | An item focus. A member name such as `Mux.Get` opens its owner with the member's docs. `id` works too. |
| `{ view: trace, entryPoint: "GET /x" }` | A trace from an entry point, by name or ID. |
| `{ view: trace, symbol: Command.execute }` | A trace from a function or method. |

A name may need its unit to stay unique: `store.Store` rather than `Store`.
`acu extract` stops with a clear message when a step points at nothing, or at
more than one thing.

Write tour text in short sentences that a newcomer can follow. Start with the
overview, follow one concrete request, then name the pattern.

## Drafted text

Text that a program (for example a language model) wrote carries a mark:

```yaml
  - id: storage
    name: Storage
    description: The store reads and writes books.
    drafted: { by: "a local model", at: "2026-11-22T10:00:00Z" }
```

The viewer labels such text as machine-drafted. A person who has checked the
text removes the `drafted` line. The write tools of the MCP server
([`mcp.md`](mcp.md)) set this mark on every region, description and tour
step that they draft.
