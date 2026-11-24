# Contributing to Atlas Carries Us

Thank you for helping. This guide tells you how to set up, how a change moves
from idea to merge, and the rules that keep the project consistent across its
languages. Coding agents read [`AGENTS.md`](AGENTS.md), which holds the same
rules in short form.

The project spans several languages: Go (the Go adapter), Rust (the `acu`
command and the Rust adapter), TypeScript (the viewer and the TypeScript
adapter), and more adapters to come. One data format
joins them. Most of the rules below exist so that a reader of any part finds
the same words, the same shapes and the same checks.

## 1. Set up

Choose one:

1. **VS Code devcontainer (recommended).** Open the folder and choose
   "Reopen in Container". The container holds the pinned Go, Node, Rust (with
   the nightly that the Rust adapter runs) and Chromium, installs the dependencies, fetches the test corpus, installs the
   Go adapter and builds the TypeScript adapter.
2. **Docker on the command line.**

   ```sh
   docker build --target toolchain -t acu-toolchain .
   docker run --rm -it -v "$PWD:/workspace" acu-toolchain bash
   ```

3. **Your own tools.** Install the versions that `rust-toolchain.toml`,
   `.node-version` and the `toolchain` line in `adapters/go/go.mod` name, and
   the nightly that `NIGHTLY` in `adapters/rust/src/toolchain.rs` names. Then:

   ```sh
   rustup toolchain install nightly-2026-11-01 --profile minimal --component rust-src,rust-analyzer
   npm ci
   npm run build -w adapters/typescript
   git submodule update --init --checkout --depth 1 corpus/go/chi corpus/go/cobra \
     corpus/typescript/immer corpus/typescript/hono corpus/svelte/realworld \
     corpus/svelte/svelte-changelog corpus/rust/ripgrep corpus/rust/axum
   (cd adapters/go && go install ./cmd/acu-adapter-go)
   ```

Behind Docker Hub's pull limit, add `--build-arg REGISTRY=mirror.gcr.io`.
Behind a TLS-inspecting proxy, see the comment at the top of the `Dockerfile`.

## 2. From idea to merge

1. **Find or open an issue.** The roadmap in issue #2 lists the phases and
   tracks; each has its own issue with acceptance criteria.
2. **Make one change per pull request.** A pull request that adds an adapter
   should not also restyle the viewer.
3. **Run `scripts/check.sh`.** It runs every check that CI runs. Use
   `scripts/check.sh --quick` while you work, and the full run before you
   push.
4. **Write the commit message** as an imperative subject line of at most 72
   characters ("Add the Express plugin"), a blank line, and a body that says
   what changed and why. Name the issue: "Closes #4" or "Part of #4".
5. **Open the pull request** with a summary, how you tested it, and what it
   leaves out. CI must pass before a merge.

## 3. Checks

`scripts/check.sh` runs, in order:

| Step | What it checks |
| --- | --- |
| Versions | `scripts/check-pins.sh`: every pinned tool version agrees across its files. |
| Go adapter | `gofmt`, `go vet`, and the tests: golden file, schema validation, named facts. |
| Node workspace | `npm ci`, and Prettier on the viewer, the TypeScript adapter and the end-to-end tests. |
| TypeScript adapter | The type check, the tests (golden file, schema validation, named facts, doc and glob units), and the build. |
| Viewer | The TypeScript type check, unit tests (including the language-word test), and the build. |
| `acu` and the Rust adapter | `cargo fmt`, `cargo clippy` with warnings as errors, the tests (for the Rust adapter: golden file, schema validation, named facts), and a release build of the workspace. |
| End to end | For each example: `acu extract`, the count snapshot, `acu build`, and the smoke test in both themes. |
| This project's map | `acu extract -c atlas.yaml`, the doc check (`tests/e2e/docs.mjs`), `acu build`, and the smoke test in both themes. |

## 4. Conventions that hold everywhere

### 4.1 The data format joins the parts

The data format (`schema/atlas.schema.json`) forms the contract between
adapters, `acu` and the viewer. A change to it touches four places in one pull
request:

1. `schema/atlas.schema.json`
2. `docs/schema.md`
3. Each adapter's types (today: `adapters/go/internal/schema/schema.go`,
   `adapters/typescript/src/schema.ts` and `adapters/rust/src/schema.rs`)
4. The viewer's types (`viewer/src/types.ts`)

