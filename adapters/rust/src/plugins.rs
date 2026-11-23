//! Entry-point plugins find the ways into the code: HTTP routes and CLI
//! commands. Each one switches on from the config file.
//!
//! - `axum`: `.route("/x", get(h).post(g))`, with `.nest("/prefix", r)` and
//!   `.merge(r)` for routers that other routers mount.
//! - `clap`: the derive API. A `Parser` struct names the program, and each
//!   variant of a `Subcommand` enum names a command.
//!
//! Neither rustdoc nor SCIP writes a syntax tree, so the plugins read tokens
//! (`lexer.rs`) inside each function, and SCIP resolves the names among them:
//! a `.route` counts only when it resolves to axum's `Router`.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{bail, Result};
use serde::Deserialize;

use crate::extract::Extractor;
use crate::lexer::{self, Kind, Token};
use crate::request::PluginConfig;
use crate::schema::{EntryPoint, Location};
use crate::scip;

/// Puts a path in front of the routes that one function registers, for a
/// mount that the plugin cannot see.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefixRule {
    /// Routes registered inside the function or method of this name gain the prefix.
    pub in_func: String,
    /// The path to put in front, such as "/api".
    pub prefix: String,
}

/// The options of the axum plugin.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RouteOptions {
    /// The prefix rules, matched by function name.
    pub prefixes: Vec<PrefixRule>,
}

/// The plugins that a request switches on.
pub struct Plugins {
    /// The axum plugin's options, when it is on.
    axum: Option<RouteOptions>,
    /// Whether the clap plugin is on.
    clap: bool,
}

/// The methods of axum's method routers, and the HTTP method of each.
const VERBS: &[(&str, &str)] = &[
    ("get", "GET"),
    ("post", "POST"),
    ("put", "PUT"),
    ("delete", "DELETE"),
    ("patch", "PATCH"),
    ("head", "HEAD"),
    ("options", "OPTIONS"),
    ("trace", "TRACE"),
    ("connect", "CONNECT"),
    ("any", "ANY"),
];

/// The crates whose traits mark clap's derive API.
const CLAP_CRATES: &[&str] = &["clap", "clap_builder", "clap_derive"];

impl Plugins {
    /// Switches on the plugins that the request names, and fails on an
    /// unknown name with a message that lists the known ones.
    pub fn new(cfgs: &[PluginConfig]) -> Result<Self> {
        let mut p = Plugins {
            axum: None,
            clap: false,
        };
        for c in cfgs {
            match c.plugin.as_str() {
                "axum" => {
                    let o: RouteOptions = if c.options.is_null() {
                        RouteOptions::default()
                    } else {
                        serde_json::from_value(c.options.clone())
                            .map_err(|e| anyhow::anyhow!("the options of plugin axum: {e}"))?
                    };
                    p.axum = Some(o);
                }
                "clap" => p.clap = true,
                other => bail!("the Rust adapter has no entry-point plugin named {other:?} (known: axum, clap)"),
            }
        }
        Ok(p)
    }

    /// Runs the plugins that are on, and adds their entry points.
    pub fn run(&self, ex: &mut Extractor) {
        if self.axum.is_none() && !self.clap {
            return;
        }
        let mut src = Sources::new(ex);
        let mut found = Vec::new();
        if let Some(o) = &self.axum {
            found.extend(axum(ex, &mut src, o));
        }
        if self.clap {
            found.extend(clap(ex, &mut src));
        }
        ex.entries.extend(found);
    }
}

/// The tokens of each source file, and the SCIP references in it, read when
/// a plugin first needs them.
struct Sources {
    /// The tokens of each file, by its path from the repository root.
    tokens: HashMap<String, Vec<Token>>,
    /// The symbol that each reference names, by file and position.
    refs: HashMap<String, HashMap<(u32, u32), String>>,
    /// Maps a symbol ID to the SCIP symbol that defines it.
    scip_of: HashMap<String, String>,
    /// Every reference to each SCIP symbol: file, line and column.
    uses: HashMap<String, Vec<(String, u32, u32)>>,
    /// Every definition of each SCIP symbol: file, line and column. Nested
    /// functions of one name share a symbol, so one symbol may have several.
    defs: HashMap<String, Vec<(String, u32, u32)>>,
}

