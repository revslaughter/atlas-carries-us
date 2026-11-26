//! The read tools of the MCP server. Each one takes flat parameters and
//! returns short, paged plain text, because small local models handle that
//! best. None of them changes anything.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::atlas::Snapshot;

/// How many rows one page of a list holds.
pub const PAGE: usize = 25;
/// How many lines `read_source` returns at most.
pub const MAX_LINES: usize = 200;
/// How many regions and tour steps `overview` lists at most.
pub const LIST: usize = 40;
/// How many rows a trace holds at most, across all its pages.
pub const MAX_TRACE: usize = 2000;
/// How large a file `read_source` reads at most, in bytes.
const MAX_FILE: u64 = 2 * 1024 * 1024;

/// Returns the array under a key, or an empty slice.
fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Returns the string under a key, or an empty string.
fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Turns text in the inline subset into plain text: it drops escapes and
/// keeps the label of each link.
pub fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            }
            ']' if chars.peek() == Some(&'(') => {
                // Skip the link target.
                for n in chars.by_ref() {
                    if n == ')' {
                        break;
                    }
                }
            }
            '[' => {}
            _ => out.push(c),
        }
    }
    out
}

/// Cuts text to at most `n` characters, with an ellipsis when it cuts.
pub fn cut(text: &str, n: usize) -> String {
    let t = text.trim();
    if t.chars().count() <= n {
        return t.to_string();
    }
    let mut out: String = t.chars().take(n.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// The first sentence of an item's doc, as plain text.
fn summary(v: &Value) -> String {
    plain(s(&v["doc"], "summary"))
}

/// Cuts a list of rows to one page and adds a header and a footer that say
/// how to get the next page.
pub fn paged(title: &str, rows: &[String], page: usize, per: usize) -> String {
    if rows.is_empty() {
        return format!("{title}: none.");
    }
    let pages = rows.len().div_ceil(per);
    let page = page.clamp(1, pages);
    let start = (page - 1) * per;
    let mut out = format!("{title} ({} in all", rows.len());
    if pages > 1 {
        out.push_str(&format!(", page {page} of {pages}"));
    }
    out.push_str("):\n");
    for r in &rows[start..(start + per).min(rows.len())] {
        out.push_str(r);
        out.push('\n');
    }
    if page < pages {
        out.push_str(&format!(
            "More rows follow; call again with page {}.\n",
            page + 1
        ));
    }
    out
}

/// The read tools' lookup tables over one document.
pub struct Lookup {
    /// The position of each symbol in `symbols`.
    sym: HashMap<String, usize>,
    /// The position of each container in `containers`.
    cont: HashMap<String, usize>,
    /// The position of each entry point in `entryPoints`.
    entry: HashMap<String, usize>,
    /// Entry points by name.
    entry_name: HashMap<String, usize>,
    /// The edges that leave each ID, as positions in `edges`.
    out: HashMap<String, Vec<usize>>,
    /// The edges that reach each ID, as positions in `edges`.
    inc: HashMap<String, Vec<usize>>,
    /// The member symbols of each symbol, sorted by ID.
    members: HashMap<String, Vec<String>>,
    /// The top-level symbols of each unit, sorted by ID.
    items: HashMap<String, Vec<String>>,
    /// The region of each unit, as (ID, name).
    region: HashMap<String, (String, String)>,
    /// The paths of the files that the map names, from the repository root.
    files: HashSet<String>,
}

impl Lookup {
    /// Builds the tables.
    pub fn new(doc: &Value) -> Lookup {
        let mut l = Lookup {
            sym: HashMap::new(),
            cont: HashMap::new(),
            entry: HashMap::new(),
            entry_name: HashMap::new(),
            out: HashMap::new(),
            inc: HashMap::new(),
            members: HashMap::new(),
            items: HashMap::new(),
            region: HashMap::new(),
            files: HashSet::new(),
        };
        let mut file_unit: HashMap<&str, &str> = HashMap::new();
        for (i, c) in arr(doc, "containers").iter().enumerate() {
            l.cont.insert(s(c, "id").to_string(), i);
        }
        // The unit of each container: itself, or the nearest unit above.
        let containers = arr(doc, "containers");
        for c in containers {
            let mut cur = c;
            let mut guard = 0;
            while cur.get("unit").and_then(Value::as_bool) != Some(true) && guard < 64 {
                match l.cont.get(s(cur, "parent")) {
                    Some(&p) => cur = &containers[p],
                    None => break,
                }
                guard += 1;
            }
            if cur.get("unit").and_then(Value::as_bool) == Some(true) {
                file_unit.insert(s(c, "id"), s(cur, "id"));
            }
        }
        for (i, sy) in arr(doc, "symbols").iter().enumerate() {
            l.sym.insert(s(sy, "id").to_string(), i);
        }
        for sy in arr(doc, "symbols") {
            let parent = s(sy, "parent");
            if l.sym.contains_key(parent) {
                l.members
                    .entry(parent.to_string())
                    .or_default()
                    .push(s(sy, "id").to_string());
            } else if let Some(u) = file_unit.get(parent) {
                l.items
                    .entry(u.to_string())
                    .or_default()
                    .push(s(sy, "id").to_string());
            }
        }
        for v in l.members.values_mut().chain(l.items.values_mut()) {
            v.sort();
        }
        for (i, e) in arr(doc, "edges").iter().enumerate() {
            l.out.entry(s(e, "from").to_string()).or_default().push(i);
            l.inc.entry(s(e, "to").to_string()).or_default().push(i);
        }
        for c in arr(doc, "containers") {
            if s(c, "kind") == "file" && !s(c, "path").is_empty() {
                l.files.insert(s(c, "path").to_string());
            }
        }
        for sy in arr(doc, "symbols") {
            l.files.insert(s(&sy["location"], "path").to_string());
        }
        for t in doc["tests"]
            .as_object()
            .into_iter()
            .flat_map(|m| m.values())
        {
            for x in t.as_array().into_iter().flatten() {
                l.files.insert(s(&x["location"], "path").to_string());
            }
        }
        for (i, e) in arr(doc, "entryPoints").iter().enumerate() {
            l.files.insert(s(&e["registeredAt"], "path").to_string());
            l.entry.insert(s(e, "id").to_string(), i);
            l.entry_name.entry(s(e, "name").to_string()).or_insert(i);
        }
        for r in arr(doc, "regions") {
            for c in arr(r, "containers") {
                if let Some(c) = c.as_str() {
                    l.region.insert(
                        c.to_string(),
                        (s(r, "id").to_string(), s(r, "name").to_string()),
                    );
                }
            }
        }
        l
    }
}

/// What a key names: a symbol, a unit or an entry point.
enum Target {
    /// A symbol, by ID.
    Symbol(String),
    /// A container, by ID.
    Container(String),
    /// An entry point, by its position.
    Entry(usize),
}

/// Resolves a key that a model gives: an exact ID, a qualified name such as
/// `Mux.Get`, a unit's path, or an entry point's name.
fn resolve(snap: &Snapshot, key: &str) -> Result<Target, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("acu: give an ID or a name; the search tool finds both".into());
    }
    let l = &snap.lookup;
    if l.sym.contains_key(key) {
        return Ok(Target::Symbol(key.to_string()));
    }
    if l.cont.contains_key(key) {
        return Ok(Target::Container(key.to_string()));
    }
    if let Some(&i) = l.entry.get(key).or_else(|| l.entry_name.get(key)) {
        return Ok(Target::Entry(i));
    }
    if let Some(u) = snap.index.units.iter().find(|u| u.1 == key || u.3 == key) {
        return Ok(Target::Container(u.0.clone()));
    }
    match snap.index.symbol(key) {
        Ok(id) => Ok(Target::Symbol(id)),
        Err(e) if e.contains("matches") => Err(format!("acu: {e}")),
        Err(_) => {
            // A bare name, such as a member's own name, may still name one
            // symbol.
            let mut ids: Vec<&str> = arr(&snap.doc, "symbols")
                .iter()
                .filter(|sy| s(sy, "name") == key)
                .map(|sy| s(sy, "id"))
                .collect();
            ids.sort();
            match ids.as_slice() {
                [one] => Ok(Target::Symbol(one.to_string())),
                [] => Err(format!(
                    "acu: nothing has the ID or name {key}; call search with a part of the name"
                )),
                many => Err(format!(
                    "acu: the name {key} matches {} symbols: {}; use an ID",
                    many.len(),
                    many.iter().take(10).copied().collect::<Vec<_>>().join(", ")
                )),
            }
        }
    }
}

