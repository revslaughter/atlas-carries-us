//! Merges adapter documents into one atlas.json and applies atlas.yaml to it:
//! project facts, regions, layout and tour. It also checks that every
//! reference in the result points at something that exists.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{bail, Result};
use globset::{Glob, GlobMatcher};
use serde_json::{json, Map, Value};

use crate::config::{Config, Links, Open};

pub const DATA_SCHEMA: &str = include_str!("../../schema/atlas.schema.json");

/// Problems found while assembling or checking. Errors stop `extract`;
/// warnings do not.
#[derive(Default, Debug)]
pub struct Report {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl Report {
    pub fn print(&self) {
        for w in &self.warnings {
            eprintln!("acu: warning: {w}");
        }
        for e in &self.errors {
            eprintln!("acu: error: {e}");
        }
    }
}

fn arr<'a>(doc: &'a Value, key: &str) -> &'a [Value] {
    doc.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Joins adapter documents. IDs must stay unique across them.
pub fn merge(docs: Vec<Value>) -> Result<Value> {
    let mut out = json!({
        "version": 1,
        "meta": { "schemaVersion": 1, "tool": {}, "generated": "", "languages": [], "adapters": [] },
        "containers": [], "symbols": [], "edges": [], "entryPoints": [], "tests": {}
    });
    let mut languages = BTreeSet::new();
    for d in docs {
        if d.get("version").and_then(Value::as_u64) != Some(1) {
            bail!("an adapter wrote a document whose version is not 1");
        }
        for key in ["containers", "symbols", "edges", "entryPoints"] {
            let items = arr(&d, key).to_vec();
            out[key].as_array_mut().unwrap().extend(items);
        }
        if let Some(t) = d.get("tests").and_then(Value::as_object) {
            let tests = out["tests"].as_object_mut().unwrap();
            for (k, v) in t {
                let list = tests.entry(k.clone()).or_insert_with(|| json!([]));
                list.as_array_mut()
                    .unwrap()
                    .extend(v.as_array().cloned().unwrap_or_default());
            }
        }
        for l in arr(&d["meta"], "languages") {
            if let Some(l) = l.as_str() {
                languages.insert(l.to_string());
            }
        }
        let adapters = arr(&d["meta"], "adapters").to_vec();
        out["meta"]["adapters"]
            .as_array_mut()
            .unwrap()
            .extend(adapters);
    }
    out["meta"]["languages"] = json!(languages);
    let mut seen = HashSet::new();
    for key in ["containers", "symbols"] {
        for item in arr(&out, key) {
            let id = s(item, "id");
            if !seen.insert(id.to_string()) {
                bail!("two items share the ID {id}; IDs must stay unique across adapters");
            }
        }
    }
    Ok(out)
}

/// What `apply` needs besides the config.
pub struct Context {
    pub tool_version: String,
    pub generated: String,
    pub commit: Option<String>,
}

fn link_templates(links: &Option<Links>, repo: &Option<String>) -> Option<Value> {
    let preset = match links {
        Some(Links::Templates(t)) => return Some(Value::Object(t.clone())),
        Some(Links::Preset(p)) => p.as_str(),
        None if repo.is_some() => "github",
        None => "none",
    };
    match preset {
        "github" => Some(json!({
            "file": "https://github.com/{repo}/blob/{commit}/{path}",
            "line": "https://github.com/{repo}/blob/{commit}/{path}#L{line}",
            "range": "https://github.com/{repo}/blob/{commit}/{path}#L{line}-L{endLine}",
            "folder": "https://github.com/{repo}/tree/{commit}/{path}"
        })),
        "gitlab" => Some(json!({
            "file": "https://gitlab.com/{repo}/-/blob/{commit}/{path}",
            "line": "https://gitlab.com/{repo}/-/blob/{commit}/{path}#L{line}",
            "range": "https://gitlab.com/{repo}/-/blob/{commit}/{path}#L{line}-{endLine}",
            "folder": "https://gitlab.com/{repo}/-/tree/{commit}/{path}"
        })),
        _ => None,
    }
}

fn glob(pattern: &str) -> Result<GlobMatcher> {
    Ok(globset::GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map(|g: Glob| g.compile_matcher())?)
}

