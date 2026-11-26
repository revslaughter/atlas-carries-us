//! The MCP server of acu: it lets a language model of the user's choice read
//! the map, answer questions about the code, and draft regions, the project
//! description and tour steps. `acu serve` offers it over streamable HTTP at
//! `/mcp`; `acu mcp` offers it over standard input and output.
//!
//! The tools take flat parameters and return short, paged text. Read tools
//! see only the project folder; write tools change only atlas.yaml, and
//! `--read-only` turns them off. docs/mcp.md describes each tool.

pub mod atlas;
pub mod read;
pub mod write;
pub mod yaml;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    JsonObject, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, ServerHandler, ServiceExt};
use serde_json::{json, Value};

pub use atlas::Atlas;

/// What the server tells a client when it connects.
const INSTRUCTIONS: &str = "This server reads one code map made by Atlas Carries Us. Start with overview. Use search to find IDs, then symbol, neighbours, path, trace and read_source. Lists come in pages; pass page to read more. Write tools (draft_*) put text into atlas.yaml with a drafted mark; give your model name in the model parameter.";

/// The MCP server: one handler over the shared live map.
#[derive(Clone)]
pub struct Server {
    /// The live map.
    atlas: Arc<Atlas>,
}

impl Server {
    /// Makes a server over a live map.
    pub fn new(atlas: Arc<Atlas>) -> Server {
        Server { atlas }
    }
}

/// Builds one tool's input schema from its parameters: (name, type,
/// description), with the names in `required` marked as such.
fn schema(params: &[(&str, &str, &str)], required: &[&str]) -> JsonObject {
    let mut props = serde_json::Map::new();
    for (name, ty, desc) in params {
        let p = if *ty == "array" {
            json!({ "type": "array", "items": { "type": "string" }, "description": desc })
        } else {
            json!({ "type": ty, "description": desc })
        };
        props.insert(name.to_string(), p);
    }
    let mut m = serde_json::Map::new();
    m.insert("type".into(), json!("object"));
    m.insert("properties".into(), Value::Object(props));
    m.insert("required".into(), json!(required));
    m
}

/// The parameter that names the drafting model.
const MODEL: (&str, &str, &str) = (
    "model",
    "string",
    "Your model's name, for the drafted mark. Default: the client's name.",
);

/// The parameter that picks a page of a list.
const PAGE: (&str, &str, &str) = ("page", "integer", "Page number, from 1. Default 1.");

/// Lists the tools. The write tools appear only when writes work.
pub fn tools(writable: bool) -> Vec<Tool> {
    let ro = || ToolAnnotations::new().read_only(true).open_world(false);
    let rw = || {
        ToolAnnotations::new()
            .read_only(false)
            .destructive(false)
            .idempotent(false)
            .open_world(false)
    };
    let mut t = vec![
        Tool::new(
            "overview",
            "Shows the project: its description, counts, regions and tour. Call it first.",
            schema(&[], &[]),
        )
        .annotate(ro()),
        Tool::new(
            "search",
            "Finds symbols, units and entry points whose name, ID or first doc sentence holds the query. Returns IDs for the other tools.",
            schema(
                &[
                    ("query", "string", "Part of a name, such as Router or ServeHTTP."),
                    ("kind", "string", "Optional: symbol, unit, entry_point, or a symbol kind such as method or struct."),
                    PAGE,
                ],
                &["query"],
            ),
        )
        .annotate(ro()),
        Tool::new(
            "symbol",
            "Shows one symbol: kind, unit, region, source place, signature, docs, members, tests and entry points. A unit shows its items.",
            schema(
                &[("id", "string", "An ID from search, or a qualified name such as Mux.Get, or a unit path.")],
                &["id"],
            ),
        )
        .annotate(ro()),
        Tool::new(
            "neighbours",
            "Lists what a symbol uses and what uses it, with the edge kind: calls, implements, extends, field_type, param_type, return_type, imports, overrides.",
            schema(
                &[
                    ("id", "string", "A symbol ID or name."),
                    ("direction", "string", "out (what it uses), in (what uses it) or both. Default both."),
                    ("kind", "string", "Optional: keep one edge kind, such as calls."),
                    PAGE,
                ],
                &["id"],
            ),
        )
        .annotate(ro()),
        Tool::new(
            "path",
            "Finds the shortest chain of uses from one symbol to another.",
            schema(
                &[
                    ("from", "string", "The start: a symbol ID or name, or an entry point."),
                    ("to", "string", "The end: a symbol ID or name."),
                    ("max_depth", "integer", "The longest chain to try, from 1 to 12. Default 8."),
                ],
                &["from", "to"],
            ),
        )
        .annotate(ro()),
        Tool::new(
            "trace",
            "Shows the calls that run from an entry point or a function, as an indented tree in source order.",
            schema(
                &[
                    ("start", "string", "An entry point (such as GET /users/{id}) or a function or method."),
                    ("depth", "integer", "How many calls deep, from 1 to 8. Default 3."),
                    PAGE,
                ],
                &["start"],
            ),
        )
        .annotate(ro()),
        Tool::new(
            "read_source",
            "Reads numbered source lines from the project folder, at most 200 per call.",
            schema(
                &[
                    ("path", "string", "A file path from the repository root, or a symbol ID or name to read its lines."),
                    ("start_line", "integer", "The first line. Default: the symbol's first line, or 1."),
                    ("end_line", "integer", "The last line. Default: the symbol's last line, or 200 lines on."),
                ],
                &["path"],
            ),
        )
        .annotate(ro()),
        Tool::new(
            "check",
            "Runs the check of acu check on the map and lists errors and warnings.",
            schema(&[], &[]),
        )
        .annotate(ro()),
    ];
    if writable {
        t.extend([
            Tool::new(
                "draft_region",
                "Adds a region to atlas.yaml, or changes one, with a drafted mark. A new region needs name and paths.",
                schema(
                    &[
                        ("id", "string", "The region ID: letters, digits, _ and -. An existing ID changes that region."),
                        ("name", "string", "The region's name, by job, such as Storage."),
                        ("description", "string", "What the region does, in short sentences."),
                        ("reading_tip", "string", "Optional: where to start reading."),
                        ("paths", "array", "Globs of unit paths, such as internal/store and internal/store/**."),
                        MODEL,
                    ],
                    &["id"],
                ),
            )
            .annotate(rw()),
            Tool::new(
                "draft_description",
                "Sets the project's overview text in atlas.yaml, with a drafted mark.",
                schema(
                    &[
                        ("text", "string", "The overview, in short sentences. A blank line starts a paragraph."),
                        MODEL,
                    ],
                    &["text"],
                ),
            )
            .annotate(rw()),
            Tool::new(
                "draft_tour_step",
                "Adds a tour step to atlas.yaml, or replaces one, with a drafted mark. The target must exist.",
                schema(
                    &[
                        ("title", "string", "The step's title."),
                        ("text", "string", "The step's text, in short sentences that match the code."),
                        ("view", "string", "overview, region, unit, symbol or trace."),
                        ("target", "string", "For region: its id. For unit: its path. For symbol: a name such as Mux.Get or an ID. For trace: an entry point or a function."),
                        ("position", "integer", "Optional: insert before this step number. Default: add at the end."),
                        ("replace", "integer", "Optional: replace this step number."),
                        MODEL,
                    ],
                    &["title", "text", "view"],
                ),
            )
            .annotate(rw()),
        ]);
    }
    t
}

