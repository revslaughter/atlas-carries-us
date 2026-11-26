# Install acu

This file tells you how to install `acu` and its adapters in three ways:
from a release archive, from the container image, or from source. To map a
repository on every push with GitHub Actions, read
[`github-action.md`](github-action.md) instead.

| Way | Best for | You install yourself |
| --- | --- | --- |
| Release archive | A laptop or a server, on Linux, macOS or Windows | The tools of each language that you map |
| Container image | Linux, CI, or no local tools at all | Docker |
| Source | Work on `acu` itself | Rust, Node, and the tools of each language |

## What each adapter needs at run time

`acu` runs one adapter for each entry in `languages` of `atlas.yaml`. Each
adapter runs the tools of its own language, so the machine needs those tools
for every language that you map, and only for those.

| Adapter | Needs at run time | Install it with |
| --- | --- | --- |
| `go` | Go 1.24 or later (`go` on the `PATH`), because it loads packages with `go list` | <https://go.dev/dl/> |
| `typescript` | Node 20 or later (`node` on the `PATH`). It reads TypeScript, JavaScript and Svelte. | <https://nodejs.org/> |
| `rust` | The nightly `nightly-2026-11-01`, with `rust-src` and `rust-analyzer` | `rustup toolchain install nightly-2026-11-01 --profile minimal --component rust-src,rust-analyzer` |
| `python` | Python 3.12 or later (`python3` on the `PATH`; `python` on Windows) | <https://www.python.org/downloads/> |

The container image holds all of these tools. The release archives hold
none of them.

## From a release archive

