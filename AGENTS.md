# AGENTS.md

Instructions for coding agents that work in this repository. People read
[`CONTRIBUTING.md`](CONTRIBUTING.md), which holds the full rules; the section
numbers below point into it. When the two disagree, CONTRIBUTING.md wins; fix
this file in the same change.

## What this project holds

Atlas Carries Us turns a code base and its doc comments into an interactive
map.

| Path | Language | Holds |
| --- | --- | --- |
| `schema/` | JSON Schema | The data format (the contract) and the config format. |
| `adapters/go/` | Go | Reads Go code with the type checker; writes the data format. |
| `adapters/typescript/` | TypeScript | Reads TypeScript and JavaScript with the compiler API; writes the data format. |
| `adapters/rust/` | Rust | Reads Rust with rustdoc JSON and rust-analyzer's SCIP index; writes the data format. |
| `adapters/python/` | Python | Reads Python with `ast` and its own resolver; writes the data format. Standard library only. |
| `cli/` | Rust | `acu`: runs adapters, applies `atlas.yaml`, builds and serves the site. |
| `viewer/` | TypeScript | The static viewer: one canvas and a docs panel. |
| `examples/`, `corpus/` | YAML, submodules | Configs, and public projects for tests. |
| `tests/e2e/` | JavaScript | The smoke test and the count snapshot check. |
| `docs/` | Markdown | The spec, the config, adapter notes, research. |

Read `docs/schema.md` before a change that touches data. Read
`docs/research/report.md` before a change to how the map looks or behaves.

## Commands

| Task | Command |
| --- | --- |
| Every check (what CI runs) | `scripts/check.sh` |
| Every check except end to end | `scripts/check.sh --quick` |
| Pinned versions agree | `scripts/check-pins.sh` |
| Go adapter tests | `cd adapters/go && go test ./...` |
| Rewrite the Go golden file | `cd adapters/go && go test ./... -update`, then read the diff |
| TypeScript adapter type check, tests, build | `npm run check -w adapters/typescript && npm test -w adapters/typescript && npm run build -w adapters/typescript` |
| Rewrite the TypeScript golden file | `npm run golden -w adapters/typescript`, then read the diff |
| Viewer type check, tests, build | `npm run check -w viewer && npm test -w viewer && npm run build -w viewer` |
| Rust adapter tests | `cargo test -p acu-adapter-rust` (needs the pinned nightly; see `adapters/rust/src/toolchain.rs`) |
| Rewrite the Rust golden file | `UPDATE_GOLDEN=1 cargo test -p acu-adapter-rust`, then read the diff |
| Python adapter tests | `python3 -m unittest discover -s adapters/python/tests` |
| Rewrite the Python golden file | `UPDATE_GOLDEN=1 python3 -m unittest discover -s adapters/python/tests`, then read the diff |
| Format TypeScript and friends | `npm run format` |
| acu and Rust adapter checks | `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` |
| Map an example | `cargo run -- extract -c examples/chi/atlas.yaml -o out/chi.json` |
| Look at it | `cargo run -- serve -d out/chi.json`, or `cargo run -- build -d out/chi.json -o out/site` |
| Map this project | `cargo run -- extract -c atlas.yaml -o out/self.json`, then `cargo run -- serve -d out/self.json` |
| Find missing docs | `node tests/e2e/docs.mjs out/self.json` |
| Screenshots of every view | `node tests/e2e/smoke.mjs out/site out/shot` (add `--light` for the light theme) |
| Write acu and its adapters to a folder | `scripts/docker-export.sh` (add `--platform darwin/arm64`, `windows/amd64` or another `os/arch` for that system) |

Build the viewer (`npm run build -w viewer`) before `cargo build`; `acu`
embeds `viewer/dist` at compile time. Build the TypeScript adapter before an
`acu` run that maps TypeScript; inside this repository, `acu` runs its build.
`cargo build --release --workspace` puts `acu-adapter-rust` beside `acu`,
which runs it from there. The Rust adapter runs rustdoc and rust-analyzer
from the nightly that `NIGHTLY` in `adapters/rust/src/toolchain.rs` names.
Install it with
`rustup toolchain install nightly-2026-11-01 --profile minimal --component rust-src,rust-analyzer`.

