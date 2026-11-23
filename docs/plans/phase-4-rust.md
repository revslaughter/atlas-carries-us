# Plan: phase 4, the Rust adapter

This plan turns issue #6 into work. Read `AGENTS.md`, `docs/schema.md`,
`docs/adapters.md` and issue #6 first.

## Decisions

1. **Two tools, one pinned nightly.** rustdoc's JSON output gives the items,
   their docs and the trait implementations. rust-analyzer's SCIP index gives
   the references. Both come from one nightly toolchain,
   `nightly-2026-11-01` (rustc 1.101.0-nightly, f9ceb4a4d, 2026-10-31), with
   the components `rust-src` and `rust-analyzer`. That nightly writes rustdoc
   JSON format 61, which `rustdoc-types` 0.61.0 (29 July 2026) reads.
2. **The adapter builds on stable.** `adapters/rust` joins the Cargo
   workspace beside `cli`, so the stable toolchain of `rust-toolchain.toml`
   builds, lints and tests it. The adapter runs the nightly only as a child
   process, through `RUSTUP_TOOLCHAIN`. The option `toolchain` names another
   one. The adapter checks the format version of each JSON file and fails
   with a message that names the toolchain to use.
3. **Dependencies.** `rustdoc-types` 0.61.0 (named in issue #6), and `serde`,
   `serde_json` and `anyhow`, which `acu` already uses. No other crate:
   - a protobuf decoder of about 120 lines reads the SCIP index, in place of
     the `scip` and `protobuf` crates;
   - the Markdown normaliser ports `adapters/typescript/src/docs.ts`, in place
     of `pulldown-cmark`;
   - a small lexer reads the code around a registration for the plugins and
     the test attributes, in place of `syn`;
   - a glob matcher ports `adapters/go/internal/extract/glob.go`.
4. **No change to the data format.** Every Rust word goes to `languageKind`;
   every kind comes from the schema's lists.
5. **Network.** rustdoc builds the dependencies of each crate, and cargo
   fetches them, as `go list` does for the Go adapter. The adapter itself
   makes no network call.
6. **Build output.** cargo and rust-analyzer write to
   `<workspace target dir>/acu`, so a run never touches the project's own
   build and keeps its cache for the next run. The option `targetDir` names
   another folder.

## What the adapter reads

1. `cargo metadata` lists the workspace members. The map holds each member's
   library, binaries and procedural macros. `include` and `exclude` match a
   package's folder or a source file, relative to the root. The option
   `packages` names the packages to keep.
2. For each target, `cargo rustdoc` with `--document-private-items` writes
   one JSON file.
3. `rust-analyzer scip` indexes the whole workspace once.
4. Items under `#[cfg(test)]` stay out of rustdoc's output, so they stay out
   of the map. SCIP still sees them; the tests come from there.

## Containers and IDs

| Container | ID | `kind`, `languageKind` |
| --- | --- | --- |
| A crate: one library, binary or procedural-macro target | `rs:crate:<crate>` | `package`, "crate" |
| A module: the unit | `rs:mod:<crate>::<path>`, or `rs:mod:<crate>` for the root | `module`, "module" |
| A file | `rs:file:<path>` | `file`, "file" |

1. A crate takes its target's crate name (`grep_searcher`). A binary whose
   crate name a library already takes gains `@bin`.
2. Modules stay flat under their crate, as Go packages sit under their
   module. A module takes its path as its name: `grep_searcher::sink`.
3. A file module (`mod sink;`) owns its file container. An inline module
   (`mod tests { … }`) has no file of its own; its items name the module as
   their parent.
4. A symbol ID joins the module path and the symbol's path:
   `rs:grep_searcher::sink#Sink.matched`. Two items of one name in one module
   (a macro and a function, say) gain `@<file>:<line>`.
5. A member of a trait implementation adds the trait to its path, so that
   `Display::fmt` and `Debug::fmt` stay apart: `rs:lending#Book.Display::fmt`.
   The member's `name` stays `fmt`, so the viewer's trace finds it under the
   trait's method.

## Symbols

| Rust | `kind` | `languageKind` |
| --- | --- | --- |
| `struct` | `struct` | "struct" |
| `union` | `struct` | "union" |
| `enum` | `enum` | "enum" |
| A variant | `constant`, member | "variant" |
| `trait` | `trait` | "trait" |
| A trait alias | `type_alias` | "trait alias" |
| `fn` | `function` | "function" |
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
   `impl Shelf for Vec<Book>`.
2. Derived implementations add no members. Their traits go to
   `annotations` as one `#[derive(…)]`, and to `satisfies`.
3. `satisfies` lists the outside traits that a type implements, by name with
   their arguments: `Debug`, `Display`, `From<String>`. Auto traits and
   blanket implementations stay out.
4. `exported` holds `true` for `pub` items. Variants, trait members and
   members of trait implementations take their owner's value.
5. Modifiers name `async`, `const`, `unsafe`, `extern "<abi>"` and `mut`.
6. A variant's fields give its `type`, as written: `(String, u32)` or
   `{ id: BookId }`. They make no symbols of their own.

## Docs

1. Format `rustdoc`. The text holds Markdown, which turns into blocks and the
   inline subset, as in the TypeScript adapter.
2. Intra-doc links (`[Store]`, `` [`Store::get`] ``, `[text](Store)`)
   resolve through rustdoc's own `links` table, so they become `sym:` links to
   symbols and modules.
3. A code block without a language, or with only rustdoc's test attributes
   (`ignore`, `no_run`, `should_panic` and so on), counts as Rust. Its hidden
   lines (`# use …;`) drop out, as rustdoc hides them.
4. `#[deprecated]` fills `deprecated`.
5. A module's `//!` doc goes to the module, and to its file container. The
   crate takes the doc of its root module.

## Edges

1. `calls`: a reference to a function or method inside the body of another
   one, from the SCIP index. The innermost function or method whose span holds
   the reference makes the call. A reference that does not call (such as
   `map(Self::check)`) counts as well, as issue #6 decides.
2. A call through a trait object or a generic bound points at the trait's
   method. The viewer follows `implements` edges to the code that can run.
3. `implements`: a type, or an `impl` item, to a local trait.
4. `extends`: a trait to a local supertrait.
5. `field_type`, `param_type`, `return_type`: from the types in rustdoc's
   signatures, which name each item by ID. `Self` adds no edge.
6. `imports`: a module to each other module whose items its code names, from
   the SCIP references. The crates of other packages go to
   `externalImports`; `std`, `core` and `alloc` stay out.

Every edge counts as `exact`: rustdoc and rust-analyzer resolve each one.

## Tests

A function with a `#[test]` attribute, or any attribute macro named `test`
(such as `#[tokio::test]`), makes a test. SCIP finds it, with the attribute
inside the function's range. Its name joins its module path and its own name,
as `cargo test` prints it: `store::tests::finds_a_book`. Each reference in
its body records the test against the symbol it names.

## Entry-point plugins

Rust has no AST in either tool, so the plugins read tokens. A small lexer
reads each function's source, and SCIP resolves the names in it.

### `axum`

1. `.route("/books/{id}", get(show).delete(remove))` makes one `http` entry
   per method: `GET /books/{id}`, `DELETE /books/{id}`. `route` must resolve
   to axum's `Router`. `any(h)` gives `ANY`.
2. The handler: the first project function that the handler expression names.
   A closure keeps the registering function as the handler and counts as
   inline. A service from outside the project fills `externalHandler`.
3. `.nest("/api", api::router())` puts `/api` in front of the routes that
   `api::router` registers. The argument may also name a local variable, or
   hold the router inline. `.merge(other())` adds no prefix.
4. The option `prefixes` (`inFunc`, `prefix`) covers a mount that the plugin
   cannot see, as in the Go and TypeScript plugins.

### `clap`

1. A struct with `#[derive(Parser)]` makes the root command. Its name comes
   from `#[command(name = "…")]`, else from the binary's name.
2. An enum with `#[derive(Subcommand)]`, named by a `#[command(subcommand)]`
   field, makes one command per variant: `#[command(name = "…")]`, else the
   variant's name in kebab case. Names join their parents: `lendd import`.
3. The handler of a subcommand: the project function that the `match` arm
   (or `if let` block) of the variant calls, when the arm holds that one call
   alone. An arm that does more serves inline, in the function that matches.
4. The handler of the root command: the function that calls `parse`,
   `try_parse`, `parse_from` or `try_parse_from` on the struct.
5. The builder API (`Command::new("x")`) stays out; a note says so.

## Fixture and tests

1. `adapters/rust/testdata/fixture`: an invented lending library, as the Go
   fixture. A workspace of two packages, `lending` (library) and `lendd`
   (binary), plus `third_party/axum` and `third_party/clap`: small stand-ins
   with the API shapes that the plugins read, so that the tests need no
   network. The fixture uses every row of the tables above.
2. A golden file `testdata/golden/fixture.json`, rewritten with
   `UPDATE_GOLDEN=1 cargo test -p acu-adapter-rust`.
3. Schema validation of the output, and named facts that read as sentences.
4. Unit tests for the SCIP decoder, the lexer, the doc normaliser and the
   type printer.
5. The tests that run the nightly skip with a message when the toolchain is
   missing outside CI, and fail inside CI (`CI` set).

## Corpus

1. `BurntSushi/ripgrep` at 15.2.0 (commit e89fff8, 15 July 2026; 16.0.0
   came out on 22 November 2026, too new for CONTRIBUTING.md §4.11.3). Since
   version 14, ripgrep parses its flags with `lexopt`, not clap, so no plugin
   finds an entry point in it; the map shows its ten crates.
2. `tokio-rs/axum` at axum-v0.8.9 (commit c59208c, 6 April 2026): the four
   library crates and a few examples, for the `axum` plugin.
3. This project's own map gains `acu` and the Rust adapter. `acu` uses clap's
   derive API and axum, so the self map exercises both plugins.

## Build and pins

1. The `Dockerfile` installs the nightly with `rust-src` and `rust-analyzer`
   in the toolchain stage, and in the runtime image beside `acu-adapter-rust`.
2. `scripts/check-pins.sh` checks that the nightly agrees in the `Dockerfile`
   (`RUST_NIGHTLY`) and in `adapters/rust/src/toolchain.rs`.
3. `acu` runs the adapter from `target/release/acu-adapter-rust` inside this
   repository, and `plugin_adapter` names `axum` and `clap`.

## Risks

1. A new nightly changes the JSON format. The pin and the format check guard
   this; a move to a new nightly moves `rustdoc-types` in the same change.
2. Code that macros generate may carry spans that point at the macro. Calls
   inside it may then count for the wrong function, or not at all.
3. The plugins read tokens, not a syntax tree. An unusual registration may
   escape them; the limits say so.