The config format works the same way: `schema/config.schema.json`,
`docs/config.md` and `cli/src/config.rs` change together.

A change that only adds an optional field keeps version 1. A change that
removes or renames a field, or changes its meaning, raises the version; open
an issue and agree it with the maintainer first.

### 4.2 One vocabulary

Use these words, with these meanings, in identifiers, docs, messages and the
interface, in every language:

| Word | Meaning |
| --- | --- |
| container | A module, package, namespace, folder or file. Containers form a tree. |
| unit | A container that the overview draws as a box (a Go package, a TypeScript folder). |
| symbol | Anything the code declares: a type, a function, a member, a constant. |
| member | A symbol inside another symbol: a method, field or property. |
| item | (Viewer only) a top-level symbol, or a group of a file's constants and variables. |
| edge | A relation from a user to the thing it uses. |
| entry point | A way into the code: an HTTP route, a CLI command, a message handler. |
| region | A named group of units, chosen by people, by job. |
| tour | Ordered steps, written by people, each opening one view. |
| adapter | A program that reads one language and writes the data format. |
| plugin | Part of an adapter that finds one kind of entry point. |

### 4.3 IDs

IDs come from qualified names, never from positions in a list, so links keep
working after a rebuild. `docs/schema.md` section 3 gives the grammar. Two
declarations with one name gain an `@file:line` suffix.

### 4.4 Edges point at the thing used

Every edge runs from the user to the thing it uses: a caller to its callee, a
type to the interface it implements, a function to its parameter's type. The
viewer draws arrows the same way.

### 4.5 Language words live in the data, not in the viewer

An adapter writes its language's own words (`struct`, `trait`, `component`) in
`languageKind`. The viewer shows those words and never names a language's
constructs itself; `viewer/test/words.test.ts` fails when it does. The neutral
kind names that the schema defines (`class`, `struct`, `interface` and so on)
may appear in viewer code.

### 4.6 Deterministic output

Adapters and `acu` sort every list they write, so two runs on the same code
give the same bytes. Golden files depend on this.

### 4.7 Messages

1. Start each message with the program's name: `acu: …`, `acu-adapter-go: …`.
2. Say what happened, then the next step: "the name Store matches 3 symbols;
   use an ID".
3. Keep it to one sentence, two at most. Never blame the reader.
4. Write messages to standard error. Standard output carries data only.
5. In the viewer, write to the reader in the second person, subject first:
   "You find no map data here. Run acu build, then reload this page."

### 4.8 Comments and doc comments

1. Every source file opens with a comment that says what it holds. In Go, a
   blank line keeps a file's comment apart from the package clause, so that
   it never becomes the package doc.
2. Every item carries a doc comment whose first sentence stands on its own:
   exported or not, because the map shows every item. So does every exported
   member. The map shows that sentence on every box, so it matters here more
   than in most projects.
3. Write the first sentence for a reader of the map. Say what the code does,
   in the project's words ("addEdge records one use"), not a label ("A
   helper function").
4. Link what belongs together. Go doc links (`[Run]`, `[schema.Document]`)
   and TSDoc links (`{@link Model}`) become links on the map, and a Markdown
   link to a file under `docs/` carries a reader to the reason.
5. A folder or package gets one doc: a Go package doc, or a TypeScript file
   that carries `@packageDocumentation` (or an `index` file).
6. A comment explains why; the code shows what.
7. This project's own map (`atlas.yaml`) must stay complete:
   `node tests/e2e/docs.mjs out/self.json` names every unit, file, item and
   exported member that lacks a doc, and `scripts/check.sh` runs it.

### 4.9 Formatting

| Language | Tool | Rule |
| --- | --- | --- |
| Go | `gofmt` | No unformatted files. |
| Rust | `rustfmt`, `clippy` | No warnings. Golden files stay as the adapter writes them. |
| TypeScript, JavaScript, CSS, HTML, JSON in `viewer/`, `adapters/typescript/` and `tests/e2e/` | Prettier (`npm run format`) | `.prettierrc.json`. Golden files stay as the adapter writes them. |
| Everything else | `.editorconfig` | UTF-8, LF, final newline, two-space indents (tabs for Go, four spaces for Rust and Python). |

### 4.10 Tests

1. **Every adapter** has a small invented fixture in its `testdata/`, a golden
   file of its output, a schema validation of that output, and a short list
   of named facts that must hold.