Each release on the
[releases page](https://github.com/revslaughter/atlas-carries-us/releases)
carries one archive for each platform, and a file `SHA256SUMS`:

| System | Archive |
| --- | --- |
| Linux, x86-64 | `acu-<version>-linux-amd64.tar.gz` |
| Linux, arm64 | `acu-<version>-linux-arm64.tar.gz` |
| macOS, Apple silicon | `acu-<version>-darwin-arm64.tar.gz` |
| macOS, Intel | `acu-<version>-darwin-amd64.tar.gz` |
| Windows, x86-64 | `acu-<version>-windows-amd64.zip` |
| Windows, arm64 | `acu-<version>-windows-arm64.zip` |

Each archive holds one folder, `acu-<version>-<os>-<arch>/`:

| File | Holds |
| --- | --- |
| `acu` (`acu.exe`) | The command, with the viewer inside it. |
| `acu-adapter-go` (`.exe`) | The Go adapter. |
| `acu-adapter-rust` (`.exe`) | The Rust adapter. |
| `acu-adapter-typescript/` | The TypeScript adapter: a Node package with its dependencies. |
| `acu-adapter-python` (`acu-adapter-python.pyz` on Windows) | The Python adapter, as one zip application that Python runs. |
| `LICENSE`, `README.md` | The licence and the project's README. |

`acu` finds its adapters in the folder that holds it, also when you start
`acu` through a symbolic link. You need no `PATH` setup for the adapters.
On Windows, `acu` runs `acu-adapter-python.pyz` with the first of `py -3`,
`python` and `python3` that it finds on the `PATH`.

### Steps

1. Download the archive for your system and `SHA256SUMS` from the same
   release.
2. Check the archive:

   ```sh
   sha256sum --check --ignore-missing SHA256SUMS                # Linux
   grep darwin-arm64 SHA256SUMS | shasum -a 256 --check         # macOS
   ```

   On Windows, in PowerShell, compare the output of
   `Get-FileHash acu-<version>-windows-amd64.zip` with the line in
   `SHA256SUMS`.

3. Unpack it, and keep the folder together:

   ```sh
   tar -xzf acu-<version>-linux-amd64.tar.gz
   ./acu-<version>-linux-amd64/acu --version
   ```

   On Windows, unpack the `.zip` with Explorer or with
   `Expand-Archive acu-<version>-windows-amd64.zip`.

4. Optional: put the folder on your `PATH`, or link `acu` into a folder on
   it with a symbolic link. Move the whole folder, never `acu` alone: `acu`
   looks for its adapters beside the real program.
5. Install the tools of each language that you map (see the table above).

### Notes per system

1. **Linux.** The programs need glibc 2.28 or later. `acu-adapter-go` runs
   on any Linux.
2. **macOS.** The programs need macOS 12 or later. Nothing signs them, so a
   download through a browser carries the quarantine mark, and Gatekeeper
   refuses to run them. Remove the mark from the folder:

   ```sh
   xattr -dr com.apple.quarantine acu-<version>-darwin-arm64
   ```

   The Python adapter starts with `#!/usr/bin/env python3`. The `python3`
   that macOS ships may predate 3.12; install a newer one, or set
   `ACU_ADAPTER_PYTHON` to `python3.13 /path/to/acu-adapter-python`.
3. **Windows.** The Rust programs link against the MinGW runtime that they
   carry, and need no extra DLL.

## From the container image

Each release also publishes the image
`ghcr.io/revslaughter/atlas-carries-us`, for `linux/amd64` and
`linux/arm64`. It holds `acu`, the four adapters, and every tool that the
adapters need: Go, Node, CPython 3.13 and the pinned nightly.

| Tag | Names |
| --- | --- |
| `0.2.0` | The release `v0.2.0`. Use a version tag in scripts and CI. |
| `latest` | The newest release that carries no hyphen, such as `v0.2.0`, but not `v0.3.0-rc.1`. |

1. Pull the image:

   ```sh
   docker pull ghcr.io/revslaughter/atlas-carries-us:latest
   ```

2. Run `acu` in your repository. The image runs `acu` in `/work`; mount the
   repository there:

   ```sh
   docker run --rm -v "$PWD:/work" ghcr.io/revslaughter/atlas-carries-us:latest extract -c atlas.yaml -o atlas.json
   docker run --rm -v "$PWD:/work" ghcr.io/revslaughter/atlas-carries-us:latest build -d atlas.json -o site
   ```

3. To read the map in a browser, serve it, and give `--addr 0.0.0.0:4321`
   so that the port reaches the host:

   ```sh
   docker run --rm -v "$PWD:/work" -p 4321:4321 ghcr.io/revslaughter/atlas-carries-us:latest \
     serve -c atlas.yaml --addr 0.0.0.0:4321
   ```

   Then open <http://127.0.0.1:4321/>.

The container runs as root, so on Linux the files it writes belong to root.
The Go and Rust adapters need root in the image: Go and cargo write to
folders that root owns. So keep root, and change the owner of the files
afterwards:

```sh
docker run --rm -v "$PWD:/work" --entrypoint chown ghcr.io/revslaughter/atlas-carries-us:latest \
  -R "$(id -u):$(id -g)" atlas.json site
```

To build the image yourself from a checkout, run `docker build -t acu .`
(see the README for builds behind a proxy or a mirror).

## From source

The README's section "Route C: compile from source" lists the tools and the
steps. In short:

```sh
npm ci
npm run build -w viewer
cargo build --release --workspace
npm run build -w adapters/typescript
```

To build release archives yourself without Rust or Node, use Docker:

```sh
scripts/docker-export.sh --platform darwin/arm64 out/acu
scripts/release-pack.sh out/acu v0.2.0 darwin/arm64 out/dist
```

`scripts/docker-export.sh` builds the Dockerfile's `export` stage for one
platform, and `scripts/release-pack.sh` packs that folder as a release
archive, as the release workflow does.

## How a release comes out

The maintainer pushes a tag such as `v0.2.0`. The workflow
[`.github/workflows/release.yml`](../.github/workflows/release.yml) then:

1. checks that the tag matches the version in `Cargo.toml`;
2. builds the `export` stage for each of the six platforms and packs each
   with `scripts/release-pack.sh`;
3. builds the `runtime` stage for `linux/amd64` and `linux/arm64` and pushes
   it to `ghcr.io/<owner>/atlas-carries-us`;
4. checks the Linux archive, writes `SHA256SUMS`, and creates the GitHub
   release with the archives and the sums.

A tag with a hyphen, such as `v0.3.0-rc.1`, makes a pre-release, and leaves
`latest` on the image where it stands. Only a push of a tag publishes. A run
by hand from the Actions tab, and a pull request that changes
`release.yml` or `scripts/release-pack.sh`, build everything and publish
nothing: a dry run.

Before each release, the maintainer:

1. sets `[workspace.package] version` in `Cargo.toml` to the new version;
2. changes each `uses: revslaughter/atlas-carries-us@v<version>` in
   `action.yml`, `examples/github-pages/atlas-pages.yml` and
   `docs/github-action.md` to the same version (`scripts/check-pins.sh`
   fails until they agree with `Cargo.toml`);
3. merges that change, then pushes the tag `v<version>` on the merge.

Once, after the first release:

1. Open the image's package settings on GitHub and set its visibility to
   public. GitHub creates a new package as private, and nobody outside the
   repository can pull it until then.