/// Resolves a key to a symbol ID; a unit or entry point is an error, except
/// that an entry point stands for its handler when `handler` is true.
fn resolve_symbol(snap: &Snapshot, key: &str, handler: bool) -> Result<String, String> {
    match resolve(snap, key)? {
        Target::Symbol(id) => Ok(id),
        Target::Entry(i) if handler => {
            Ok(s(&arr(&snap.doc, "entryPoints")[i], "handler").to_string())
        }
        Target::Entry(_) => Err(format!(
            "acu: {key} names an entry point; give the symbol of its handler"
        )),
        Target::Container(_) => Err(format!(
            "acu: {key} names a unit or file, not a symbol; call symbol on it to list its items"
        )),
    }
}

/// The symbol at an ID.
fn sym<'a>(snap: &'a Snapshot, id: &str) -> Option<&'a Value> {
    snap.lookup
        .sym
        .get(id)
        .map(|&i| &arr(&snap.doc, "symbols")[i])
}

/// The container at an ID.
fn cont<'a>(snap: &'a Snapshot, id: &str) -> Option<&'a Value> {
    snap.lookup
        .cont
        .get(id)
        .map(|&i| &arr(&snap.doc, "containers")[i])
}

/// A symbol's name with its owner's name in front, as in `Mux.Get`.
fn qual(snap: &Snapshot, id: &str) -> String {
    match sym(snap, id) {
        Some(sy) => match sym(snap, s(sy, "parent")) {
            Some(p) => format!("{}.{}", s(p, "name"), s(sy, "name")),
            None => s(sy, "name").to_string(),
        },
        None => match cont(snap, id) {
            Some(c) => s(c, "name").to_string(),
            None => id.to_string(),
        },
    }
}

/// Where a symbol sits, as `path:line` or `path:line-endLine`.
fn place(v: &Value) -> String {
    let l = &v["location"];
    let line = l.get("line").and_then(Value::as_u64).unwrap_or(0);
    match l.get("endLine").and_then(Value::as_u64) {
        Some(e) if e > line => format!("{}:{line}-{e}", s(l, "path")),
        _ => format!("{}:{line}", s(l, "path")),
    }
}

/// `overview`: the project, its counts, its regions, its tour and whether
/// the write tools work.
pub fn overview(snap: &Snapshot, write_note: Option<&str>) -> String {
    let d = &snap.doc;
    let p = &d["project"];
    let mut out = format!("Project: {}\n", or(s(p, "name"), "(no name)"));
    if !s(p, "repo").is_empty() {
        out.push_str(&format!("Repository: {}\n", s(p, "repo")));
    }
    let langs: Vec<&str> = arr(&d["meta"], "languages")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    out.push_str(&format!(
        "Languages: {}. {} units, {} symbols, {} edges, {} entry points.\n",
        or(&langs.join(", "), "none"),
        snap.index.units.len(),
        arr(d, "symbols").len(),
        arr(d, "edges").len(),
        arr(d, "entryPoints").len()
    ));
    if !s(p, "description").is_empty() {
        out.push_str(&format!(
            "Description{}: {}\n",
            drafted(&p["descriptionDrafted"]),
            cut(&plain(s(p, "description")).replace('\n', " "), 700)
        ));
    }
    out.push_str("Regions (id: name, units):\n");
    let regions = arr(d, "regions");
    for r in regions.iter().take(LIST) {
        out.push_str(&format!(
            "  {}: {}, units: {}{}",
            s(r, "id"),
            s(r, "name"),
            arr(r, "containers").len(),
            drafted(&r["drafted"])
        ));
        if !s(r, "description").is_empty() {
            out.push_str(&format!(" — {}", cut(&plain(s(r, "description")), 160)));
        }
        out.push('\n');
    }
    if regions.len() > LIST {
        out.push_str(&format!(
            "  … and {} more regions; call search with kind unit to find units.\n",
            regions.len() - LIST
        ));
    }
    let tour = arr(d, "tour");
    if tour.is_empty() {
        out.push_str("Tour: no steps yet.\n");
    } else {
        out.push_str("Tour:\n");
        for (i, st) in tour.iter().enumerate().take(LIST) {
            out.push_str(&format!(
                "  {}. {} ({}){}\n",
                i + 1,
                s(st, "title"),
                s(&st["open"], "view"),
                drafted(&st["drafted"])
            ));
        }
        if tour.len() > LIST {
            out.push_str(&format!("  … and {} more steps.\n", tour.len() - LIST));
        }
    }
    match write_note {
        None => {
            out.push_str("Write tools: on. Each draft lands in atlas.yaml with a drafted mark.\n")
        }
        Some(why) => out.push_str(&format!("Write tools: off; {why}.\n")),
    }
    out.push_str("Next: call search to find a symbol, or trace with an entry point.");
    out
}

/// Returns `a`, or `b` when `a` is empty.
fn or<'a>(a: &'a str, b: &'a str) -> &'a str {
    if a.is_empty() {
        b
    } else {
        a
    }
}

/// Says " [drafted by X]" for text with a provenance mark, or "".
fn drafted(mark: &Value) -> String {
    if mark.is_object() {
        format!(" [drafted by {}, not checked]", s(mark, "by"))
    } else {
        String::new()
    }
}

/// Reports whether `path` ends with `q` and a separator (`.`, `:`, `/` or
/// `#`) stands just before it, as `mux.get` ends with `get`.
fn ends_at_boundary(path: &str, q: &str) -> bool {
    path.len() > q.len()
        && path.ends_with(q)
        && path[..path.len() - q.len()].ends_with(['.', ':', '/', '#'])
}

