# Atlas Carries Us

Atlas Carries Us turns a code base and its doc comments into an interactive
map. One command, `acu`, reads your code, applies a config file, and either
serves the map on your computer or writes it as a static site.

This file explains how to compile `acu`, how to configure a project, and how
to run it. Other files hold the details:

| Topic | File |
| --- | --- |
| Install from a release, the image or source | [`docs/install.md`](docs/install.md) |
| Map on every push, and publish to GitHub Pages | [`docs/github-action.md`](docs/github-action.md) |
| Every `atlas.yaml` field | [`docs/config.md`](docs/config.md) |
| The adapters and their plugins | [`docs/adapters.md`](docs/adapters.md) |
| The data format | [`docs/schema.md`](docs/schema.md) |
| The MCP server for language models | [`docs/mcp.md`](docs/mcp.md) |
| Set up and edit a map in the browser | [`docs/studio.md`](docs/studio.md) |
| The workflow and the conventions | [`CONTRIBUTING.md`](CONTRIBUTING.md) |

## How it works

1. An **adapter** reads code in one language and writes the data format.
2. `acu` runs the adapters that `atlas.yaml` names, merges their output,
   applies the regions and the tour from the config, and checks the result.
3. The **viewer** reads that data (`atlas.json`) and draws the map in a
   browser. `acu` embeds the viewer in its own program.

| Language | Adapter | Reads code with | Needs at run time |
| --- | --- | --- | --- |
| Go | `go` | The Go type checker | `go` |
| TypeScript, JavaScript, Svelte | `typescript` | The TypeScript compiler API | `node` 20 or later |
| Rust | `rust` | rustdoc JSON and rust-analyzer | The pinned nightly toolchain, `nightly-2026-11-01` |
| Python | `python` | The `ast` module | `python3` 3.12 or later |

One config may name several adapters. `acu` merges them into one map.

## 1. Get the programs

Choose one route. [`docs/install.md`](docs/install.md) holds the full steps
for each. To map a repository on every push with GitHub Actions, use the
action in [`action.yml`](action.yml); see
[`docs/github-action.md`](docs/github-action.md).

### Route A: the container image

The image holds `acu`, the four adapters and every tool that the adapters
need at run time. It runs on Linux, for amd64 and arm64. Each release
publishes it as `ghcr.io/revslaughter/atlas-carries-us`:

```sh
docker run --rm -v "$PWD:/work" -p 4321:4321 ghcr.io/revslaughter/atlas-carries-us:latest \
  serve -c atlas.yaml --addr 0.0.0.0:4321
```

To build the image from a checkout instead:

```sh
docker build -t acu .
docker run --rm -v "$PWD:/work" -p 4321:4321 acu serve -c atlas.yaml --addr 0.0.0.0:4321
```

