# Atlas Carries Us data format, version 1

This document specifies `atlas.json`, the file that adapters write and the
viewer reads. The JSON Schema in `schema/atlas.schema.json` holds the same
rules in machine-checkable form. When the two disagree, the JSON Schema wins,
and the disagreement counts as a bug in this document.

## 1. Purpose and principles

1. One format describes code in any language. Language words such as
   "struct", "trait" or "module" appear only as data (`languageKind`), never as
   structure.
2. A type checker or compiler produces the facts. Each edge states how far a
   reader may trust it (`confidence`).
3. IDs come from qualified names, so a link to a symbol keeps working after a
   rebuild.
4. Every edge points from the user to the thing it uses.
5. Docs appear twice: as written (`raw`) and in a normal form that the viewer
   draws without knowing the doc-comment style.
6. Human-written explanation (regions, tour, project description) lives beside
   the facts, never mixed into them. Text that a program drafted carries a
   `drafted` mark.

## 2. Top level

| Field | Required | Holds |
| --- | --- | --- |
| `version` | yes | The number `1`. |
| `meta` | yes | Tool, adapters, commit and generation time. See section 4. |
| `project` | no | Name, repository, link templates, overview text. See section 5. |
| `containers` | yes | Modules, packages, namespaces, directories and files. See section 6. |
| `symbols` | yes | Types, functions, members, constants and variables. See section 7. |
| `edges` | yes | Relations between symbols or containers. See section 9. |
| `entryPoints` | no | HTTP routes, CLI commands and other ways in. See section 10. |
| `tests` | no | Tests that use each symbol. See section 11. |
| `regions` | no | Named groups of units for the overview. See section 12. |
| `layout` | no | Placement of regions on the overview. |
| `tour` | no | Guided steps. See section 13. |

An adapter writes `version`, `meta`, `containers`, `symbols`, `edges`, and,
where it finds them, `entryPoints` and `tests`. The `acu` command adds
`project`, `regions`, `layout` and `tour` from `atlas.yaml`.

## 3. IDs

An ID names one thing and stays the same across runs while the code keeps its
names. The viewer uses IDs in links (`#symbol=<id>`), so a change of ID breaks
old links.

1. Every ID starts with a language tag and a colon: `go:`, `ts:`, `rs:`, and
   so on.
2. Container IDs add a kind tag: `<lang>:<kind>:<name or path>`. Examples:
   `go:mod:github.com/go-chi/chi/v5`, `go:pkg:github.com/go-chi/chi/v5/middleware`,
   `go:file:middleware/logger.go`.
3. Symbol IDs join the unit's qualified name and the symbol's path inside the
   unit with `#`: `go:github.com/go-chi/chi/v5#Mux.ServeHTTP`. A member's path
   extends its parent's path with a dot.
4. Where two declarations share a name (for example two `init` functions in
   Go), the adapter adds `@<file>:<line>` to keep the IDs apart.
5. Entry point IDs use the plugin's kind and name: `http:GET /posts/{id}`,
   `cli:serve`. Duplicates gain `@<file>:<line>`.
6. All IDs in one file differ from each other. The `acu` command rejects a
   merged file with a duplicate ID.

## 4. `meta`

| Field | Holds |
| --- | --- |
| `schemaVersion` | The number `1`. |
| `tool` | `{ name, version }` of the program that wrote the final file. |
| `generated` | Time of the run, RFC 3339, UTC. |
| `commit` | The commit the adapters read, when known. |
| `languages` | Languages present, for example `["go"]`. |
| `adapters` | One entry per adapter run: `name`, `version`, `language`, `root`, and `notes` that list the limits of the run in plain words. |

## 5. `project`

| Field | Holds |
| --- | --- |
| `name` | The project's name, shown in the header. |
| `repo` | `owner/name` on the code host. |
| `links` | URL templates `file`, `line`, `range` and `folder`. The placeholders `{repo}`, `{commit}`, `{path}`, `{line}` and `{endLine}` take their values from the project, `meta` and the location. |
| `description` | Overview text in the inline subset (section 8.3). Blank lines split paragraphs. |
| `descriptionDrafted` | A provenance mark (section 14) when a program drafted the description. |
| `autolinks` | Rules that turn plain text in docs into links: a JavaScript regular expression `pattern` and a `url` with `$1`…`$9` for groups and `{repo}`, `{commit}`. |
| `entryPointNotes` | Notes shown beside matching entry points: an optional `kind`, an optional `pathPrefix`, an optional `title`, and `text`. |
| `excluded` | Path patterns that the adapters left out, shown in the help text so readers know what the map omits. |

