# The MCP server

`acu` hosts an MCP server (Model Context Protocol). With it, a language model
of your choice, a local open model included, can read the map, answer
questions about the code, and draft regions, the project description and tour
steps. Every draft lands in `atlas.yaml` with a `drafted` mark, and the viewer
labels it until a person checks it.

## Start the server

| Command | Connection | Writes |
| --- | --- | --- |
| `acu serve -c atlas.yaml` | Streamable HTTP at `http://127.0.0.1:4321/mcp`, beside the viewer | on |
| `acu mcp -c atlas.yaml` | Standard input and output, for desktop clients | on |
| `acu serve -c atlas.yaml --read-only` | HTTP | off |
| `acu mcp -c atlas.yaml --read-only` | Standard input and output | off |
| `acu serve -d atlas.json`, `acu mcp -d atlas.json` | Either | off |
| `acu serve -c atlas.yaml --addr 192.168.1.5:4321` | HTTP, reachable from other computers | off, unless `--allow-remote-writes` |
| `acu studio` | HTTP at `/mcp`, beside the studio ([`studio.md`](studio.md)) | on, once a project opens |

1. Both commands run the adapters first, as `acu extract` does, so the tools
   answer from a fresh map. The first answer can take a few seconds.
2. `--addr` moves `acu serve` to another address; the MCP server moves with
   it, to `/mcp` on that address. On an address that other computers can
   reach (any address but a loopback one), anyone on the network could call
   the write tools, so they stay off and `acu` says so.
   `--allow-remote-writes` turns them on there. For a client on another
   computer, give the computer's own address, such as
   `--addr 192.168.1.5:4321`. On `0.0.0.0`, `/mcp` and the API answer only
   a `Host` of `127.0.0.1`, `localhost`, `[::1]` or `0.0.0.0`, each with
   the port. So a client on another computer gets 403. A container that
   listens on `0.0.0.0` still works when the host forwards the same port
   number to its `127.0.0.1`, such as `-p 4321:4321`.
   `acu serve -c` also offers the studio's editor at the same address
   ([`studio.md`](studio.md)); its saves and the write tools share one path.
3. `acu mcp` writes the protocol to standard output and every message to
   standard error. Start it from a client, not by hand.
4. Over HTTP, every call to `/mcp` needs the session token. `acu serve` and
   `acu studio` print it at start (see "Connect a client" below).
   `acu mcp` needs no token: only the client that started it can reach its
   standard input. Over HTTP, the token goes only in the header
   `Authorization: Bearer <token>`.
5. `acu studio` mounts the same server at `/mcp`. The server follows the
   project that the studio has open: after you open another project or
   extract again, the tools read the new map. Before a project opens, every
   tool answers that no project shows yet.
6. With `-d`, no config stands behind the map, so the write tools stay off.
   `read_source` reads files from the folder that `--root` names, else from
   the current folder. The map's paths start at the root of the mapped
   repository, so pass that folder:
   `acu serve -d out/chi.json --root corpus/go/chi`. Without it,
   `read_source` finds no file and says how to pass `--root`. When the data
   file changes (after a new `acu extract`), the next call reads it again.

## Connect a client

A desktop client that starts servers itself takes a command. Most clients
read a JSON entry of this shape; give absolute paths:

```json
{
  "mcpServers": {
    "atlas": {
      "command": "/path/to/acu",
      "args": ["mcp", "-c", "/path/to/project/atlas.yaml"]
    }
  }
}
```

A client that connects over HTTP takes the address of a running
`acu serve`, `http://127.0.0.1:4321/mcp`, and the session token.

1. Start `acu serve`. It prints lines like these:

   ```text
   acu: serving the map at http://127.0.0.1:4321/?token=<token> and MCP at http://127.0.0.1:4321/mcp, writes on  (Ctrl-C stops)
   acu: open the address above, with its token; an MCP client sends the header "Authorization: Bearer <token>"; for an MCP client's config, ACU_TOKEN fixes the token across restarts
   acu: pages answer only this server's own names on port 4321; for a forwarded name, start acu serve again with --allow-host <name[:port]>
   ```

2. Give the client the header `Authorization: Bearer <token>`. `/mcp`
   takes only this header. The viewer's cookie does not open it.
3. A call without the header, or with a wrong token, gets status 401. The
   error reads "acu: the MCP request lacks the session token; send the
   header Authorization: Bearer <token> with the token that acu printed".
   - The answer carries `WWW-Authenticate: Bearer realm="acu"`.
   - A sent but wrong token adds `, error="invalid_token"`.
   - acu offers no sign-in. A client that looks for one under
     `/.well-known/oauth-*` gets 404. Give the token by hand.
4. A foreign `Host` or `Origin` gets 403 before acu looks at the token.
   That answer carries no `WWW-Authenticate`.
5. Use `http://127.0.0.1:<port>/mcp`, as acu prints it, in a client's
   config. The name `localhost` may lead to another process that listens
   on `[::1]`.