2. **Every adapter** also runs on at least one public project under
   `corpus/`, with a config in `examples/<name>/` and a count snapshot in
   `examples/<name>/counts.json`.
3. **The viewer** has unit tests for its pure parts and a smoke test that
   opens every view in both themes and fails on any console error.
4. **Update a golden file or a snapshot only on purpose.** Run the update
   (`go test ./... -update`, `npm run golden -w adapters/typescript`,
   `UPDATE_GOLDEN=1 cargo test -p acu-adapter-rust`,
   `UPDATE_GOLDEN=1 python3 -m unittest discover -s adapters/python/tests`,
   `node tests/e2e/counts.mjs … --update`), read the diff, and say in the
   commit message why it changed.
5. **Never skip or delete a test to make CI pass.**

### 4.11 Versions

Each tool version lives in two or more files. Change a group together;
`scripts/check-pins.sh` fails when they disagree.

| Tool | Files |
| --- | --- |
| Go | `GO_IMAGE` in `Dockerfile`; `toolchain` in `adapters/go/go.mod` |
| Node | `NODE_IMAGE` in `Dockerfile`; `.node-version` |
| Rust | `RUST_IMAGE` in `Dockerfile`; `rust-toolchain.toml` |
| Playwright | `PLAYWRIGHT_VERSION` in `Dockerfile`; `tests/e2e/package.json`; `package-lock.json` |
| TypeScript | `viewer/package.json`; `adapters/typescript/package.json`; `package-lock.json` |
| `svelte`, `svelte2tsx` | `adapters/typescript/package.json`; `package-lock.json` |
| Python | `PYTHON_IMAGE` in `Dockerfile`; `.python-version` |
| Rust nightly | `RUST_NIGHTLY` in `Dockerfile`; `NIGHTLY` in `adapters/rust/src/toolchain.rs` |
| `rustdoc-types` | `adapters/rust/Cargo.toml` (exact); `Cargo.lock`. It must read the nightly's JSON format; move both together. |

1. Pin base images by tag and digest.
2. Pin npm packages to exact versions.
3. Prefer releases at least two weeks old.
4. Add a dependency only when it earns its place; say why in the pull request.
5. Keep TypeScript on major version 6. The adapters and `svelte2tsx` use the
   TypeScript 6 compiler API; TypeScript 7 offers only an unstable one.
   `scripts/check-pins.sh` fails on another major version.

### 4.12 Colour and access

1. Never pair red with green. Never let colour carry meaning alone: add a dash
   pattern, a shape or a text label.
2. Text reaches 4.5:1 contrast on its background, and lines and control edges
   3:1, in both themes.
3. Take colours from `viewer/src/theme.ts` and the tokens in
   `viewer/src/styles.css`, not from new literals.
4. Give every new interaction a keyboard path, and respect reduced motion.
5. The Ahro brand styles the frame around the map. The map itself stays plain.

### 4.13 Writing

1. Write in plain, simplified technical English: short declarative sentences,
   one idea each, no metaphors, numbered steps for procedures.
2. In documents (READMEs, files under `docs/`), avoid forms of "to be" where it
   reads naturally (E-Prime).
3. Interface text follows the Ahro voice: second person, subject first, and it
   names the next step.
4. Region descriptions and tour text come from people. Text that a program
   drafts carries a `drafted` mark until a person checks it.

### 4.14 Public code only

Anyone can read this repository. Tests use invented fixtures and public projects
pinned as submodules. Never commit private code, private project names, or
generated maps of private code. Never commit generated files such as
`atlas.json`, `dist/` or screenshots.

## 5. Adding a language adapter

1. Open an issue that records the design: which compiler or tool reads the
   code, what counts as a unit, and which entry-point plugins it offers.
2. Create `adapters/<language>/`. Use the language's own ecosystem and its own
   type checker where one exists.
3. Speak protocol 1 (`docs/adapters.md`): a JSON request on standard input,
   one document on standard output.
4. Follow sections 4.1 to 4.8. In particular, write the language's words in
   `languageKind`, and mark every edge's `confidence`.
5. Add the tests in section 4.10, and an example config with a tour that you
   checked against the source.
6. Teach `acu` the adapter's built-in plugins (`plugin_adapter` in
   `cli/src/adapter.rs`) and add the adapter's toolchain to the `Dockerfile`.
7. Record the decisions in `docs/adapters.md`.

## 6. Licence

You contribute under the MIT licence of this repository. The projects under
`corpus/` keep their own licences.
