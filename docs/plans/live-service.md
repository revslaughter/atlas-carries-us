# Plan: from the local studio to a hosted live service

This plan proposes how Atlas Carries Us could run as a hosted service later,
without giving up the local-first design of `acu studio`
([`studio.md`](../studio.md)). The research behind it sits in
[`research/notes/live-service.md`](../research/notes/live-service.md); the
numbers in brackets below name its sections. Read issue #40 first.

## Principles

1. **`atlas.yaml` in git stays the only source of truth.** Every durable
   product in the survey keeps its human text in the repository; a
   hosted-only product can vanish with its vendor (1). The service stores
   maps, never the only copy of a person's text.
2. **Edits become pull requests.** A hosted editor never writes to the
   default branch. Review in the pull request replaces real-time merging
   (2), and the repository's owners keep control.
3. **Extraction counts as running untrusted code.** Rust build scripts and
   procedural macros run by design, and Go can run code through cgo flags
   and the `go.mod` toolchain line (3.1). A host must sandbox every
   adapter run.
4. **Read and write stay apart.** As Backstage TechDocs and Sourcegraph do,
   one isolated job builds the map, and a separate, read-only tier serves it
   (1, 3.3).
5. **The local studio keeps working without the service.** The service adds
   hosting and sharing; it takes nothing away from `acu studio`.

## Phases

### Phase 1: the local studio (now)

`acu studio` and the editor in `acu serve -c` (issue #40). A person picks a
folder or clones a repository, extracts, edits the regions and the tour, and
commits `atlas.yaml` with their own git tools.

### Phase 2: shared, read-only hosting of built sites

1. The GitHub Action already builds the site in CI and uploads it to GitHub
   Pages ([`github-action.md`](../github-action.md)). Public projects can
   use it today, on their own CI minutes (5.3).
2. A hosted tier adds the same for private repositories: a GitHub App
   receives push webhooks, builds the map in a sandbox, and serves it behind
   the organisation's sign-in.
3. The served MCP endpoint stays read-only: it serves the map of one commit,
   and its write tools stay off (5.1).

### Phase 3: hosted editing through pull requests

1. The hosted viewer offers the studio's editor.
2. A save goes to an edit API with the same request shape as the local
   `/api/tour`, `/api/regions` and `/api/project` calls.
3. The edit API applies the change with acu's own `atlas.yaml` editor, so
   comments and layout stay as people wrote them. It commits the result on
   a branch such as `atlas/edit-<user>` and opens or updates a pull request.
4. The revision check turns into the git blob SHA of `atlas.yaml` on the
   branch; a stale save gets 409 (7.3, 8).
5. CI on the pull request runs `acu check`, so a step whose target vanished
   fails before a merge (7.4).

### Phase 4: collaborative editing (later, if people ask)

Two people who edit one tour at the same moment need a merge of concurrent
changes. CRDT libraries offer that, but Yjs has no move operation for lists,
and Automerge's move exists only as research (2). Tour editing consists
mostly of moves. So postpone this phase. When it comes, use a CRDT as a live
session layer that writes back to `atlas.yaml` through phase 3, not as a
second store.

## Architecture

```text
push ──▶ webhook receiver ──▶ job queue ──▶ extraction worker (microVM)
                                                │
                                                ▼
            CDN (static viewer) ◀── object storage: <repo>/<commit>/<acu>/atlas.json
                    │
                    ▼
               edit API ──▶ branch + pull request on atlas.yaml
```

1. **Webhook receiver.** Checks the `X-Hub-Signature-256` HMAC over the raw
   body, in constant time, before anything else (4.1). Enqueues
   `(repository, commit)` and drops duplicates.
2. **Job queue.** One job per repository, commit, hash of `atlas.yaml`, and
   acu version (5.1). A known key skips the job.
3. **Extraction worker.** For each job (3.4):
   1. Start a fresh Firecracker or Kata microVM. Destroy it after the job.
      Hardware virtualisation resists escape better than namespaces (3.2).
   2. Clone with a GitHub App installation token for one repository. The
      token expires after one hour; drop it before any adapter runs (3.4,
      4.1).
   3. Fetch dependencies with network access, without running a build.
      `cargo fetch` runs no dependency code (3.4).
   4. Run the adapters with outbound network denied, with limits on CPU,
      memory, disk, processes and time. Set `GOTOOLCHAIN=local` and
      `GOFLAGS=-mod=readonly`; never run `npm install` scripts (3.4).
   5. Upload `atlas.json` through a pre-signed URL scoped to one object.
4. **Object storage.** Key `repo/commit/acu-version/atlas.json`, with a
   `latest` pointer per branch (5.1).
5. **Viewer on a CDN.** The static viewer loads `atlas.json` by URL and
   needs no server code (5.1).
6. **Edit API.** Phase 3. It needs `contents: write` and
   `pull_requests: write`; extraction needs only `contents: read` (4.1).

GitLab offers push webhooks and group access tokens that create a bot user
(4.2). The same design fits it, with a token per group.

## Security

1. The microVM, the dropped token and the closed network bound the damage of
   hostile code in a repository (3.4).
2. A job never reuses another tenant's build cache (5.3).
3. The edit API authenticates the person, checks their access to the
   repository through the GitHub App, and writes only `atlas.yaml`.
4. The viewer escapes all text from the map and the config (7.5).
5. The local studio's guards stay in place for local use (6.2). Before any
   hosting, the local studio also gains a session secret, as Jupyter uses
   (6.1, 6.2), so other users on a shared machine cannot call its API.

## Cost

1. Storage costs little. On Cloudflare R2, at $0.015 per GB-month and no
   egress fee, a 5 MB map for each of 10,000 commits costs about $0.75 a
   month (5.3).
2. Compute costs the most: the Rust adapter compiles dependencies. Cache
   them per lockfile hash within one repository (5.2, 5.3).
3. Keep maps for branch heads and tags; expire other commits after some days
   (5.3).
4. Public projects can keep using the GitHub Action on their own minutes
   (5.3).

## Prepare in the code now

These changes help the hosted service and also help the local studio:

1. **Cache adapter output apart from the merged map.** A push that changes
   only `atlas.yaml` then needs no adapter run, and a Go-only change skips
   the Rust adapter (5.2). Locally, a config edit already reuses the run in
   memory; a cache on disk would also survive a restart.
2. **Stable IDs for regions and steps.** Regions have IDs; tour steps do
   not. An optional `id` on each step lets a three-way merge work per step,
   not per line (2). This adds an optional field, so it keeps version 1 of
   the config format.
3. **One request shape.** Keep the studio API small HTTP and JSON. The
   hosted edit API takes the same bodies with a pull request backend in
   place of the file backend (8).
4. **Pluggable auth.** A session secret locally, a GitHub App token hosted
   (8).
5. **Broken steps.** After each extraction, mark a step or region whose
   target vanished, and let `acu check` fail on it in CI behind a flag
   (7.4).

## Open questions

1. Which sandbox to run: Firecracker needs KVM, which only some cloud
   machine types expose (3.2). Kata on Kubernetes may suit a small team
   better.
2. Whether to offer a restricted mode that skips the Rust adapter's build
   scripts and procedural macros, at the cost of precision (6.5).
3. How long to keep maps of old commits, and whether to keep one per tag
   forever.
4. Whether the hosted MCP endpoint should accept drafts and turn them into
   pull requests, as the edit API does.
5. How GitLab's tokens compare with GitHub's fine-grained installation
   tokens; the research did not check this in depth (Gaps, 4).