acu makes a new random token at each start: 32 random bytes, written as 64
hex digits. So a client config that holds the token stops working after a
restart. To keep one token, set the environment variable `ACU_TOKEN` for
`acu serve`. It takes 32 to 512 characters, each a letter, a digit, `-`,
`.`, `_` or `~`. An empty value counts as unset. acu refuses any other
value and does not start.

Set the variable on the command line of `acu serve` alone, not with
`export`. Then only that one process gets it. acu removes it from its own
environment at start, so the adapters and the tools that they run never
inherit it. An exported variable would stay in your shell, and every other
program that the shell starts would inherit it.

```sh
rm -f ~/.acu-token                                 # once: drop any old file
(umask 077; openssl rand -hex 32 > ~/.acu-token)   # only you can read it
ACU_TOKEN="$(cat ~/.acu-token)" acu serve -c atlas.yaml
```

Claude Code adds the server with the header:

```sh
claude mcp add --transport http atlas http://127.0.0.1:4321/mcp \
  --header "Authorization: Bearer $(cat ~/.acu-token)"
```

VS Code reads `.vscode/mcp.json`. An input keeps the token out of the
file:

```json
{
  "inputs": [
    {
      "id": "acu-token",
      "type": "promptString",
      "description": "acu session token",
      "password": true
    }
  ],
  "servers": {
    "atlas": {
      "type": "http",
      "url": "http://127.0.0.1:4321/mcp",
      "headers": { "Authorization": "Bearer ${input:acu-token}" }
    }
  }
}
```

Keep the token out of a file that you commit. A fixed token in a client's
config goes to whatever process holds the port while acu does not run. So
use a fixed token only on a computer that you alone use.

### A local open model

`acu` makes no network call and needs no API key. The model, the client and
`acu` can all run on your computer:

1. Run a local model with tool calling, in a local runner such as Ollama or
   llama.cpp's server.
2. Pick an MCP client that can use that runner as its model. Choose one
   that lists tool calling for local models.
3. Add `acu mcp -c atlas.yaml` to the client as above, or point it at
   `http://127.0.0.1:4321/mcp` while `acu serve` runs, with the header
   `Authorization: Bearer <token>`.
4. Ask a question, such as "How does a request reach its handler?". A good
   session starts with `overview`, then `search`, then `trace` or `symbol`.

The tools suit models with small context windows: every tool takes flat
parameters, and every answer comes as short plain text. Lists come in pages:
25 rows for `search` and `neighbours`, 60 for `trace` and a unit's items, 40
names for a symbol's members, and 200 lines for `read_source`. `overview`
lists at most 40 regions and 40 tour steps, and says how many it leaves out.
A trace stops at 2000 rows in all, and then says so. No test with a local
model has checked these sizes yet.

Give the model your model's name in the `model` parameter of the write
tools, or the mark names only the client.

## Tools

Every tool returns plain text. A failed call returns an error result whose
text starts with `acu:` and names the next step.

### Read tools

| Tool | Parameters | Answers with |
| --- | --- | --- |
| `overview` | none | The project's name, description, counts, regions and tour, and whether the write tools work. |
| `search` | `query`; `kind` (`symbol`, `unit`, `entry_point`, or a symbol kind such as `method`); `page` | Symbols, units and entry points whose name, ID or first doc sentence holds the query, exact names first, with their IDs. |
| `symbol` | `id`: an ID, a qualified name (`Mux.Get`, `chi.Router`) or a unit's path; `page` | One symbol's kind, unit, region, source place, signature, docs, members, edge counts, tests and entry points. A unit lists its items; an entry point names its handler. `page` pages the members or the items. |
| `neighbours` | `id`; `direction` (`out`, `in` or `both`); `kind` (an edge kind such as `calls`); `page` | The edges of the symbol and of its members: what it uses and what uses it. |
| `path` | `from`, `to`; `max_depth` (1 to 12, default 8) | The shortest chain of uses from one symbol (or entry point) to another. A type's members count as part of the type; a step that leaves from a member names it. Without a chain, a chain that ignores the direction of the edges, marked as such. |
| `trace` | `start`: an entry point or a function; `depth` (1 to 8, default 3); `page` | The calls that run from the start, as an indented tree in source order. A call to an interface method lists the implementing methods as "may run". Each function opens once, at the shallowest depth where it occurs; other rows say "seen above" or "opens at depth N". |
| `read_source` | `path`: a file path from the repository root, or a symbol's ID or name; `start_line`, `end_line` | Numbered source lines, at most 200 per call. A symbol reads its own lines. |
| `check` | none | The result of `acu check` on the current map, with the warnings of the last assembly. |

### Write tools

