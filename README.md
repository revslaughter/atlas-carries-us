# Atlas Carries Us

Atlas Carries Us turns a code base and its doc comments into an interactive
map. The map answers the questions that developers ask most and find hardest:
"how does control get from here to the data?" and "what does this do, and why?"

You open a region of the code, then a package, then one type, and the map
redraws each time with only that piece and its direct relations: what uses it
on the left, what it uses on the right, with every line ending at the exact
method or field. A trace view follows the calls from an HTTP route or any
function, in order, through each interface to the code that runs. A docs panel
beside the map holds the doc comments, signatures, tests and links to the
source. A guided tour, written by people who know the code, ties it together.

The command `acu` builds the map as a static site, or serves it on your own
computer.

## Status

All four phases work today:

| Part | State |
| --- | --- |
| Data format, version 1 | Done: [`schema/atlas.schema.json`](schema/atlas.schema.json), spec in [`docs/schema.md`](docs/schema.md). |
| Go adapter | Done: [`adapters/go`](adapters/go). |
| TypeScript adapter | Done: [`adapters/typescript`](adapters/typescript), with Express and Hono routes. One map may hold several languages together. |
| `acu` command (Rust) | `extract`, `build`, `serve` and `check` work. |
| Viewer | Done: [`viewer`](viewer). |
| Svelte and SvelteKit | Done (phase 3), on the TypeScript adapter: components through `svelte2tsx`, and SvelteKit routes, form actions and layouts with the `sveltekit` plugin. |
| Rust adapter | Done (phase 4): [`adapters/rust`](adapters/rust), from rustdoc's JSON output and rust-analyzer's SCIP index, with axum routes and clap commands. It runs a pinned nightly toolchain. |
| MCP server in `acu serve` | Planned: lets a language model of your choice read the map, draft regions and tours, and answer questions. |

## Try it

The quickest way builds the container image. It holds `acu`, the three
adapters, the Go toolchain, Node and the nightly Rust toolchain that the Rust
adapter runs:

```sh
docker build -t acu .
docker run --rm -v "$PWD:/work" -p 4321:4321 acu serve -c atlas.yaml --addr 0.0.0.0:4321
```

To build from source instead, you need Go 1.24 or later, Node 20 or later,
and a Rust toolchain. The pinned versions sit in `adapters/go/go.mod`,
`.node-version` and `rust-toolchain.toml`. The Rust adapter also needs its
pinned nightly: `rustup toolchain install nightly-2026-11-01 --profile minimal
--component rust-src,rust-analyzer`.

1. Build the viewer:

   ```sh
   npm ci
   npm run build -w viewer
   ```

2. Build the command and the adapters. `acu` runs the TypeScript adapter
   from its build in this checkout, and the Rust adapter from beside itself:

   ```sh
   cargo build --release --workspace
   (cd adapters/go && go install ./cmd/acu-adapter-go)
   npm run build -w adapters/typescript
   ```

3. Fetch a public example (chi, cobra, immer, hono, realworld,
   svelte-changelog, ripgrep or axum) and look at it:

   ```sh
   git submodule update --init --checkout --depth 1 corpus/go/chi
   ./target/release/acu serve -c examples/chi/atlas.yaml
   ```

   Then open <http://127.0.0.1:4321/>. Press `T` for the tour.

## The map of this project

Atlas Carries Us maps itself. [`atlas.yaml`](atlas.yaml) in the repository
root covers the Go adapter, the TypeScript adapter and the viewer, with
regions named by job and a tour that follows one run through the code. After
step 2 above, run this in the repository root:

```sh
./target/release/acu serve
```

Every unit, file and function on that map carries a doc comment, and
`scripts/check.sh` fails when one does not. The `acu` command joins the map in
phase 4, with the Rust adapter.

## Map your own project

1. Write an `atlas.yaml` beside your code. The smallest one:

   ```yaml
   version: 1
   project: { name: My service, repo: me/my-service }
   languages:
     - adapter: go # or typescript
   ```