/// `search`: finds symbols, units and entry points whose name, ID or first
/// doc sentence holds the query. `kind` narrows the result to `symbol`,
/// `unit`, `entry_point`, or one neutral symbol kind such as `method`.
pub fn search(snap: &Snapshot, query: &str, kind: &str, page: usize) -> String {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return "acu: give a query, such as part of a name".into();
    }
    let kind = kind.trim().to_lowercase().replace([' ', '-'], "_");
    let want = |k: &str, sk: &str| {
        kind.is_empty() || kind == k || (k == "symbol" && kind == sk) || kind == "all"
    };
    let mut hits: Vec<(u8, String, String)> = Vec::new();
    for sy in arr(&snap.doc, "symbols") {
        if !want("symbol", s(sy, "kind")) {
            continue;
        }
        let id = s(sy, "id");
        let name = s(sy, "name").to_lowercase();
        let path = id.split_once('#').map_or(id, |p| p.1).to_lowercase();
        let score = if name == q || path == q || id.to_lowercase() == q {
            0
        } else if ends_at_boundary(&path, &q) {
            1
        } else if name.starts_with(&q) {
            2
        } else if name.contains(&q) {
            3
        } else if path.contains(&q) {
            4
        } else if summary(sy).to_lowercase().contains(&q) {
            5
        } else {
            continue;
        };
        hits.push((
            score,
            id.to_string(),
            format!(
                "{} {} — {} — {}",
                s(sy, "kind"),
                id,
                place(sy),
                or(&cut(&summary(sy), 110), "no doc")
            ),
        ));
    }
    if want("unit", "") {
        for (id, path, name, _) in &snap.index.units {
            let n = name.to_lowercase();
            let p = path.to_lowercase();
            let score = if n == q || p == q {
                0
            } else if n.contains(&q) || p.contains(&q) {
                3
            } else {
                continue;
            };
            let c = cont(snap, id).unwrap_or(&Value::Null);
            hits.push((
                score,
                id.clone(),
                format!(
                    "unit {id} — {path} — {}",
                    or(&cut(&summary(c), 110), "no doc")
                ),
            ));
        }
    }
    if want("entry_point", "") {
        for e in arr(&snap.doc, "entryPoints") {
            let n = s(e, "name").to_lowercase();
            let score = if n == q {
                0
            } else if n.contains(&q) || s(e, "id").to_lowercase().contains(&q) {
                3
            } else if qual(snap, s(e, "handler")).to_lowercase().contains(&q) {
                4
            } else {
                continue;
            };
            hits.push((
                score,
                s(e, "id").to_string(),
                format!(
                    "entry_point {} — handler {}",
                    s(e, "id"),
                    qual(snap, s(e, "handler"))
                ),
            ));
        }
    }
    hits.sort();
    let rows: Vec<String> = hits.into_iter().map(|h| h.2).collect();
    let mut out = paged(
        &format!("Matches for \"{}\"", query.trim()),
        &rows,
        page,
        PAGE,
    );
    if !rows.is_empty() {
        out.push_str("Next: call symbol with an ID from the list.");
    }
    out
}

/// `symbol`: one symbol's kind, place, signature, docs, members, uses and
/// tests; or one unit's docs and items; or one entry point. `page` pages a
/// symbol's members and a unit's items.
pub fn symbol(snap: &Snapshot, key: &str, page: usize) -> String {
    match resolve(snap, key) {
        Err(e) => e,
        Ok(Target::Symbol(id)) => symbol_text(snap, &id, page),
        Ok(Target::Container(id)) => container_text(snap, &id, page),
        Ok(Target::Entry(i)) => entry_text(&arr(&snap.doc, "entryPoints")[i]),
    }
}

/// How many member names one page of `symbol` holds.
const MEMBERS: usize = 40;

/// Writes the detail of one symbol, with one page of its members.
fn symbol_text(snap: &Snapshot, id: &str, page: usize) -> String {
    let Some(sy) = sym(snap, id) else {
        return format!("acu: no symbol has the ID {id}");
    };
    let l = &snap.lookup;
    let mut out = format!(
        "{} {} ({})\nID: {id}\n",
        s(sy, "kind"),
        qual(snap, id),
        s(sy, "languageKind")
    );
    if let Some(u) = snap.index.unit_of.get(id) {
        let unit = cont(snap, u).map_or(u.as_str(), |c| s(c, "name"));
        out.push_str(&format!("Unit: {unit} ({u})"));
        if let Some((rid, rname)) = l.region.get(u) {
            out.push_str(&format!(", region {rname} ({rid})"));
        }
        out.push('\n');
    }
    if let Some(p) = sym(snap, s(sy, "parent")) {
        out.push_str(&format!("Member of: {}\n", s(p, "id")));
    }
    out.push_str(&format!(
        "Source: {} (read_source with this ID shows it)\n",
        place(sy)
    ));
    if !s(&sy["signature"], "text").is_empty() {
        out.push_str(&format!("Signature: {}\n", s(&sy["signature"], "text")));
    } else if !s(&sy["type"], "text").is_empty() {
        out.push_str(&format!("Type: {}\n", s(&sy["type"], "text")));
    }
    if !s(sy, "value").is_empty() {
        out.push_str(&format!("Value: {}\n", s(sy, "value")));
    }
    out.push_str(&doc_text(&sy["doc"]));
    if let Some(ms) = l.members.get(id) {
        let pages = ms.len().div_ceil(MEMBERS);
        let page = page.clamp(1, pages);
        let names: Vec<&str> = ms
            .iter()
            .skip((page - 1) * MEMBERS)
            .take(MEMBERS)
            .filter_map(|m| sym(snap, m))
            .map(|m| s(m, "name"))
            .collect();
        out.push_str(&format!("Members ({}", ms.len()));
        if pages > 1 {
            out.push_str(&format!(", page {page} of {pages}"));
        }
        out.push_str(&format!("): {}\n", names.join(", ")));
        if page < pages {
            out.push_str(&format!(
                "More members follow; call again with page {}.\n",
                page + 1
            ));
        }
    }
    let n_out = l.out.get(id).map_or(0, Vec::len);
    let n_in = l.inc.get(id).map_or(0, Vec::len);
    out.push_str(&format!(
        "Edges out: {n_out}. Edges in: {n_in}. Call neighbours for the list.\n"
    ));
    let tests = arr(&snap.doc["tests"], id);
    if !tests.is_empty() {
        let names: Vec<&str> = tests.iter().map(|t| s(t, "name")).take(8).collect();
        out.push_str(&format!("Tests ({}): {}\n", tests.len(), names.join(", ")));
    }
    let entries: Vec<&str> = arr(&snap.doc, "entryPoints")
        .iter()
        .filter(|e| s(e, "handler") == id)
        .map(|e| s(e, "name"))
        .collect();
    if !entries.is_empty() {
        out.push_str(&format!("Serves: {}\n", entries.join(", ")));
    }
    out.trim_end().to_string()
}

/// Writes a doc's summary and its description, cut to a short length.
fn doc_text(doc: &Value) -> String {
    if !doc.is_object() {
        return "Doc: none.\n".into();
    }
    let mut out = format!("Doc: {}\n", plain(s(doc, "summary")));
    let mut rest = String::new();
    for b in arr(doc, "description") {
        let t = match s(b, "type") {
            "code" => format!("```\n{}\n```", s(b, "code")),
            "list" => arr(b, "items")
                .iter()
                .map(|i| format!("- {}", plain(i.as_str().unwrap_or(""))))
                .collect::<Vec<_>>()
                .join("\n"),
            _ => plain(s(b, "text")),
        };
        rest.push_str(&t);
        rest.push('\n');
    }
    let rest = rest.trim();
    if !rest.is_empty() && rest != plain(s(doc, "summary")).trim() {
        out.push_str(&cut(rest, 1500));
        out.push('\n');
    }
    if doc.get("deprecated").is_some() {
        out.push_str(&format!("Deprecated: {}\n", plain(s(doc, "deprecated"))));
    }
    out
}