1. The image runs `acu` in `/work`. Mount your project there.
2. Give `--addr 0.0.0.0:4321` so that the port reaches the host.
3. Open the address that `acu` prints. On `0.0.0.0` it prints
   `http://127.0.0.1:4321/?token=<token>`, an address that a browser can
   open. The map's pages also answer without a token. The API and the MCP
   server need the session token that `acu` prints
   ([`docs/studio.md`](docs/studio.md#the-session-token)). On `0.0.0.0`,
   the write tools stay off without `--allow-remote-writes`, so the editor
   does not show.

### Route B: programs for your own system

Each [release](https://github.com/revslaughter/atlas-carries-us/releases)
carries an archive for Linux, macOS and Windows, on amd64 and arm64, with a
`SHA256SUMS` file. Unpack the archive and run `acu` from its folder.
[`docs/install.md`](docs/install.md) gives the steps.

To build the same programs yourself, Docker builds them and writes them to a
folder. The wrapper script takes the target system from `--platform`:

```sh
scripts/docker-export.sh                           # Linux, this machine's architecture
scripts/docker-export.sh --platform darwin/arm64   # macOS on Apple silicon
scripts/docker-export.sh --platform windows/amd64  # Windows on x86-64
```

1. `--platform` accepts `linux/amd64`, `linux/arm64`, `darwin/amd64`,
   `darwin/arm64`, `windows/amd64` and `windows/arm64`.
2. Without `--platform`, Docker picks a Linux platform, also on macOS and
   Windows. Name your system there.
3. The script writes to `out/docker`. Give another folder as its argument.
   Pass options to `docker build` after `--`, for example
   `-- --build-arg REGISTRY=mirror.gcr.io`.
4. The build cross-compiles, so no step runs under emulation.

Without the script, run the build yourself:

```sh
docker build --platform darwin/arm64 --target export --provenance=false \
  --output type=local,dest=out/docker .
```

The folder, like a release archive, holds `acu`, `acu-adapter-go`,
`acu-adapter-python`, `acu-adapter-rust` and the folder
`acu-adapter-typescript/`. On Windows the three compiled programs end in
`.exe`, and the Python adapter takes the name `acu-adapter-python.pyz`.

1. `acu` finds its adapters beside itself. You need no `PATH` setup.
2. The adapters still need their tools at run time: Go, Node, Python and the
   pinned nightly (see the table above).
3. The Python adapter comes as one zip application that `python3` runs.
   On Windows, `acu` runs `acu-adapter-python.pyz` with Python by itself.
4. The Linux programs need glibc 2.28 or later. `acu-adapter-go` runs on any
   Linux. The macOS programs need macOS 12 or later.
5. Nothing signs the programs. A local `--output` sets no quarantine mark, so
   Gatekeeper lets them run. A copy that came through a browser download
   needs `xattr -d com.apple.quarantine <file>` first.

### Route C: compile from source

#### Requirements

| Tool | Version | Where the pin sits |
| --- | --- | --- |
| Rust | 1.97.1 | `rust-toolchain.toml` (rustup installs it for you) |
| Node | 26.13.1 builds the project; 20 or later runs the adapter | `.node-version` |
| Go | 1.24 or later | `adapters/go/go.mod` |
| Python | 3.12 or later (3.13.17 in the image) | `.python-version` |
| Rust nightly | `nightly-2026-11-01` | `adapters/rust/src/toolchain.rs` |

You need only the tools of the languages that you map, plus Rust and Node to
build `acu` and the viewer. Install the nightly for the Rust adapter once:

```sh
rustup toolchain install nightly-2026-11-01 --profile minimal \
  --component rust-src,rust-analyzer
```

#### Build

Run these steps in the repository root, in this order.

1. Build the viewer. `acu` embeds `viewer/dist` when it compiles. Without
   it, `acu` embeds a placeholder page, so this step must come first:

   ```sh
   npm ci
   npm run build -w viewer
   ```

2. Build `acu` and the Rust adapter. The Rust adapter lands beside `acu`,
   which runs it from there:

   ```sh
   cargo build --release --workspace
   ```

3. Build the TypeScript adapter. Skip this step if you do not map TypeScript,
   JavaScript or Svelte:

   ```sh
   npm run build -w adapters/typescript
   ```

4. Optional: install the Go adapter. Without it, `acu` runs the adapter
   source with `go run`, which compiles it on every run:

   ```sh
   (cd adapters/go && go install ./cmd/acu-adapter-go)
   ```

   `go install` writes to `$(go env GOPATH)/bin`. Put that folder on your
   `PATH`.

The Python adapter needs no build. Inside a checkout, `acu` runs it with
`python3`.

The result sits at `target/release/acu`. Check it:

```sh
./target/release/acu --version
```

#### How `acu` finds an adapter

For each entry in `languages`, `acu` picks the first match in this list:

1. The `command` field of that entry in `atlas.yaml`: a list of the program
   and its arguments.
2. The variable `ACU_ADAPTER_<NAME>`, for example `ACU_ADAPTER_GO`. The value
   holds the program and its arguments, split on spaces.
3. A program named `acu-adapter-<name>` on the `PATH`.
4. A program (or, for TypeScript, a folder) named `acu-adapter-<name>` in the
   folder that holds `acu`, or a file `acu-adapter-<name>.pyz` there, which
   `acu` runs with Python. A symbolic link to `acu` counts as the real
   program, so `acu` looks in the folder of the program that the link names.
5. The adapter source in this checkout, for the build that you made here.

When none matches, `acu` stops and names the options. Messages from `acu`
begin with `acu:` and go to standard error.

### Restricted networks

1. Docker Hub may refuse anonymous pulls with "429 Too Many Requests". Build
   with `--build-arg REGISTRY=mirror.gcr.io`. The pinned digests stay the
   same.
2. A TLS-inspecting proxy needs its CA certificate in the image build. Pass
   `--secret id=extra_ca,src=<ca.pem>` and the proxy variables as build
   arguments. Add `--network host` when the proxy listens on the loopback
   address of the host.
3. The Go adapter loads packages with `go list`. The Rust adapter runs
   rustdoc, which builds each crate's dependencies, so cargo fetches them from
   crates.io on a first run. The Python adapter makes no network call.

## 2. Configure a project

`acu` reads a config file, `atlas.yaml`, that you keep beside your code. Every
relative path in it resolves against the folder that holds it.

### Set up in the browser

`acu studio` writes the config for you. Run it, and open the address that
it prints, such as `http://127.0.0.1:4321/?token=<token>`. Then pick a
folder or paste an HTTPS address of a repository. The studio proposes an
`atlas.yaml` from the languages it finds, runs the adapters, and opens the
map. Then you edit the regions and the tour in the browser: record the
current view as a step, write its text, and drag the steps into order. Each
save goes into `atlas.yaml`, with its comments kept.
[`docs/studio.md`](docs/studio.md) explains each step. The sections below
describe the file itself.

### The smallest config

```yaml
version: 1
project: { name: My service, repo: me/my-service }
languages:
  - adapter: go # go, typescript, rust or python
```

This config maps the Go code in the folder that holds `atlas.yaml`. Run
`acu check` to see whether it works (see "Run it" below).

### A fuller config

```yaml
version: 1
project:
  name: My service
  repo: me/my-service
  links: github              # github, gitlab, none, or your own templates
  description: |
    One paragraph that says what the service does.
languages:
  - adapter: go
    root: .                  # the folder to read; default: this file's folder
    exclude: ["**/testdata/**"]
  - adapter: typescript
    root: web
    options: { tsconfig: tsconfig.build.json }
entryPoints:                 # ways into the code: routes, commands
  - plugin: go-chi
  - plugin: cobra
regions:                     # groups of packages, named by their job
  - id: http
    name: HTTP layer
    paths: ["internal/http", "internal/http/**"]
  - id: other
    name: Everything else
    default: true
tour:
  - title: Start here
    open: { view: overview }
    text: The service has two layers. The HTTP layer calls the rules.
```

### What each part does

1. `languages` lists one entry per adapter run. Use `root` to point an entry
   at a folder, and `include` and `exclude` (globs of folders) to narrow it.
   Put adapter settings in `options`: Go reads `buildTags`, TypeScript reads
   `tsconfig`, Rust reads `packages`, `features`, `allFeatures`, `toolchain`
   and `targetDir`, and Python reads `sourceRoots`.
2. `entryPoints` switches on plugins that find HTTP routes and commands.
   `acu` knows which adapter runs each plugin below. Name another adapter with
   `adapter:` for a plugin that `acu` does not know.

   | Adapter | Plugins |
   | --- | --- |
   | `go` | `go-chi`, `go-nethttp`, `cobra` |
   | `typescript` | `express`, `hono`, `sveltekit` |
   | `rust` | `axum`, `clap` |
   | `python` | `flask`, `fastapi`, `django`, `click`, `typer`, `python-main` |

3. `regions` groups packages for the overview. Name each region by its job,
   not by its folder. Without regions, `acu` makes one per top-level folder.
   `layout` sets the rows in which the regions sit.
4. `tour` holds guided steps. Each step opens one view: the overview, a
   region, a unit, an item, or a trace from an entry point. `acu` stops with
   a message when a step points at nothing.
5. Doc comments feed the map. Every box shows the first sentence of its doc
   comment. Write a first sentence that says what the code does and stands
   alone.
6. Mark text that a program drafted (a region description, a tour step) with
   `drafted: { by: "...", at: "..." }`. The viewer labels it as
   machine-drafted. A person who has checked the text removes the mark.

[`docs/config.md`](docs/config.md) lists every field. [`examples/`](examples)
holds fifteen complete configs, and [`atlas.yaml`](atlas.yaml) maps this
project.

Your editor can check the file against
[`schema/config.schema.json`](schema/config.schema.json). `acu` runs the same
check when it loads the file.

## 3. Run it

### The commands

| Command | What it does |
| --- | --- |
| `acu extract` | Runs the adapters, applies the config, checks the result and writes `atlas.json`. |
| `acu serve` | Serves the map on your computer, with an MCP server at `/mcp` ([`docs/mcp.md`](docs/mcp.md)). |
| `acu mcp` | Runs the MCP server over standard input and output, for desktop clients. |
| `acu build` | Writes a static site (the viewer and the data) to a folder. |
| `acu check` | Reports problems in the config and the data. It writes nothing. |
| `acu studio` | Sets up and edits a map in the browser: pick a folder or clone a repository, extract, and edit the regions and the tour ([`docs/studio.md`](docs/studio.md)). |

### Options

| Command | Option | Default | Meaning |
| --- | --- | --- | --- |
| `extract` | `-c`, `--config <file>` | `atlas.yaml` | The project config. |
| `extract` | `-o`, `--out <file>` | `atlas.json` | The data file to write. |
| `extract` | `--pretty` | off | Indent the JSON. |
| `serve`, `mcp`, `build`, `check` | `-c`, `--config <file>` | `atlas.yaml` | Extract from this config first. |
| `serve`, `mcp`, `build`, `check` | `-d`, `--data <file>` | none | Use a finished `atlas.json`. Cannot combine with `-c`. |
| `build` | `-o`, `--out <folder>` | `site` | The folder to write. |
| `serve` | `--addr <host:port>` | `127.0.0.1:4321` | The address to listen on. |
| `serve`, `mcp` | `--read-only` | off | Turn off the MCP tools that write `atlas.yaml`. |
| `serve`, `mcp` | `--root <folder>` | current folder | With `-d`: the mapped repository, where the MCP tool `read_source` reads files. |
| `serve` | `--allow-remote-writes` | off | Let the MCP write tools work on an address that other computers can reach. For a client on another computer, give the computer's own address, such as `192.168.1.5:4321`. On `0.0.0.0`, `/mcp` and the API answer only loopback names with the same port, such as a container's `-p 4321:4321`. |
| `serve` | `--allow-host <name[:port]>` | none | On a loopback address, also show the map's pages and data to this forwarded name, such as `localhost:9000` behind `ssh -L`, or the forwarded name that GitHub Codespaces may send, such as `<codespace>-4321.app.github.dev`. A refused page's 403 names the `Host` that acu received; give that value. May repeat. The API and MCP keep their own names. |

`acu serve` and `acu studio` print an address that holds a session token,
such as `http://127.0.0.1:4321/?token=<token>`. Open that address. The API
and the MCP server answer only requests that carry the token. For an MCP
client's config, the environment variable `ACU_TOKEN` fixes the token
across restarts. Set it on the command line of `acu serve`, not with
`export`. [`docs/studio.md`](docs/studio.md#the-session-token)
explains the token, and [`docs/mcp.md`](docs/mcp.md#connect-a-client) shows
how an MCP client sends it.

### Common tasks

1. Look at your map. Run this in the folder that holds `atlas.yaml`, then open
   the address that it prints, such as
   `http://127.0.0.1:4321/?token=<token>`:

   ```sh
   acu serve
   ```

   Press `T` in the viewer for the tour.

2. Split extraction from viewing. Extraction can take a while on a large
   project, so write the data once and serve the file. `serve -d` reads the
   file on each request, so a fresh `extract` shows after a reload:

   ```sh
   acu extract -c atlas.yaml -o out/map.json --pretty
   acu serve -d out/map.json
   ```

3. Publish a static site. Put the output folder on any web host:

   ```sh
   acu build -d out/map.json -o site
   ```

4. Check without writing anything. Run this after you change the config or
   the code that the tour names:

   ```sh
   acu check
   ```

   `acu` exits with status 1 and a count of problems when it finds any. Use
   it in CI.

`acu` prints one summary line after `extract` and `check`. It counts the
containers, symbols, edges, entry points, regions and tour steps.

### Try an example

The examples need no setup of your own.

1. A map with no extra downloads. The Python fixture needs only `python3`:

   ```sh
   ./target/release/acu serve -c examples/lending-python/atlas.yaml
   ```

   The other fixtures sit in the same folder: `lending` (Go),
   `lending-js`, `lending-rust` and `mixed` (Go and TypeScript together).

2. A map of a public project. The projects sit in `corpus/` as shallow
   submodules, and a plain clone leaves them out. Fetch one, then serve its
   example:

   ```sh
   git submodule update --init --checkout --depth 1 corpus/go/chi
   ./target/release/acu serve -c examples/chi/atlas.yaml
   ```

   Ten examples read public projects: `chi`, `cobra`, `immer`, `hono`,
   `commander`, `flask`, `realworld`, `svelte-changelog`, `ripgrep` and
   `axum`. Each one reads the submodule in `corpus/` that carries its name.

3. The map of this project. It needs every adapter: Go, TypeScript, Rust and
   Python. Run it in the repository root:

   ```sh
   ./target/release/acu serve
   ```

   Every unit, file and function on that map carries a doc comment.
   `scripts/check.sh` fails when one does not.

## Troubleshooting

| Message or symptom | Cause and fix |
| --- | --- |
| `no program runs the <name> adapter` | `acu` found no adapter. Build it, put it on the `PATH`, set `ACU_ADAPTER_<NAME>`, or give `command` in `atlas.yaml`. |
| `the TypeScript adapter ... has no build yet` | Run `npm ci && npm run build -w adapters/typescript`. |
| The map shows a placeholder page, "This acu binary holds no viewer" | You built `acu` before the viewer. Run `npm run build -w viewer`, then `cargo build --release --workspace` again. `cargo` warns about `viewer/dist` while it builds. |
| The Rust adapter reports a missing toolchain | Install the pinned nightly with the `rustup` command above. |
| `breaks the config schema` | The message lists each problem and its place in `atlas.yaml`. |
| A tour step points at nothing | The code moved. Fix the step's `name`, `path` or `id`. |
| The port stays busy | Give `acu serve --addr 127.0.0.1:<another port>`. |
| `the request lacks the session token` | Open the address with `?token=` that `acu` printed. An MCP client sends `Authorization: Bearer <token>`. |
| `the server answers only requests for its own address` | Open `127.0.0.1` or `localhost` with the port that `acu` printed. For a forwarded name, start `acu serve` again with `--allow-host` and the `Host` that the message names. |

## Develop

The `toolchain` stage of the [`Dockerfile`](Dockerfile) pins Go, Node, Rust
and the Chromium that the smoke test uses. Three places use it:

1. **VS Code.** Open the folder and choose "Reopen in Container". The
   devcontainer installs the dependencies, fetches the test corpus, installs
   the Go adapter and builds the TypeScript adapter.
2. **The command line.**

   ```sh
   docker build --target toolchain -t acu-toolchain .
   docker run --rm -v "$PWD:/workspace" acu-toolchain scripts/check.sh
   ```

3. **CI**, which runs the same `scripts/check.sh` in the same container. It
   skips draft pull requests and changes to docs alone.

`scripts/check.sh` runs every check: formatting, lints, unit tests, golden
files, count snapshots and the smoke test. `scripts/check.sh --quick` skips
the end-to-end run. [`CONTRIBUTING.md`](CONTRIBUTING.md) describes the
workflow. Coding agents read [`AGENTS.md`](AGENTS.md). The roadmap lives in
issue #2.

## Repository layout

| Path | Holds |
| --- | --- |
| `atlas.yaml`, `tsconfig.atlas.json` | The map of this project. |
| `schema/` | The data format and the config format, as JSON Schema. |
| `adapters/go/` | The Go adapter. |
| `adapters/typescript/` | The TypeScript adapter. |
| `adapters/rust/` | The Rust adapter. |
| `adapters/python/` | The Python adapter. |
| `cli/` | `acu`, the command (Rust). It embeds the built viewer. |
| `viewer/` | The viewer: TypeScript, one canvas plus a docs panel, no framework. |
| `examples/` | Configs for the test fixtures and for the public corpus. |
| `corpus/` | Public projects, pinned as shallow submodules, for testing. |
| `tests/e2e/` | The smoke test, the count check and the doc check. |
| `Dockerfile`, `.devcontainer/` | The pinned toolchain, the release image and the devcontainer. |
| `scripts/` | `check.sh` (every check), `check-pins.sh`, `docker-export.sh`, `release-pack.sh`. |
| `action.yml`, `.github/workflows/` | The GitHub Action for users; CI, the image check and the release. |
| `docs/` | The data format, the config, adapter notes, and the research. |

## Design

The design follows research on how developers read code
([`docs/research/report.md`](docs/research/report.md)):

1. One focus item and its direct relations, about twenty boxes, work better
   than a map of everything.
2. Incoming relations sit on the left and outgoing relations on the right.
   Each lane carries a verb. Arrows point at the thing used.
3. Names and first doc sentences appear on every box, because readers
   navigate by them.
4. Colour never works alone. Every relation also has a dash pattern and a text
   label, and the palette avoids red-green pairs.
5. Reference (the map and the panel) and explanation (region notes, the tour)
   stay apart. Text that a program drafted carries a visible label until a
   person checks it.

## Status

The data format (version 1), the four adapters, `acu` and the viewer all
work. Svelte and SvelteKit ride on the TypeScript adapter. The MCP server in
`acu serve`, which lets a language model read the map and draft regions and
tours, remains planned.

## Licence

Atlas Carries Us carries the MIT licence. The viewer ships the Inter, Fraunces
and JetBrains Mono fonts under the SIL Open Font License 1.1. The Ahro mark
belongs to Ahro. The projects under `corpus/` keep their own licences: chi
(MIT), Cobra (Apache 2.0), Immer (MIT), Hono (MIT), RealWorld for SvelteKit
(MIT) and Svelte Changelog (MIT).
