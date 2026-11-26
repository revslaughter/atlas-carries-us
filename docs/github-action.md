# The GitHub Action

This repository carries a GitHub Action in [`action.yml`](../action.yml). It
maps your repository with `acu extract`, writes the map as a static site with
`acu build`, and can hand the site to GitHub Pages. A workflow that runs it
on each push to the default branch keeps the map in step with the code.

> **Warning:** Pages from a private repository stays public unless the
> account has GitHub Enterprise Cloud. With `publish: true`, anyone with the
> link can read the map of a private repository: every name, every doc
> comment, and the source links. Leave `publish` at `false` for private code,
> and keep the site as a workflow artifact instead (see "Keep the map
> private" below).

## Quick start

1. Add an `atlas.yaml` to the repository root. [`config.md`](config.md)
   describes it; the smallest one names a project and one language.
2. In the repository's settings, choose **Pages**, then **Source: GitHub
   Actions**.
3. Copy [`examples/github-pages/atlas-pages.yml`](../examples/github-pages/atlas-pages.yml)
   to `.github/workflows/atlas.yml`.
4. Push to the default branch. The workflow's summary links to the map.

The example workflow holds two jobs:

```yaml
jobs:
  map:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: revslaughter/atlas-carries-us@v0.1.0
        with:
          publish: true

  deploy:
    needs: map
    runs-on: ubuntu-latest
    permissions:
      pages: write
      id-token: write
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - id: deployment
        uses: actions/deploy-pages@v4
```

1. The job `map` runs the action. With `publish: true`, the action uploads
   the site with `actions/upload-pages-artifact`.
2. The job `deploy` publishes that upload with `actions/deploy-pages`. It
   alone holds the `pages: write` and `id-token: write` permissions, and it
   names the `github-pages` environment, as GitHub Pages requires. An action
   cannot name an environment for its job, so the deploy stays in a job of
   your workflow.

## Inputs

| Input | Default | Meaning |
| --- | --- | --- |
| `config` | `atlas.yaml` | The path of the config, relative to the repository root. |
| `data` | `atlas.json` | The path of the data that `acu extract` writes. |
| `output` | `site` | The folder that `acu build` writes the site to. |
| `version` | empty | The `acu` release to run, such as `0.2.0` or `v0.2.0`, or `latest`. Empty takes the version in `Cargo.toml` at the action's own commit: `@v0.2.0`, or the commit SHA of that release, runs `0.2.0`. |
| `image` | `ghcr.io/revslaughter/atlas-carries-us` | The image to run `acu` from, without a tag. Change it for a fork or a mirror. |
| `publish` | `false` | `true` uploads the site as the Pages artifact. |

Every path stays inside the repository: the action refuses an absolute path
and a path with `..`.

## Outputs

| Output | Holds |
| --- | --- |
| `data` | The path of `atlas.json`. |
| `site` | The path of the site folder. |
| `image` | The image and tag that ran `acu`. |

## Why the action runs the container image

The action runs `acu` from the runtime image that each release publishes to
the GitHub Container Registry, and not from a release archive. The reasons:

1. **Every adapter works without setup.** The image holds Go, Node, CPython
   and the pinned Rust nightly. With the archive, each workflow would need
   `actions/setup-go`, `actions/setup-node`, `actions/setup-python` and a
   `rustup` install of the nightly, and would have to keep their versions in
   step with the adapters.
2. **One download.** The runner pulls one image. The nightly alone takes
   minutes to install with `rustup`.
3. **The same programs as everywhere else.** The image comes from the same
   Dockerfile that CI tests, with the same pinned versions.

The image has costs too:

1. It runs on Linux runners only. The action stops with a message on macOS
   and Windows runners.
2. The image carries four toolchains. It takes about 600 MB compressed and
   2.6 GB unpacked, against about 15 MB for an archive. A pull takes about a
   minute on a GitHub-hosted runner.
3. The container runs as root. After the run, the action changes the owner
   of the data file, the site, and each folder that `acu` made above them
   (from the first part of each path down) to the runner's user, so later
   steps can change or delete them. cargo writes the Rust adapter's build
   output to `/tmp` inside the container (`CARGO_TARGET_DIR`), so no
   `target/` folder owned by root stays in the repository.

## What your repository may need first

The image covers the adapters' tools, not your project's own dependencies.

1. **TypeScript.** The adapter reads types from your dependencies in
   `node_modules`. Run `npm ci` (or your package manager's install) before
   the action; otherwise types from packages stay unresolved.
2. **Go.** The adapter runs `go list`, which downloads the modules in
   `go.sum` from the module proxy on each run.
3. **Rust.** rustdoc builds each crate's dependencies, so cargo fetches them
   from crates.io on each run.
4. **Python.** The adapter reads the source alone. It needs no install.

## Pin a version

`uses: revslaughter/atlas-carries-us@v0.1.0` pins the action, and through
it the image `0.1.0`: with `version` empty, the action reads the version
from `Cargo.toml` in its own checkout. The commit SHA of a release tag pins
the same way, and also guards against a moved tag.

A branch such as `@main` runs the action's newest steps, with the image of
the version that `Cargo.toml` names on that branch. That image may not
exist until its release, and the branch may change without notice. Pin a
tag or a SHA in workflows that others depend on, and move it with each
release.

## Keep the map private

Leave `publish` at `false` and upload the site as a workflow artifact.
People with read access to the repository can download it from the run's
page:

```yaml
      - uses: revslaughter/atlas-carries-us@v0.1.0
      - uses: actions/upload-artifact@v4
        with:
          name: atlas
          path: site
```

To read it, unpack the artifact and serve the folder with any static file
server, or run `acu serve -d atlas.json` with the data from the same run.

## Map on pull requests

The action works on any event. A workflow on `pull_request` with
`publish: false` checks that the config still matches the code: `acu
extract` fails when a tour step points at nothing.