/// Writes the detail of one container, with one page of the items of its
/// unit (or of the file).
fn container_text(snap: &Snapshot, id: &str, page: usize) -> String {
    let Some(c) = cont(snap, id) else {
        return format!("acu: no container has the ID {id}");
    };
    let mut out = format!(
        "{} {} ({})\nID: {id}\nPath: {}\n",
        s(c, "kind"),
        s(c, "name"),
        s(c, "languageKind"),
        or(s(c, "path"), ".")
    );
    if let Some((rid, rname)) = snap.lookup.region.get(id) {
        out.push_str(&format!("Region: {rname} ({rid})\n"));
    }
    out.push_str(&doc_text(&c["doc"]));
    let unit = if c.get("unit").and_then(Value::as_bool) == Some(true) {
        id.to_string()
    } else {
        snap.index
            .units
            .iter()
            .find(|u| u.0 == s(c, "parent"))
            .map(|u| u.0.clone())
            .unwrap_or_default()
    };
    let items = snap.lookup.items.get(&unit).cloned().unwrap_or_default();
    let rows: Vec<String> = items
        .iter()
        .filter_map(|i| sym(snap, i))
        .filter(|sy| {
            c.get("unit").and_then(Value::as_bool) == Some(true)
                || s(&sy["location"], "path") == s(c, "path")
        })
        .map(|sy| {
            format!(
                "{} {} — {}",
                s(sy, "kind"),
                s(sy, "id"),
                or(&cut(&summary(sy), 90), "no doc")
            )
        })
        .collect();
    out.push_str(&paged("Items", &rows, page, 60));
    out.trim_end().to_string()
}

/// Writes the detail of one entry point.
fn entry_text(e: &Value) -> String {
    let at = &e["registeredAt"];
    format!(
        "entry point {} ({})\nID: {}\nHandler: {}\nRegistered at: {}:{}\nPlugin: {}\nNext: call trace with this ID.",
        s(e, "name"),
        s(e, "kind"),
        s(e, "id"),
        s(e, "handler"),
        s(at, "path"),
        at.get("line").and_then(Value::as_u64).unwrap_or(0),
        s(e, "plugin")
    )
}

/// `neighbours`: the edges of a symbol and of its members. `direction` is
/// `out` (what it uses), `in` (what uses it) or `both`; `kind` keeps one
/// edge kind, such as `calls`.
pub fn neighbours(snap: &Snapshot, key: &str, direction: &str, kind: &str, page: usize) -> String {
    let id = match resolve(snap, key) {
        Ok(Target::Symbol(id)) | Ok(Target::Container(id)) => id,
        Ok(Target::Entry(i)) => s(&arr(&snap.doc, "entryPoints")[i], "handler").to_string(),
        Err(e) => return e,
    };
    let dir = direction.trim().to_lowercase();
    let (want_out, want_in) = match dir.as_str() {
        "" | "both" | "all" => (true, true),
        "out" | "uses" | "outgoing" => (true, false),
        "in" | "used_by" | "incoming" => (false, true),
        d => return format!("acu: the direction {d} means nothing here; use out, in or both"),
    };
    let kind = kind.trim();
    let l = &snap.lookup;
    let mut ids = vec![id.clone()];
    ids.extend(l.members.get(&id).cloned().unwrap_or_default());
    let mut rows = BTreeSet::new();
    let edges = arr(&snap.doc, "edges");
    for x in &ids {
        let via = if *x == id {
            String::new()
        } else {
            format!(" (from member {})", qual(snap, x))
        };
        if want_out {
            for &i in l.out.get(x).map_or(&[][..], Vec::as_slice) {
                let e = &edges[i];
                if (kind.is_empty() || s(e, "kind") == kind) && !ids.iter().any(|y| y == s(e, "to"))
                {
                    rows.insert(format!(
                        "uses: {} {} — {}{via}",
                        s(e, "kind"),
                        s(e, "to"),
                        edge_note(e)
                    ));
                }
            }
        }
        if want_in {
            for &i in l.inc.get(x).map_or(&[][..], Vec::as_slice) {
                let e = &edges[i];
                if (kind.is_empty() || s(e, "kind") == kind)
                    && !ids.iter().any(|y| y == s(e, "from"))
                {
                    let via = if via.is_empty() {
                        String::new()
                    } else {
                        format!(" (to member {})", qual(snap, x))
                    };
                    rows.insert(format!(
                        "used by: {} {} — {}{via}",
                        s(e, "kind"),
                        s(e, "from"),
                        edge_note(e)
                    ));
                }
            }
        }
    }
    let rows: Vec<String> = rows.into_iter().collect();
    paged(&format!("Edges of {}", qual(snap, &id)), &rows, page, PAGE)
}

/// Says how far to trust an edge, and its line.
fn edge_note(e: &Value) -> String {
    let mut n = s(e, "confidence").to_string();
    if let Some(line) = e.get("line").and_then(Value::as_u64) {
        n.push_str(&format!(", line {line}"));
    }
    n
}

/// Writes a count with its noun, in the singular for one: "1 use", "2 uses".
fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// The owner of a member, or the ID itself for a symbol that no symbol owns.
fn owner(snap: &Snapshot, id: &str) -> String {
    match sym(snap, id) {
        Some(sy) if snap.lookup.sym.contains_key(s(sy, "parent")) => s(sy, "parent").to_string(),
        _ => id.to_string(),
    }
}

/// `path`: the shortest chain of uses from one symbol to another, at most
/// `max_depth` edges long. When no chain follows the direction of the
/// edges, it looks again in both directions and says so.
pub fn path(snap: &Snapshot, from: &str, to: &str, max_depth: usize) -> String {
    let a = match resolve_symbol(snap, from, true) {
        Ok(a) => a,
        Err(e) => return e,
    };
    let b = match resolve_symbol(snap, to, true) {
        Ok(b) => b,
        Err(e) => return e,
    };
    if a == b {
        return format!(
            "acu: from and to both name {}; give two different symbols",
            qual(snap, &a)
        );
    }
    if owner(snap, &a) == b || owner(snap, &b) == a {
        let (m, o) = if owner(snap, &a) == b {
            (&a, &b)
        } else {
            (&b, &a)
        };
        return format!(
            "{} belongs to {} as a member; no chain of uses joins a member to its owner.",
            qual(snap, m),
            qual(snap, o)
        );
    }
    let depth = max_depth.clamp(1, 12);
    for directed in [true, false] {
        if let Some(chain) = bfs(snap, &a, &b, depth, directed) {
            let mut out = if directed {
                format!(
                    "A chain of {} leads from {} to {}:\n",
                    count(chain.len(), "use", "uses"),
                    qual(snap, &a),
                    qual(snap, &b)
                )
            } else {
                format!(
                    "No chain of uses leads from {} to {}. A chain that ignores the direction of edges ({}):\n",
                    qual(snap, &a),
                    qual(snap, &b),
                    count(chain.len(), "step", "steps")
                )
            };
            out.push_str(&format!("  {a}\n"));
            for step in chain {
                let arrow = if step.forward {
                    format!("{} →", step.kind)
                } else {
                    format!("← {} (used by)", step.kind)
                };
                let via = match &step.via {
                    Some(m) => format!(" (from member {})", qual(snap, m)),
                    None => String::new(),
                };
                out.push_str(&format!("  {arrow} {}{via}\n", step.to));
            }
            return out.trim_end().to_string();
        }
    }
    format!(
        "acu: no chain of at most {depth} edges joins {} and {}; raise max_depth, or call neighbours on each",
        qual(snap, &a),
        qual(snap, &b)
    )
}