/// The arguments of one tool call, with readers that forgive small models:
/// a number may come as text, and a list as one comma-separated text.
struct Args<'a>(&'a JsonObject);

impl Args<'_> {
    /// A text argument, or "".
    fn text(&self, k: &str) -> String {
        match self.0.get(k) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        }
    }

    /// A whole-number argument, or `default`.
    fn num(&self, k: &str, default: usize) -> usize {
        match self.0.get(k) {
            Some(Value::Number(n)) => n.as_u64().map_or(default, |n| n as usize),
            Some(Value::String(s)) => s.trim().parse().unwrap_or(default),
            _ => default,
        }
    }

    /// A list of texts, or an empty list.
    fn list(&self, k: &str) -> Vec<String> {
        match self.0.get(k) {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            Some(Value::String(s)) => s
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// Runs one tool and returns its text, or an error text for the model.
pub fn call(atlas: &Atlas, name: &str, args: &JsonObject, client: &str) -> Result<String, String> {
    let a = Args(args);
    let by = {
        let m = a.text("model");
        if !m.trim().is_empty() {
            m.trim().to_string()
        } else if !client.is_empty() {
            format!("a model in {client}")
        } else {
            "a model over MCP".to_string()
        }
    };
    let snap = atlas.snapshot()?;
    let text = match name {
        "overview" => read::overview(&snap, atlas.why_read_only()),
        "search" => read::search(&snap, &a.text("query"), &a.text("kind"), a.num("page", 1)),
        "symbol" => read::symbol(&snap, &a.text("id")),
        "neighbours" | "neighbors" => read::neighbours(
            &snap,
            &a.text("id"),
            &a.text("direction"),
            &a.text("kind"),
            a.num("page", 1),
        ),
        "path" => read::path(&snap, &a.text("from"), &a.text("to"), a.num("max_depth", 8)),
        "trace" => read::trace(&snap, &a.text("start"), a.num("depth", 3), a.num("page", 1)),
        "read_source" => read::read_source(
            &snap,
            atlas.roots(),
            &a.text("path"),
            a.num("start_line", 0),
            a.num("end_line", 0),
        ),
        "check" => read::check(&snap),
        "draft_region" | "draft_description" | "draft_tour_step" => {
            if let Some(why) = atlas.why_read_only() {
                return Err(format!("acu: {why}"));
            }
            return match name {
                "draft_region" => write::draft_region(
                    atlas,
                    write::RegionDraft {
                        id: &a.text("id"),
                        name: &a.text("name"),
                        description: &a.text("description"),
                        reading_tip: &a.text("reading_tip"),
                        paths: a.list("paths"),
                    },
                    &by,
                ),
                "draft_description" => write::draft_description(atlas, &a.text("text"), &by),
                _ => write::draft_tour_step(
                    atlas,
                    write::StepDraft {
                        title: &a.text("title"),
                        text: &a.text("text"),
                        view: &a.text("view"),
                        target: &a.text("target"),
                        replace: a.num("replace", 0),
                        position: a.num("position", 0),
                    },
                    &by,
                ),
            }
            .map_err(|e| {
                if e.starts_with("acu:") {
                    e
                } else {
                    format!("acu: {e}")
                }
            });
        }
        other => {
            return Err(format!(
                "acu: no tool has the name {other}; call tools/list for the names"
            ))
        }
    };
    if text.starts_with("acu: ") {
        Err(text)
    } else {
        Ok(text)
    }
}

impl ServerHandler for Server {
    /// Names acu, offers tools, and gives the instructions.
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("acu", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }

    /// Lists the tools; the write tools only when writes work.
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(tools(
            self.atlas.writable(),
        )))
    }

    /// Runs one tool. A tool's failure comes back as a tool error with a
    /// message for the model, not as a protocol error.
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let client = context.client_info().map(|i| i.name).unwrap_or_default();
        let args = request.arguments.unwrap_or_default();
        let atlas = self.atlas.clone();
        let name = request.name.to_string();
        let out = tokio::task::spawn_blocking(move || call(&atlas, &name, &args, &client))
            .await
            .map_err(|e| ErrorData::internal_error(format!("acu: a tool stopped: {e}"), None))?;
        Ok(match out {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Err(text) => CallToolResult::error(vec![ContentBlock::text(text)]),
        }
        .into())
    }
}