impl Sources {
    /// Indexes the references of the SCIP index by file and by symbol.
    fn new(ex: &Extractor) -> Self {
        let mut refs: HashMap<String, HashMap<(u32, u32), String>> = HashMap::new();
        let mut uses: HashMap<String, Vec<(String, u32, u32)>> = HashMap::new();
        let mut defs: HashMap<String, Vec<(String, u32, u32)>> = HashMap::new();
        for d in &ex.index.documents {
            let file = ex.rel(std::path::Path::new(&d.relative_path));
            let m = refs.entry(file.clone()).or_default();
            for o in &d.occurrences {
                if o.is_definition() {
                    defs.entry(o.symbol.clone()).or_default().push((
                        file.clone(),
                        o.range.start_line,
                        o.range.start_col,
                    ));
                    continue;
                }
                m.insert((o.range.start_line, o.range.start_col), o.symbol.clone());
                uses.entry(o.symbol.clone()).or_default().push((
                    file.clone(),
                    o.range.start_line,
                    o.range.start_col,
                ));
            }
        }
        for list in uses.values_mut() {
            list.sort();
        }
        let scip_of = ex
            .by_scip
            .iter()
            .map(|(s, id)| (id.clone(), s.clone()))
            .collect();
        Sources {
            tokens: HashMap::new(),
            refs,
            scip_of,
            uses,
            defs,
        }
    }

    /// Returns the tokens of a file, reading it on first use.
    fn tokens(&mut self, ex: &Extractor, file: &str) -> &[Token] {
        self.tokens.entry(file.to_string()).or_insert_with(|| {
            std::fs::read_to_string(ex.repo_root.join(file))
                .map(|s| lexer::lex(&s))
                .unwrap_or_default()
        })
    }

    /// The SCIP symbol that a token names, if SCIP saw a reference there.
    fn symbol_at(&self, file: &str, t: &Token) -> Option<&str> {
        self.refs
            .get(file)?
            .get(&(t.line, t.col))
            .map(String::as_str)
    }

    /// The symbol or container ID that a token names, if the map holds it.
    fn target_at<'e>(&self, ex: &'e Extractor, file: &str, t: &Token) -> Option<&'e String> {
        ex.by_scip.get(self.symbol_at(file, t)?)
    }
}

/// Finds the index of the first token at or after a position.
fn token_index(tokens: &[Token], line: u32, col: u32) -> usize {
    tokens.partition_point(|t| (t.line, t.col) < (line, col))
}