/// One step of a shortest path.
struct Step {
    /// The edge kind.
    kind: String,
    /// True when the step follows the edge's direction.
    forward: bool,
    /// The ID that the step reaches.
    to: String,
    /// The member of the start that the edge leaves from, when it leaves
    /// from a member and not from the start itself.
    via: Option<String>,
}

/// Finds a shortest path by breadth-first search. The start's members count
/// as part of the start, and the goal's members as part of the goal, so a
/// path may start or end at a type; a step from a member names it.
fn bfs(snap: &Snapshot, a: &str, b: &str, depth: usize, directed: bool) -> Option<Vec<Step>> {
    let l = &snap.lookup;
    let edges = arr(&snap.doc, "edges");
    let goal = |id: &str| id == b || owner(snap, id) == b;
    let start_ids: HashSet<String> = std::iter::once(a.to_string())
        .chain(l.members.get(a).cloned().unwrap_or_default())
        .collect();
    let mut prev: HashMap<String, (String, Step)> = HashMap::new();
    let mut seen: HashSet<String> = start_ids.clone();
    let mut queue = VecDeque::from([(a.to_string(), 0usize)]);
    while let Some((cur, d)) = queue.pop_front() {
        if !start_ids.contains(&cur) && goal(&cur) {
            let mut chain = Vec::new();
            let mut at = cur.clone();
            while let Some((p, step)) = prev.remove(&at) {
                chain.push(step);
                at = p;
            }
            chain.reverse();
            return Some(chain);
        }
        if d >= depth {
            continue;
        }
        let mut starts = vec![cur.clone()];
        if d == 0 {
            starts.extend(l.members.get(&cur).cloned().unwrap_or_default());
        }
        let mut next: Vec<(String, String, bool, String)> = Vec::new();
        for x in &starts {
            for &i in l.out.get(x).map_or(&[][..], Vec::as_slice) {
                let e = &edges[i];
                next.push((s(e, "to").into(), s(e, "kind").into(), true, x.clone()));
            }
            if !directed {
                for &i in l.inc.get(x).map_or(&[][..], Vec::as_slice) {
                    let e = &edges[i];
                    next.push((s(e, "from").into(), s(e, "kind").into(), false, x.clone()));
                }
            }
        }
        next.sort();
        for (n, kind, forward, from) in next {
            if seen.insert(n.clone()) {
                let via = (from != cur).then_some(from);
                let step = Step {
                    kind,
                    forward,
                    to: n.clone(),
                    via,
                };
                prev.insert(n.clone(), (cur.clone(), step));
                queue.push_back((n, d + 1));
            }
        }
    }
    None
}

/// `trace`: the calls that run from an entry point or a function, as an
/// indented tree, `depth` calls deep. A call to an interface method lists
/// the methods of the types that implement it. Each function opens once,
/// at the shallowest depth where it occurs.
pub fn trace(snap: &Snapshot, start: &str, depth: usize, page: usize) -> String {
    let (root, title) = match resolve(snap, start) {
        Ok(Target::Entry(i)) => {
            let e = &arr(&snap.doc, "entryPoints")[i];
            (
                s(e, "handler").to_string(),
                format!(
                    "Trace from {} (handler {})",
                    s(e, "name"),
                    qual(snap, s(e, "handler"))
                ),
            )
        }
        Ok(Target::Symbol(id)) => (id.clone(), format!("Trace from {}", qual(snap, &id))),
        Ok(Target::Container(_)) => {
            return format!("acu: {start} names a unit; give an entry point or a function")
        }
        Err(e) => return e,
    };
    let depth = depth.clamp(1, 8);
    let calls = Calls::new(snap);
    let shallowest = calls.shallowest(&root, depth);
    let mut walk = Walk {
        calls: &calls,
        shallowest: &shallowest,
        depth,
        opened: HashSet::new(),
        rows: Vec::new(),
        stopped: false,
    };
    walk.visit(&root, 0, "");
    let mut out = paged(&title, &walk.rows, page, 60);
    if walk.stopped {
        out.push_str(&format!(
            "The trace stops at {MAX_TRACE} rows; lower depth, or trace from a function further in.\n"
        ));
    }
    out.push_str("Indent shows depth. \"may run\" marks a method behind an interface. Calls through function values do not appear.");
    out
}

/// The calls that a trace follows.
struct Calls<'a> {
    /// The map.
    snap: &'a Snapshot,
    /// Each interface's ID, mapped to the types that implement it.
    implementers: HashMap<String, Vec<String>>,
}

impl<'a> Calls<'a> {
    /// Collects the `implements` edges of the map.
    fn new(snap: &'a Snapshot) -> Calls<'a> {
        let mut implementers: HashMap<String, Vec<String>> = HashMap::new();
        for e in arr(&snap.doc, "edges") {
            if s(e, "kind") == "implements" {
                implementers
                    .entry(s(e, "to").to_string())
                    .or_default()
                    .push(s(e, "from").to_string());
            }
        }
        for v in implementers.values_mut() {
            v.sort();
        }
        Calls { snap, implementers }
    }

    /// Lists what a function calls, in source order, each with its label:
    /// "" for a call, "may run: " for a method behind an interface.
    fn next(&self, id: &str) -> Vec<(String, &'static str)> {
        let snap = self.snap;
        let l = &snap.lookup;
        let edges = arr(&snap.doc, "edges");
        let mut calls: Vec<&Value> = l
            .out
            .get(id)
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .map(|&i| &edges[i])
            .filter(|e| s(e, "kind") == "calls")
            .collect();
        calls.sort_by_key(|e| {
            (
                e.get("line").and_then(Value::as_u64).unwrap_or(0),
                s(e, "to"),
            )
        });
        let mut out = Vec::new();
        for e in calls {
            let to = s(e, "to");
            out.push((to.to_string(), ""));
            // A call to an interface method may run each implementer's method.
            if let Some(sy) = sym(snap, to) {
                let owner = s(sy, "parent");
                for t in self.implementers.get(owner).map_or(&[][..], Vec::as_slice) {
                    for m in l.members.get(t).map_or(&[][..], Vec::as_slice) {
                        if sym(snap, m).map(|x| s(x, "name")) == Some(s(sy, "name")) {
                            out.push((m.clone(), "may run: "));
                        }
                    }
                }
            }
        }
        out
    }

    /// Finds the shallowest depth of each function that the trace reaches,
    /// by breadth-first search.
    fn shallowest(&self, root: &str, depth: usize) -> HashMap<String, usize> {
        let mut d = HashMap::from([(root.to_string(), 0usize)]);
        let mut queue = VecDeque::from([root.to_string()]);
        while let Some(cur) = queue.pop_front() {
            let at = d[&cur];
            if at >= depth {
                continue;
            }
            for (n, _) in self.next(&cur) {
                if !d.contains_key(&n) {
                    d.insert(n.clone(), at + 1);
                    queue.push_back(n);
                }
            }
        }
        d
    }
}

/// The state of one trace as it writes its rows.
struct Walk<'a> {
    /// The calls to follow.
    calls: &'a Calls<'a>,
    /// The shallowest depth of each function.
    shallowest: &'a HashMap<String, usize>,
    /// The deepest level to open.
    depth: usize,
    /// The functions already opened.
    opened: HashSet<String>,
    /// The rows written so far.
    rows: Vec<String>,
    /// True when the trace hit [`MAX_TRACE`].
    stopped: bool,
}

