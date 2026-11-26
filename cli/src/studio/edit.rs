//! The editor's calls: read atlas.yaml with the map's units, save the tour,
//! the regions and the project text, preview which units a region's paths
//! match, and run the adapters again. Each save goes through
//! [`Atlas::save`], so it keeps the file's comments, passes the config
//! schema and the check of `acu extract`, and refuses to overwrite a newer
//! file.

use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use super::{fail, Reply, Studio};
use crate::assemble;
use crate::mcp::atlas::{SaveError, Written};
use crate::mcp::yaml::{self, Change};
use crate::mcp::Atlas;

/// Returns the map on show, or an answer that says none shows.
fn atlas(s: &Studio) -> Result<Arc<Atlas>, Reply> {
    s.live.get().map_err(|e| fail(StatusCode::CONFLICT, e))
}

/// Reads the revision that a save starts from.
fn base(args: &Map<String, Value>) -> Result<String, Reply> {
    args.get("revision")
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| {
            fail(
                StatusCode::BAD_REQUEST,
                "send the revision that GET /api/config gave",
            )
        })
}

/// Reads a list of objects from the body.
fn objects(args: &Map<String, Value>, key: &str) -> Result<Vec<Value>, Reply> {
    let list = args
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| fail(StatusCode::BAD_REQUEST, format!("send {key} as a list")))?;
    if list.iter().any(|v| !v.is_object()) {
        return Err(fail(
            StatusCode::BAD_REQUEST,
            format!("each entry of {key} must be an object"),
        ));
    }
    Ok(list.clone())
}

/// Turns the result of a save into an answer: the new revision, how acu
/// edited the file, and the warnings of the new map.
fn saved(a: &Atlas, out: Result<Written, SaveError>) -> Reply {
    match out {
        Ok(w) => {
            let rev = a.config_text().map(|t| t.1).unwrap_or_default();
            (
                StatusCode::OK,
                json!({ "revision": rev, "how": w.how, "warnings": w.warnings }),
            )
        }
        Err(SaveError::Stale(e)) => fail(StatusCode::CONFLICT, e),
        Err(SaveError::Failed(e)) if !a.writable() => fail(StatusCode::FORBIDDEN, e),
        Err(SaveError::Failed(e)) => fail(StatusCode::UNPROCESSABLE_ENTITY, e),
    }
}

