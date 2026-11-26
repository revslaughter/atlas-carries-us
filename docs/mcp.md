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

1. Both commands run the adapters first, as `acu extract` does, so the tools
   answer from a fresh map. The first answer can take a few seconds.
2. `--addr` moves `acu serve` to another address; the MCP server moves with
   it, to `/mcp` on that address.
3. `acu mcp` writes the protocol to standard output and every message to
   standard error. Start it from a client, not by hand.
4. With `-d`, no config stands behind the map, so the write tools stay off.
   `read_source` then reads files from the current folder. When the file
   changes (after a new `acu extract`), the next call reads it again.

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
`acu serve`: `http://127.0.0.1:4321/mcp`.

### A local open model

`acu` makes no network call and needs no API key, so the whole loop can stay
on your computer:

1. Run a local model with tool calling, in a local runner such as Ollama or
   llama.cpp's server.
2. Pick an MCP client that can use that runner as its model. Choose one
   that lists tool calling for local models.
3. Add `acu mcp -c atlas.yaml` to the client as above, or point it at
   `http://127.0.0.1:4321/mcp` while `acu serve` runs.
4. Ask a question, such as "How does a request reach its handler?". A good
   session starts with `overview`, then `search`, then `trace` or `symbol`.

Small models handle the tools well: every tool takes flat parameters, and
every answer comes as short plain text in pages of at most 25 rows (60 for a
trace or a unit's items, 200 lines for source). Give the model your model's
name in the `model` parameter of the write tools, or the mark names only the
client.

## Tools

Every tool returns plain text. A failed call returns an error result whose
text starts with `acu:` and names the next step.

### Read tools

| Tool | Parameters | Answers with |
| --- | --- | --- |
| `overview` | none | The project's name, description, counts, regions and tour, and whether the write tools work. |
| `search` | `query`; `kind` (`symbol`, `unit`, `entry_point`, or a symbol kind such as `method`); `page` | Symbols, units and entry points whose name, ID or first doc sentence holds the query, exact names first, with their IDs. |
| `symbol` | `id`: an ID, a qualified name (`Mux.Get`, `chi.Router`) or a unit's path | One symbol's kind, unit, region, source place, signature, docs, members, edge counts, tests and entry points. A unit lists its items; an entry point names its handler. |
| `neighbours` | `id`; `direction` (`out`, `in` or `both`); `kind` (an edge kind such as `calls`); `page` | The edges of the symbol and of its members: what it uses and what uses it. |
| `path` | `from`, `to`; `max_depth` (1 to 12, default 8) | The shortest chain of uses from one symbol (or entry point) to another. Without one, a chain that ignores the direction of the edges, marked as such. |
| `trace` | `start`: an entry point or a function; `depth` (1 to 8, default 3); `page` | The calls that run from the start, as an indented tree in source order. A call to an interface method lists the implementing methods as "may run". |
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
   language in the config, or the current folder with `-d`. `read_source`
   resolves `..` and symbolic links first, and refuses a path that then
   leaves the folder, an absolute path, and a file over 2 MB.
2. The write tools change only the `atlas.yaml` that the server started
   with, and nothing else: no source file, no other config.
3. `--read-only` turns the write tools off. Clients then see eight tools.
4. The HTTP endpoint accepts only requests whose `Host` names the loopback
   address or the address it listens on, and refuses a request whose
   `Origin` names another site. A web page elsewhere cannot reach it.
5. A write applies the config to the adapters' output from the start of the
   run. After a change to `languages` or `entryPoints`, or to the code,
   restart the server.

## Tests

`tests/e2e/mcp.mjs` acts as a scripted client. It speaks JSON-RPC to
`acu mcp` on the chi and Cobra examples, calls every tool, and checks the
answers against facts of the code. It then drafts a tour step through
`acu serve` over HTTP, opens the served map in Chromium, and checks for the
"Machine-drafted" label. `scripts/check.sh` runs it:

```sh
node tests/e2e/mcp.mjs ./target/release/acu out/mcp
```

The Rust unit tests in `cli/src/mcp/` cover each tool on an invented
fixture, the path limits, and the edits of `atlas.yaml`.