impl Walk<'_> {
    /// Writes one row and opens the calls below it, when this row sits at
    /// the function's shallowest depth and the function has not opened yet.
    fn visit(&mut self, id: &str, d: usize, label: &str) {
        if self.rows.len() >= MAX_TRACE {
            self.stopped = true;
            return;
        }
        let snap = self.calls.snap;
        let place = match sym(snap, id) {
            Some(sy) => format!(" — {}", place(sy)),
            None => String::new(),
        };
        let min = self.shallowest.get(id).copied().unwrap_or(d);
        let note = if self.opened.contains(id) {
            " (seen above)".to_string()
        } else if min < d {
            format!(" (opens at depth {min})")
        } else {
            String::new()
        };
        self.rows
            .push(format!("{}{label}{id}{place}{note}", "  ".repeat(d)));
        if !note.is_empty() || d >= self.depth {
            return;
        }
        self.opened.insert(id.to_string());
        for (n, l) in self.calls.next(id) {
            self.visit(&n, d + 1, l);
        }
    }
}

/// The files that `read_source` may open: files under the project's roots
/// that git tracks or that the map names, never a hidden one.
pub struct Sources {
    /// The canonical folders that the read tools may read from.
    pub roots: Vec<PathBuf>,
    /// The paths, relative to a root, that git tracks there.
    tracked: HashSet<String>,
    /// A note for a path that finds no file, such as how to pass `--root`.
    pub hint: Option<String>,
}

impl Sources {
    /// Lists the files that git tracks under each root. A root outside git
    /// lists none, so only the files that the map names stay open.
    pub fn new(roots: Vec<PathBuf>, hint: Option<String>) -> Sources {
        let mut tracked = HashSet::new();
        for root in &roots {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(root)
                .args(["ls-files", "-z"])
                .stderr(std::process::Stdio::null())
                .output();
            if let Some(o) = out.ok().filter(|o| o.status.success()) {
                for f in o.stdout.split(|&b| b == 0).filter(|f| !f.is_empty()) {
                    tracked.insert(String::from_utf8_lossy(f).to_string());
                }
            }
        }
        Sources {
            roots,
            tracked,
            hint,
        }
    }

    /// Adds the note for a path that finds no file.
    fn missing(&self, msg: String) -> String {
        match &self.hint {
            Some(h) => format!("{msg}. {h}"),
            None => msg,
        }
    }
}

/// Why `open_inside` gives no file.
pub enum Refusal {
    /// No file sits at the path under any root.
    Missing,
    /// A file may sit there, but the tools may not read it; the text says
    /// why.
    Closed(String),
}

/// `read_source`: numbered lines of a file under the project folder. The
/// key is a path relative to the repository root, or a symbol's ID or name,
/// which reads that symbol's lines. Paths that leave the project folder,
/// symbolic links among them, hidden files, and files that neither git
/// nor the map knows stay closed.
pub fn read_source(snap: &Snapshot, src: &Sources, key: &str, start: usize, end: usize) -> String {
    let key = key.trim();
    let from_symbol = |id: &str| {
        let sy = sym(snap, id).unwrap_or(&Value::Null);
        let l = &sy["location"];
        let line = l.get("line").and_then(Value::as_u64).unwrap_or(1) as usize;
        let endl = l
            .get("endLine")
            .and_then(Value::as_u64)
            .map_or(line, |e| e as usize);
        (s(l, "path").to_string(), line, endl)
    };
    // An exact symbol ID wins; then a file at that path; then a name.
    let (rel, mut start, mut end) = if snap.lookup.sym.contains_key(key) {
        from_symbol(key)
    } else {
        match open_inside(src, snap, key) {
            Ok(_) => (key.to_string(), start, end),
            Err(Refusal::Closed(e)) => return e,
            Err(Refusal::Missing) => match resolve_symbol(snap, key, true) {
                Ok(id) => from_symbol(&id),
                Err(e) if e.contains("nothing has") => {
                    return src.missing(format!(
                        "acu: no file sits at {key}, and no symbol has that name; give a path from the repository root, or call search"
                    ))
                }
                Err(e) => return e,
            },
        }
    };
    if start == 0 {
        start = 1;
    }
    if end == 0 || end < start {
        end = start.saturating_add(MAX_LINES - 1);
    }
    let file = match open_inside(src, snap, &rel) {
        Ok(f) => f,
        Err(Refusal::Closed(e)) => return e,
        Err(Refusal::Missing) => {
            return src.missing(format!(
            "acu: the map places {key} in {rel}, but no file sits there under the project folder"
        ))
        }
    };
    let text = match std::fs::read(&file) {
        Ok(b) => String::from_utf8_lossy(&b).to_string(),
        Err(e) => return format!("acu: read {rel}: {e}; check the file and call again"),
    };
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return format!("acu: {rel} holds no lines; pick another file");
    }
    if start > lines.len() {
        return format!(
            "acu: {rel} has {} lines; give a start_line from 1 to {}",
            lines.len(),
            lines.len()
        );
    }
    let cut_at = start
        .saturating_add(MAX_LINES - 1)
        .min(end)
        .min(lines.len());
    let mut out = format!("{rel}, lines {start}-{cut_at} of {}:\n", lines.len());
    for (i, line) in lines[start - 1..cut_at].iter().enumerate() {
        out.push_str(&format!("{:>5}  {line}\n", start + i));
    }
    if cut_at < end.min(lines.len()) {
        out.push_str(&format!(
            "More lines follow; call again with start_line {}.\n",
            cut_at + 1
        ));
    }
    out
}

/// Reports whether a relative path has a part that starts with a dot, such
/// as `.git/config` or `.env`.
fn hidden(p: &Path) -> bool {
    p.components().any(|c| match c {
        std::path::Component::Normal(n) => n.to_string_lossy().starts_with('.'),
        _ => false,
    })
}