/// Splits the tokens between two brackets into arguments at the commas of
/// depth zero. Returns each argument as a range of indexes.
fn arguments(tokens: &[Token], open: usize, close: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = open + 1;
    for (i, t) in tokens.iter().enumerate().take(close).skip(open + 1) {
        if t.kind != Kind::Punct {
            continue;
        }
        match t.text.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth -= 1,
            "," if depth == 0 => {
                out.push((start, i));
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < close {
        out.push((start, close));
    }
    out
}

/// Joins two route paths with one slash between them.
fn join_path(a: &str, b: &str) -> String {
    if a.is_empty() {
        return if b.is_empty() { "/".into() } else { b.into() };
    }
    if b.is_empty() || b == "/" {
        return a.into();
    }
    let b = if b.starts_with('/') {
        b.to_string()
    } else {
        format!("/{b}")
    };
    format!("{}{b}", a.trim_end_matches('/'))
}

/// Reports whether a SCIP symbol names a method of axum's `Router` by name,
/// and not one of the routers inside it, such as `PathRouter`.
fn is_router_method(sym: Option<&str>, name: &str) -> bool {
    sym.and_then(scip::parse_symbol).is_some_and(|s| {
        s.package == "axum"
            && s.name() == name
            && s.descriptors
                .iter()
                .any(|d| d.name == "Router" || d.name.starts_with("Router<"))
    })
}

/// Reports whether a SCIP symbol names a method or function of axum by name.
fn is_axum(sym: Option<&str>, name: &str) -> bool {
    sym.and_then(scip::parse_symbol)
        .is_some_and(|s| s.package == "axum" && s.name() == name)
}

/// The handler of a route or command: the first function of the map that an
/// expression names. A closure keeps the registering function and counts as
/// inline; anything else names an outside handler.
struct Handler {
    /// The function that serves the entry point.
    id: String,
    /// Whether the handler sits inline in the registering function.
    inline: bool,
    /// The handler as written, when it comes from outside the map.
    external: String,
}

/// Reads a handler expression: the tokens `range` in a file.
fn handler(
    ex: &Extractor,
    src: &Sources,
    file: &str,
    tokens: &[Token],
    range: (usize, usize),
    registrar: &str,
) -> Handler {
    let (a, b) = range;
    let first = tokens.get(a);
    if first.is_some_and(|t| t.is("|") || t.is_ident("move") || t.is_ident("async")) {
        return Handler {
            id: registrar.to_string(),
            inline: true,
            external: String::new(),
        };
    }
    for t in &tokens[a..b] {
        if t.kind != Kind::Ident {
            continue;
        }
        if let Some(id) = src.target_at(ex, file, t) {
            if ex
                .syms
                .get(id)
                .is_some_and(|s| matches!(s.kind.as_str(), "function" | "method"))
            {
                return Handler {
                    id: id.clone(),
                    inline: false,
                    external: String::new(),
                };
            }
        }
        // A function nested in another one stays out of rustdoc's output,
        // so the function around it serves inline: the registrar's own
        // nested function first, else the first one of that name.
        if let Some(owner) = nested_owner(ex, src, file, t, registrar) {
            return Handler {
                id: owner,
                inline: true,
                external: String::new(),
            };
        }
    }
    let text: String = tokens[a..b]
        .iter()
        .map(|t| {
            if t.kind == Kind::Str {
                format!("{:?}", t.text)
            } else {
                t.text.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("");
    Handler {
        id: registrar.to_string(),
        inline: false,
        external: text,
    }
}

/// Finds the function of the map that holds the definition of a nested
/// function that a token names. Prefers the registrar's own nested function.
fn nested_owner(
    ex: &Extractor,
    src: &Sources,
    file: &str,
    t: &Token,
    registrar: &str,
) -> Option<String> {
    let sym = src.symbol_at(file, t)?;
    if !sym.ends_with(").") || ex.by_scip.contains_key(sym) {
        return None;
    }
    let owners: Vec<&str> = src
        .defs
        .get(sym)?
        .iter()
        .filter_map(|(f, l, c)| ex.callable_at(f, *l, *c))
        .collect();
    owners
        .iter()
        .find(|o| **o == registrar)
        .or(owners.first())
        .map(|o| o.to_string())
}

/// Reads a string literal that fills a range of tokens alone.
fn literal(tokens: &[Token], (a, b): (usize, usize)) -> Option<String> {
    (b == a + 1 && tokens[a].kind == Kind::Str).then(|| tokens[a].text.clone())
}

/// Finds which parameter of a function a range of tokens names, when the
/// range holds that parameter's name alone.
fn param_index(
    ex: &Extractor,
    func: &str,
    tokens: &[Token],
    (a, b): (usize, usize),
) -> Option<usize> {
    if b != a + 1 || tokens[a].kind != Kind::Ident {
        return None;
    }
    let sig = ex.syms.get(func)?.signature.as_ref()?;
    sig.params.iter().position(|p| p.name == tokens[a].text)
}

/// A function of the project that wraps `.route(path, router)` and takes the
/// path, and perhaps the method router, as parameters. Its callers register
/// the routes.
struct Helper {
    /// The wrapping function.
    func: String,
    /// The function's file.
    file: String,
    /// The parameter that takes the path.
    path_param: usize,
    /// The parameter that takes the method router, or the tokens of the
    /// method router inside the helper.
    router: Result<usize, (usize, usize)>,
}

/// One route as the scan finds it, before the mounts that set its full path.
struct Route {
    /// The HTTP method.
    method: String,
    /// The path as written.
    path: String,
    /// Who serves it.
    handler: Handler,
    /// The function that registers it.
    registrar: String,
    /// The token where the registration starts, in the registrar's file.
    at: usize,
    /// Where the registration happens.
    location: Location,
}

/// One mount: `.nest("/api", child)` or `.merge(child)` inside a function.
struct Mount {
    /// The function that mounts.
    parent: String,
    /// The token of the `.nest` or `.merge`.
    at: usize,
    /// The path that the mount puts in front.
    prefix: String,
    /// What the mount adds: a function's routes, or the routes in a range of
    /// tokens of the parent's file.
    child: Child,
}

/// The routes that a mount adds.
enum Child {
    /// The routes that a function registers.
    Function(String),
    /// The routes registered in a range of tokens of the parent's file.
    Region(usize, usize),
}

/// The functions of each file: ID, first position and the position after the end.
type FileFunctions = BTreeMap<String, Vec<(String, (u32, u32), (u32, u32))>>;

/// Runs the axum plugin over every function of the map.
fn axum(ex: &Extractor, src: &mut Sources, opts: &RouteOptions) -> Vec<EntryPoint> {
    let mut routes = Vec::new();
    let mut mounts = Vec::new();
    let mut helpers = Vec::new();
    let mut files: FileFunctions = BTreeMap::new();
    for (id, sp) in &ex.spans {
        if ex
            .syms
            .get(id)
            .is_some_and(|s| matches!(s.kind.as_str(), "function" | "method"))
        {
            files
                .entry(sp.file.clone())
                .or_default()
                .push((id.clone(), sp.start, sp.end));
        }
    }
    for (file, fns) in &files {
        let tokens = src.tokens(ex, file).to_vec();
        for (id, start, end) in fns {
            let a = token_index(&tokens, start.0, start.1);
            let b = token_index(&tokens, end.0, end.1).min(tokens.len());
            for i in a..b {
                let t = &tokens[i];
                if t.kind != Kind::Ident
                    || i == 0
                    || !tokens[i - 1].is(".")
                    || !tokens.get(i + 1).is_some_and(|n| n.is("("))
                {
                    continue;
                }
                // The innermost function owns the registration.
                if ex.callable_at(file, t.line, t.col) != Some(id.as_str()) {
                    continue;
                }
                let sym = src.symbol_at(file, t);
                let name = t.text.as_str();
                if !matches!(name, "route" | "route_service" | "nest" | "merge")
                    || !is_router_method(sym, name)
                {
                    continue;
                }
                let close = lexer::closing(&tokens, i + 1);
                let args = arguments(&tokens, i + 1, close);
                let location = Location {
                    path: file.clone(),
                    line: t.line + 1,
                    end_line: 0,
                };
                let literal = |r: (usize, usize)| literal(&tokens, r);
                match (name, args.as_slice()) {
                    ("route", [p, mr]) => {
                        let Some(path) = literal(*p) else {
                            if let Some(pi) = param_index(ex, id, &tokens, *p) {
                                helpers.push(Helper {
                                    func: id.clone(),
                                    file: file.clone(),
                                    path_param: pi,
                                    router: param_index(ex, id, &tokens, *mr).ok_or(*mr),
                                });
                            }
                            continue;
                        };
                        for (method, h) in method_router(ex, src, file, &tokens, *mr, id) {
                            routes.push(Route {
                                method,
                                path: path.clone(),
                                handler: h,
                                registrar: id.clone(),
                                at: i,
                                location: location.clone(),
                            });
                        }
                    }
                    ("route_service", [p, svc]) => {
                        let Some(path) = literal(*p) else { continue };
                        routes.push(Route {
                            method: "ANY".into(),
                            path,
                            handler: handler(ex, src, file, &tokens, *svc, id),
                            registrar: id.clone(),
                            at: i,
                            location,
                        });
                    }
                    ("nest", [p, child]) => {
                        let Some(prefix) = literal(*p) else { continue };
                        mounts.push(Mount {
                            parent: id.clone(),
                            at: i,
                            prefix,
                            child: child_of(ex, src, file, &tokens, *child),
                        });
                    }
                    ("merge", [child]) => mounts.push(Mount {
                        parent: id.clone(),
                        at: i,
                        prefix: String::new(),
                        child: child_of(ex, src, file, &tokens, *child),
                    }),
                    _ => {}
                }
            }
        }
    }
    routes.extend(helper_routes(ex, src, &helpers));
    let mut out = Vec::new();
    for r in routes {
        let rule = opts
            .prefixes
            .iter()
            .find(|x| {
                ex.syms
                    .get(&r.registrar)
                    .is_some_and(|s| s.name == x.in_func)
            })
            .map(|x| x.prefix.clone())
            .unwrap_or_default();
        let mut paths = BTreeSet::new();
        for p in prefixes_at(&mounts, &r.registrar, r.at, &mut Vec::new()) {
            paths.insert(join_path(&join_prefix(&rule, &p), &r.path));
        }
        for path in paths {
            let name = format!("{} {path}", r.method);
            out.push(EntryPoint {
                id: format!("http:{name}"),
                kind: "http".into(),
                name,
                method: r.method.clone(),
                path,
                handler: r.handler.id.clone(),
                inline: r.handler.inline,
                external_handler: r.handler.external.clone(),
                registrar: r.registrar.clone(),
                registered_at: r.location.clone(),
                plugin: "axum".into(),
            });
        }
    }
    out
}

/// Turns each call of a route helper into routes, registered by the caller.
fn helper_routes(ex: &Extractor, src: &mut Sources, helpers: &[Helper]) -> Vec<Route> {
    let mut out = Vec::new();
    for h in helpers {
        let Some(sym) = src.scip_of.get(&h.func).cloned() else {
            continue;
        };
        let uses = src.uses.get(&sym).cloned().unwrap_or_default();
        for (file, line, col) in uses {
            let Some(caller) = ex.callable_at(&file, line, col).map(str::to_string) else {
                continue;
            };
            let tokens = src.tokens(ex, &file).to_vec();
            let u = token_index(&tokens, line, col);
            // A plain call only: `route("/x", get(h))`, not `.route(…)`.
            if !tokens.get(u + 1).is_some_and(|t| t.is("(")) || (u > 0 && tokens[u - 1].is(".")) {
                continue;
            }
            let args = arguments(&tokens, u + 1, lexer::closing(&tokens, u + 1));
            let Some(path) = args.get(h.path_param).and_then(|r| literal(&tokens, *r)) else {
                continue;
            };
            let found = match h.router {
                Ok(mi) => match args.get(mi) {
                    Some(r) => method_router(ex, src, &file, &tokens, *r, &caller),
                    None => continue,
                },
                Err(r) => {
                    let own = src.tokens(ex, &h.file).to_vec();
                    method_router(ex, src, &h.file, &own, r, &caller)
                }
            };
            for (method, handler) in found {
                out.push(Route {
                    method,
                    path: path.clone(),
                    handler,
                    registrar: caller.clone(),
                    at: u,
                    location: Location {
                        path: file.clone(),
                        line: line + 1,
                        end_line: 0,
                    },
                });
            }
        }
    }
    out
}

/// Reads the method router of a route: `get(h)`, `get(h).post(g)`,
/// `routing::any(h)`. Returns each method with its handler.
fn method_router(
    ex: &Extractor,
    src: &Sources,
    file: &str,
    tokens: &[Token],
    (a, b): (usize, usize),
    registrar: &str,
) -> Vec<(String, Handler)> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut i = a;
    while i < b {
        let t = &tokens[i];
        if t.kind == Kind::Punct {
            match t.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
            i += 1;
            continue;
        }
        if depth == 0 && t.kind == Kind::Ident && tokens.get(i + 1).is_some_and(|n| n.is("(")) {
            // `post_service(svc)` serves POST with a service.
            let base = t.text.strip_suffix("_service").unwrap_or(&t.text);
            let verb = VERBS.iter().find(|(v, _)| *v == base);
            if let Some((_, method)) = verb {
                if is_axum(src.symbol_at(file, t), &t.text) {
                    let close = lexer::closing(tokens, i + 1);
                    if let Some(arg) = arguments(tokens, i + 1, close).into_iter().next() {
                        out.push((
                            method.to_string(),
                            handler(ex, src, file, tokens, arg, registrar),
                        ));
                    }
                    i = close + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// Reads what a `.nest` or `.merge` mounts: a call to a function of the map,
/// a local variable that holds a router, or a router written inline.
fn child_of(
    ex: &Extractor,
    src: &Sources,
    file: &str,
    tokens: &[Token],
    (a, b): (usize, usize),
) -> Child {
    if b == a + 1 && tokens[a].kind == Kind::Ident {
        if let Some(sym) = src.symbol_at(file, &tokens[a]) {
            if sym.starts_with("local ") {
                if let Some(region) = local_initializer(ex, file, tokens, sym, a) {
                    return Child::Region(region.0, region.1);
                }
            }
        }
    }
    for i in a..b {
        let t = &tokens[i];
        if t.kind == Kind::Ident && tokens.get(i + 1).is_some_and(|n| n.is("(")) {
            if let Some(id) = src.target_at(ex, file, t) {
                if ex
                    .syms
                    .get(id)
                    .is_some_and(|s| matches!(s.kind.as_str(), "function" | "method"))
                {
                    return Child::Function(id.clone());
                }
            }
        }
    }
    Child::Region(a, b)
}

/// Finds the initializer of a local variable: the tokens after `=` in its
/// `let`, up to the `;`. The definition comes from SCIP.
fn local_initializer(
    ex: &Extractor,
    file: &str,
    tokens: &[Token],
    sym: &str,
    before: usize,
) -> Option<(usize, usize)> {
    let doc = ex
        .index
        .documents
        .iter()
        .find(|d| ex.rel(std::path::Path::new(&d.relative_path)) == file)?;
    let use_at = &tokens[before];
    let def = doc
        .occurrences
        .iter()
        .filter(|o| {
            o.is_definition()
                && o.symbol == sym
                && (o.range.start_line, o.range.start_col) < (use_at.line, use_at.col)
        })
        .max_by_key(|o| o.range)?;
    let at = token_index(tokens, def.range.start_line, def.range.start_col);
    let mut depth = 0;
    let mut start = None;
    for (i, t) in tokens.iter().enumerate().skip(at) {
        if t.kind != Kind::Punct {
            continue;
        }
        match t.text.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth -= 1,
            "=" if depth == 0 && start.is_none() => start = Some(i + 1),
            ";" if depth == 0 => return start.map(|s| (s, i)),
            _ => {}
        }
    }
    None
}

/// Joins two prefixes; an empty prefix adds nothing.
fn join_prefix(a: &str, b: &str) -> String {
    if a.is_empty() {
        return b.to_string();
    }
    if b.is_empty() {
        return a.to_string();
    }
    join_path(a, b)
}

/// Lists the prefixes under which a registration at token `at` in function
/// `f` serves: the mounts of regions around it, then the mounts of `f`.
fn prefixes_at(mounts: &[Mount], f: &str, at: usize, seen: &mut Vec<String>) -> Vec<String> {
    let mut inner: Vec<(usize, &str)> = mounts
        .iter()
        .filter(|m| m.parent == f)
        .filter_map(|m| match m.child {
            Child::Region(a, b) if a <= at && at < b => Some((a, m.prefix.as_str())),
            _ => None,
        })
        .collect();
    inner.sort();
    let region = inner
        .iter()
        .fold(String::new(), |acc, (_, p)| join_prefix(&acc, p));
    let parents: Vec<&Mount> = mounts
        .iter()
        .filter(|m| matches!(&m.child, Child::Function(c) if c == f))
        .collect();
    if parents.is_empty() || seen.iter().any(|s| s == f) {
        return vec![region];
    }
    seen.push(f.to_string());
    let mut out = Vec::new();
    for m in parents {
        for p in prefixes_at(mounts, &m.parent, m.at, seen) {
            out.push(join_prefix(&join_prefix(&p, &m.prefix), &region));
        }
    }
    seen.pop();
    out
}

/// Writes a variant's name in kebab case, as clap's derive does: `ImportBooks`
/// gives `import-books`, `HTTPServer` gives `http-server`.
pub fn kebab(name: &str) -> String {
    let c: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for i in 0..c.len() {
        let up = c[i].is_uppercase();
        let prev_lower = i > 0 && (c[i - 1].is_lowercase() || c[i - 1].is_ascii_digit());
        let next_lower = c.get(i + 1).is_some_and(|x| x.is_lowercase());
        let prev_upper = i > 0 && c[i - 1].is_uppercase();
        if i > 0 && up && (prev_lower || (prev_upper && next_lower)) {
            out.push('-');
        }
        if c[i] == '_' {
            out.push('-');
        } else {
            out.extend(c[i].to_lowercase());
        }
    }
    out
}

/// What the attributes before an item say to clap: the name it sets, and
/// whether it marks a subcommand field.
#[derive(Default)]
struct ClapAttrs {
    /// The value of `name = "…"` in `#[command(…)]`.
    name: Option<String>,
    /// Whether `#[command(subcommand)]` marks the item.
    subcommand: bool,
}

/// Reads the `#[command(…)]` attributes just before the token at `at`.
fn clap_attrs(tokens: &[Token], at: usize) -> ClapAttrs {
    let mut out = ClapAttrs::default();
    let mut end = at;
    while end > 0 && tokens[end - 1].is("]") {
        // Walk back to the `#[` that opens this attribute.
        let mut depth = 0;
        let mut open = None;
        for j in (0..end).rev() {
            let t = &tokens[j];
            if t.is("]") || t.is(")") || t.is("}") {
                depth += 1;
            } else if t.is("[") || t.is("(") || t.is("{") {
                depth -= 1;
                if depth == 0 {
                    open = Some(j);
                    break;
                }
            }
        }
        let Some(open) = open else { break };
        if open == 0 || !tokens[open - 1].is("#") {
            break;
        }
        let body = &tokens[open + 1..end - 1];
        if body.first().is_some_and(|t| t.is_ident("command")) {
            for (k, t) in body.iter().enumerate() {
                if t.is_ident("subcommand") {
                    out.subcommand = true;
                }
                if t.is_ident("name") && body.get(k + 1).is_some_and(|x| x.is("=")) {
                    if let Some(v) = body.get(k + 2).filter(|v| v.kind == Kind::Str) {
                        out.name.get_or_insert(v.text.clone());
                    }
                }
            }
        }
        end = open - 1;
    }
    out
}

/// Runs the clap plugin: one `cli` entry for each command that runs code.
fn clap(ex: &Extractor, src: &mut Sources) -> Vec<EntryPoint> {
    let implements = |id: &str, t: &str| {
        ex.outside_traits.get(id).is_some_and(|set| {
            set.iter().any(|x| {
                x.rsplit_once("::")
                    .is_some_and(|(k, n)| n == t && CLAP_CRATES.contains(&k))
            })
        })
    };
    let mut out = Vec::new();
    let roots: Vec<String> = ex
        .syms
        .keys()
        .filter(|id| implements(id, "Parser"))
        .cloned()
        .collect();
    for root in roots {
        let s = &ex.syms[&root];
        let attrs = attrs_of(ex, src, &root);
        let crate_info = ex.crates.iter().find(|c| {
            root.starts_with(&format!("rs:{}::", c.key))
                || root.starts_with(&format!("rs:{}#", c.key))
        });
        let program = attrs
            .name
            .or_else(|| {
                crate_info.map(|c| {
                    if c.is_bin {
                        c.target.clone()
                    } else {
                        c.package.clone()
                    }
                })
            })
            .unwrap_or_else(|| s.name.to_lowercase());
        let subs = subcommand_enums(ex, src, &root, &implements);
        if subs.is_empty() {
            if let Some(h) = parse_caller(ex, src, &root) {
                out.push(cli_entry(ex, &program, &root, h, false));
            }
            continue;
        }
        let mut seen = vec![root.clone()];
        for e in subs {
            commands(ex, src, &program, &e, &implements, &mut seen, &mut out);
        }
    }
    out
}

/// Makes one `cli` entry.
fn cli_entry(
    ex: &Extractor,
    name: &str,
    declared: &str,
    handler: String,
    inline: bool,
) -> EntryPoint {
    let loc = ex
        .syms
        .get(declared)
        .map(|s| s.location.clone())
        .unwrap_or(Location {
            path: String::new(),
            line: 1,
            end_line: 0,
        });
    EntryPoint {
        id: format!("cli:{name}"),
        kind: "cli".into(),
        name: name.to_string(),
        method: String::new(),
        path: String::new(),
        handler,
        inline,
        external_handler: String::new(),
        registrar: String::new(),
        registered_at: Location {
            path: loc.path,
            line: loc.line,
            end_line: 0,
        },
        plugin: "clap".into(),
    }
}

/// Reads the clap attributes before a symbol's declaration.
fn attrs_of(ex: &Extractor, src: &mut Sources, id: &str) -> ClapAttrs {
    let Some(sp) = ex.spans.get(id) else {
        return ClapAttrs::default();
    };
    let tokens = src.tokens(ex, &sp.file);
    clap_attrs(tokens, token_index(tokens, sp.start.0, sp.start.1))
}

/// Lists the `Subcommand` enums that a struct's `#[command(subcommand)]`
/// fields name.
fn subcommand_enums(
    ex: &Extractor,
    src: &mut Sources,
    owner: &str,
    implements: &dyn Fn(&str, &str) -> bool,
) -> Vec<String> {
    let mut out = Vec::new();
    let prefix = format!("{owner}.");
    let fields: Vec<String> = ex
        .syms
        .range(prefix.clone()..)
        .take_while(|(k, _)| k.starts_with(&prefix))
        .filter(|(_, s)| s.kind == "field" && s.parent == owner)
        .map(|(k, _)| k.clone())
        .collect();
    for f in fields {
        if !attrs_of(ex, src, &f).subcommand {
            continue;
        }
        for r in ex.syms[&f].type_text.iter().flat_map(|t| &t.refs) {
            if implements(r, "Subcommand") && !out.contains(r) {
                out.push(r.clone());
            }
        }
    }
    out
}

/// Adds the commands of one `Subcommand` enum under a parent command name.
/// A variant whose payload holds subcommands adds those instead of itself.
fn commands(
    ex: &Extractor,
    src: &mut Sources,
    parent: &str,
    enum_id: &str,
    implements: &dyn Fn(&str, &str) -> bool,
    seen: &mut Vec<String>,
    out: &mut Vec<EntryPoint>,
) {
    if seen.iter().any(|s| s == enum_id) {
        return;
    }
    seen.push(enum_id.to_string());
    let mut variants: Vec<&crate::schema::Symbol> = ex
        .syms
        .values()
        .filter(|s| s.parent == enum_id && s.language_kind == "variant")
        .collect();
    variants.sort_by_key(|s| s.location.line);
    for v in variants {
        let attrs = attrs_of(ex, src, &v.id);
        let name = format!("{parent} {}", attrs.name.unwrap_or_else(|| kebab(&v.name)));
        let refs: Vec<String> = v.type_text.iter().flat_map(|t| t.refs.clone()).collect();
        let mut nested = Vec::new();
        for r in &refs {
            if implements(r, "Subcommand") {
                nested.push(r.clone());
            } else if implements(r, "Args") || implements(r, "Parser") {
                nested.extend(subcommand_enums(ex, src, r, implements));
            }
        }
        if !nested.is_empty() {
            for e in nested {
                commands(ex, src, &name, &e, implements, seen, out);
            }
            continue;
        }
        if let Some((h, inline)) = dispatch(ex, src, &v.id) {
            out.push(cli_entry(ex, &name, &v.id, h, inline));
        }
    }
}

/// Finds the function that calls `parse` (or `try_parse`, `parse_from`,
/// `try_parse_from`) on a `Parser` struct.
fn parse_caller(ex: &Extractor, src: &mut Sources, root: &str) -> Option<String> {
    let sym = src.scip_of.get(root)?.clone();
    let uses = src.uses.get(&sym)?.clone();
    for (file, line, col) in uses {
        let tokens = src.tokens(ex, &file);
        let i = token_index(tokens, line, col);
        let next = tokens.get(i + 2).map(|t| t.text.as_str());
        if tokens.get(i + 1).is_some_and(|t| t.is("::"))
            && matches!(
                next,
                Some("parse" | "try_parse" | "parse_from" | "try_parse_from")
            )
        {
            if let Some(f) = ex.callable_at(&file, line, col) {
                return Some(f.to_string());
            }
        }
    }
    None
}

/// Finds who runs a subcommand: the first function of the map that the
/// `match` arm (or `if let` block) of the variant calls. Without one, the
/// function that matches serves the command inline.
fn dispatch(ex: &Extractor, src: &mut Sources, variant: &str) -> Option<(String, bool)> {
    let sym = src.scip_of.get(variant)?.clone();
    let uses = src.uses.get(&sym)?.clone();
    let owner = ex
        .syms
        .get(variant)
        .map(|s| s.parent.clone())
        .unwrap_or_default();
    let owner_span = ex.spans.get(&owner).cloned();
    for (file, line, col) in uses {
        if owner_span
            .as_ref()
            .is_some_and(|sp| sp.file == file && sp.contains(line, col))
        {
            continue;
        }
        let Some(f) = ex.callable_at(&file, line, col).map(str::to_string) else {
            continue;
        };
        let tokens = src.tokens(ex, &file).to_vec();
        let at = token_index(&tokens, line, col);
        let Some((a, b)) = arm_body(&tokens, at) else {
            continue;
        };
        for i in a..b {
            let t = &tokens[i];
            if t.kind != Kind::Ident || !tokens.get(i + 1).is_some_and(|n| n.is("(")) {
                continue;
            }
            if let Some(id) = src.target_at(ex, &file, t) {
                if ex
                    .syms
                    .get(id)
                    .is_some_and(|s| matches!(s.kind.as_str(), "function" | "method"))
                {
                    return Some((id.clone(), false));
                }
            }
        }
        return Some((f, true));
    }
    None
}

/// Finds the code that runs for a pattern that starts at token `at`: the
/// body of its `match` arm, or the block of its `if let`. Returns a range of
/// token indexes.
fn arm_body(tokens: &[Token], at: usize) -> Option<(usize, usize)> {
    let mut depth = 0;
    let mut i = at + 1;
    while i < tokens.len() {
        let t = &tokens[i];
        if t.is("(") || t.is("[") || t.is("{") {
            depth += 1;
        } else if t.is(")") || t.is("]") || t.is("}") {
            if depth == 0 {
                return None;
            }
            depth -= 1;
        } else if depth == 0 && t.is("=>") {
            let start = i + 1;
            if tokens.get(start).is_some_and(|x| x.is("{")) {
                return Some((start + 1, lexer::closing(tokens, start)));
            }
            let mut d = 0;
            for (j, x) in tokens.iter().enumerate().skip(start) {
                if x.is("(") || x.is("[") || x.is("{") {
                    d += 1;
                } else if x.is(")") || x.is("]") || x.is("}") {
                    if d == 0 {
                        return Some((start, j));
                    }
                    d -= 1;
                } else if d == 0 && x.is(",") {
                    return Some((start, j));
                }
            }
            return Some((start, tokens.len()));
        } else if depth == 0 && t.is("=") {
            // `if let Pattern = value { … }`: the block after the value.
            let mut d = 0;
            for (j, x) in tokens.iter().enumerate().skip(i + 1) {
                if d == 0 && x.is("{") {
                    return Some((j + 1, lexer::closing(tokens, j)));
                }
                if x.is("(") || x.is("[") {
                    d += 1;
                } else if x.is(")") || x.is("]") {
                    d -= 1;
                } else if d == 0 && x.is(";") {
                    return None;
                }
            }
            return None;
        } else if depth == 0 && (t.is(";") || t.is(",")) {
            return None;
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kebab_case_follows_clap() {
        assert_eq!(kebab("ImportBooks"), "import-books");
        assert_eq!(kebab("HTTPServer"), "http-server");
        assert_eq!(kebab("Serve"), "serve");
    }

    #[test]
    fn paths_join_with_one_slash() {
        assert_eq!(join_path("/api", "/search"), "/api/search");
        assert_eq!(join_path("/api/", "x"), "/api/x");
        assert_eq!(join_path("", "/x"), "/x");
        assert_eq!(join_path("/api", "/"), "/api");
    }

    #[test]
    fn arm_bodies_end_at_the_comma_or_the_block() {
        let t = lexer::lex("match c { A(x) => run(x), B { y } => { go(y); } }");
        let a = t.iter().position(|x| x.text == "A").unwrap();
        let (s, e) = arm_body(&t, a).unwrap();
        assert_eq!(t[s].text, "run");
        assert!(t[e].is(","));
        let b = t.iter().position(|x| x.text == "B").unwrap();
        let (s, _) = arm_body(&t, b).unwrap();
        assert_eq!(t[s].text, "go");
    }

    #[test]
    fn clap_attributes_give_names_and_subcommands() {
        let t = lexer::lex("#[derive(Subcommand)]\n#[command(name = \"import-books\", about = \"x\")]\nImport(Args)");
        let at = t.iter().position(|x| x.text == "Import").unwrap();
        assert_eq!(clap_attrs(&t, at).name.as_deref(), Some("import-books"));
        let t = lexer::lex("#[command(subcommand)] command: Command");
        let at = t
            .iter()
            .position(|x| x.text == "command" && x.line == 0 && x.col > 20)
            .unwrap();
        assert!(clap_attrs(&t, at).subcommand);
    }
}