## 6. Containers

Containers form a tree through `parent`. A typical Go tree runs module →
package → file. A typical TypeScript tree runs directory → file.

| Field | Required | Holds |
| --- | --- | --- |
| `id` | yes | See section 3. |
| `kind` | yes | `module`, `package`, `namespace`, `directory` or `file`. |
| `languageKind` | yes | The language's own word, for example "package" or "crate". |
| `name` | yes | A short display name, for example `middleware`. |
| `path` | no | Path relative to the repository root, with forward slashes. |
| `parent` | no | The enclosing container. |
| `language` | no | The language tag. |
| `unit` | no | `true` for containers that the overview draws as boxes. A Go package, a TypeScript directory and a Rust module each count as a unit. |
| `doc` | no | The container's docs (section 8). |
| `externalImports` | no | Outside libraries that the container imports. |

Every symbol belongs to exactly one unit: the nearest container with
`unit: true` above it.

## 7. Symbols

Symbols cover everything that code declares and a reader may want to find:
types, functions, and their members. A member (method, field, property,
constructor) has its own symbol, whose `parent` names the owning symbol.

| Field | Required | Holds |
| --- | --- | --- |
| `id` | yes | See section 3. |
| `kind` | yes | One neutral kind from the list below. |
| `languageKind` | yes | The language's own word, shown to readers. |
| `name` | yes | The declared name. |
| `parent` | yes | The owning symbol, or the container (usually a file) for a top-level symbol. |
| `language` | no | The language tag. |
| `exported` | yes | `true` when code outside the unit may use it. |
| `location` | yes | `{ path, line, endLine }`. |
| `doc` | no | Section 8. |
| `typeParams` | no | Type parameters of a generic type, as written, for example `[T any]`. |
| `signature` | no | For callables: `typeParams`, `params`, `results` and optional `text`. Each parameter holds `name`, `type`, `refs`, and optional `optional`, `rest` and `default`. |
| `type` | no | `{ text, refs }`: the declared type of a field, property, constant or variable, or the type underneath a named type or alias. |
| `value` | no | The value of a constant, cut to 80 characters. |
| `group` | no | A key that declarations written together share, unique within one file (for example one Go `const ( … )` block). |
| `modifiers` | no | Words such as `static`, `async`, `abstract`, `readonly`, `embedded`, `pointer receiver`. |
| `annotations` | no | Decorators, attributes or field tags, as written. |
| `satisfies` | no | Well-known outside interfaces that the symbol satisfies, for example `error` or `fmt.Stringer`. |

Neutral kinds: `class`, `struct`, `interface`, `trait`, `enum`, `type` (a
named type over another type), `type_alias`, `function`, `method`,
`constructor`, `field`, `property`, `constant`, `variable`, `event`, `macro`,
`namespace`.

A `refs` list names the IDs of symbols that a type string mentions, so the
viewer can link `[]*Route` to `Route`.

## 8. Docs

### 8.1 Fields

| Field | Required | Holds |
| --- | --- | --- |
| `raw` | yes | The comment as written, without comment markers. |
| `format` | yes | `godoc`, `jsdoc`, `tsdoc`, `xmldoc`, `rustdoc`, `docstring-google`, `docstring-numpy`, `docstring-sphinx`, `markdown` or `plain`. |
| `summary` | yes | The first sentence, in the inline subset. |
| `description` | yes | A list of blocks (section 8.2). |
| `params`, `typeParams` | no | Maps from a parameter's name to its text. |
| `returns` | no | Text about the result. |
| `throws` | no | A list of `{ type, text }`. |
| `examples` | no | A list of `{ title, code, language }`. |
| `seeAlso` | no | A list of inline texts. |
| `deprecated` | no | Present when the item counts as deprecated. It may hold an empty string. |
| `since` | no | A version string. |
| `tags` | no | Other block tags as `{ name, text }`. |

The viewer reads only the normal form. It shows `raw` on request.

### 8.2 Blocks