/// Finds a file under one of the roots. It resolves symbolic links and
/// `..` first, and refuses any file that then lies outside every root, sits
/// in a hidden file or folder, or neither git nor the map knows.
pub fn open_inside(src: &Sources, snap: &Snapshot, rel: &str) -> Result<PathBuf, Refusal> {
    let closed = |m: String| Err(Refusal::Closed(m));
    let outside = || {
        Refusal::Closed(format!(
            "acu: {rel} lies outside the project folder; the read tools see only the project"
        ))
    };
    let hidden_msg = || {
        Refusal::Closed(format!(
            "acu: {rel} sits in a hidden file or folder; read_source opens no hidden files"
        ))
    };
    let p = Path::new(rel);
    if rel.is_empty() {
        return Err(Refusal::Missing);
    }
    if p.is_absolute() {
        return closed(format!(
            "acu: give a path relative to the project folder, not {rel}"
        ));
    }
    // A `..` that climbs above the root leaves the project, whether or not a
    // file sits there.
    let mut depth: i32 = 0;
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => depth -= 1,
            std::path::Component::Normal(_) => depth += 1,
            _ => {}
        }
        if depth < 0 {
            return Err(outside());
        }
    }
    if hidden(p) {
        return Err(hidden_msg());
    }
    let mut out = false;
    for root in &src.roots {
        let Ok(full) = root.join(p).canonicalize() else {
            continue;
        };
        let Ok(inner) = full.strip_prefix(root) else {
            out = true;
            continue;
        };
        if hidden(inner) {
            return Err(hidden_msg());
        }
        let inner = inner
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let meta = std::fs::metadata(&full)
            .map_err(|e| Refusal::Closed(format!("acu: read {rel}: {e}; check the path")))?;
        if !meta.is_file() {
            return closed(format!("acu: {rel} is no file; give a file's path"));
        }
        if !src.tracked.contains(&inner) && !snap.lookup.files.contains(&inner) {
            return closed(format!(
                "acu: {rel} holds no file that git tracks or the map names; read_source opens project files only"
            ));
        }
        if meta.len() > MAX_FILE {
            return closed(format!(
                "acu: {rel} holds more than 2 MB; acu reads smaller files only"
            ));
        }
        return Ok(full);
    }
    if out {
        Err(outside())
    } else {
        Err(Refusal::Missing)
    }
}