## Rules

1. Run `scripts/check.sh` before you call a change done. Report failures
   with their output; never skip, delete or weaken a test to pass (4.10).
2. Change the data format in four places at once: the schema, `docs/schema.md`,
   the adapter types, and `viewer/src/types.ts`. Add fields; do not remove or
   rename them without the maintainer's agreement (4.1).
3. Use the project's vocabulary: container, unit, symbol, member, item, edge,
   entry point, region, tour, adapter, plugin (4.2).
4. Build IDs from qualified names. Point every edge from the user to the thing
   used (4.3, 4.4).
5. Keep the viewer free of language words. It shows `languageKind` from the
   data. `viewer/test/words.test.ts` enforces this (4.5).
6. Sort every list an adapter or `acu` writes (4.6).
7. Start every message with the program's name; say what happened and the
   next step; write messages to standard error (4.7).
8. Open every new file with a comment that says what it holds. Give every
   item, exported or not, and every exported member a doc comment whose
   first sentence says what it does and stands alone. This project's own map
   must stay complete; `tests/e2e/docs.mjs` checks it (4.8).
9. Format with `gofmt`, `rustfmt` and Prettier; follow `.editorconfig` (4.9).
10. Change a pinned version in every file of its group;
    `scripts/check-pins.sh` lists them. Pin images by digest and npm packages
    exactly. Keep TypeScript on major version 6. `svelte` and `svelte2tsx`
    sit pinned in `adapters/typescript/package.json` and the lock file. The
    Rust nightly sits in the `Dockerfile` and `toolchain.rs`; `rustdoc-types`
    must read that nightly's JSON format, so move both together (4.11).
11. Never pair red with green, and never let colour carry meaning alone. Keep
    text at 4.5:1 and lines at 3:1 in both themes. Take colours from
    `theme.ts` and `styles.css` (4.12).
12. Write plain, simplified technical English: short sentences, one idea
    each, no metaphors. Use E-Prime in documents (4.13).
13. Mark text that you draft for regions, descriptions or tours with a
    `drafted` entry (`by`, `at`). Do not remove such a mark yourself; a person
    removes it after checking (4.13).
14. Commit public code only: invented fixtures, public corpus projects. Never
    commit generated maps, `dist/`, screenshots or private code (4.14).
15. Update a golden file or count snapshot only when the change means it to
    change. Say why in the commit message (4.10).

## Verify what you build

1. After a change to the viewer, build a site from an example, run
   `tests/e2e/smoke.mjs` with a screenshot prefix, and look at the
   screenshots for both themes.
2. After a change to an adapter, run it on the fixture and on a corpus
   project, and compare the count snapshot.
3. After a change to `atlas.yaml` handling, run `acu check` on every example.
4. For a tour or region text, check each claim against the source code.

## Ask first

Ask the maintainer before you:

1. Raise the data format's version, or remove or rename a field.
2. Change what a colour, dash pattern or shape means on the map.
3. Add a runtime dependency, or a network call at run time.
4. Change the Ahro brand assets in `viewer/assets/`.

## Working in a restricted network

1. Docker Hub may refuse anonymous pulls ("429 Too Many Requests"). Build with
   `--build-arg REGISTRY=mirror.gcr.io`; the pinned digests stay the same.
2. A TLS-inspecting proxy needs its CA certificate in the image build: pass
   `--secret id=extra_ca,src=<ca.pem>` and the proxy variables as build
   arguments, and build with `--network host` when the proxy listens on the
   host's loopback.
3. The Go adapter needs `go` at run time, because it loads packages with
   `go list`.
4. The Rust adapter needs cargo, rustdoc and rust-analyzer from its pinned
   nightly at run time. rustdoc builds each crate's dependencies, so cargo
   fetches them from crates.io on a first run.
5. The Python adapter needs only `python3` (3.12 or later; the image pins 3.13)
   at run time. It makes no network call and installs nothing.