/// Lookup tables over a document.
pub struct Index {
    pub ids: HashSet<String>,
    pub unit_of: HashMap<String, String>,
    /// Units as (id, path from the repository root, name, path from the language root).
    pub units: Vec<(String, String, String, String)>,
    by_name: HashMap<String, Vec<String>>,
    entry_by_name: HashMap<String, String>,
    entry_ids: HashSet<String>,
}

impl Index {
    pub fn new(doc: &Value) -> Index {
        let mut parent = HashMap::new();
        let mut ids = HashSet::new();
        let mut units: Vec<(String, String, String, String)> = Vec::new();
        let mut unit_ids = HashSet::new();
        let mut lang_root: HashMap<&str, &str> = HashMap::new();
        for a in arr(&doc["meta"], "adapters") {
            lang_root.entry(s(a, "language")).or_insert(s(a, "root"));
        }
        for c in arr(doc, "containers") {
            ids.insert(s(c, "id").to_string());
            parent.insert(s(c, "id").to_string(), s(c, "parent").to_string());
            if c.get("unit").and_then(Value::as_bool) == Some(true) {
                let path = s(c, "path");
                let root = lang_root.get(s(c, "language")).copied().unwrap_or("");
                let local = if root.is_empty() {
                    path
                } else if path == root {
                    ""
                } else {
                    path.strip_prefix(root)
                        .and_then(|p| p.strip_prefix('/'))
                        .unwrap_or(path)
                };
                let dot = |p: &str| {
                    if p.is_empty() {
                        ".".to_string()
                    } else {
                        p.to_string()
                    }
                };
                units.push((
                    s(c, "id").into(),
                    dot(path),
                    s(c, "name").into(),
                    dot(local),
                ));
                unit_ids.insert(s(c, "id").to_string());
            }
        }
        for sy in arr(doc, "symbols") {
            ids.insert(s(sy, "id").to_string());
            parent.insert(s(sy, "id").to_string(), s(sy, "parent").to_string());
        }
        let unit_name: HashMap<&str, &str> =
            units.iter().map(|u| (u.0.as_str(), u.2.as_str())).collect();
        let mut unit_of = HashMap::new();
        let mut by_name: HashMap<String, Vec<String>> = HashMap::new();
        for sy in arr(doc, "symbols") {
            let id = s(sy, "id");
            let mut cur = parent.get(id).cloned().unwrap_or_default();
            let mut guard = 0;
            while !cur.is_empty() && !unit_ids.contains(&cur) && guard < 64 {
                cur = parent.get(&cur).cloned().unwrap_or_default();
                guard += 1;
            }
            if unit_ids.contains(&cur) {
                let path = id.split_once('#').map(|(_, p)| p).unwrap_or(id).to_string();
                by_name
                    .entry(path.clone())
                    .or_default()
                    .push(id.to_string());
                if let Some(u) = unit_name.get(cur.as_str()) {
                    let last = u.rsplit('/').next().unwrap_or(u);
                    by_name
                        .entry(format!("{last}.{path}"))
                        .or_default()
                        .push(id.to_string());
                    if last != *u {
                        by_name
                            .entry(format!("{u}.{path}"))
                            .or_default()
                            .push(id.to_string());
                    }
                }
                unit_of.insert(id.to_string(), cur);
            }
        }
        let mut entry_by_name = HashMap::new();
        let mut entry_ids = HashSet::new();
        for e in arr(doc, "entryPoints") {
            entry_ids.insert(s(e, "id").to_string());
            entry_by_name
                .entry(s(e, "name").to_string())
                .or_insert_with(|| s(e, "id").to_string());
        }
        Index {
            ids,
            unit_of,
            units,
            by_name,
            entry_by_name,
            entry_ids,
        }
    }

    /// Resolves a symbol by ID or by qualified name ("Mux.Get", "chi.Mux.Get").
    pub fn symbol(&self, key: &str) -> Result<String, String> {
        if self.ids.contains(key) && self.unit_of.contains_key(key) {
            return Ok(key.to_string());
        }
        match self.by_name.get(key).map(|v| v.as_slice()) {
            Some([one]) => Ok(one.clone()),
            Some(many) if !many.is_empty() => Err(format!(
                "the name {key} matches {} symbols: {}; use an ID",
                many.len(),
                many.join(", ")
            )),
            _ => Err(format!("no symbol has the ID or name {key}")),
        }
    }