2. Run `acu extract` to write `atlas.json`, then `acu build` to write a static
   site to `site/`, or `acu serve` to look at it locally.
3. Add regions (groups of packages, named by their job), entry-point plugins
   (HTTP routes, CLI commands) and a tour. [`docs/config.md`](docs/config.md)
   lists every field. [`examples/`](examples) holds six full configs, and
   [`atlas.yaml`](atlas.yaml) maps this project.
4. Write doc comments for the map. Every box shows the first sentence of its
   doc, so a first sentence that says what the code does, in the project's
   own words, helps a reader most.

`acu check` reports problems, such as a tour step that points at code that no
longer exists, without writing anything.

## Develop

One container definition, the `toolchain` stage of the [`Dockerfile`](Dockerfile),
pins Go, Node, Rust and the Chromium that the smoke test uses, each base image
by digest. Three places use it:

1. **VS Code.** Open the folder and choose "Reopen in Container". The
   devcontainer installs the dependencies, fetches the test corpus, installs
   the Go adapter and builds the TypeScript adapter.
2. **The command line.**

   ```sh
   docker build --target toolchain -t acu-toolchain .
   docker run --rm -v "$PWD:/workspace" acu-toolchain scripts/check.sh
   ```

3. **CI**, which runs the same `scripts/check.sh` in the same container.

`scripts/check.sh` runs every check: formatting, lints, unit tests, golden
files, count snapshots and the smoke test. `scripts/check.sh --quick` skips the
end-to-end run.

## Contribute

[`CONTRIBUTING.md`](CONTRIBUTING.md) describes the set-up, the workflow, and
the conventions that keep the languages consistent. Coding agents read
[`AGENTS.md`](AGENTS.md). The roadmap lives in issue #2.

## Repository layout

| Path | Holds |
| --- | --- |
| `atlas.yaml`, `tsconfig.atlas.json` | The map of this project. |
| `schema/` | The data format and the config format, as JSON Schema. |
| `adapters/go/` | The Go adapter: reads a module with the Go type checker. |
| `adapters/typescript/` | The TypeScript adapter: reads a project with the TypeScript compiler API. |
| `cli/` | `acu`, the command (Rust). It embeds the built viewer. |
| `viewer/` | The viewer: TypeScript, one canvas plus a docs panel, no framework. |
| `examples/` | Configs for the test fixtures and for the public corpus. |
| `corpus/` | Public projects pinned as shallow submodules, for testing. A plain clone leaves them out. |
| `tests/e2e/` | The smoke test (opens every view in Chromium and fails on console errors), the count check and the doc check. |
| `Dockerfile`, `.devcontainer/` | The pinned toolchain, the release image and the VS Code devcontainer. |
| `scripts/check.sh` | Every check that CI runs. |
| `docs/` | The data format, the config, adapter notes, and the research behind the design. |

## Design

The design follows research on how developers read code
([`docs/research/report.md`](docs/research/report.md)):

1. One focus item and its direct relations, about twenty boxes, works better
   than a map of everything.
2. Incoming relations sit on the left, outgoing on the right, each lane named
   by a verb. Arrows point at the thing used.
3. Names and first doc sentences appear on every box, because readers navigate
   by them.
4. Colour never works alone. Every relation also has a dash pattern and a text
   label, and the palette avoids red-green pairs.
5. Reference (the map and the panel) and explanation (region notes, the tour)
   stay apart. Text that a program drafted carries a visible label until a
   person checks it.

## Licence

Atlas Carries Us carries the MIT licence. The viewer ships the Inter, Fraunces
and JetBrains Mono fonts under the SIL Open Font License 1.1. The Ahro mark
belongs to Ahro. The projects under `corpus/` keep their own licences: chi
(MIT), Cobra (Apache 2.0), Immer (MIT), Hono (MIT), RealWorld for SvelteKit
(MIT) and Svelte Changelog (MIT).