| `type` | Fields | Meaning |
| --- | --- | --- |
| `paragraph` | `text` | Running text. The adapter joins wrapped lines with spaces. |
| `heading` | `level`, `text` | A heading inside a doc. |
| `list` | `ordered`, `items` | A list. Each item holds inline text. |
| `code` | `code`, `language` | A block of code, kept as written. |
| `quote` | `text` | A quoted passage. |

### 8.3 The inline subset

Inline text uses a small, fixed markup, so that every language's links and
code spans survive:

1. `` `code` `` marks code.
2. `[label](https://…)` links to a web page.
3. `[label](sym:<id>)` links to a symbol or container by ID.
4. `*emphasis*` and `**strong**` mark emphasis.
5. A backslash escapes the next character. Adapters escape `\`, `` ` ``, `*`,
   `[` and `]` in plain text.

The viewer escapes all HTML and renders nothing else.

## 9. Edges

| Field | Required | Holds |
| --- | --- | --- |
| `from` | yes | The user. |
| `to` | yes | The thing used. |
| `kind` | yes | One kind from the table below. |
| `confidence` | yes | `exact` when a type checker resolved the edge. `heuristic` when the adapter matched names. The viewer draws heuristic edges in a different style and says so in the legend. |
| `line` | no | The first line of the use, in the file of `from`. For calls, the first call site; calls inside one function sort in source order by it. |
| `count` | no | How many places make the same use. |

| Kind | From | To | Reads as |
| --- | --- | --- | --- |
| `calls` | function, method or constructor | function, method or constructor | "calls" |
| `implements` | a type | an interface or trait | "implements" |
| `extends` | a type | the type it extends or embeds | "extends" |
| `field_type` | a field or property | a type it names | "has a field of" |
| `param_type` | a callable | a type that a parameter names | "takes as a parameter" |
| `return_type` | a callable | a type that a result names | "returns" |
| `imports` | a container | a container | "imports" |
| `overrides` | a method | the method it overrides | "overrides" |

A call through an interface points at the interface's method. The viewer
follows `implements` edges to show the code that can run.

## 10. Entry points

| Field | Required | Holds |
| --- | --- | --- |
| `id` | yes | See section 3. |
| `kind` | yes | `http`, `cli`, `message`, `job` or `grpc`. |
| `name` | yes | For example `GET /posts/{id}`. |
| `method`, `path` | no | HTTP method and path. |
| `handler` | yes | The ID of the function or method that serves it. |
| `inline` | no | `true` when the handler sits inline in the registering code; `handler` then names the registering function. |
| `externalHandler` | no | The handler, when it lives outside the code base (for example `pprof.Index`). `handler` then names the registering function. |
| `registrar` | no | The function that registers it. |
| `registeredAt` | yes | Where the registration happens. |
| `plugin` | yes | The plugin that found it, for example `go-chi`. |

## 11. Tests

`tests` maps a symbol ID to the tests that use it: `{ name, location }`. A use
of a member records the member's ID. The viewer rolls member uses up to the
owning type.

## 12. Regions and layout

A region groups units by their job.

| Field | Required | Holds |
| --- | --- | --- |
| `id` | yes | Letters, digits, `_` and `-`. |
| `name` | yes | The region's name. Name it by purpose, not by folder. |
| `color` | no | An index from 0 to 8 into the viewer's colour-blind-safe palette. |
| `containers` | yes | The unit IDs in the region. |
| `description` | no | What the region does. |
| `readingTip` | no | How to read it. |
| `drafted` | no | A provenance mark. |

`layout.rows` lists region IDs row by row, top to bottom.

## 13. Tour

Each step holds `title`, `text` (inline subset), `open` and an optional
`drafted` mark. `open` names a view:

1. `{ "view": "overview" }`
2. `{ "view": "region", "id": "<region id>" }`
3. `{ "view": "unit", "id": "<container id>" }`
4. `{ "view": "symbol", "id": "<symbol id>" }`
5. `{ "view": "trace", "entryPoint": "<entry point id>" }`
6. `{ "view": "trace", "symbol": "<symbol id>" }`

## 14. Provenance

A `drafted` mark holds `by` (the program or model) and `at` (a time). The
viewer labels marked text as machine-drafted. A person who checks the text
removes the mark from `atlas.yaml`.

## 15. Versioning

A change that only adds optional fields keeps `version: 1`. A change that
removes or renames a field, or changes its meaning, raises the version. The
viewer refuses a file whose version it does not know, and says so.