    fn unit(&self, open: &Open) -> Result<String, String> {
        if let Some(id) = &open.id {
            return self
                .units
                .iter()
                .find(|u| &u.0 == id)
                .map(|u| u.0.clone())
                .ok_or(format!("no unit has the ID {id}"));
        }
        if let Some(p) = &open.path {
            let p = if p.is_empty() { "." } else { p.as_str() };
            return self
                .units
                .iter()
                .find(|u| u.1 == p)
                .or_else(|| self.units.iter().find(|u| u.3 == p))
                .map(|u| u.0.clone())
                .ok_or(format!("no unit sits at the path {p}"));
        }
        Err("a unit step needs `id` or `path`".into())
    }

    fn entry(&self, key: &str) -> Result<String, String> {
        if self.entry_ids.contains(key) {
            return Ok(key.to_string());
        }
        self.entry_by_name
            .get(key)
            .cloned()
            .ok_or(format!("no entry point has the ID or name {key}"))
    }
}

/// Applies the config to a merged document.
pub fn apply(doc: &mut Value, cfg: &Config, ctx: &Context) -> Report {
    let mut rep = Report::default();
    let p = &cfg.project;
    let mut project = Map::new();
    project.insert("name".into(), json!(p.name));
    if let Some(r) = &p.repo {
        project.insert("repo".into(), json!(r));
    }
    if let Some(l) = link_templates(&p.links, &p.repo) {
        project.insert("links".into(), l);
    }
    if let Some(d) = &p.description {
        project.insert("description".into(), json!(d.trim()));
    }
    if let Some(d) = &p.description_drafted {
        project.insert("descriptionDrafted".into(), d.clone());
    }
    if !p.autolinks.is_empty() {
        project.insert("autolinks".into(), json!(p.autolinks));
    }
    let notes: Vec<Value> = cfg
        .entry_points
        .iter()
        .flat_map(|e| e.notes.iter())
        .map(|n| {
            let mut m = Map::new();
            if let Some(pp) = &n.path_prefix {
                m.insert("pathPrefix".into(), json!(pp));
            }
            if let Some(t) = &n.title {
                m.insert("title".into(), json!(t));
            }
            m.insert("text".into(), json!(n.text.trim()));
            Value::Object(m)
        })
        .collect();
    if !notes.is_empty() {
        project.insert("entryPointNotes".into(), json!(notes));
    }
    let excluded: BTreeSet<&String> = cfg
        .languages
        .iter()
        .flat_map(|l| l.exclude.iter())
        .collect();
    if !excluded.is_empty() {
        project.insert("excluded".into(), json!(excluded));
    }
    doc["project"] = Value::Object(project);
    doc["meta"]["tool"] = json!({ "name": "acu", "version": ctx.tool_version });
    doc["meta"]["generated"] = json!(ctx.generated);
    match &ctx.commit {
        Some(c) => doc["meta"]["commit"] = json!(c),
        None => {
            doc["meta"].as_object_mut().unwrap().remove("commit");
        }
    }

    let idx = Index::new(doc);
    regions(doc, cfg, &idx, &mut rep);
    tour(doc, cfg, &idx, &mut rep);
    if arr(doc, "entryPoints").is_empty() {
        doc.as_object_mut().unwrap().remove("entryPoints");
    }
    if doc
        .get("tests")
        .and_then(Value::as_object)
        .is_some_and(|t| t.is_empty())
    {
        doc.as_object_mut().unwrap().remove("tests");
    }
    rep
}

