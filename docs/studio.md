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
write tools work (see [`mcp.md`](mcp.md#start-the-server)). `acu serve -d`
serves a finished `atlas.json`; no config stands behind it, so the editor
stays off.

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

1. Run the command. It prints the address, by default
   `http://127.0.0.1:4321/`.
2. Open the address in a browser.

| Option | Default | Effect |
| --- | --- | --- |
| `project` | none | A project folder, or its `atlas.yaml`, to open at the start. |
| `--addr` | `127.0.0.1:4321` | The address. acu accepts a loopback address only. |
| `--root` | your home folder | The folder that the folder picker shows, with every folder below it. |
| `--workspace` | `$XDG_CACHE_HOME/acu/repos`, else `~/.cache/acu/repos` | The folder that receives clones. |

`acu studio` refuses an address that other computers can reach. To share a
finished map, use `acu build` or `acu serve -d`.

## Set up a project

### Pick a folder

1. Open the studio. With no project open, it shows the setup screen.
2. Move through the folders. The picker marks each folder that holds an
   `atlas.yaml` or a git repository.
3. Choose a folder.
   - A folder with an `atlas.yaml` opens with that file. The studio never
     replaces an existing `atlas.yaml`. It refuses an `atlas.yaml` that
     forms a symbolic link, because a link could point the editor's saves at
     any file.
   - A folder without one gets a proposal (below).

### Clone a repository

1. Paste an HTTPS address, such as `https://github.com/go-chi/chi`.
2. Choose **Clone**. The studio clones the repository into the workspace,
   at `<workspace>/<host>/<owner>/<name>`, and shows each phase of git's
   progress when it ends.
3. The clone then opens as a picked folder. A repository cloned before opens
   as it stands; the studio does not fetch it again.

### Read the proposal

The studio proposes a first `atlas.yaml` from what the folder holds:

1. **Languages.** It looks for marker files in the folder and in each folder
   one level down, as a monorepo often holds them. `go.mod` means Go.
   `Cargo.toml` means Rust. `package.json` or `tsconfig.json` means
   TypeScript (the TypeScript adapter also reads JavaScript).
   `pyproject.toml`, `setup.py`, `setup.cfg`, `requirements.txt` or a `.py`
   file means Python. A language found one level down gets that folder as
   its `root`, unless the same adapter already reads the parent folder.
2. **Entry-point plugins.** The dependencies suggest plugins:
   `go-chi` and `cobra` from `go.mod`, `go-nethttp` for every Go project,
   `express`, `hono` and `sveltekit` from `package.json`, `axum` and `clap`
   from `Cargo.toml`, and `flask`, `fastapi`, `django`, `click` and `typer`
   from the Python marker files. A dependency must appear as a whole name:
   `click` does not match `clickhouse-driver`.
3. **Repository and links.** A git remote named `origin` on GitHub or GitLab
   gives `project.repo` and the matching `links` preset. The studio reads
   `.git/config` as text; it runs no git before you confirm trust.
4. **Name.** The folder's name becomes `project.name`.

The proposal holds no regions and no tour; you add them after the first
extraction. Read the proposed YAML and change it as you need before you
write it. [`config.md`](config.md) explains each field.

### Confirm trust and extract

Extraction runs the project's own build tools. The Rust adapter runs
`build.rs` scripts and procedural macros, and the Go toolchain can run code
through cgo. So the studio asks you to confirm that you trust the code before
the first extraction. Open only code that you would also build.

1. Confirm that you trust the code.
2. Choose **Write and extract** (or **Open and extract** for an existing
   `atlas.yaml`). The studio writes the new `atlas.yaml`, runs each adapter,
   and shows each line of progress.
3. The map opens when the check passes. A failure shows the reason; fix the
   config or install the missing toolchain, then try again.

## Edit the tour

Open the editor with **Edit**. The drawer has four tabs: **Tour**,
**Regions**, **Project** and **Extract**. The left and right arrow keys move
between the tabs, and Home and End jump to the first and the last. Escape
closes the drawer, outside a field and not during a save. The **Tour** tab
lists the steps in order.

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
5. **Reorder.** Drag a step by its row; it lands before the row that you
   drop it on, or after the row when you drop it on the row's lower half.
   Or use the ↑ and ↓ buttons, or press Alt+ArrowUp and Alt+ArrowDown while
   a step's row has focus. Enter opens the step's form.
6. **Delete.** Choose **Delete step** in a step's form.
7. **Accept a drafted step.** A step that a program drafted carries a
   `drafted` mark (see [`config.md`](config.md#drafted-text)). Read it
   against the code, then choose **Mark as checked**. That removes the mark.
   Only a person chooses it; the studio never removes a mark on its own.

## Edit the regions

The **Regions** tab lists the regions in order. For each region you can set:

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
   repository root, or untick them to remove them. A tick never changes
   another unit that shares the shorter path from a language's root.
5. **Colour**: **Automatic**, or **Colour 1** to **Colour 9**, which
   `atlas.yaml` stores as `color` 0 to 8, an index into the colour-blind-safe
   palette.
6. **Default region**: units that match no region go to the default region.
7. **Layout row**: the row of the overview that holds the region, from 1.
   The studio writes the rows as `layout`. Within a row, regions keep their
   order from the file. A region without a row goes after the others.

**Suggest regions from folders** makes one region per top-level folder of
the units. Each suggestion carries a `drafted` mark by "acu studio", because
a program chose the grouping. Rename and regroup them, then mark them as
checked. A region's mark covers the whole region (its name, paths and
text), so it stays until you choose **Mark as checked**, even when you
clear the region's text.

## Edit the project

The **Project** tab sets the project's name and its description, the text
of the overview. A drafted description shows **Mark as checked**, as a
drafted step does. Clearing the description also removes its mark.

## Save and extract again

1. **Save to atlas.yaml** writes every changed section, in this order: the
   project, the regions, the tour. When both the regions and the tour
   changed, they go in one write. A section that you did not change stays as
   it stands. **Discard changes** reads the file again.
2. After a save, the page reloads and shows the change.
3. On the **Extract** tab, **Run the adapters again** extracts afresh after
   a change to the code, to `languages` or to `entryPoints`, and shows the
   new map. Save or discard your changes first. While the adapters run, a
   save waits: acu answers 409 until they finish.

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
3. It rewrites only the changed section. In a list save (regions or tour):
   - Comments under the key, up to the last blank line before the first
     item, stay under the key: they speak of the whole list. Comments
     directly above the first item, with no blank line between, belong to
     that item and move with it.
   - Each item equal to an old item keeps its old lines and the comments
     above them.
   - A changed item keeps the comments above the old item with the same ID
     (a region), else with the same title (a step). Only when the list kept
     its length does a changed item take the comments of the old item at
     its place. So an insert or a deletion never hands one item's comment
     to another.
4. It checks the new config against the config schema and runs the check of
   `acu extract`. A step whose target matches nothing, or a layout that
   names a missing region, fails with status 422 and the reason. acu then
   writes nothing.
5. It replaces the file in one step, through a temporary file.

## The API

The editor talks to acu through a JSON API under `/api/`. The tests and other
local tools may use it too.

| Call | Body | Answer |
| --- | --- | --- |
| `GET /api/state` | none | The mode (`serve` or `studio`), whether a map shows, whether writes work and why not, the open project, the last job, and the picker's root and workspace. |
| `GET /api/config` | none | The file's `revision` and `path`; `project`, `regions`, `layout` and `tour` as written; `resolved.tour` and `resolved.regions` as acu resolved them; every unit with its path and region; the map's warnings. |
| `GET /api/job` | none | The last job: `id`, `kind` (`extract`, `open` or `clone`), `state` (`running`, `done` or `failed`), `log`, `error`, and the `path` that the job opened or cloned. |
| `POST /api/tour` | `revision`, `tour` | Replaces the tour. Answers the new `revision`, `how` and `warnings`. |
| `POST /api/regions` | `revision`, `regions`, optional `layout`, optional `tour` | Replaces the regions; with `layout`, also the layout (`null` removes it); with `tour`, also the tour, in the same write. |
| `POST /api/project` | `revision`, `name`, `description`, `descriptionDrafted` | Sets the project's name and description; a missing or null `descriptionDrafted` removes the mark. |
| `POST /api/match` | `paths` | The IDs of the units that the globs match, and the globs that do not parse. |
| `POST /api/extract` | none | Starts a job that runs the adapters again on the open config. |
| `POST /api/folders` | optional `path` | The folders inside `path` (default: the root), each with `config` and `git` marks, and the parent folder. `acu studio` only. |
| `POST /api/propose` | `path` | The folder's `atlas.yaml`, or a proposal, with the languages and plugins found. `acu studio` only. |
| `POST /api/open` | `path`, optional `config`, `trust: true` | Starts a job that writes `config` as a new `atlas.yaml` (never over an existing one), extracts, and opens the map. `acu studio` only. |
| `POST /api/clone` | `url` | Starts a job that clones the repository into the workspace. `acu studio` only. |

Every answer holds JSON. A failed call answers `{ "error": "acu: …" }`, and
the text names the next step.

| Status | Means |
| --- | --- |
| 200 | Done. |
| 202 | A job started; read `GET /api/job` for its progress. |
| 400 | The body lacks a field or holds the wrong shape. |
| 403 | The guards refused the request, or writes stay off. |
| 404 | No such call, folder or file. |
| 409 | The file changed since the editor read it, no project shows yet, a job runs already (a save waits for it too), or `atlas.yaml` exists already. |
| 413 | The body exceeds 4 MB. |
| 415 | The body does not come as `application/json`. |
| 422 | The change fails the schema or the check; the error gives the reason. |
| 428 | `open` lacks `trust: true`. |
| 500 | A lock broke after an earlier failure, or a call stopped; restart acu. |

## Security

The studio listens on a loopback address, but a web page in your browser
could still try to reach it. These guards stop that:

1. **Host.** The API answers only a `Host` of `127.0.0.1:<port>`,
   `localhost:<port>`, `[::1]:<port>`, or the address that acu listens on,
   with the port that the system gave (so `--addr 127.0.0.1:0` works). In
   `acu studio` the rule covers the whole site, the map's data included.
   `acu serve` applies it to the API only, so a map behind a forwarded port
   still shows. This blocks DNS rebinding. acu checks the guards before it
   reads a request's body.
2. **Origin.** A request that sends `Origin` must name one of those hosts.
3. **Changing calls.** Every POST needs all of these:
   - the header `X-Acu-Studio: 1`;
   - a body of type `application/json`;
   - an `Origin` header;
   - a `Sec-Fetch-Site` of `same-origin` or `none`. A request without the
     header counts as `same-origin`.

   A page on another site cannot set the custom header without a preflight,
   which acu never answers.
4. **Responses.** Every response sends `X-Content-Type-Options: nosniff`,
   `Referrer-Policy: no-referrer` and `Content-Security-Policy:
   frame-ancestors 'none'`, so no other site can frame the studio.
5. **Folder picker.** acu resolves each path (`..` and symbolic links
   included) before it checks it. A path must lie inside the root or the
   workspace. The picker lists folders only, never files. It hides names
   that start with a dot, and `node_modules`, `target`, `vendor`, `dist`,
   `build`, `__pycache__` and `venv`. It lists at most 500 folders.
6. **Clone.** acu accepts an `https://` address with a plain host name and a
   path of plain names; it refuses anything else (another protocol, a user
   name, a query, `..`, a leading `-`). It runs git without a shell:
   - `git clone --depth 1 --single-branch --no-tags --no-recurse-submodules -- <url> <folder>`;
   - with `protocol.allow=never` and `protocol.https.allow=always`, and
     `core.hooksPath=/dev/null`;
   - with `GIT_ALLOW_PROTOCOL=https`, `GIT_PROTOCOL_FROM_USER=0` and
     `GIT_TERMINAL_PROMPT=0`;
   - only with git 2.45.1 or later, which fixed a code execution in clone
     (CVE-2024-32002);
   - in a process group of its own, for at most 10 minutes; acu then stops
     the group and removes the folder, with the host and owner folders that
     stay empty;
   - into a staging folder (`<name>.acu-partial`) that takes the final name
     only after git succeeds, so a clone that Ctrl-C stopped halfway never
     opens later as a good one. Ctrl-C stops a running clone's group.
7. **Trust.** Opening a project needs `trust: true`, because extraction runs
   the project's build tools.

[`research/notes/live-service.md`](research/notes/live-service.md) gives the
sources for each guard.

## Limits

1. One job runs at a time. acu refuses a second job with status 409; start
   it again after the first ends.
2. The studio holds no session token yet. Any process on your computer can
   call the API, and so can another user on a shared machine. Run the studio
   only on a computer that you alone use. A follow-up adds a session secret,
   as Jupyter uses.
3. The MCP server at `/mcp` follows the open project: after you open another
   project or extract again, its tools read the new map. Before a project
   opens, every tool answers that no project shows yet.
4. A tour step or region that the code no longer matches fails the check on
   save. Fix its target, or delete it, then save again.
5. One studio per `atlas.yaml`. Two servers on one file do not wait for each
   other; the revision check catches the second save, but not a draft from
   the MCP write tools of another process.

## Later: a hosted service

[`plans/live-service.md`](plans/live-service.md) proposes a hosted service
that builds maps on each push and turns edits into pull requests.
[`research/notes/live-service.md`](research/notes/live-service.md) holds the
research behind it.
