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
        for (i, e) in arr(doc, "entryPoints").iter().enumerate() {
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
    for r in arr(d, "regions") {
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
    let tour = arr(d, "tour");
    if tour.is_empty() {
        out.push_str("Tour: no steps yet.\n");
    } else {
        out.push_str("Tour:\n");
        for (i, st) in tour.iter().enumerate() {
            out.push_str(&format!(
                "  {}. {} ({}){}\n",
                i + 1,
                s(st, "title"),
                s(&st["open"], "view"),
                drafted(&st["drafted"])
            ));
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
        } else if path.ends_with(&format!(".{q}")) || path.ends_with(&q) {
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
/// tests; or one unit's docs and items; or one entry point.
pub fn symbol(snap: &Snapshot, key: &str) -> String {
    match resolve(snap, key) {
        Err(e) => e,
        Ok(Target::Symbol(id)) => symbol_text(snap, &id),
        Ok(Target::Container(id)) => container_text(snap, &id),
        Ok(Target::Entry(i)) => entry_text(&arr(&snap.doc, "entryPoints")[i]),
    }
}

/// Writes the detail of one symbol.
fn symbol_text(snap: &Snapshot, id: &str) -> String {
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
        let names: Vec<&str> = ms
            .iter()
            .filter_map(|m| sym(snap, m))
            .map(|m| s(m, "name"))
            .take(40)
            .collect();
        out.push_str(&format!("Members ({}): {}", ms.len(), names.join(", ")));
        if ms.len() > names.len() {
            out.push_str(", …");
        }
        out.push('\n');
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

/// Writes the detail of one container: a unit lists its items.
fn container_text(snap: &Snapshot, id: &str) -> String {
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
        .take(60)
        .collect();
    out.push_str(&paged("Items", &rows, 1, 60));
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
    let depth = max_depth.clamp(1, 12);
    for directed in [true, false] {
        if let Some(chain) = bfs(snap, &a, &b, depth, directed) {
            let mut out = if directed {
                format!(
                    "A chain of {} uses leads from {} to {}:\n",
                    chain.len(),
                    qual(snap, &a),
                    qual(snap, &b)
                )
            } else {
                format!(
                    "No chain of uses leads from {} to {}. A chain that ignores the direction of edges ({} steps):\n",
                    qual(snap, &a),
                    qual(snap, &b),
                    chain.len()
                )
            };
            out.push_str(&format!("  {a}\n"));
            for (kind, forward, id) in chain {
                let arrow = if forward {
                    format!("{kind} →")
                } else {
                    format!("← {kind} (used by)")
                };
                out.push_str(&format!("  {arrow} {id}\n"));
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

/// The steps of a shortest path: the edge kind, whether the step follows
/// the edge's direction, and the ID it reaches.
type Chain = Vec<(String, bool, String)>;

/// Finds a shortest path by breadth-first search. A member counts as part
/// of its owner, so a path may start or end at a type.
fn bfs(snap: &Snapshot, a: &str, b: &str, depth: usize, directed: bool) -> Option<Chain> {
    let l = &snap.lookup;
    let edges = arr(&snap.doc, "edges");
    let owner = |id: &str| -> String {
        match sym(snap, id) {
            Some(sy) if l.sym.contains_key(s(sy, "parent")) => s(sy, "parent").to_string(),
            _ => id.to_string(),
        }
    };
    let goal = |id: &str| id == b || owner(id) == b;
    let mut prev: HashMap<String, (String, String, bool)> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::from([a.to_string()]);
    let mut queue = VecDeque::from([(a.to_string(), 0usize)]);
    while let Some((cur, d)) = queue.pop_front() {
        if cur != a && goal(&cur) {
            let mut chain = Vec::new();
            let mut at = cur.clone();
            while let Some((p, kind, fwd)) = prev.get(&at) {
                chain.push((kind.clone(), *fwd, at.clone()));
                at = p.clone();
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
        let mut next: Vec<(String, String, bool)> = Vec::new();
        for x in &starts {
            for &i in l.out.get(x).map_or(&[][..], Vec::as_slice) {
                let e = &edges[i];
                next.push((s(e, "to").to_string(), s(e, "kind").to_string(), true));
            }
            if !directed {
                for &i in l.inc.get(x).map_or(&[][..], Vec::as_slice) {
                    let e = &edges[i];
                    next.push((s(e, "from").to_string(), s(e, "kind").to_string(), false));
                }
            }
        }
        next.sort();
        for (n, kind, fwd) in next {
            if seen.insert(n.clone()) {
                prev.insert(n.clone(), (cur.clone(), kind, fwd));
                queue.push_back((n, d + 1));
            }
        }
    }
    None
}

/// `trace`: the calls that run from an entry point or a function, as an
/// indented tree, `depth` calls deep. A call to an interface method lists
/// the methods of the types that implement it.
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
    let implementers = implementers(snap);
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    walk(
        snap,
        &implementers,
        &root,
        0,
        depth,
        "",
        &mut seen,
        &mut rows,
    );
    let mut out = paged(&title, &rows, page, 60);
    out.push_str("Indent shows depth. \"may run\" marks a method behind an interface. Calls through function values do not appear.");
    out
}

/// Maps each interface's ID to the IDs of the types that implement it.
fn implementers(snap: &Snapshot) -> HashMap<String, Vec<String>> {
    let mut m: HashMap<String, Vec<String>> = HashMap::new();
    for e in arr(&snap.doc, "edges") {
        if s(e, "kind") == "implements" {
            m.entry(s(e, "to").to_string())
                .or_default()
                .push(s(e, "from").to_string());
        }
    }
    for v in m.values_mut() {
        v.sort();
    }
    m
}

/// Writes one row of a trace and walks the calls below it in source order.
#[allow(clippy::too_many_arguments)]
fn walk(
    snap: &Snapshot,
    implementers: &HashMap<String, Vec<String>>,
    id: &str,
    d: usize,
    depth: usize,
    label: &str,
    seen: &mut HashSet<String>,
    rows: &mut Vec<String>,
) {
    if rows.len() >= 2000 {
        return;
    }
    let pad = "  ".repeat(d);
    let first = seen.insert(id.to_string());
    let note = match sym(snap, id) {
        Some(sy) => format!(" — {}", place(sy)),
        None => String::new(),
    };
    rows.push(format!(
        "{pad}{label}{}{note}{}",
        id,
        if first { "" } else { " (seen above)" }
    ));
    if !first || d >= depth {
        return;
    }
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
    for e in calls {
        let to = s(e, "to");
        walk(snap, implementers, to, d + 1, depth, "", seen, rows);
        // A call to an interface method may run each implementer's method.
        if let Some(sy) = sym(snap, to) {
            let owner = s(sy, "parent");
            for t in implementers.get(owner).map_or(&[][..], Vec::as_slice) {
                for m in l.members.get(t).map_or(&[][..], Vec::as_slice) {
                    if sym(snap, m).map(|x| s(x, "name")) == Some(s(sy, "name")) {
                        walk(snap, implementers, m, d + 1, depth, "may run: ", seen, rows);
                    }
                }
            }
        }
    }
}

/// `read_source`: numbered lines of a file under the project folder. The
/// key is a path relative to the repository root, or a symbol's ID or name,
/// which reads that symbol's lines. Paths that leave the project folder,
/// symbolic links among them, stay closed.
pub fn read_source(
    snap: &Snapshot,
    roots: &[PathBuf],
    key: &str,
    start: usize,
    end: usize,
) -> String {
    let key = key.trim();
    // An exact symbol ID wins; then a file at that path; then a name.
    let found = open_inside(roots, key);
    let as_file = !snap.lookup.sym.contains_key(key)
        && (found.is_ok()
            || found.as_ref().is_err_and(|e| !e.contains("no file sits"))
            || key.contains('/') && !key.contains('#'));
    let (rel, mut start, mut end) = if as_file {
        (key.to_string(), start, end)
    } else {
        match resolve_symbol(snap, key, true) {
            Ok(id) => {
                let sy = sym(snap, &id).unwrap_or(&Value::Null);
                let l = &sy["location"];
                let line = l.get("line").and_then(Value::as_u64).unwrap_or(1) as usize;
                let endl = l
                    .get("endLine")
                    .and_then(Value::as_u64)
                    .map_or(line, |e| e as usize);
                (s(l, "path").to_string(), line, endl)
            }
            Err(e) => return e,
        }
    };
    if start == 0 {
        start = 1;
    }
    if end == 0 || end < start {
        end = start + MAX_LINES - 1;
    }
    let file = match open_inside(roots, &rel) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let text = match std::fs::read(&file) {
        Ok(b) => String::from_utf8_lossy(&b).to_string(),
        Err(e) => return format!("acu: read {rel}: {e}"),
    };
    let lines: Vec<&str> = text.lines().collect();
    if start > lines.len() {
        return format!("acu: {rel} has {} lines; ask for fewer", lines.len());
    }
    let cut_at = (start + MAX_LINES - 1).min(end).min(lines.len());
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

/// Finds a file under one of the roots. It resolves symbolic links and
/// `..` first, and refuses any file that then lies outside every root.
pub fn open_inside(roots: &[PathBuf], rel: &str) -> Result<PathBuf, String> {
    let p = Path::new(rel);
    if p.is_absolute() {
        return Err(format!(
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
            return Err(format!(
                "acu: {rel} lies outside the project folder; the read tools see only the project"
            ));
        }
    }
    let mut outside = false;
    for root in roots {
        let Ok(full) = root.join(p).canonicalize() else {
            continue;
        };
        if !full.starts_with(root) {
            outside = true;
            continue;
        }
        let meta = std::fs::metadata(&full).map_err(|e| format!("acu: read {rel}: {e}"))?;
        if !meta.is_file() {
            return Err(format!("acu: {rel} is no file; give a file's path"));
        }
        if meta.len() > MAX_FILE {
            return Err(format!(
                "acu: {rel} holds more than 2 MB; acu reads smaller files only"
            ));
        }
        return Ok(full);
    }
    if outside {
        Err(format!(
            "acu: {rel} lies outside the project folder; the read tools see only the project"
        ))
    } else {
        Err(format!(
            "acu: no file sits at {rel} in the project folder; paths start at the repository root"
        ))
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
    use super::*;

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
        let t = symbol(&snap, "Service.Buy");
        assert!(t.contains("ID: go:shop#Service.Buy"), "{t}");
        assert!(t.contains("Signature: func (s *Service) Buy"), "{t}");
        assert!(t.contains("Tests (1): TestBuy"), "{t}");
        assert!(t.contains("Serves: POST /buy"), "{t}");
        assert!(t.contains("region Selling"), "{t}");
        let t = symbol(&snap, "Service");
        assert!(t.contains("Members (1): Buy"), "{t}");
        assert!(t.contains("from a `Store`"), "{t}");
        assert!(symbol(&snap, "shop").contains("Items (2 in all)"));
        assert!(symbol(&snap, "Get").contains("matches 2 symbols"));
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
        let roots = vec![dir.path().join("shop").canonicalize().unwrap()];
        let t = read_source(&snap, a.roots(), "shop/shop.go", 3, 4);
        assert!(t.contains("    3  // Service sells books."), "{t}");
        let t = read_source(&snap, a.roots(), "Service.Buy", 0, 0);
        assert!(
            t.contains("lines 7-9") && t.contains("func (s *Service) Buy"),
            "{t}"
        );
        assert!(read_source(&snap, &roots, "../outside.txt", 1, 1).contains("outside the project"));
        assert!(read_source(&snap, &roots, "/etc/passwd", 1, 1).contains("relative"));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                dir.path().join("outside.txt"),
                dir.path().join("shop/link.txt"),
            )
            .unwrap();
            let t = read_source(&snap, &roots, "link.txt", 1, 1);
            assert!(t.contains("outside the project"), "{t}");
        }
    }

    #[test]
    fn check_reports_a_clean_map() {
        let (_d, a) = fixture();
        assert!(check(&a.snapshot().unwrap()).contains("passes the check"));
    }
}