fn regions(doc: &mut Value, cfg: &Config, idx: &Index, rep: &mut Report) {
    let mut members: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let mut unassigned = Vec::new();
    let mut matchers = Vec::new();
    for (i, r) in cfg.regions.iter().enumerate() {
        let mut ms = Vec::new();
        for pat in &r.paths {
            match glob(pat) {
                Ok(m) => ms.push(m),
                Err(e) => rep.errors.push(format!(
                    "region {}: the path pattern {pat} does not parse: {e}",
                    r.id
                )),
            }
        }
        matchers.push((i, ms));
    }
    let default = cfg.regions.iter().position(|r| r.default);
    for (id, path, _, local) in &idx.units {
        let hit = matchers
            .iter()
            .find(|(_, ms)| ms.iter().any(|m| m.is_match(path) || m.is_match(local)))
            .map(|(i, _)| *i);
        match hit.or(default) {
            Some(i) => members.entry(i).or_default().push(id.clone()),
            None => unassigned.push((id.clone(), local.clone())),
        }
    }
    let mut out = Vec::new();
    for (i, r) in cfg.regions.iter().enumerate() {
        let units = members.remove(&i).unwrap_or_default();
        if units.is_empty() {
            rep.warnings.push(format!(
                "region {} ({}) holds no units; the overview leaves it out",
                r.id, r.name
            ));
            continue;
        }
        let mut m = Map::new();
        m.insert("id".into(), json!(r.id));
        m.insert("name".into(), json!(r.name));
        m.insert("color".into(), json!(r.color.unwrap_or((i % 9) as u8)));
        m.insert("containers".into(), json!(units));
        if let Some(d) = &r.description {
            m.insert("description".into(), json!(d.trim()));
        }
        if let Some(t) = &r.reading_tip {
            m.insert("readingTip".into(), json!(t.trim()));
        }
        if let Some(d) = &r.drafted {
            m.insert("drafted".into(), d.clone());
        }
        out.push(Value::Object(m));
    }
    if !unassigned.is_empty() {
        if !cfg.regions.is_empty() {
            rep.warnings.push(format!(
                "{} units match no region and have no default region; they go to \"Other code\": {}",
                unassigned.len(),
                unassigned.iter().map(|u| u.1.as_str()).collect::<Vec<_>>().join(", ")
            ));
            out.push(
                json!({ "id": "other", "name": "Other code", "color": (out.len() % 9),
                "containers": unassigned.iter().map(|u| &u.0).collect::<Vec<_>>() }),
            );
        } else {
            // No regions in the config: one region per top-level folder.
            let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for (id, path) in &unassigned {
                let top = path.split('/').next().unwrap_or(".").to_string();
                groups.entry(top).or_default().push(id.clone());
            }
            for (i, (top, units)) in groups.into_iter().enumerate() {
                let name = if top == "." {
                    "Root".to_string()
                } else {
                    top.clone()
                };
                let id: String = name
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                    .collect();
                out.push(json!({ "id": id, "name": name, "color": i % 9, "containers": units }));
            }
        }
    }
    let ids: Vec<String> = out.iter().map(|r| s(r, "id").to_string()).collect();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut placed = HashSet::new();
    if let Some(l) = &cfg.layout {
        for row in &l.rows {
            let mut r = Vec::new();
            for id in row {
                if ids.contains(id) && placed.insert(id.clone()) {
                    r.push(id.clone());
                } else if !ids.contains(id) && cfg.regions.iter().any(|x| &x.id == id) {
                    // An empty region: already warned about.
                } else if !ids.contains(id) {
                    rep.errors.push(format!(
                        "layout names the region {id}, which the config does not define"
                    ));
                }
            }
            if !r.is_empty() {
                rows.push(r);
            }
        }
    }
    let rest: Vec<String> = ids
        .iter()
        .filter(|i| !placed.contains(*i))
        .cloned()
        .collect();
    for chunk in rest.chunks(3) {
        rows.push(chunk.to_vec());
    }
    doc["regions"] = json!(out);
    doc["layout"] = json!({ "rows": rows });
}

fn tour(doc: &mut Value, cfg: &Config, idx: &Index, rep: &mut Report) {
    let region_ids: HashSet<String> = arr(doc, "regions")
        .iter()
        .map(|r| s(r, "id").to_string())
        .collect();
    let mut steps = Vec::new();
    for (n, st) in cfg.tour.iter().enumerate() {
        let where_ = format!("tour step {} ({})", n + 1, st.title);
        let o = &st.open;
        let open: Result<Value, String> = match o.view.as_str() {
            "overview" => Ok(json!({ "view": "overview" })),
            "region" => match &o.id {
                Some(id) if region_ids.contains(id) => Ok(json!({ "view": "region", "id": id })),
                Some(id) => Err(format!("no region has the ID {id}")),
                None => Err("a region step needs `id`".into()),
            },
            "unit" => idx.unit(o).map(|id| json!({ "view": "unit", "id": id })),
            "symbol" => match o.id.as_ref().or(o.name.as_ref()) {
                Some(k) => idx
                    .symbol(k)
                    .map(|id| json!({ "view": "symbol", "id": id })),
                None => Err("a symbol step needs `id` or `name`".into()),
            },
            "trace" => {
                if let Some(e) = &o.entry_point {
                    idx.entry(e)
                        .map(|id| json!({ "view": "trace", "entryPoint": id }))
                } else if let Some(k) = o.symbol.as_ref().or(o.name.as_ref()).or(o.id.as_ref()) {
                    idx.symbol(k)
                        .map(|id| json!({ "view": "trace", "symbol": id }))
                } else {
                    Err("a trace step needs `entryPoint` or `symbol`".into())
                }
            }
            v => Err(format!("unknown view {v}")),
        };
        match open {
            Ok(open) => {
                let mut m = Map::new();
                m.insert("title".into(), json!(st.title));
                m.insert("text".into(), json!(st.text.trim()));
                m.insert("open".into(), open);
                if let Some(d) = &st.drafted {
                    m.insert("drafted".into(), d.clone());
                }
                steps.push(Value::Object(m));
            }
            Err(e) => rep.errors.push(format!("{where_}: {e}")),
        }
    }
    if !steps.is_empty() {
        doc["tour"] = json!(steps);
    }
}