/// Runs the MCP server over standard input and output until the client
/// closes them. Messages go to standard error, because standard output
/// carries the protocol.
pub async fn serve_stdio(atlas: Arc<Atlas>) -> Result<()> {
    eprintln!(
        "acu: the MCP server reads standard input; writes {}",
        if atlas.writable() { "on" } else { "off" }
    );
    let service = Server::new(atlas)
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| anyhow::anyhow!("start the MCP server: {e}"))?;
    service
        .waiting()
        .await
        .map_err(|e| anyhow::anyhow!("the MCP server stopped: {e}"))?;
    Ok(())
}

/// Makes the streamable HTTP service that `acu serve` mounts at `/mcp`. It
/// accepts only requests for the address it listens on, so a web page on
/// another site cannot reach it. It also returns a function that closes its
/// open streams, for when the server shuts down.
pub fn http_service(
    atlas: Arc<Atlas>,
    addr: SocketAddr,
) -> (
    StreamableHttpService<Server, LocalSessionManager>,
    impl FnOnce() + Send + 'static,
) {
    let port = addr.port();
    let mut hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    let ip = addr.ip().to_string();
    if !hosts.contains(&ip) {
        hosts.push(ip.clone());
    }
    let host = if addr.is_ipv6() {
        format!("[{ip}]")
    } else {
        ip
    };
    let origins = vec![
        format!("http://localhost:{port}"),
        format!("http://127.0.0.1:{port}"),
        format!("http://{host}:{port}"),
    ];
    let config = StreamableHttpServerConfig::default()
        .with_allowed_hosts(hosts)
        .with_allowed_origins(origins)
        .enforce_origin_validation();
    let token = config.cancellation_token.clone();
    let service = StreamableHttpService::new(
        move || Ok(Server::new(atlas.clone())),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    (service, move || token.cancel())
}

#[cfg(test)]
mod tests {
    use super::atlas::tests::fixture;
    use super::*;

    #[test]
    fn the_tool_list_hides_writes_when_read_only() {
        let all: Vec<String> = tools(true).iter().map(|t| t.name.to_string()).collect();
        assert_eq!(
            all,
            [
                "overview",
                "search",
                "symbol",
                "neighbours",
                "path",
                "trace",
                "read_source",
                "check",
                "draft_region",
                "draft_description",
                "draft_tour_step"
            ]
        );
        assert_eq!(tools(false).len(), 8);
    }

    #[test]
    fn calls_forgive_numbers_as_text_and_name_the_client() {
        let (_d, a) = fixture();
        let args = json!({"query": "Buy", "page": "1"});
        let t = call(&a, "search", args.as_object().unwrap(), "").unwrap();
        assert!(t.contains("go:shop#Service.Buy"), "{t}");
        let args = json!({"text": "A shop."});
        let t = call(
            &a,
            "draft_description",
            args.as_object().unwrap(),
            "some-client",
        )
        .unwrap();
        assert!(t.contains("drafted by a model in some-client"), "{t}");
        let e = call(&a, "nothing", &JsonObject::new(), "").unwrap_err();
        assert!(e.starts_with("acu: no tool"), "{e}");
        let e = call(&a, "symbol", json!({"id": "Nope"}).as_object().unwrap(), "").unwrap_err();
        assert!(e.contains("search"), "{e}");
    }
}