/// `check`: runs the check of `acu check` on the current map, and lists the
/// warnings of the last assembly.
pub fn check(snap: &Snapshot) -> String {
    let rep = crate::assemble::check(&snap.doc);
    let mut out = String::new();
    for e in &rep.errors {
        out.push_str(&format!("error: {e}\n"));
    }
    for w in snap.warnings.iter().chain(rep.warnings.iter()) {
        out.push_str(&format!("warning: {w}\n"));
    }
    if rep.errors.is_empty() {
        out.push_str("The map passes the check: no errors.");
    } else {
        out.push_str(&format!("{} errors found.", rep.errors.len()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::atlas::tests::fixture;
    use super::super::atlas::Atlas;
    use super::*;
    use serde_json::json;

    #[test]
    fn plain_drops_escapes_and_link_targets() {
        assert_eq!(plain("a \\`b\\` [Mux](sym:go:x#Mux) c"), "a `b` Mux c");
    }

    #[test]
    fn overview_lists_regions_and_tour() {
        let (_d, a) = fixture();
        let t = overview(&a.snapshot().unwrap(), None);
        assert!(t.contains("Project: Shop"), "{t}");
        assert!(t.contains("selling: Selling, units: 1"), "{t}");
        assert!(t.contains("2. Buying (symbol)"), "{t}");
        assert!(t.contains("Write tools: on"), "{t}");
    }

    #[test]
    fn search_ranks_exact_names_first_and_pages() {
        let (_d, a) = fixture();
        let snap = a.snapshot().unwrap();
        let t = search(&snap, "get", "", 1);
        let first = t.lines().nth(1).unwrap();
        assert!(
            first.contains("go:store#Mem.Get") || first.contains("go:store#Store.Get"),
            "{t}"
        );
        assert!(search(&snap, "shop", "unit", 1).contains("unit go:pkg:shop"));
        assert!(search(&snap, "buy", "entry_point", 1).contains("http:POST /buy"));
        assert!(search(&snap, "zzz", "", 1).contains("none"));
        let rows: Vec<String> = (0..60).map(|i| format!("r{i}")).collect();
        let p = paged("Rows", &rows, 2, 25);
        assert!(
            p.contains("page 2 of 3") && p.contains("r25") && p.contains("page 3"),
            "{p}"
        );
    }

    #[test]
    fn symbol_shows_docs_members_tests_and_entry_points() {
        let (_d, a) = fixture();
        let snap = a.snapshot().unwrap();
        let t = symbol(&snap, "Service.Buy", 1);
        assert!(t.contains("ID: go:shop#Service.Buy"), "{t}");
        assert!(t.contains("Signature: func (s *Service) Buy"), "{t}");
        assert!(t.contains("Tests (1): TestBuy"), "{t}");
        assert!(t.contains("Serves: POST /buy"), "{t}");
        assert!(t.contains("region Selling"), "{t}");
        let t = symbol(&snap, "Service", 1);
        assert!(t.contains("Members (1): Buy"), "{t}");
        assert!(t.contains("from a `Store`"), "{t}");
        assert!(symbol(&snap, "shop", 1).contains("Items (2 in all)"));
        assert!(symbol(&snap, "Get", 1).contains("matches 2 symbols"));
    }

    #[test]
    fn neighbours_follow_both_directions_and_members() {
        let (_d, a) = fixture();
        let snap = a.snapshot().unwrap();
        let t = neighbours(&snap, "Service", "both", "", 1);
        assert!(t.contains("uses: calls go:store#Store.Get"), "{t}");
        assert!(t.contains("used by: calls go:shop#Run"), "{t}");
        let t = neighbours(&snap, "Service.Buy", "out", "param_type", 1);
        assert!(
            t.contains("param_type go:store#Store") && !t.contains("calls"),
            "{t}"
        );
    }

    #[test]
    fn path_finds_chains_and_falls_back_to_undirected() {
        let (_d, a) = fixture();
        let snap = a.snapshot().unwrap();
        let t = path(&snap, "Run", "store.Store.Get", 6);
        assert!(t.starts_with("A chain of 2 uses"), "{t}");
        let t = path(&snap, "Mem", "Run", 6);
        assert!(t.contains("ignores the direction"), "{t}");
        let t = path(&snap, "Service.Buy", "store.Store.Get", 6);
        assert!(t.starts_with("A chain of 1 use leads"), "{t}");
    }

    #[test]
    fn path_names_the_member_that_an_edge_leaves_from() {
        let (_d, a) = fixture();
        let snap = a.snapshot().unwrap();
        let t = path(&snap, "Service", "store.Store.Get", 6);
        assert!(
            t.contains("calls → go:store#Store.Get (from member Service.Buy)"),
            "{t}"
        );
        let t = path(&snap, "Service", "Service", 6);
        assert!(t.starts_with("acu: from and to both name Service"), "{t}");
        let t = path(&snap, "Service.Buy", "Service", 6);
        assert!(t.contains("belongs to Service as a member"), "{t}");
    }

    #[test]
    fn trace_follows_calls_and_interfaces() {
        let (_d, a) = fixture();
        let snap = a.snapshot().unwrap();
        let t = trace(&snap, "POST /buy", 3, 1);
        assert!(t.contains("go:shop#Service.Buy"), "{t}");
        assert!(t.contains("  go:store#Store.Get"), "{t}");
        assert!(t.contains("may run: go:store#Mem.Get"), "{t}");
    }

    #[test]
    fn read_source_stays_inside_the_project() {
        let (dir, a) = fixture();
        let snap = a.snapshot().unwrap();
        let shop = Sources::new(vec![dir.path().join("shop").canonicalize().unwrap()], None);
        let t = read_source(&snap, a.sources(), "shop/shop.go", 3, 4);
        assert!(t.contains("    3  // Service sells books."), "{t}");
        let t = read_source(&snap, a.sources(), "Service.Buy", 0, 0);
        assert!(
            t.contains("lines 7-9") && t.contains("func (s *Service) Buy"),
            "{t}"
        );
        assert!(read_source(&snap, &shop, "../outside.txt", 1, 1).contains("outside the project"));
        assert!(read_source(&snap, &shop, "/etc/passwd", 1, 1).contains("relative"));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                dir.path().join("outside.txt"),
                dir.path().join("shop/link.txt"),
            )
            .unwrap();
            let t = read_source(&snap, &shop, "link.txt", 1, 1);
            assert!(t.contains("outside the project"), "{t}");
        }
    }

    #[test]
    fn read_source_opens_no_hidden_or_unknown_files() {
        let (dir, a) = fixture();
        let snap = a.snapshot().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/config"), "[core]\n").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "private\n").unwrap();
        let t = read_source(&snap, a.sources(), ".git/config", 1, 1);
        assert!(t.contains("hidden"), "{t}");
        let t = read_source(&snap, a.sources(), "notes.txt", 1, 1);
        assert!(t.contains("git tracks or the map names"), "{t}");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                dir.path().join(".git/config"),
                dir.path().join("shop/config.go"),
            )
            .unwrap();
            let t = read_source(&snap, a.sources(), "shop/config.go", 1, 1);
            assert!(t.contains("hidden"), "{t}");
        }
    }

    #[test]
    fn read_source_survives_odd_lines_and_explains_misses() {
        let (dir, a) = fixture();
        let snap = a.snapshot().unwrap();
        let t = read_source(&snap, a.sources(), "shop/shop.go", usize::MAX, 0);
        assert!(t.contains("has 13 lines"), "{t}");
        let t = read_source(
            &snap,
            a.sources(),
            "shop/shop.go",
            usize::MAX - 2,
            usize::MAX,
        );
        assert!(t.contains("has 13 lines"), "{t}");
        let t = read_source(&snap, a.sources(), "shop/shop.go", 12, usize::MAX);
        assert!(t.contains("lines 12-13 of 13"), "{t}");
        let t = read_source(&snap, a.sources(), "nothing.go", 1, 1);
        assert!(
            t.contains("no file sits at nothing.go, and no symbol has that name"),
            "{t}"
        );
        std::fs::create_dir_all(dir.path().join("store")).unwrap();
        std::fs::write(dir.path().join("store/store.go"), "").unwrap();
        let t = read_source(&snap, a.sources(), "store/store.go", 1, 1);
        assert!(t.contains("holds no lines"), "{t}");
    }

    #[test]
    fn a_served_file_reads_from_its_root_or_says_how_to_set_one() {
        let (dir, _) = fixture();
        let data = dir.path().join("atlas.json");
        std::fs::write(
            &data,
            serde_json::to_vec(&super::super::atlas::tests::fixture_doc()).unwrap(),
        )
        .unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let a = Atlas::from_file(&data, elsewhere.path(), false).unwrap();
        let t = read_source(&a.snapshot().unwrap(), a.sources(), "shop/shop.go", 1, 1);
        assert!(t.contains("start it with --root"), "{t}");
        let a = Atlas::from_file(&data, dir.path(), true).unwrap();
        let t = read_source(&a.snapshot().unwrap(), a.sources(), "shop/shop.go", 1, 1);
        assert!(t.contains("package shop"), "{t}");
    }

    /// A document with one unit of functions named by `names`, and a call
    /// edge for each pair in `calls`.
    fn graph(names: &[String], calls: &[(String, String)]) -> Snapshot {
        let symbols: Vec<Value> = names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                json!({"id": format!("go:g#{n}"), "kind": "function", "languageKind": "func", "name": n,
                       "parent": "go:file:g/g.go", "exported": true, "location": {"path": "g/g.go", "line": i + 1}})
            })
            .collect();
        let edges: Vec<Value> = calls
            .iter()
            .enumerate()
            .map(|(i, (f, t))| {
                json!({"from": format!("go:g#{f}"), "to": format!("go:g#{t}"), "kind": "calls", "confidence": "exact", "line": i + 1})
            })
            .collect();
        let doc = json!({
            "version": 1,
            "meta": {"schemaVersion": 1, "tool": {"name": "t", "version": "0"}, "generated": "2000-01-01T00:00:00Z",
                     "languages": ["go"], "adapters": [{"name": "go", "version": "0", "language": "go", "root": ""}]},
            "containers": [
                {"id": "go:pkg:g", "kind": "package", "languageKind": "package", "name": "g", "path": "g", "language": "go", "unit": true},
                {"id": "go:file:g/g.go", "kind": "file", "languageKind": "file", "name": "g.go", "path": "g/g.go", "parent": "go:pkg:g"}
            ],
            "symbols": symbols,
            "edges": edges
        });
        Snapshot::new(doc, Vec::new())
    }

    /// Turns string pairs into owned pairs.
    fn pairs(p: &[(&str, &str)]) -> Vec<(String, String)> {
        p.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn trace_opens_each_function_at_its_shallowest_depth() {
        let names: Vec<String> = ["A", "B", "X", "Y"].iter().map(|s| s.to_string()).collect();
        let snap = graph(
            &names,
            &pairs(&[("A", "B"), ("B", "X"), ("A", "X"), ("X", "Y")]),
        );
        let t = trace(&snap, "go:g#A", 2, 1);
        assert!(t.contains("    go:g#Y"), "{t}");
        assert!(t.contains("go:g#X — g/g.go:3 (opens at depth 1)"), "{t}");
    }

    #[test]
    fn trace_says_when_it_stops() {
        let mut names = vec!["root".to_string()];
        let mut calls = Vec::new();
        for i in 0..50 {
            names.push(format!("f{i}"));
            names.push(format!("g{i}"));
            calls.push(("root".to_string(), format!("f{i}")));
        }
        for i in 0..50 {
            for j in 0..50 {
                calls.push((format!("f{i}"), format!("g{j}")));
            }
        }
        let snap = graph(&names, &calls);
        let t = trace(&snap, "go:g#root", 2, 1);
        assert!(t.contains(&format!("stops at {MAX_TRACE} rows")), "{t}");
    }

    #[test]
    fn a_unit_pages_its_items() {
        let names: Vec<String> = (0..70).map(|i| format!("F{i:02}")).collect();
        let snap = graph(&names, &[]);
        let t = symbol(&snap, "go:pkg:g", 1);
        assert!(t.contains("Items (70 in all, page 1 of 2)"), "{t}");
        assert!(t.contains("call again with page 2"), "{t}");
        let t = symbol(&snap, "go:pkg:g", 2);
        assert!(t.contains("page 2 of 2") && t.contains("go:g#F69"), "{t}");
    }

    #[test]
    fn search_needs_a_boundary_before_a_suffix() {
        let names: Vec<String> = ["mGET", "GetClientIP"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let snap = graph(&names, &[]);
        let t = search(&snap, "get", "", 1);
        let first = t.lines().nth(1).unwrap();
        assert!(first.contains("GetClientIP"), "{t}");
    }

    #[test]
    fn check_reports_a_clean_map() {
        let (_d, a) = fixture();
        assert!(check(&a.snapshot().unwrap()).contains("passes the check"));
    }
}
