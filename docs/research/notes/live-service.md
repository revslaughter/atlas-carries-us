# From a local studio to a live hosted service (as of Nov 2026)

This note studies two questions. First: how could the `acu studio` design
later grow into a hosted service? Second: what does the local-first studio
design miss today? Comparable products keep tour and doc text in the
repository, couple it to code, and flag drift in CI
([CodeTour](https://github.com/microsoft/codetour),
[Swimm](https://docs.swimm.io/features/commit-docs-to-repository)). Git as
storage plus pull requests gives ownership and review, but no real-time
co-editing
([Ink & Switch](https://www.inkandswitch.com/essay/local-first/)). A hosted
service must treat extraction as untrusted code execution, because
`build.rs`, proc macros, cgo flags and npm scripts all run code
([Rust project goal](https://rust-lang.github.io/rust-project-goals/2024h2/sandboxed-build-script.html),
[Go vulndb GO-2022-0476](https://go.googlesource.com/vulndb/+/bd3b53db9a0a/data/osv/GO-2022-0476.json)).
The local studio carries its own risks today: DNS rebinding, cross-site
requests to localhost, and git option injection through a pasted URL. Section
6 lists concrete requirements for those risks. Section 8 lists the decisions
to make now so that a hosted service stays possible later.

## 1. Comparable products

| Product            | Authoring and storage                                                                                                                                                            | Sync with code                                                                                                                                                                  | Source                                                                                                                                                                                                                                                                                                                                                                         |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| CodeTour           | Records steps in VS Code; steps on a line, directory, selection, or no file; reorder with up and down arrows or "Move Up"/"Move Down"; JSON files in `.tours/` in the repository | Pins a tour to a branch, commit or tag; a step can anchor on a regex `pattern`; "Change Git Ref" moves a tour; CodeTour Watch comments on pull requests that touch toured files | [README](https://github.com/microsoft/codetour), [codetour-watch](https://awesome.ecosyste.ms/projects/github.com%2Fpozil%2Fcodetour-watch)                                                                                                                                                                                                                                    |
| Swimm              | Markdown files in `.swm/` in the repository; a published copy on Swimm's servers for readers without code access                                                                 | Checks code couplings on each commit; "Auto-sync" fixes small drift; a person fixes the rest                                                                                    | [Swimm docs](https://docs.swimm.io/features/commit-docs-to-repository), [Atlassian Community](https://community.atlassian.com/t5/App-Central-articles/Get-a-Health-Score-amp-Auto-sync-Your-Docs-With-the-Swimm-and/ba-p/2135802), [Swimm help](https://swimm.zendesk.com/hc/en-us/articles/39139297934107-Keeping-Documentation-Up-to-Date-with-Swimm-s-Visual-Studio-Plugin) |
| CodeSee            | Hosted code maps and tours; GitKraken bought it in May 2024; no source confirms that tours survived                                                                              | Hosted                                                                                                                                                                          | [GitKraken](https://gitkraken.com/press/gitkraken-acquires-codesee-launches-devex-platform)                                                                                                                                                                                                                                                                                    |
| Sourcegraph        | No tours; one SCIP index per repository and commit                                                                                                                               | "Auto-indexing" on executors; CI indexing advised for builds with credentials                                                                                                   | [Sourcegraph](https://sourcegraph.com/docs/code-navigation/precise-code-navigation)                                                                                                                                                                                                                                                                                            |
| DeepWiki           | Generated wiki; `.devin/wiki.json` in the repository steers it with `repo_notes` and `pages` ("exactly the pages you specify")                                                   | Generated on connect                                                                                                                                                            | [Devin docs](https://docs.devin.ai/work-with-devin/deepwiki)                                                                                                                                                                                                                                                                                                                   |
| Google Code Wiki   | Generated wiki with diagrams and code links; public preview for public repositories                                                                                              | "regenerates the documentation after each change"                                                                                                                               | [Google blog](https://developers.googleblog.com/ja/introducing-code-wiki-accelerating-your-code-understanding/)                                                                                                                                                                                                                                                                |
| Backstage TechDocs | Docs built in CI, published to object storage; the backend only reads                                                                                                            | CI holds the write token; the backend a read-only one                                                                                                                           | [TechDocs architecture](https://backstage.io/docs/features/techdocs/architecture), [CI/CD](https://backstage.io/docs/features/techdocs/configuring-ci-cd)                                                                                                                                                                                                                      |

Lessons for acu:

1. Every durable product keeps human text in the repository. `atlas.yaml`
   already does. A hosted-only product can vanish, as CodeSee shows.
2. CodeTour has no drag and drop; it reorders with buttons and menu items.
3. Our steps anchor on IDs from qualified names, not lines. That survives
   line drift, but not a rename or a removal (section 7.4).
4. Swimm shows three useful states per item: fresh, auto-fixed and awaiting
   review, broken. Our `drafted` mark already means "awaiting review".
5. Generated wikis regenerate on each push and take a small steering file.
   A hosted acu should do the same, with `atlas.yaml` as the steering file.
6. TechDocs and Sourcegraph both split "build in an isolated job" from
   "serve read-only". `acu build` already writes a static site, and our
   GitHub Action already builds in CI ([`docs/github-action.md`](../../github-action.md)).

## 2. Local-first, CRDTs, and git as storage

1. The Ink & Switch essay names seven ideals: no spinners, work on many
   devices, an optional network, seamless collaboration, the long now,
   security and privacy by default, and user ownership and control
   ([Ink & Switch](https://www.inkandswitch.com/essay/local-first/)).
2. On git, the essay says: "Git works fully offline, it is fast, it gives
   full control to users." A local repository "is a primary copy of the
   data". But "Git has no capability for real-time, fine-grained
   collaboration", and merging "often results in merge conflicts"
   ([Ink & Switch](https://www.inkandswitch.com/essay/local-first/)).
3. CRDTs merge concurrent edits on their own and "can sync their state via
   any communication channel"
   ([Ink & Switch](https://www.inkandswitch.com/essay/local-first/)). Yjs and
   Automerge lead the field ([crdt.tech](https://crdt.tech/implementations)).
4. Yjs has no built-in move for `Y.Array`. Reordering means delete and
   insert, which can lose a concurrent edit to the moved item
   ([Yjs forum](https://discuss.yjs.dev/t/moving-elements-in-lists/92/3)). A
   move operation for JSON CRDTs exists as research with a plan to add it to
   Automerge ([arXiv 2311.14007](https://arxiv.org/html/2311.14007v1)).

Tour editing consists mostly of list reordering, and a CRDT would need a
second format beside `atlas.yaml`. Recommendation:

1. Keep `atlas.yaml` in git as the only source of truth.
2. Publish hosted edits through a branch and a pull request. Review in the
   pull request replaces real-time merge.
3. Give each region and each tour step a stable ID. A three-way merge then
   works per item, not per line.
4. Postpone CRDTs until two people must edit one tour at the same moment.
   Then use a CRDT as a live session layer that writes back to
   `atlas.yaml`, not as storage.

## 3. Running untrusted code analysis as a service

### 3.1 Extraction runs the repository's code

1. Build scripts and proc macros both execute arbitrary code; Cargo treats
   this as "by design"
   ([Rust project goal](https://rust-lang.github.io/rust-project-goals/2024h2/sandboxed-build-script.html)).
2. `cargo check`, `cargo test` and rust-analyzer compile dependencies and so
   run every `build.rs` and proc macro in the graph
   ([Rust internals](https://internals.rust-lang.org/t/build-security/24166)).
3. rust-analyzer enables build scripts and proc macros by default
   ([rust-analyzer config](https://rust-analyzer.github.io/book/configuration)).
   Its security notes say it "assumes that all code is trusted"
   ([mirror of rust-analyzer security.md](https://git.bendn.org/rust-analyzer/plain/docs/book/src/security.md)).
4. A `#cgo` directive with crafted flags let `go get` run code at build time
   ([GO-2022-0476](https://go.googlesource.com/vulndb/+/bd3b53db9a0a/data/osv/GO-2022-0476.json)).
   The `toolchain` line in `go.mod` once let the go command run scripts from
   the module root ([GO-2023-2042](https://go.googlesource.com/vulndb/+/d6ddad7c08ccb7d96332134c1a9de246fc44f5b1/data/reports/GO-2023-2042.yaml)).
5. Go states that `go list` should not run downloaded code, and treats any
   such path as a bug ([Go blog](https://go.googlesource.com/website/+/master/_content/blog/path-security.md)).
   The go command can still download a toolchain that `go.mod` names, unless
   `GOTOOLCHAIN=local` ([Safeguard](https://safeguard.sh/resources/blog/go-toolchain-distribution-security)).
6. npm runs lifecycle scripts such as `postinstall` on `npm install`.
   `--ignore-scripts` turns them off
   ([WordPress ticket](https://core.trac.wordpress.org/ticket/64543),
   [Sonar RSPEC-6505](https://rules.sonarsource.com/shell/RSPEC-6505)).

Consequence: the Rust adapter (rustdoc and rust-analyzer) runs repository
code by design. The Go adapter may run code through cgo. The TypeScript and
Python adapters run none, as long as nothing installs packages.

### 3.2 Sandbox choices

1. gVisor intercepts system calls in a user-space kernel. Firecracker runs
   each workload in a microVM. Kata wraps a VMM such as Firecracker or QEMU
   for Kubernetes ([Northflank](https://northflank.com/blog/kata-containers-vs-firecracker-vs-gvisor)).
2. Hardware virtualization boundaries resist escape better than namespace
   boundaries ([Unit 42](https://unit42.paloaltonetworks.com/making-containers-more-isolated-an-overview-of-sandboxed-container-technologies/)).
3. nsjail isolates one process with namespaces, resource limits and
   seccomp-bpf filters ([nsjail](https://github.com/google/nsjail)).
4. Firecracker needs KVM, which only some cloud machine types expose
   ([Sourcegraph Firecracker](https://sourcegraph.com/docs/self-hosted/executors/firecracker)).

### 3.3 How other services isolate builds

| Service               | Isolation                                                             | Limits                                            | Source                                                                                                                                                                                                                                                               |
| --------------------- | --------------------------------------------------------------------- | ------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GitHub-hosted runners | A new VM per job, decommissioned after; single-CPU runners share a VM | 15 min for single-CPU jobs                        | [GitHub docs](https://docs.github.com/en/actions/concepts/runners/github-hosted-runners)                                                                                                                                                                             |
| Sourcegraph executors | Firecracker microVM, or Docker or Kubernetes (weaker)                 | One repository clone per job                      | [Sourcegraph](https://sourcegraph.com/docs/admin/executors)                                                                                                                                                                                                          |
| Read the Docs         | A Docker container from the `build.os` image per build                | 15 to 30 min, 3 to 7 GB, by plan and docs version | [RTD build process](https://me-readthedocs.readthedocs.io/en/latest/builds.html)                                                                                                                                                                                     |
| Netlify               | Firecracker microVMs (earlier: Docker with git credentials dropped)   | 15 min by default                                 | [Netlify blog](https://www.netlify.com/blog/your-builds-just-got-faster), [2016 post](https://www.netlify.com/blog/2016/10/18/how-our-build-bots-build-sites/), [support guide](https://answers.netlify.com/t/support-guide-how-long-should-netlify-builds-take/151) |

### 3.4 Requirements for a hosted extraction worker

1. Run each job in a fresh microVM (Firecracker or Kata). Destroy it after
   the job.
2. Clone in a separate step that holds the repository token. Drop the token
   before any adapter runs, as Netlify does
   ([Netlify 2016](https://www.netlify.com/blog/2016/10/18/how-our-build-bots-build-sites/)).
3. Fetch dependencies in a separate step with network access, without running
   any build. `cargo fetch` runs no dependency code
   ([Rust internals](https://internals.rust-lang.org/t/build-security/24166)).
   Then run the adapters with egress denied.
4. Set CPU, memory, disk, process count and wall-clock limits per job.
5. Set `GOTOOLCHAIN=local` and `GOFLAGS=-mod=readonly`. Never run
   `npm install` scripts.
6. Give the job no cloud credentials. Return `atlas.json` through a
   pre-signed upload URL scoped to one object.

## 4. Auth and repository access

### 4.1 GitHub

1. GitHub recommends GitHub Apps over OAuth apps. An app requests
   fine-grained permissions and reaches only the repositories that the
   installation grants. An OAuth app reaches everything the user can reach
   ([GitHub docs](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/differences-between-github-apps-and-oauth-apps)).
2. An installation token expires after one hour. The token request can name
   up to 500 repositories and a smaller permission set
   ([GitHub REST](https://docs.github.com/en/rest/apps/installations),
   [token guide](https://docs.github.com/en/enterprise-server@3.15/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app)).
3. A GitHub App gets one webhook for all its repositories. An OAuth app must
   register a webhook per repository
   ([GitHub docs](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/differences-between-github-apps-and-oauth-apps)).
4. Validate each webhook with the `X-Hub-Signature-256` HMAC over the raw
   body. Compare in constant time. Validate before any other processing
   ([GitHub docs](https://docs.github.com/en/webhooks/using-webhooks/validating-webhook-deliveries)).

Permissions for acu: `contents: read` to extract. For the edit flow,
`contents: write` and `pull_requests: write` on a branch. Mint the token per
job, for one repository.

### 4.2 GitLab

1. Project and group webhooks fire on push. A push of more than three
   branches or tags triggers no webhook by default
   ([GitLab webhook events](https://docs.gitlab.com/user/project/integrations/webhook_events/),
   [GitLab webhooks](https://docs.gitlab.com/user/project/integrations/webhooks/)).
2. A group access token creates a bot user and works for the API and for git
   over HTTPS ([GitLab docs](https://docs.gitlab.com/ee/user/group/settings/group_access_tokens.html)).
3. GitLab sends a webhook seven days before a project or group token expires
   ([GitLab webhook events](https://docs.gitlab.com/user/project/integrations/webhook_events/)).

## 5. Architecture sketch for a hosted service

### 5.1 Components

1. **Webhook receiver.** Validates the signature. Enqueues `(repo, commit)`.
   Drops duplicates.
2. **Job queue.** One job per `(repo, commit, config hash, acu version)`.
3. **Extraction worker.** Section 3.4. Writes `atlas.json`.
4. **Object storage.** Key `repo/commit/acu-version/atlas.json`. Store a
   `latest` pointer per branch.
5. **Viewer on a CDN.** The static viewer loads `atlas.json` by URL. It needs
   no server code.
6. **Edit API.** Accepts a change to regions or the tour. Opens or updates a
   pull request that changes `atlas.yaml` on a branch. Never writes to the
   default branch.
7. **Read-only MCP.** Serves the map of one commit. Write tools stay off;
   drafts go through the edit API.

### 5.2 Incremental extraction

1. Key every result by commit. A known key skips the job.
2. A push that changes only `atlas.yaml` needs no adapter run. Re-apply the
   config to the cached adapter output of the same code tree. This needs
   `acu` to cache adapter output separately from the merged map.
3. Key adapter output by the hash of the files that the adapter reads
   (a git tree hash per language root). A Go-only change then skips the Rust
   adapter.
4. Cache Rust dependency builds per lockfile hash.

### 5.3 Cost notes

1. Cloudflare R2 storage costs $0.015 per GB-month with no egress fee
   ([R2 pricing](https://developers.cloudflare.com/r2/pricing/index.md)).
   A 5 MB map for each of 10,000 commits costs about $0.75 a month.
2. Compute dominates: the Rust adapter compiles dependencies. Reuse the
   dependency cache within one repository, never across tenants.
3. Keep branch heads and tags. Expire other commits after some days.
4. Run a free tier for public repositories on the GitHub Action, with the
   user's minutes.

## 6. Security of the local studio

### 6.1 Threats

1. **DNS rebinding.** A page on `attacker.example` changes its DNS to
   127.0.0.1. The browser then treats the local server as same-origin with
   the attacker page. The request carries `Host: attacker.example`
   ([NCC Group](https://github.com/nccgroup/singularity/wiki/Preventing-DNS-Rebinding-Attacks)).
   Real MCP servers shipped this flaw; the fix added Host and Origin checks
   ([SecureLayer7](https://securelayer7.net/lab/cve-2026-55637-genieacs-mcp-dns-rebinding-origin-validation),
   [GHSA-89vp-x53w-74fx](https://osv.dev/vulnerability/GHSA-89vp-x53w-74fx)).
2. **Cross-site requests to localhost.** Any web page can send a "simple"
   POST to `127.0.0.1` without a preflight. Zoom's local server on port
   19421 let any site start a call
   ([NVD CVE-2019-13450](https://nvd.nist.gov/vuln/detail/CVE-2019-13450)).
3. **The 0.0.0.0 bypass.** Browsers let public pages reach local services
   through `0.0.0.0` on macOS and Linux. Oligo advises authorization and
   CSRF tokens even on localhost
   ([Oligo](https://oligo.security/blog/0-0-0-0-day-exploiting-localhost-apis-from-the-browser)).
4. **Browser protection stays partial.** Chrome 142 asks permission before a
   public site reaches loopback (Local Network Access). Requests from one
   local server to another stay outside the first milestone. Other browsers
   differ ([Chrome blog](https://developer.chrome.com/blog/local-network-access)).
5. **Other local users and processes.** Any process on the machine can
   connect to 127.0.0.1. Jupyter therefore requires a token by default
   ([Jupyter Server security](https://jupyter-server.readthedocs.io/en/latest/operators/security.html)).

### 6.2 Requirements for `acu studio`

1. Bind to 127.0.0.1 (and optionally ::1) only. Refuse any other address for
   studio. The MCP spec says local servers "SHOULD bind only to localhost"
   ([MCP spec](https://modelcontextprotocol.io/specification/latest/basic/transports/streamable-http)).
2. Check `Host` on every request against an exact list:
   `127.0.0.1:<port>`, `localhost:<port>`, `[::1]:<port>`. Answer anything
   else, a missing `Host` included, with 403 or 400. A browser cannot forge
   `Host` ([NCC Group](https://github.com/nccgroup/singularity/wiki/Preventing-DNS-Rebinding-Attacks),
   [Go issue 23993](https://golang.org/issue/23993),
   [GHSA-89vp-x53w-74fx](https://osv.dev/vulnerability/GHSA-89vp-x53w-74fx)).
3. Check `Origin` on every request. When present, it must equal
   `http://` plus one of the allowed hosts. Else answer 403, as the MCP spec
   demands ([MCP spec](https://modelcontextprotocol.io/specification/latest/basic/transports/streamable-http)).
4. For each state-changing request (POST, PUT, PATCH, DELETE), apply the
   Fetch Metadata check: reject `Sec-Fetch-Site` other than `same-origin`
   or `none`. For the studio API, also require `Origin` to be present
   ([Filippo Valsorda](https://words.filippo.io/csrf/),
   [web.dev](https://web.dev/articles/fetch-metadata),
   [MDN](https://developer.mozilla.org/docs/Web/HTTP/Headers/Sec-Fetch-Site)).
5. Require a session secret. Generate 32 random bytes at start. Print a URL
   with the secret to standard error, as Jupyter does. Exchange it once for
   a cookie with `HttpOnly`, `SameSite=Strict` and `Path=/`. Then redirect
   to a URL without the secret
   ([Jupyter Server security](https://jupyter-server.readthedocs.io/en/latest/operators/security.html)).
   Compare in constant time.
6. Require a custom request header (for example `X-Acu-Studio: 1`) and
   `Content-Type: application/json` on every API call. A cross-site page
   cannot send either without a CORS preflight
   ([MDN CORS](https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/CORS)).
7. Never send `Access-Control-Allow-Origin`. Answer every `OPTIONS`
   preflight with 403.
8. Never change state on GET.
9. Send `Content-Security-Policy: default-src 'self'; frame-ancestors 'none'`,
   `X-Content-Type-Options: nosniff` and `Referrer-Policy: no-referrer`.
   `frame-ancestors` blocks clickjacking; `no-referrer` keeps the secret URL
   out of `Referer`
   ([MDN frame-ancestors](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/frame-ancestors),
   [MDN Referrer-Policy](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Referrer-Policy)).
10. Apply items 2 to 4 to `/mcp` on the studio port too. `acu serve` already
    checks Host and Origin ([`docs/mcp.md`](../../mcp.md)).
11. Pick a random free port by default. A fixed port lets a hostile page
    find the server.

### 6.3 Requirements for the folder browser

1. List directories only. Never return file contents through the browser
   API.
2. Start at the home directory. Allow only paths under a root list (home by
   default).
3. Canonicalize each requested path (resolve `..` and symlinks) before the
   root check. Check again after canonicalization
   ([OWASP path traversal](https://owasp.org/www-community/attacks/Path_Traversal)).
4. Return names, not absolute paths of symlink targets. Skip entries that
   resolve outside the roots.
5. Hide dot directories by default. Cap entries per answer.
6. Accept a path only as JSON in a POST body that passes 6.2. Never take it
   from a query string.

### 6.4 Requirements for git clone of a pasted URL

1. Parse the URL. Accept `https://` only by default. Offer `ssh://` and
   `git@host:path` only behind an explicit option. Reject `file:`, `ext::`,
   any `<transport>::` form, local paths, a leading `-`, whitespace and
   control characters. `ext::` runs a command named in the URL
   ([git-remote-ext](https://git-scm.com/docs/git-remote-ext)).
2. Run git with an argument vector, never through a shell.
3. Put `--` before the URL: `git clone [options] -- <url> <dir>`. Clone
   follows the POSIX convention, so `--` ends options there
   ([nesbitt.io](https://nesbitt.io/2026/07/21/end-of-options.html)).
   Wrappers that skipped this let `--upload-pack` or `-u` run commands
   ([GHSA-8jmw-wjr8-2x66](https://github.com/advisories/GHSA-8jmw-wjr8-2x66),
   [GitPython advisory](https://advisories.gitlab.com/pypi/gitpython/GHSA-v396-v7q4-x2qj/)).
4. Set `GIT_ALLOW_PROTOCOL=https` (or `https:ssh`). Git then treats every
   other protocol as `never`. Also set `GIT_PROTOCOL_FROM_USER=0`, which
   git's docs suggest "for programs which feed potentially-untrusted URLs to
   git commands" ([git-config protocol](https://git-scm.com/docs/git-config),
   [git(1)](https://git-scm.com/docs/git)).
5. Do not recurse into submodules (`--no-recurse-submodules`). Recursive
   clones of hostile repositories gave code execution in CVE-2024-32002
   ([NVD](https://nvd.nist.gov/vuln/detail/cve-2024-32002)). Local clones
   followed symlinks in CVE-2022-39253; git then set `protocol.file.allow`
   to `user` ([NVD](https://nvd.nist.gov/vuln/detail/CVE-2022-39253),
   [GitHub blog](https://github.blog/2022-10-17-git-security-vulnerabilities-announced)).
6. Check the git version at start. Refuse versions older than the latest
   security release line that the studio tests against (at least 2.45.1).
7. Set `GIT_TERMINAL_PROMPT=0` so that git never waits for a password on the
   server's terminal ([git(1)](https://git-scm.com/docs/git)).
8. Clone with `--depth 1 --single-branch` into a new empty directory that
   acu creates. Refuse an existing non-empty target.
9. Validate a user-given branch name with `git check-ref-format --branch`.
   Pass it as `--branch=<name>` so that it cannot become an option.
10. Set a wall-clock timeout and a size cap. Kill the process group on
    either.
11. Treat the clone as untrusted after the clone. Section 6.5 applies.

### 6.5 Extraction of an untrusted folder

1. Before the first extraction of a cloned or unknown folder, show a trust
   prompt. Say which adapters run code (Rust always; Go with cgo). VS Code
   uses the same idea as "Workspace Trust"
   ([VS Code docs](https://code.visualstudio.com/docs/editing/workspaces/workspace-trust)).
2. Offer a restricted mode: skip the Rust adapter, or run rust-analyzer with
   `cargo.buildScripts.enable` and `procMacro.enable` off. Analysis gets
   less precise ([rust-analyzer config](https://rust-analyzer.github.io/book/configuration)).
3. Set `GOTOOLCHAIN=local` for the Go adapter in every mode
   ([Safeguard](https://safeguard.sh/resources/blog/go-toolchain-distribution-security)).
4. Never run `npm install` for the user's project. If ever needed, pass
   `--ignore-scripts`.
5. Offer "run in the container image" as the safe route for strangers' code.

## 7. Editor features the studio may miss

### 7.1 Accessible reordering

1. WCAG 2.2 criterion 2.5.7 (AA) requires a single-pointer way to do
   anything that dragging does. Move-up and move-down buttons satisfy it
   ([W3C Understanding 2.5.7](https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements)).
2. A text field with the step number also counts as an alternative
   ([W3C Understanding 2.5.7](https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements)).
3. The keyboard pattern: Space picks up, arrows move, Space drops, Escape
   cancels ([dnd-kit guide](https://mintlify.com/clauderic/dnd-kit/guides/accessibility)).
4. Announce each change in a live region, for example "Step 3, grabbed.
   Position 3 of 8." ([Salesforce UX](https://medium.com/salesforce-ux/4-major-patterns-for-accessible-drag-and-drop-1d43f64ebf09),
   [Smashing Magazine](https://www.smashingmagazine.com/2018/01/dragon-drop-accessible-list-reordering/)).
5. Keep focus on the moved item after each move
   ([DEV guide](https://dev.to/therizwansaleem/building-an-accessible-reorderable-list-in-frontend-apps-3co1)).
6. GitHub's Primer team found few accessible solutions in the industry and
   documents its own pattern
   ([GitHub blog](https://github.blog/engineering/user-experience/exploring-the-challenges-in-creating-an-accessible-sortable-list-drag-and-drop/),
   [Primer](https://primer.style/accessibility/patterns/drag-and-drop)).

Recommendation: ship buttons and keyboard moves first. Add drag and drop
later as an extra.

### 7.2 Undo

Keep an in-memory stack of whole-config snapshots; a config stays small.
Bind Ctrl+Z and Ctrl+Shift+Z and show buttons. Make delete undoable instead
of asking for confirmation. Git gives undo across sessions: offer "discard
changes since the last commit".

### 7.3 Conflict detection when the file changes on disk

1. Return a version tag with every read of `atlas.yaml` (a hash of the
   bytes). Require it in an `If-Match` header on each write. Answer a
   mismatch with 412 Precondition Failed
   ([MDN If-Match](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/If-Match),
   [MDN conditional requests](https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/Conditional_requests)).
2. On 412, show a diff between the user's version and the disk version. Let
   the user merge per region or per step.
3. Write through a temporary file and an atomic rename.
4. Watch the file and tell the user when an editor, `git pull` or the MCP
   write tools change it. `docs/mcp.md` notes that two servers on one config
   overwrite each other today ([`docs/mcp.md`](../../mcp.md)).
5. Route MCP draft writes through the same versioned write path.

### 7.4 Stale steps and regions

1. After each extraction, resolve every step target and every region member
   against the new map.
2. Mark a step "broken" when its target ID vanished. Show it in the list,
   never drop it silently.
3. Suggest a repair: the same name in another container, or a symbol with
   the same doc comment. Mark the repair as `drafted`, as Swimm marks
   auto-synced docs for review
   ([Atlassian Community](https://community.atlassian.com/t5/App-Central-articles/Get-a-Health-Score-amp-Auto-sync-Your-Docs-With-the-Swimm-and/ba-p/2135802)).
4. Store the commit at which a person last checked each step. Flag a step
   "maybe stale" when its target's source changed after that commit, as
   CodeTour Watch flags tours on pull requests
   ([codetour-watch](https://awesome.ecosyste.ms/projects/github.com%2Fpozil%2Fcodetour-watch)).
5. Let `acu check` fail on broken steps in CI, behind a flag.

### 7.5 Other items

Play the tour from any step in the real viewer. Show the save state
("saved", "unsaved", "conflict") at all times. Show `git status` for
`atlas.yaml` and offer a commit, never a push. Escape all text in the
viewer. Show `drafted.by` and offer "mark checked".

## 8. Decisions to make now for a later hosted service

1. Keep the studio API small HTTP+JSON. A hosted edit API reuses it with a
   pull request backend instead of a file backend.
2. Separate "edit `atlas.yaml`" from "run adapters"; a host sandboxes only
   the second.
3. Cache adapter output apart from the merged map (5.2).
4. Version each write with a content hash (7.3); hosted, a git blob SHA.
5. Add stable IDs to regions and steps (2).
6. Keep auth pluggable: a session secret locally, a GitHub App token hosted.

## Gaps

1. The npm v12 change to lifecycle scripts rests on one third-party analysis
   ([Checkmarx](https://checkmarx.com/zero-post/npm-v12-lifecycle-script-limits-a-real-malicious-package-risk-reduction-or-just-moving-risk-around/));
   this note does not rely on it.
2. Git's protocol docs came through a summary of the source text, not a
   verbatim quote.
3. No current official numbers for Read the Docs and Netlify build machines
   came up; the table gives ranges.
4. This pass did not check GitLab's equivalent of fine-grained installation
   tokens in depth.
