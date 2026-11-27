# The studio

`acu studio` sets up and edits a map in the browser. With it you need not
write `atlas.yaml` by hand. The studio does these jobs:

1. Pick a project: a folder on your computer, or a repository that it clones
   over HTTPS.
2. Propose a first `atlas.yaml` from the languages and plugins that the
   folder shows.
3. Run the adapters, show their progress, and open the map.
4. Edit the tour, the regions and the project text, and save them into
   `atlas.yaml`.
5. Run the adapters again after a change to the code.

`acu serve -c atlas.yaml` offers the same editor for one project, while its
write tools work (see [`mcp.md`](mcp.md#start-the-server)). The editor works
only in a browser that reaches acu through a loopback name, such as
`127.0.0.1` or `localhost`. The browser also needs the session token from
the address that acu prints. A browser on another computer sees the editor
only through a forward that keeps the name and the port, such as `ssh -L`.
`acu serve -d` serves a finished `atlas.json`. It has no config, so the
editor stays off.

Everything stays on your computer. The studio writes only `atlas.yaml` in the
project folder, and clones only into its workspace folder. Git keeps the
history of `atlas.yaml`, as for any other file.

## Start it

```sh
acu studio                         # pick a project in the browser
acu studio path/to/project         # open that project's atlas.yaml at the start
acu studio --addr 127.0.0.1:5000   # another port
acu studio --root ~/code           # show only ~/code in the folder picker
acu studio --workspace ~/clones    # clone repositories into ~/clones
```

1. Run the command. It prints the address with a session token, by
   default `http://127.0.0.1:4321/?token=<token>`.
2. Open that whole address in a browser. acu exchanges the token for a
   cookie. Then it drops the token from the address bar (see
   [The session token](#the-session-token)).

| Option | Default | Effect |
| --- | --- | --- |
| `project` | none | A project folder, or its `atlas.yaml`, to open at the start. |
| `--addr` | `127.0.0.1:4321` | The address. acu accepts a loopback address only. |
| `--root` | your home folder | The folder that the folder picker shows, with every folder below it. |
| `--workspace` | `$XDG_CACHE_HOME/acu/repos`, else `~/.cache/acu/repos` | The folder that receives clones. |

`acu studio` refuses an address that other computers can reach. To share a
finished map, use `acu build` or `acu serve -d`.

## The session token

`acu studio` and `acu serve` make a session token at start. Every call to
the API under `/api/` and to the MCP server at `/mcp` must carry it. So
another process or another user on the computer cannot send calls to the
server, for example to open a folder with `trust: true`.

1. acu makes the token from 32 random bytes of the operating system, and
   writes it as 64 hex digits.
2. The environment variable `ACU_TOKEN` fixes the token instead. It takes
   32 to 512 characters, each a letter, a digit, `-`, `.`, `_` or `~`. An
   empty value counts as unset. Any other value stops acu at start, and the
   message says how to fix it. A fixed token lets an MCP client's config
   work after a restart.
3. Every acu command reads `ACU_TOKEN` at start and removes it from its own
   environment. So the adapters, build scripts and git that acu runs never
   inherit the token.
4. acu prints the address with the token. A second line names the header
   for MCP clients. For `acu serve` on a loopback address, a third line
   names the `Host` rule (see
   [Forwarded names](#forwarded-names-in-acu-serve)):

   ```text
   acu: serving the studio at http://127.0.0.1:4321/?token=<token> and MCP at http://127.0.0.1:4321/mcp, no project open yet  (Ctrl-C stops)
   acu: open the address above, with its token; an MCP client sends the header "Authorization: Bearer <token>"; for an MCP client's config, ACU_TOKEN fixes the token across restarts
   ```

5. A page address with a good `?token=` answers 303. The answer sends the
   browser to the same path without the token. Other parts of the query
   stay. acu reads the value percent-decoded. When the address holds
   `token` more than once, the last one counts.
6. The 303 sets this cookie:
   `acu-token-<port>=<cookie value>; Path=/; HttpOnly; SameSite=Lax`.
   - The cookie value differs from the token. acu makes it from 32 more
     random bytes at each start, and writes it as 64 hex digits.
   - The cookie lasts until the browser closes. No script can read it.
   - The name holds the port, because a browser sends a cookie of
     `127.0.0.1` to every port. So two servers on one computer keep their
     cookies apart.
7. A wrong or old token in the address answers 401 with "acu: the address
   holds a wrong or old token; open the address that acu printed last". No
   cookie follows.
8. A call to `/api/` must carry the cookie or the header
   `Authorization: Bearer <token>`. The token itself in the cookie does not
   count. A call without either answers 401 with "acu: the request lacks
   the session token; open the address with ?token= that acu printed, or
   send the header Authorization: Bearer <token>".
9. A call to `/mcp` must carry `Authorization: Bearer <token>`. The cookie
   does not open `/mcp`. So a page in the browser never reaches the MCP
   tools.
10. A call to `/mcp` without a good bearer token answers 401 with "acu: the
    MCP request lacks the session token; send the header Authorization:
    Bearer <token> with the token that acu printed".
    - The answer carries `WWW-Authenticate: Bearer realm="acu"`.
    - A sent but wrong token adds `, error="invalid_token"`.
    - The header names no sign-in server. A client that looks for one
      under `/.well-known/oauth-*` gets 404.
11. acu compares the token and the cookie value in a time that does not
    depend on where they differ.
12. The viewer's page carries the API mark (`<meta name="acu-api">`) only
    when the request names the server's own `Host` and carries the cookie
    or the token. A page without the mark never calls the API, so the
    browser logs no refused request.
13. In `acu studio`, `/data/atlas.json` needs the cookie too, because it
    holds the open project. Without it, the studio page shows "acu refused
    the map data: this page lacks the session token, or holds an old one.
    Open the address that acu printed last, with its ?token= part."
14. `acu serve` keeps its pages and `/data/atlas.json` open without a
    token, so that it can share a map. A page without the cookie shows the
    map without the editor and calls no API. The same holds after a
    restart, because the old cookie no longer counts: open the new address
    that acu printed.
15. On a forwarded name (see `--allow-host` below), the map and its data
    show, but the editor does not. For the editor, forward the same port to
    the same name, such as `ssh -L 4321:127.0.0.1:4321 host`, and open the
    printed address.

`acu mcp` speaks over standard input and output and needs no token.
[`mcp.md`](mcp.md#connect-a-client) shows how an MCP client sends the
header.

## Forwarded names in acu serve

On a loopback address, `acu serve` answers only a `Host` that names the
server itself: `127.0.0.1`, `localhost` or `[::1]` with its port, or the
address that it listens on. So a web page that points a foreign name at your
computer (DNS rebinding) reads neither the pages nor the data.

Another `Host` gets 403 with "acu: the server answers only requests for its
own address; open it at the address that acu printed, or start acu serve
again with --allow-host <the Host received> for a forwarded name". The
message names the `Host` that acu received. Copy that value into
`--allow-host`. acu prints the rule at start:

```text
acu: pages answer only this server's own names on port 4321; for a forwarded name, start acu serve again with --allow-host <name[:port]>
```

A forward that keeps the name and the port needs nothing more. For example,
`ssh -L 4321:127.0.0.1:4321 host` lets the browser on your computer open
`http://127.0.0.1:4321/?token=<token>`, editor included.

A forward that changes the port or the name needs `--allow-host`:

```sh
# On the remote computer:
acu serve -c atlas.yaml --allow-host localhost:9000
# On your computer:
ssh -L 9000:127.0.0.1:4321 host
```

GitHub Codespaces forwards a port over HTTPS. It may send its forwarded
name as the `Host`, such as `<codespace>-4321.app.github.dev`. If so, give
that name without a port:

```sh
acu serve -c atlas.yaml --allow-host <codespace>-4321.app.github.dev
```

If the page answers 403, the message names the `Host` that acu received.
Give that value instead.

1. Give the name as the browser sends it in `Host`, with or without a
   port: `localhost:9000`, `myhost.example:8080`, `[fd00::1]:4321`,
   `[fd00::1]` or `x-4321.app.github.dev`. `--help` shows the form as
   `NAME[:PORT]`.
2. acu writes the value in lower case. A name may hold letters, digits,
   `-`, `.` and `_`. acu writes an IPv4-mapped address in hex groups, as a
   browser does, such as `[::ffff:a00:1]`. It refuses port 0, a port that
   holds anything but digits, and characters that no host name holds.
3. A value with a port matches a `Host` with that port. A browser leaves out
   the default port, 80 or 443. So a value that ends in `:80` or `:443` also
   matches a `Host` of the same name without a port. A value without a port
   matches only a `Host` without a port.
4. The option may repeat. acu sorts the names, drops repeats, and lists
   them in the start-up line.
5. A forwarded name reaches the pages and `/data/atlas.json` only. The API
   and `/mcp` keep their own names. So the editor and MCP clients work only
   through the server's own names.
6. On an address that other computers can reach, such as `0.0.0.0:4321`,
   the pages and the data answer any name, and `--allow-host` changes
   nothing. acu then says: "acu: 0.0.0.0:4321 answers any name, so
   --allow-host changes nothing; drop it, or listen on a loopback address".
7. `acu studio` takes no `--allow-host`.

A project that you name on the command line opens and extracts at the start.
The studio asks no trust question for it: the name on the command line counts
as your trust. acu refuses an `atlas.yaml` that forms a symbolic link, as the
folder picker does.

## Set up a project

### Pick a folder

1. Open the studio. With no project open, it shows the setup screen.
2. Move through the folders. The picker marks each folder that holds an
   `atlas.yaml` or a git repository.
3. Choose a folder.
   - A folder with an `atlas.yaml` opens with that file. The studio never
     replaces an existing `atlas.yaml`.
   - A folder without one gets a proposal (below).

The studio refuses an `atlas.yaml` that forms a symbolic link. A link could
point the editor's saves at any file.

**Up one folder** works only inside the root. In the workspace it stays off,
so for a clone outside the root you cannot move up from the clone.

To pick another project later, choose **Projects** in the header or in the
editor. That opens the setup screen again. While a map shows, Escape closes
the setup screen.

### Clone a repository

1. Paste an HTTPS address, such as `https://github.com/go-chi/chi`.
2. Choose **Clone**. The studio clones the repository into the workspace,
   at `<workspace>/<host>/<path>`, and shows each phase of git's progress
   when it ends.
3. The clone then opens as a picked folder.

The path holds 2 to 6 parts after the host, such as `<owner>/<name>`, or a
GitLab group with subgroups. acu writes the host in lower case. A repository
cloned before opens as it stands; the studio does not fetch it again.

### Read the proposal

The studio proposes a first `atlas.yaml` from what the folder holds:

1. **Languages.** It looks for marker files in the folder and in each folder
   one level down, as a monorepo often holds them. A language found one
   level down gets that folder as its `root`, unless the same adapter
   already reads the parent folder.

   | Marker file | Language |
   | --- | --- |
   | `go.mod` | Go |
   | `Cargo.toml` | Rust |
   | `package.json`, `tsconfig.json` | TypeScript (the adapter also reads JavaScript) |
   | `pyproject.toml`, `setup.py`, `setup.cfg`, `requirements.txt`, a `.py` file | Python |

2. **Entry-point plugins.** The dependencies in the marker files suggest
   plugins. A dependency must appear as a whole name: `click` does not match
   `clickhouse-driver`.

   | Marker file | Plugins |
   | --- | --- |
   | `go.mod` | `go-nethttp` (always), `go-chi`, `cobra` |
   | `package.json` | `express`, `hono`, `sveltekit` |
   | `Cargo.toml` | `axum`, `clap` |
   | Python marker files | `flask`, `fastapi`, `django`, `click`, `typer` |

3. **Repository and links.** A git remote named `origin` on GitHub or GitLab
   gives `project.repo` and the matching `links` preset. The studio reads
   `.git/config` as text; it runs no git before you confirm trust.
4. **Name.** The folder's name becomes `project.name`.

Detection looks at no more than 200 subfolders, the first that the system
lists. It reads a marker file only
up to 1 MB, and it never follows a symbolic link to one. An `atlas.yaml` over
1 MB shows as empty in the proposal.

The proposal holds no regions and no tour; you add them after the first
extraction. Read the proposed YAML and change it as you need before you
write it. [`config.md`](config.md) explains each field.

### Confirm trust and extract

Extraction runs the project's own build tools. The Rust adapter runs
`build.rs` scripts and procedural macros, and the Go toolchain can run code
through cgo. So the studio asks you to confirm that you trust the code before
the first extraction of a picked folder. Open only code that you would also
build.

1. Confirm that you trust the code.
2. Choose **Write and extract** (or **Open and extract** for an existing
   `atlas.yaml`). The studio writes the new `atlas.yaml`, runs each adapter,
   and shows each line of progress.
3. The map opens when the check passes. A failure shows the reason; fix the
   config or install the missing toolchain, then try again.

## Open the editor

1. Choose **Edit**. The editor opens in a drawer with four tabs: **Tour**,
   **Regions**, **Project** and **Extract**.
2. To move between the tabs, press the left and right arrow keys. Home moves
   to the first tab, and End to the last.
3. To close the drawer, press Escape outside a field. Escape does nothing
   during a save.

## Edit the tour

The **Tour** tab lists the steps in order.

1. **Record a step.** Move the map to the view that the step should open:
   the overview, a region, a unit, an item, or a trace. Choose **Record this
   view as a step**. The studio adds a step that opens that view, by ID. The
   title field stays empty and shows the view's name as a hint, because a
   person writes the step's text.
2. **Write the text.** Give the step a title and a text. The text takes the
   inline subset of Markdown that the viewer shows, and a preview follows
   the field.
3. **Change the view.** Move the map, then choose **Use the current view**
   on a step.
4. **Preview.** Choose **Show** on a step to open its view.
5. **Reorder.** Use one of these ways:
   - Drag a step by its row, and drop it on another row. A drop on the upper
     half moves the step before that row. A drop on the lower half moves it
     after that row.
   - Choose the ↑ or ↓ button.
   - Give a step's row the focus, then press Alt+ArrowUp or Alt+ArrowDown.
6. **Open a step's form.** Give the step's row the focus, then press Enter
   or Space.
7. **Delete.** Choose **Delete step** in a step's form.
8. **Accept a drafted step.** A step that a program drafted carries a
   `drafted` mark (see [`config.md`](config.md#drafted-text)). Read it
   against the code, then choose **Mark as checked**. That removes the mark.
   Only a person chooses it; the studio never removes a mark on its own.

## Edit the regions

The **Regions** tab lists the regions in order. Enter or Space on a region's
row opens its form, as on a step. For each region you can set:

1. **Name** and **ID**. The ID takes letters, digits, `_` and `-`. A new
   region's ID follows its name until you edit the ID. When you change the
   ID of a region, the save also points the tour steps that open it at the
   new ID, in the same write. A deleted region that a step opens stops the
   save, with the step's number.
2. **What it does** and **How to read it**: the region's `description` and
   `readingTip`.
3. **Paths, one pattern per line**: globs over unit paths. While you type,
   the studio lists the units that the globs match, by the same rules as
   `acu extract`. A unit joins the first region that matches it.
4. **Pick units**: tick units in the list to add their paths from the
   repository root, or untick them to remove them. Some units match only
   through a shorter path from a language's root. The list shows such a
   unit's box greyed out, with the text "edit Paths to change it". Change
   that path in **Paths**.
5. **Colour**: **Automatic**, or **Colour 1** to **Colour 9**, which
   `atlas.yaml` stores as `color` 0 to 8, an index into the colour-blind-safe
   palette.
6. **Default region**: units that match no region go to the default region.
7. **Layout row**: the row of the overview that holds the region, from 1 to
   20. The studio writes the rows as `layout`.
   - The studio drops empty rows. So rows 1 and 3 become rows 1 and 2.
   - A region that you move into another row goes to the end of that row.
   - A region without a row goes after the others.

**Suggest regions from folders** shows only while the project has no
regions. It makes one region per top-level folder of the units. The folder
comes from each unit's path from its language's root, not from the
repository root. Each suggestion gets its own colour, and a `drafted` mark by
"acu studio", because a program chose the grouping. Rename and regroup the
suggestions, then mark them as checked. A region's mark covers the whole
region (its name, paths and text). So the mark stays until you choose
**Mark as checked**, even when you clear the region's text.

## Edit the project

The **Project** tab sets the project's name and its description, the text
of the overview. A drafted description shows **Mark as checked**, as a
drafted step does. Clearing the description also removes its mark.

## Save and extract again

1. Choose **Save to atlas.yaml**.
2. To throw away your changes instead, choose **Discard changes**. The
   studio reads the file again.
3. After a change to the code, to `languages` or to `entryPoints`, save or
   discard your changes first.
4. On the **Extract** tab, choose **Run the adapters again**.

A save writes each changed section, in this order: the project, the regions,
the tour. Each section goes in its own write. The regions and the tour go in
one write when both changed. A section that you did not change stays as it
stands. If a later write fails, the earlier writes stay in the file.

After a save, the page reloads and shows the change. After a new extraction,
the studio shows the new map.

While the adapters run, a save fails at once with status 409 and the text
"wait until the adapters finish, then save again". acu does not hold the
save or retry it.

### How a save works

A save takes the same path as a draft from an MCP write tool
([`mcp.md`](mcp.md#drafts)):

1. acu reads `atlas.yaml` from disk.
2. It compares the file's revision with the revision that the editor read.
   The revision holds an FNV-1a hash of the file's text. A file that changed
   meanwhile, in a text editor or another window, fails the save with status
   409. Choose **Discard changes** to read the file again, then make the
   change again. A save that reaches a map that a new extraction has just
   replaced also fails with 409, for the same remedy.
3. It rewrites only the changed section. A list save (regions or tour)
   follows the rules below. Each line that acu leaves keeps its own line
   ending, CR LF or LF. A new line takes the ending of most lines in the
   file.
4. It checks the new config against the config schema and runs the check of
   `acu extract`. A step whose target matches nothing, or a layout that
   names a missing region, fails with status 422 and the reason. acu then
   writes nothing.
5. It replaces the file in one step, through a temporary file. On Linux
   and macOS, a file with more than one hard link keeps its links: acu
   first writes the temporary file, then copies the new text into the file
   itself, so every name shows the change. A reader at that moment can see
   part of the old text. A copy that fails halfway leaves the temporary
   file, and the message names it.

In a list save, acu keeps comments by these rules:

1. Comments under the key, up to the last blank line before the first entry,
   stay under the key. They describe the whole list.
2. Comments directly above the first entry, with no blank line between,
   belong to that entry. They move with it.
3. An entry equal to an old entry keeps its old lines and the comments above
   them.
4. A changed entry pairs with the old entry that shares the most fields
   with it. An equal ID counts most. A pair needs an equal ID, an equal
   title, or two equal fields; one shared field, such as the same view,
   forms no pair.
5. A changed entry keeps the comments above its old entry, and the comments
   inside it: acu rewrites only the keys that changed.
6. A new entry that pairs with no old entry takes no comments.

## The API

The editor talks to acu through a JSON API under `/api/`. The tests and other
local tools may use it too. Each call needs the session token, in the
cookie or as `Authorization: Bearer <token>` (see
[The session token](#the-session-token)).

| Call | Body | Answer |
| --- | --- | --- |
| `GET /api/state` | none | The mode (`serve` or `studio`), whether a map shows, whether writes work and why not, the open project, the last job, and the picker's root and workspace. |
| `GET /api/config` | none | The file's `revision` and `path`; `writable` and `why`; `project`, `regions`, `layout` and `tour` as written; `resolved.tour` and `resolved.regions` as acu resolved them; every unit with its path and region; the map's warnings. |
| `GET /api/job` | none | The last job: `id`, `kind` (`extract`, `open` or `clone`), `state` (`running`, `done` or `failed`), `log`, `error`, and the `path` that the job opened or cloned. |
| `POST /api/tour` | `revision`, `tour` | Replaces the tour. Answers the new `revision`, `how` (`in place`, `section`, or `unchanged` when the save changed nothing) and `warnings`. |
| `POST /api/regions` | `revision`, `regions`, optional `layout`, optional `tour` | Replaces the regions; with `layout`, also the layout (`null` removes it); with `tour`, also the tour, in the same write. Answers as `POST /api/tour` does. |
| `POST /api/project` | `revision`, `name`, `description`, `descriptionDrafted` | Sets the project's name and description; a missing or null `descriptionDrafted` removes the mark. |
| `POST /api/match` | `paths` | The IDs of the units that the globs match, and the globs that do not parse. |
| `POST /api/extract` | none | Starts a job that runs the adapters again on the open config. |
| `POST /api/folders` | optional `path` | The folders inside `path` (default: the root), each with `config` and `git` marks; the listed folder's own `config` and `git`; `more` when the list stopped at 500; the parent folder, inside the root only. `acu studio` only. |
| `POST /api/propose` | `path` | The folder's `atlas.yaml`, or a proposal, with the languages and plugins found, and `exists` when an `atlas.yaml` exists. `acu studio` only. |
| `POST /api/open` | `path`, optional `config`, `trust: true` | Starts a job that writes `config` as a new `atlas.yaml` (never over an existing one), extracts, and opens the map. `acu studio` only. |
| `POST /api/clone` | `url` | Starts a job that clones the repository into the workspace. `acu studio` only. |

Every answer holds JSON. A failed call answers `{ "error": "acu: …" }`, and
the text names the next step.

| Status | Means |
| --- | --- |
| 200 | Done. |
| 202 | A job started; read `GET /api/job` for its progress. |
| 303 | A page address with a good `?token=`: the answer sets the cookie and sends the browser to the address without the token. Pages only, never the API. |
| 400 | The body lacks a field or holds the wrong shape; or a token exchange cannot build its redirect. |
| 401 | The call lacks the session token, or carries a wrong one; or a page address holds a wrong or old `?token=`. |
| 403 | The `Host` or another guard refused the request; writes stay off; the folder lies outside the root and the workspace; acu cannot read the folder; the `atlas.yaml` forms a symbolic link; the clone target passes through a link; or a setup call came to `acu serve`. |
| 404 | No such call, folder or file; `GET /data/atlas.json` before a project opens; or `/api/open` without `config` on a folder that holds no `atlas.yaml`. |
| 409 | The file changed since the editor read it, no project shows yet, a job runs already (a save fails at once, too), or `atlas.yaml` exists already. |
| 413 | The body exceeds 4 MB. |
| 415 | The content type differs from `application/json`. |
| 422 | The change fails the schema or the check; the error gives the reason. |
| 428 | `open` lacks `trust: true`. |
| 500 | A lock broke after an earlier failure, or a call stopped; restart acu. |

## Security

The studio listens on a loopback address, but a web page in your browser,
or another process on your computer, could still try to reach it. These
guards stop that.

For a call to `/api/` or `/mcp`, acu checks them in this order: the `Host`,
then the `Origin`, then the token, then the rest. So a request that names
another site in `Origin` gets 403, with or without a token. A preflight
from another site gets 403 too.

1. **Host.** The API answers only a `Host` of `127.0.0.1:<port>`,
   `localhost:<port>`, `[::1]:<port>`, or the address that acu listens on,
   with the port that the system gave (so `--addr 127.0.0.1:0` works).
   Another `Host` gets 403. This blocks DNS rebinding. acu checks the
   guards before it reads a request's body.
   - In `acu studio`, the rule covers the viewer's pages, the map's data
     and the API.
   - In `acu serve` on a loopback address, the rule also covers every page
     and the data. `--allow-host <name[:port]>` admits a forwarded name for
     the pages and the data (see
     [Forwarded names](#forwarded-names-in-acu-serve)). The API and `/mcp`
     keep their own names. `/mcp` and the API share one list of names.
   - In `acu serve` on an address that other computers can reach, the pages
     and the data answer any name. The API keeps the rule.
2. **Origin.** A request that sends `Origin` must name one of those hosts.
   Another `Origin` gets 403.
3. **Token.** Every API call needs the cookie or
   `Authorization: Bearer <token>`. In `acu studio`, the map's data needs
   the cookie or the token too. Every call to `/mcp` needs
   `Authorization: Bearer <token>`. The cookie does not count there. A call
   without a good token gets 401. See
   [The session token](#the-session-token).
4. **Changing calls.** Every POST needs all of these:
   - the header `X-Acu-Studio: 1`;
   - the content type `application/json`, with parameters such as `charset`
     allowed;
   - an `Origin` header;
   - a `Sec-Fetch-Site` of `same-origin` or `none`. A request without the
     header counts as `same-origin`.

   A page on another site cannot set the custom header without a preflight.
   acu refuses every cross-site preflight (`OPTIONS`) with 403, and never
   sends an `Access-Control-Allow-*` header.
5. **Responses.** The viewer's pages and the API send
   `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer` and
   `Content-Security-Policy: frame-ancestors 'none'`, so no other site can
   frame the studio.
6. **MCP.** `/mcp` answers through the MCP library.
   - Before the library reads the request, acu checks the `Host` and the
     `Origin` (403 each, without `WWW-Authenticate`), then the bearer token
     (401). This order matches the API's.
   - The library then checks `Host` and `Origin` again, with the same list
     as guard 1. Each name in it holds the port: `localhost:<port>`,
     `127.0.0.1:<port>`, `[::1]:<port>` and the listening address. So
     `/mcp` refuses a loopback name with another port.
   - The library's answers carry none of the headers in guard 5. acu's own
     403 and 401 answers carry them.
   - The custom-header and Fetch-Metadata rules of guard 4 do not apply to
     `/mcp`.
7. **Folder picker.** acu resolves each path (`..` and symbolic links
   included) before it checks it. A path must lie inside the root or the
   workspace. The picker lists folders only, never files. It hides names
   that start with a dot, and `node_modules`, `target`, `vendor`, `dist`,
   `build`, `__pycache__` and `venv`. It reads at most 500 folders, in the
   order that the system lists them, and shows those sorted by name.
   After the check, acu opens the folder once: one name at a time from the
   root or the workspace, with no link followed on the way. It lists the
   folder, reads its marker files and its `atlas.yaml`, and writes a new
   `atlas.yaml`, all through that open folder. So a folder that someone
   swaps for a link after the check cannot send those calls elsewhere.
   Just before the adapters start, acu checks that the path still leads to
   the open folder. On Windows, the picker takes paths.
8. **Clone address.** acu accepts an `https://` address with a plain host
   name and a path of plain names. It refuses anything else: another
   protocol, a user name, a query, `..`, a leading `-`.
9. **Clone target.** acu refuses a target when a folder between the
   workspace and the target forms a symbolic link or holds a git
   repository. It also refuses a target whose real path leaves the
   workspace.
10. **Trust.** Opening a picked folder needs `trust: true`, because
    extraction runs the project's build tools.

acu runs the clone in these steps:

1. It checks the git version. It accepts git 2.45.1 or later. It also
   accepts a patched release of an older line: 2.39.4, 2.40.2, 2.41.1,
   2.42.2, 2.43.4 or 2.44.1, or a later release of the same line. The May
   2024 releases fixed several flaws in clone (CVE-2024-32002, -32004,
   -32020, -32021 and -32465).
2. It runs git without a shell, with this command line:
   `git clone --progress --depth 1 --single-branch --no-tags
   --no-recurse-submodules -- <url> <folder>`.
   `--no-recurse-submodules` alone blocks CVE-2024-32002.
3. It sets `protocol.allow=never`, `protocol.https.allow=always` and
   `core.hooksPath=/dev/null`.
4. It sets `GIT_ALLOW_PROTOCOL=https`, `GIT_PROTOCOL_FROM_USER=0` and
   `GIT_TERMINAL_PROMPT=0`.
5. It runs git in a process group of its own, for at most 10 minutes. After
   that time, acu stops the group. It removes the folder, with the host and
   owner folders that stay empty.
6. Git writes into a hidden staging folder (`.<name>.acu-partial`). The
   folder takes the final name only after git succeeds. So a clone that
   Ctrl-C stopped halfway never opens later as a good one. Ctrl-C stops a
   running clone's group.

[`research/notes/live-service.md`](research/notes/live-service.md) and
[`research/notes/session-secret.md`](research/notes/session-secret.md) give
the sources for each guard.

### Decisions on the token and the Host

1. `--allow-host` admits a forwarded name for the pages and the data only.
   A forwarded name never reaches the API or `/mcp`. So acu needs no rule
   for an `https://` `Origin`.
2. After the token exchange, acu answers with a redirect that drops the
   token from the address. Jupyter does not redirect.
3. The cookie holds a random value of its own, not the token. The cookie
   opens `/api/` and the studio's data only. `/mcp` takes only the bearer
   token. So a server that reads the cookie cannot call the MCP tools.
4. The cookie takes `SameSite=Lax`, not `Strict`. A first visit can come
   from a link in a web terminal or a chat page, which counts as another
   site. With `Strict`, the browser could then leave the cookie out of the
   request after the 303. With `Lax`, a write from another site still
   meets the `Origin`, Fetch Metadata and `X-Acu-Studio` guards.
5. `Lax` has a cost. A link on another site sends the cookie with a
   top-level GET, such as a link to `/api/config`. The answer goes to the
   person's own tab, and the other site cannot read it. So every GET call
   of the API must change nothing. Today `state`, `config` and `job` only
   read. A new call that changes something must take POST.

## Limits

1. One job runs at a time. acu refuses a second job with status 409; start
   it again after the first ends.
2. The session token and the cookie keep out a caller that lacks them.
   Some callers can still learn one of them:
   - A browser sends a cookie of `127.0.0.1` to every port, not only to
     acu's (RFC 6265, section 8.5). A hostile server on another local port
     learns the cookie value when your browser visits that port. The
     `SameSite` attribute does not help, because every port of one host
     counts as the same site. With the cookie, such a server can call
     `/api/`. It cannot call `/mcp`, which takes only the token.
   - acu prints the token on standard error. Any process that can read
     that output learns it. The token also stays in your shell history if
     you type it into a command.
   - acu removes `ACU_TOKEN` from its own environment. The shell that
     started acu still holds it if you exported it.
   - A fixed `ACU_TOKEN` in an MCP client's config goes to whatever process
     holds the port while acu does not run. A random token per start
     avoids this.
   - The 303 drops the token from the address bar. The first address may
     still stay in the browser's history.

   So run the studio on a computer that you alone use. Visit no untrusted
   local server while it runs. Open `http://127.0.0.1:<port>/`, as acu
   prints it, not `localhost`. A cookie of `127.0.0.1` does not go to
   `localhost`. And `localhost` may lead to another process that listens
   on `[::1]`.
3. A local user who owns a folder under the root can still swap that
   folder for a symbolic link in one short window: after acu's last check
   and before the adapters and the editor take the path (see guard 7). On
   Windows, the picker takes paths, so the swap can fall anywhere between
   the check and the use. The token does not stop a swap on the disk: use
   the studio only on a computer that you alone use.
4. `acu serve` keeps its pages and `/data/atlas.json` open without a token,
   because it shares maps. Any process on your computer, and any user on a
   shared machine, can read the served map. On a loopback address, the
   `Host` rule still keeps out a web page that uses DNS rebinding. For a
   map of private code on a shared machine, use `acu studio`, whose data
   needs the token.
5. The MCP server at `/mcp` follows the open project: after you open another
   project or extract again, its tools read the new map. Before a project
   opens, every tool answers that no project shows yet.
6. A tour step or region that the code no longer matches fails the check on
   save. Fix its target, or delete it, then save again.
7. A save across sections does not happen in one atomic step. Only the
   regions and the tour go in one write. See "Save and extract again".
8. Run one studio per `atlas.yaml`. Two servers on one file share no lock.
   - The revision check stops a studio save after a change by the other
     server.
   - An MCP draft in the other server reads the file from disk first, so it
     starts from the newest text.
   - A write can still fall between the other server's read and its write.
     Then the later write replaces the file, and the earlier change gets
     lost.
9. Some guards from the research note do not exist yet:
   `Content-Security-Policy: default-src 'self'`, a random default port, a
   size cap on a clone, a restricted mode for untrusted code, and
   `GOTOOLCHAIN=local` for the Go adapter. Issue #44 tracks them.
10. On Windows, a save writes a new file and renames it over `atlas.yaml`.
    Another hard link to the old file keeps the old text.

## Later: a hosted service

[`plans/live-service.md`](plans/live-service.md) proposes a hosted service
that builds maps on each push and turns edits into pull requests.
[`research/notes/live-service.md`](research/notes/live-service.md) holds the
research behind it.