/// `GET /api/config`: the editable parts of atlas.yaml as written, the
/// same parts as acu resolved them, every unit with its region, and the
/// file's revision.
pub fn config(s: &Studio) -> Reply {
    let a = match atlas(s) {
        Ok(a) => a,
        Err(r) => return r,
    };
    let (text, rev) = match a.config_text() {
        Ok(t) => t,
        Err(e) => return fail(StatusCode::CONFLICT, e),
    };
    let v = match yaml::parse(&text) {
        Ok(v) => v,
        Err(e) => return fail(StatusCode::UNPROCESSABLE_ENTITY, e),
    };
    let snap = match a.snapshot() {
        Ok(s) => s,
        Err(e) => return fail(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let mut region_of: std::collections::HashMap<&str, &str> = Default::default();
    for r in snap.doc["regions"].as_array().into_iter().flatten() {
        for c in r["containers"].as_array().into_iter().flatten() {
            if let (Some(c), Some(id)) = (c.as_str(), r["id"].as_str()) {
                region_of.insert(c, id);
            }
        }
    }
    let units: Vec<Value> = snap
        .index
        .units
        .iter()
        .map(|(id, path, name, local)| {
            json!({ "id": id, "path": path, "local": local, "name": name,
                    "region": region_of.get(id.as_str()) })
        })
        .collect();
    let or_list = |k: &str| {
        v.get(k)
            .filter(|x| x.is_array())
            .cloned()
            .unwrap_or(json!([]))
    };
    (
        StatusCode::OK,
        json!({
            "revision": rev,
            "path": a.config_path().map(|p| p.display().to_string()),
            "writable": a.writable(),
            "why": a.why_read_only(),
            "project": v.get("project").cloned().unwrap_or(json!({})),
            "regions": or_list("regions"),
            "layout": v.get("layout").cloned().unwrap_or(Value::Null),
            "tour": or_list("tour"),
            "resolved": {
                "tour": snap.doc.get("tour").cloned().unwrap_or(json!([])),
                "regions": snap.doc.get("regions").cloned().unwrap_or(json!([])),
            },
            "units": units,
            "warnings": snap.warnings,
        }),
    )
}

/// `POST /api/tour`: replaces the tour with the given steps. Steps that
/// equal a step of the file keep their lines and comments.
pub fn save_tour(s: &Studio, args: &Map<String, Value>) -> Reply {
    let a = match atlas(s) {
        Ok(a) => a,
        Err(r) => return r,
    };
    let (rev, steps) = match (base(args), objects(args, "tour")) {
        (Ok(r), Ok(t)) => (r, t),
        (Err(e), _) | (_, Err(e)) => return e,
    };
    let out = a.save(&rev, |text| {
        yaml::edit(
            text,
            &Change::List {
                key: "tour",
                items: steps,
            },
        )
    });
    saved(&a, out)
}

/// `POST /api/regions`: replaces the regions, and the layout when the body
/// names one (null removes it).
pub fn save_regions(s: &Studio, args: &Map<String, Value>) -> Reply {
    let a = match atlas(s) {
        Ok(a) => a,
        Err(r) => return r,
    };
    let (rev, regions) = match (base(args), objects(args, "regions")) {
        (Ok(r), Ok(t)) => (r, t),
        (Err(e), _) | (_, Err(e)) => return e,
    };
    let layout = args.get("layout").cloned();
    if let Some(l) = &layout {
        if !(l.is_null() || l.get("rows").is_some_and(Value::is_array)) {
            return fail(
                StatusCode::BAD_REQUEST,
                "send layout as null or as { rows: [[region ids]] }",
            );
        }
    }
    let out = a.save(&rev, |text| {
        let (mut text, mut how) = yaml::edit(
            text,
            &Change::List {
                key: "regions",
                items: regions,
            },
        )?;
        if let Some(l) = layout {
            let (t, h) = yaml::edit(
                &text,
                &Change::Top {
                    key: "layout",
                    value: l,
                },
            )?;
            text = t;
            if h != "in place" {
                how = h;
            }
        }
        Ok((text, how))
    });
    saved(&a, out)
}

/// `POST /api/project`: sets the project's name and description, and keeps
/// or removes the description's drafted mark as the body says.
pub fn save_project(s: &Studio, args: &Map<String, Value>) -> Reply {
    let a = match atlas(s) {
        Ok(a) => a,
        Err(r) => return r,
    };
    let rev = match base(args) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if name.is_empty() {
        return fail(StatusCode::BAD_REQUEST, "give the project a name");
    }
    let text_or_null = |k: &str| match args.get(k).and_then(Value::as_str).map(str::trim) {
        Some(t) if !t.is_empty() => json!(t),
        _ => Value::Null,
    };
    let description = text_or_null("description");
    let drafted = match args.get("descriptionDrafted") {
        Some(v @ Value::Object(_)) => v.clone(),
        _ => Value::Null,
    };
    let out = a.save(&rev, |text| {
        yaml::edit(
            text,
            &Change::Keys {
                key: "project",
                values: vec![
                    ("name", json!(name)),
                    ("description", description),
                    ("descriptionDrafted", drafted),
                ],
            },
        )
    });
    saved(&a, out)
}

/// `POST /api/match`: lists the units that a region's path patterns match,
/// as acu matches them, and the patterns that do not parse.
pub fn matching(s: &Studio, args: &Map<String, Value>) -> Reply {
    let a = match atlas(s) {
        Ok(a) => a,
        Err(r) => return r,
    };
    let Some(paths) = args.get("paths").and_then(Value::as_array) else {
        return fail(StatusCode::BAD_REQUEST, "send paths as a list of patterns");
    };
    let snap = match a.snapshot() {
        Ok(s) => s,
        Err(e) => return fail(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let mut matchers = Vec::new();
    let mut errors = Vec::new();
    for p in paths.iter().filter_map(Value::as_str) {
        match assemble::glob(p) {
            Ok(m) => matchers.push(m),
            Err(e) => errors.push(format!("{p}: {e}")),
        }
    }
    let units: Vec<&str> = snap
        .index
        .units
        .iter()
        .filter(|(_, path, _, local)| {
            matchers
                .iter()
                .any(|m| m.is_match(path) || m.is_match(local))
        })
        .map(|u| u.0.as_str())
        .collect();
    (StatusCode::OK, json!({ "units": units, "errors": errors }))
}

/// `POST /api/extract`: runs the adapters again on the open config, as a
/// job, and shows the new map when they finish.
pub fn extract(s: &Arc<Studio>) -> Reply {
    let a = match atlas(s) {
        Ok(a) => a,
        Err(r) => return r,
    };
    if let Some(why) = a.why_read_only() {
        return fail(StatusCode::FORBIDDEN, why);
    }
    let Some(path) = a.config_path().map(|p| p.to_path_buf()) else {
        return fail(
            StatusCode::CONFLICT,
            "no config stands behind this map; start acu with -c atlas.yaml",
        );
    };
    s.start_job("extract", move |st, log| {
        open_config(st, &path, log)?;
        Ok(None)
    })
}

/// Reads a config, runs its adapters, and puts the checked map on show.
/// Every line of progress goes to `log`.
pub fn open_config(
    st: &Studio,
    path: &std::path::Path,
    log: &(dyn Fn(&str) + Sync),
) -> Result<(), String> {
    let err = |e: anyhow::Error| format!("acu: {e:#}");
    let loaded = crate::config::load(path).map_err(err)?;
    let run = crate::run_adapters_with(&loaded, log).map_err(err)?;
    log("acu: checking the map");
    let atlas = Atlas::from_config(path, run, None).map_err(err)?;
    let snap = atlas.snapshot()?;
    let n = |k: &str| snap.doc[k].as_array().map_or(0, Vec::len);
    log(&format!(
        "acu: the map holds {} containers, {} symbols, {} edges and {} entry points",
        n("containers"),
        n("symbols"),
        n("edges"),
        n("entryPoints")
    ));
    for w in &snap.warnings {
        log(&format!("warning: {w}"));
    }
    drop(snap);
    st.live.set(Arc::new(atlas));
    Ok(())
}

/// Returns the revision of the file on disk, for tests.
#[cfg(test)]
fn current(a: &Atlas) -> String {
    crate::mcp::atlas::revision(&a.config_text().unwrap().0)
}

#[cfg(test)]
mod tests {
    use super::super::tests::studio;
    use super::super::Mode;
    use super::*;
    use crate::mcp::atlas::tests::fixture;
    use crate::mcp::Live;

    /// Makes a body from JSON.
    fn args(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn config_gives_the_file_the_resolved_views_and_the_units() {
        let (_dir, a) = fixture();
        let s = studio(Live::of(Arc::new(a)), Mode::Serve);
        let (code, v) = config(&s);
        assert_eq!(code, StatusCode::OK, "{v}");
        assert_eq!(v["tour"][1]["open"]["name"], "Service.Buy");
        assert_eq!(
            v["resolved"]["tour"][1]["open"]["id"],
            "go:shop#Service.Buy"
        );
        assert_eq!(v["regions"][0]["id"], "selling");
        let shop = v["units"]
            .as_array()
            .unwrap()
            .iter()
            .find(|u| u["id"] == "go:pkg:shop")
            .unwrap();
        assert_eq!(shop["region"], "selling");
        assert_eq!(v["revision"].as_str().unwrap().len(), 16);
    }

    #[test]
    fn a_reordered_tour_saves_and_keeps_comments() {
        let (dir, a) = fixture();
        let a = Arc::new(a);
        let s = studio(Live::of(a.clone()), Mode::Serve);
        let (_, v) = config(&s);
        let mut steps = v["tour"].as_array().unwrap().clone();
        steps.reverse();
        steps.push(json!({"title": "Storage", "text": "Store keeps books.", "open": {"view": "unit", "id": "go:pkg:store"}}));
        let (code, out) = save_tour(&s, &args(json!({"revision": v["revision"], "tour": steps})));
        assert_eq!(code, StatusCode::OK, "{out}");
        assert_eq!(out["revision"], json!(current(&a)));
        let text = std::fs::read_to_string(dir.path().join("atlas.yaml")).unwrap();
        assert!(
            text.contains("  # The second step.\n  - title: Buying"),
            "{text}"
        );
        let doc = &a.snapshot().unwrap().doc;
        assert_eq!(doc["tour"][0]["title"], "Buying");
        assert_eq!(doc["tour"][2]["open"]["id"], "go:pkg:store");
        // The old revision no longer saves.
        let (code, out) = save_tour(&s, &args(json!({"revision": v["revision"], "tour": []})));
        assert_eq!(code, StatusCode::CONFLICT, "{out}");
    }

    #[test]
    fn a_step_that_points_at_nothing_is_refused_with_the_reason() {
        let (_dir, a) = fixture();
        let s = studio(Live::of(Arc::new(a)), Mode::Serve);
        let (_, v) = config(&s);
        let steps =
            json!([{"title": "Gone", "text": "x", "open": {"view": "symbol", "name": "Nope"}}]);
        let (code, out) = save_tour(&s, &args(json!({"revision": v["revision"], "tour": steps})));
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{out}");
        assert!(out["error"].as_str().unwrap().contains("Nope"), "{out}");
        let steps =
            json!([{"title": "Bad", "text": "x", "open": {"view": "overview"}, "extra": 1}]);
        let (code, out) = save_tour(&s, &args(json!({"revision": v["revision"], "tour": steps})));
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{out}");
        assert!(out["error"].as_str().unwrap().contains("schema"), "{out}");
    }

    #[test]
    fn regions_and_layout_save_together() {
        let (_dir, a) = fixture();
        let a = Arc::new(a);
        let s = studio(Live::of(a.clone()), Mode::Serve);
        let (_, v) = config(&s);
        let regions = json!([
            {"id": "all", "name": "All of it", "paths": ["**"], "description": "Everything."}
        ]);
        let (code, out) = save_regions(
            &s,
            &args(
                json!({"revision": v["revision"], "regions": regions, "layout": {"rows": [["all"]]}}),
            ),
        );
        assert_eq!(code, StatusCode::OK, "{out}");
        let doc = &a.snapshot().unwrap().doc;
        assert_eq!(doc["regions"].as_array().unwrap().len(), 1);
        assert_eq!(doc["layout"]["rows"], json!([["all"]]));
        // A layout that names a missing region fails the check.
        let rev = current(&a);
        let (code, out) = save_regions(
            &s,
            &args(json!({"revision": rev, "regions": regions, "layout": {"rows": [["ghost"]]}})),
        );
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{out}");
    }

    #[test]
    fn the_project_text_and_its_mark_change() {
        let (dir, a) = fixture();
        let s = studio(Live::of(Arc::new(a)), Mode::Serve);
        let (_, v) = config(&s);
        let (code, out) = save_project(
            &s,
            &args(
                json!({"revision": v["revision"], "name": "Book shop", "description": "Sells books."}),
            ),
        );
        assert_eq!(code, StatusCode::OK, "{out}");
        let text = std::fs::read_to_string(dir.path().join("atlas.yaml")).unwrap();
        assert!(
            text.contains("name: \"Book shop\"") || text.contains("name: Book shop"),
            "{text}"
        );
        assert!(text.contains("# The description follows."), "{text}");
        let (code, _) = save_project(&s, &args(json!({"revision": out["revision"], "name": " "})));
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn matching_uses_the_rules_of_regions() {
        let (_dir, a) = fixture();
        let s = studio(Live::of(Arc::new(a)), Mode::Serve);
        let (code, v) = matching(&s, &args(json!({"paths": ["sh*", "[bad"]})));
        assert_eq!(code, StatusCode::OK);
        assert_eq!(v["units"], json!(["go:pkg:shop"]));
        assert_eq!(v["errors"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn calls_without_a_map_say_so() {
        let s = studio(Live::empty(), Mode::Studio);
        let (code, v) = config(&s);
        assert_eq!(code, StatusCode::CONFLICT);
        assert!(v["error"].as_str().unwrap().contains("no project"));
    }
}