/// Checks a finished document: the schema, and that every reference points
/// at something that exists.
pub fn check(doc: &Value) -> Report {
    let mut rep = Report::default();
    match crate::config::validate(doc, DATA_SCHEMA) {
        Ok(p) => rep
            .errors
            .extend(p.into_iter().map(|p| format!("schema: {p}"))),
        Err(e) => rep.errors.push(format!("schema: {e}")),
    }
    let idx = Index::new(doc);
    let missing = |what: &str, id: &str, rep: &mut Report| {
        if !id.is_empty() && !idx.ids.contains(id) {
            rep.errors
                .push(format!("{what} names {id}, which does not exist"));
        }
    };
    for sy in arr(doc, "symbols") {
        missing(
            &format!("the parent of {}", s(sy, "id")),
            s(sy, "parent"),
            &mut rep,
        );
    }
    for c in arr(doc, "containers") {
        missing(
            &format!("the parent of {}", s(c, "id")),
            s(c, "parent"),
            &mut rep,
        );
    }
    for e in arr(doc, "edges") {
        missing("an edge", s(e, "from"), &mut rep);
        missing("an edge", s(e, "to"), &mut rep);
    }
    for e in arr(doc, "entryPoints") {
        missing(
            &format!("entry point {}", s(e, "id")),
            s(e, "handler"),
            &mut rep,
        );
    }
    if let Some(t) = doc.get("tests").and_then(Value::as_object) {
        for k in t.keys() {
            missing("the test index", k, &mut rep);
        }
    }
    for r in arr(doc, "regions") {
        for c in arr(r, "containers") {
            missing(
                &format!("region {}", s(r, "id")),
                c.as_str().unwrap_or(""),
                &mut rep,
            );
        }
    }
    let entries: HashSet<&str> = arr(doc, "entryPoints").iter().map(|e| s(e, "id")).collect();
    let regions: HashSet<&str> = arr(doc, "regions").iter().map(|r| s(r, "id")).collect();
    for (n, st) in arr(doc, "tour").iter().enumerate() {
        let o = &st["open"];
        let ok = match s(o, "view") {
            "overview" => true,
            "region" => regions.contains(s(o, "id")),
            "unit" | "symbol" => idx.ids.contains(s(o, "id")),
            "trace" => entries.contains(s(o, "entryPoint")) || idx.ids.contains(s(o, "symbol")),
            _ => false,
        };
        if !ok {
            rep.errors.push(format!(
                "tour step {} ({}) opens a view whose target does not exist",
                n + 1,
                s(st, "title")
            ));
        }
    }
    rep.errors.truncate(60);
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Value {
        json!({
            "version": 1,
            "meta": { "schemaVersion": 1, "tool": {"name": "t", "version": "0"}, "generated": "2000-01-01T00:00:00Z",
                      "languages": ["go"], "adapters": [{"name": "go", "version": "0", "language": "go", "root": "src"}] },
            "containers": [
                {"id": "go:pkg:a", "kind": "package", "languageKind": "package", "name": "a", "path": "src/a", "language": "go", "unit": true},
                {"id": "go:pkg:b", "kind": "package", "languageKind": "package", "name": "lib/b", "path": "src/lib/b", "language": "go", "unit": true},
                {"id": "go:file:src/a/a.go", "kind": "file", "languageKind": "file", "name": "a.go", "parent": "go:pkg:a"},
                {"id": "go:file:src/lib/b/b.go", "kind": "file", "languageKind": "file", "name": "b.go", "parent": "go:pkg:b"}
            ],
            "symbols": [
                {"id": "go:a#Store", "kind": "struct", "languageKind": "struct", "name": "Store", "parent": "go:file:src/a/a.go", "exported": true, "location": {"path": "src/a/a.go", "line": 1}},
                {"id": "go:a#Store.Get", "kind": "method", "languageKind": "method", "name": "Get", "parent": "go:a#Store", "exported": true, "location": {"path": "src/a/a.go", "line": 3}},
                {"id": "go:b#Store", "kind": "struct", "languageKind": "struct", "name": "Store", "parent": "go:file:src/lib/b/b.go", "exported": true, "location": {"path": "src/lib/b/b.go", "line": 1}}
            ],
            "edges": [{"from": "go:a#Store.Get", "to": "go:b#Store", "kind": "return_type", "confidence": "exact"}]
        })
    }

    fn config(yaml: &str) -> Config {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    fn ctx() -> Context {
        Context {
            tool_version: "0".into(),
            generated: "2000-01-01T00:00:00Z".into(),
            commit: None,
        }
    }

    #[test]
    fn names_resolve_and_ambiguity_is_reported() {
        let idx = Index::new(&doc());
        assert_eq!(idx.symbol("Store.Get").unwrap(), "go:a#Store.Get");
        assert_eq!(idx.symbol("a.Store").unwrap(), "go:a#Store");
        assert_eq!(idx.symbol("b.Store").unwrap(), "go:b#Store");
        assert_eq!(idx.symbol("lib/b.Store").unwrap(), "go:b#Store");
        assert!(idx
            .symbol("Store")
            .unwrap_err()
            .contains("matches 2 symbols"));
        assert!(idx.symbol("Nothing").unwrap_err().contains("no symbol"));
    }

    #[test]
    fn regions_match_language_root_paths_and_fall_back_to_the_default() {
        let mut d = doc();
        let cfg = config(
            "version: 1\nproject: {name: x}\nlanguages: [{adapter: go}]\n\
             regions:\n  - {id: core, name: Core, paths: [a]}\n  - {id: rest, name: Rest, default: true}\n\
             tour:\n  - {title: t, text: x, open: {view: unit, path: lib/b}}\n  - {title: u, text: y, open: {view: symbol, name: a.Store}}\n",
        );
        let rep = apply(&mut d, &cfg, &ctx());
        assert!(rep.errors.is_empty(), "{:?}", rep.errors);
        assert_eq!(d["regions"][0]["containers"], json!(["go:pkg:a"]));
        assert_eq!(d["regions"][1]["containers"], json!(["go:pkg:b"]));
        assert_eq!(
            d["tour"][0]["open"],
            json!({"view": "unit", "id": "go:pkg:b"})
        );
        assert_eq!(
            d["tour"][1]["open"],
            json!({"view": "symbol", "id": "go:a#Store"})
        );
        let chk = check(&d);
        assert!(chk.errors.is_empty(), "{:?}", chk.errors);
    }

    #[test]
    fn broken_tour_steps_and_layout_names_are_errors() {
        let mut d = doc();
        let cfg = config(
            "version: 1\nproject: {name: x}\nlanguages: [{adapter: go}]\n\
             regions: [{id: core, name: Core, paths: ['**']}]\nlayout: {rows: [[core, ghost]]}\n\
             tour: [{title: t, text: x, open: {view: symbol, name: Missing}}]\n",
        );
        let rep = apply(&mut d, &cfg, &ctx());
        assert_eq!(rep.errors.len(), 2, "{:?}", rep.errors);
        assert!(rep.errors.iter().any(|e| e.contains("ghost")));
        assert!(rep.errors.iter().any(|e| e.contains("tour step 1")));
    }

    #[test]
    fn check_finds_dangling_references() {
        let mut d = doc();
        d["edges"].as_array_mut().unwrap().push(json!({"from": "go:a#Store", "to": "go:x#Gone", "kind": "calls", "confidence": "exact"}));
        let rep = check(&d);
        assert!(
            rep.errors.iter().any(|e| e.contains("go:x#Gone")),
            "{:?}",
            rep.errors
        );
    }

    #[test]
    fn merge_rejects_duplicate_ids() {
        assert!(merge(vec![doc(), doc()]).is_err());
    }
}