| Tool | Parameters | Changes in `atlas.yaml` |
| --- | --- | --- |
| `draft_region` | `id`; `name`, `description`, `reading_tip`, `paths` (a list, or one comma-separated text); `model` | Adds a region (a new one needs `name` and `paths`), or changes the given fields of the region with that `id`. Sets its `drafted` mark. |
| `draft_description` | `text`; `model` | Sets `project.description` and `project.descriptionDrafted`. |
| `draft_tour_step` | `title`, `text`, `view` (`overview`, `region`, `unit`, `symbol` or `trace`); `target`; `position` or `replace` (step numbers from 1); `model` | Adds a step at the end, inserts one before `position`, or replaces step `replace`. Sets its `drafted` mark. |

`target` depends on the view: a region's `id`, a unit's path or ID, a
symbol's name or ID, or an entry point or a function for a trace. `acu` writes
the matching field of `open` (see [config.md](config.md#tour)).

## Drafts

Each write follows the same steps:

1. `acu` reads `atlas.yaml` from disk, so it never overwrites an edit that a
   person saved meanwhile.
2. It rewrites only the lines of the one entry that changes. Comments,
   blank lines and the order of everything else stay as written. A section
   in flow style (`tour: [...]`) gets written again whole; its comments
   then go.
3. It adds the mark: `drafted: { by: <model>, at: <RFC 3339 time> }`. `by`
   takes the `model` parameter, else "a model in" and the client's name.
4. It checks the new config against the config schema and runs the check of
   `acu extract` on the map it makes. A tour step whose target matches
   nothing, or matches more than one thing, fails here. On any error it
   writes nothing and returns the problems.
5. It replaces the file in one step, and the served map changes at once. A
   reload of the viewer shows the draft with the "Machine-drafted" label.

To accept a draft, read it against the code, fix it, and delete its
`drafted` line (`descriptionDrafted` for the description). The viewer then
shows the text as a person's own. [config.md](config.md#drafted-text)
describes the mark.

Tours, regions and the description all live in `atlas.yaml`; this project
keeps no other guide files yet, so the write tools change that one file.

## Limits

1. The read tools see only the project folder: the repository root of each
   language in the config, or with `-d` the `--root` folder (default: the
   current folder). `read_source` resolves `..` and symbolic links first. It
   refuses an absolute path, a path that then leaves the folder, a file or
   folder whose name starts with a dot (such as `.git/config` or `.env`),
   a file over 2 MB, and a file that neither git tracks (`git ls-files`)
   nor the map names.
2. The write tools change only the `atlas.yaml` that the server started
   with, and nothing else: no source file, no other config. Each line that
   a write leaves keeps its own line ending, CR LF or LF. A new line takes
   the ending of most lines in the file. On Linux and macOS, a file with
   more than one hard link keeps its links; see
   [`studio.md`](studio.md#how-a-save-works), step 5.
3. One `acu` process writes one draft at a time. Two processes that serve
   the same `atlas.yaml` do not wait for each other: each writes the whole
   file, and the later write wins. Run one server per config.
4. `--read-only` turns the write tools off. Clients then see eight tools.
   So does an address that other computers can reach, without
   `--allow-remote-writes`.
5. The HTTP endpoint accepts only requests whose `Host` names a loopback
   name or the address it listens on, each with the server's port. It
   refuses a request whose `Origin` names another site. So a web page
   elsewhere cannot reach it.
   `--allow-host` changes nothing here: a forwarded name reaches the map's
   pages, never `/mcp`.
6. Every HTTP call needs the session token as `Authorization: Bearer
   <token>`. The viewer's cookie does not count. acu checks the `Host`,
   the `Origin` and then the token, in the API's order, before the MCP
   library reads the request. So another process or user on the computer
   cannot call the tools without the token.
7. A write applies the config to the adapters' output from the start of the
   run. After a change to `languages` or `entryPoints`, or to the code,
   restart the server.

## Tests

`tests/e2e/mcp.mjs` acts as a scripted client. It speaks JSON-RPC to
`acu mcp` on the chi and Cobra examples, calls every tool, and checks the
answers against facts of the code. Then it runs `acu serve` with
`--allow-host Fwd.Example:9999`, and reads the session token from its
standard error. Over HTTP it checks these:

1. A client without the token gets 401.
2. A foreign `Origin` on `/mcp` gets exactly 403.
3. A foreign `Host` gets 403, and the message holds the `--allow-host`
   hint.
4. The allowed name gets the page with 200, without the API mark, and its
   data with 200.
5. The allowed name gets 403 on `/api/`, even with the token.
6. The allowed name with another port gets 403.

It then drafts a tour step with `Authorization: Bearer <token>`, opens the
served map in Chromium at `/?token=<token>`, and checks for the
"Machine-drafted" label. `scripts/check.sh` runs it:

```sh
node tests/e2e/mcp.mjs ./target/release/acu out/mcp
```

The Rust unit tests in `cli/src/mcp/` cover each tool on an invented
fixture, the path limits, and the edits of `atlas.yaml`.
