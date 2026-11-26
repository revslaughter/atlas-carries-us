//! The write tools of the MCP server: draft a region, the project's
//! description, or a tour step. Each draft lands in atlas.yaml with a
//! `drafted` mark (who and when), and only after the new config passes the
//! check of `acu extract`. A person removes the mark after checking the text.

use serde_json::{json, Map, Value};

use super::atlas::{Atlas, Written};
use super::yaml::{self, Change};

/// Makes the provenance mark: who drafted the text, and when (RFC 3339).
pub fn mark(by: &str) -> Value {
    json!({
        "by": by,
        "at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    })
}

/// Says what a write did, for the model to read.
fn report(what: &str, by: &str, w: &Written) -> String {
    let mut out = format!(
        "Wrote {what} to {} ({}), marked as drafted by {by}. The served map shows it now; a person removes the drafted mark after checking the text.",
        w.path.display(),
        w.how
    );
    for warn in &w.warnings {
        out.push_str(&format!("\nwarning: {warn}"));
    }
    out
}

/// The parameters of `draft_region`.
pub struct RegionDraft<'a> {
    /// The region's ID; an existing ID changes that region.
    pub id: &'a str,
    /// The region's name.
    pub name: &'a str,
    /// What the region does.
    pub description: &'a str,
    /// How to read the region.
    pub reading_tip: &'a str,
    /// Globs of unit paths; empty keeps the region's paths.
    pub paths: Vec<String>,
}

/// `draft_region`: adds a region, or changes the name, text or paths of an
/// existing one, and marks it as drafted.
pub fn draft_region(atlas: &Atlas, d: RegionDraft<'_>, by: &str) -> Result<String, String> {
    let id = d.id.trim();
    if id.is_empty() {
        return Err("acu: give the region's id, such as storage".into());
    }
    let w = atlas.write(|text| {
        let v = yaml::parse(text)?;
        let regions = v["regions"].as_array().cloned().unwrap_or_default();
        let at = regions.iter().position(|r| r["id"] == id);
        let mut r: Map<String, Value> = match at {
            Some(i) => regions[i].as_object().cloned().unwrap_or_default(),
            None => {
                if d.name.trim().is_empty() || d.paths.is_empty() {
                    return Err(format!(
                        "acu: no region has the id {id}; a new region needs name and paths"
                    ));
                }
                let mut m = Map::new();
                m.insert("id".into(), json!(id));
                m
            }
        };
        if !d.name.trim().is_empty() {
            r.insert("name".into(), json!(d.name.trim()));
        }
        if !d.paths.is_empty() {
            r.insert("paths".into(), json!(d.paths));
        }
        if !d.description.trim().is_empty() {
            r.insert("description".into(), json!(d.description.trim()));
        }
        if !d.reading_tip.trim().is_empty() {
            r.insert("readingTip".into(), json!(d.reading_tip.trim()));
        }
        r.insert("drafted".into(), mark(by));
        yaml::edit(
            text,
            &Change::Item {
                key: "regions",
                at: at.unwrap_or(regions.len()),
                value: Value::Object(r),
                insert: at.is_none(),
            },
        )
    })?;
    Ok(report(&format!("region {id}"), by, &w))
}

/// `draft_description`: sets the project's overview text and marks it as
/// drafted.
pub fn draft_description(atlas: &Atlas, text: &str, by: &str) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("acu: give the description's text".into());
    }
    let w = atlas.write(|file| {
        yaml::edit(
            file,
            &Change::Keys {
                key: "project",
                values: vec![("description", json!(t)), ("descriptionDrafted", mark(by))],
            },
        )
    })?;
    Ok(report("the project description", by, &w))
}

/// The parameters of `draft_tour_step`.
pub struct StepDraft<'a> {
    /// The step's title.
    pub title: &'a str,
    /// The step's text.
    pub text: &'a str,
    /// The view: overview, region, unit, symbol or trace.
    pub view: &'a str,
    /// The view's target: a region ID, a unit path or ID, a symbol name or
    /// ID, or an entry point.
    pub target: &'a str,
    /// The step number to replace, or 0.
    pub replace: usize,
    /// The step number to insert before, or 0 to add the step at the end.
    pub position: usize,
}

/// `draft_tour_step`: adds a tour step, or replaces one, and marks it as
/// drafted. The target must resolve, as `acu extract` demands.
pub fn draft_tour_step(atlas: &Atlas, d: StepDraft<'_>, by: &str) -> Result<String, String> {
    if d.title.trim().is_empty() || d.text.trim().is_empty() {
        return Err("acu: a tour step needs a title and a text".into());
    }
    let open = open_for(atlas, d.view.trim(), d.target.trim())?;
    let mut w_what = String::new();
    let w = atlas.write(|text| {
        let v = yaml::parse(text)?;
        let n = v["tour"].as_array().map_or(0, Vec::len);
        let (at, insert) = if d.replace > 0 {
            if d.replace > n {
                return Err(format!(
                    "acu: the tour has {n} steps; replace one from 1 to {n}"
                ));
            }
            (d.replace - 1, false)
        } else if d.position > 0 {
            if d.position > n + 1 {
                return Err(format!(
                    "acu: the tour has {n} steps; insert at a position from 1 to {}",
                    n + 1
                ));
            }
            (d.position - 1, true)
        } else {
            (n, true)
        };
        w_what = format!("tour step {}", at + 1);
        let step = json!({
            "title": d.title.trim(),
            "open": open,
            "text": d.text.trim(),
            "drafted": mark(by),
        });
        yaml::edit(
            text,
            &Change::Item {
                key: "tour",
                at,
                value: step,
                insert,
            },
        )
    })?;
    Ok(report(&w_what, by, &w))
}

/// Builds a step's `open` from a view and one target, picking the field
/// that the target fills: an exact ID where one matches, else a name or a
/// path.
fn open_for(atlas: &Atlas, view: &str, target: &str) -> Result<Value, String> {
    let snap = atlas.snapshot()?;
    let need = |what: &str| {
        if target.is_empty() {
            Err(format!("acu: a {view} step needs a target: {what}"))
        } else {
            Ok(())
        }
    };
    let doc = &snap.doc;
    let is_id = |key: &str, list: &str| {
        doc[list]
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x[key] == target))
    };
    match view {
        "overview" | "" => Ok(json!({ "view": "overview" })),
        "region" => {
            need("a region id")?;
            Ok(json!({ "view": "region", "id": target }))
        }
        "unit" => {
            need("a unit's path or ID")?;
            if snap.index.units.iter().any(|u| u.0 == target) {
                Ok(json!({ "view": "unit", "id": target }))
            } else {
                Ok(json!({ "view": "unit", "path": target }))
            }
        }
        "symbol" => {
            need("a symbol's name or ID")?;
            if is_id("id", "symbols") {
                Ok(json!({ "view": "symbol", "id": target }))
            } else {
                Ok(json!({ "view": "symbol", "name": target }))
            }
        }
        "trace" => {
            need("an entry point, or a function")?;
            if is_id("id", "entryPoints") || is_id("name", "entryPoints") {
                Ok(json!({ "view": "trace", "entryPoint": target }))
            } else {
                Ok(json!({ "view": "trace", "symbol": target }))
            }
        }
        v => Err(format!(
            "acu: the view {v} means nothing here; use overview, region, unit, symbol or trace"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::super::atlas::tests::fixture;
    use super::*;

    #[test]
    fn a_drafted_step_lands_in_the_config_and_the_map() {
        let (dir, a) = fixture();
        let out = draft_tour_step(
            &a,
            StepDraft {
                title: "Storage",
                text: "Store hides where books live.",
                view: "symbol",
                target: "store.Store",
                replace: 0,
                position: 2,
            },
            "a test model",
        )
        .unwrap();
        assert!(
            out.contains("tour step 2") && out.contains("in place"),
            "{out}"
        );
        let text = std::fs::read_to_string(dir.path().join("atlas.yaml")).unwrap();
        assert!(
            text.starts_with("# The shop example. Keep this comment.\n"),
            "{text}"
        );
        assert!(
            text.contains("  # The second step.\n  - title: Buying"),
            "{text}"
        );
        assert!(
            text.contains("drafted: { by: \"a test model\", at: \"20"),
            "{text}"
        );
        let snap = a.snapshot().unwrap();
        let step = &snap.doc["tour"][1];
        assert_eq!(step["title"], "Storage");
        assert_eq!(
            step["open"],
            json!({"view": "symbol", "id": "go:store#Store"})
        );
        assert_eq!(step["drafted"]["by"], "a test model");
        assert_eq!(snap.doc["tour"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn a_step_whose_target_resolves_to_nothing_is_refused() {
        let (dir, a) = fixture();
        let before = std::fs::read_to_string(dir.path().join("atlas.yaml")).unwrap();
        let err = draft_tour_step(
            &a,
            StepDraft {
                title: "Gone",
                text: "Nothing.",
                view: "symbol",
                target: "Nothing.Here",
                replace: 0,
                position: 0,
            },
            "m",
        )
        .unwrap_err();
        assert!(
            err.contains("no symbol has the ID or name Nothing.Here"),
            "{err}"
        );
        assert_eq!(
            before,
            std::fs::read_to_string(dir.path().join("atlas.yaml")).unwrap()
        );
    }

    #[test]
    fn steps_replace_and_trace_from_entry_points() {
        let (_d, a) = fixture();
        draft_tour_step(
            &a,
            StepDraft {
                title: "Follow a purchase",
                text: "The handler asks the store.",
                view: "trace",
                target: "POST /buy",
                replace: 1,
                position: 0,
            },
            "m",
        )
        .unwrap();
        let snap = a.snapshot().unwrap();
        assert_eq!(
            snap.doc["tour"][0]["open"],
            json!({"view": "trace", "entryPoint": "http:POST /buy"})
        );
        assert_eq!(snap.doc["tour"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn regions_change_or_join_with_a_mark() {
        let (dir, a) = fixture();
        draft_region(
            &a,
            RegionDraft {
                id: "storage",
                name: "",
                description: "The store keeps books.",
                reading_tip: "Read Store first.",
                paths: vec![],
            },
            "m",
        )
        .unwrap();
        let snap = a.snapshot().unwrap();
        let r = &snap.doc["regions"][1];
        assert_eq!(r["name"], "Storage");
        assert_eq!(r["description"], "The store keeps books.");
        assert_eq!(r["drafted"]["by"], "m");
        let text = std::fs::read_to_string(dir.path().join("atlas.yaml")).unwrap();
        assert!(text.contains("  # The selling side.\n  - id: selling\n    name: Selling\n    paths: [shop]\n    color: 1\n"), "{text}");
        let err = draft_region(
            &a,
            RegionDraft {
                id: "new",
                name: "",
                description: "x",
                reading_tip: "",
                paths: vec![],
            },
            "m",
        )
        .unwrap_err();
        assert!(err.contains("needs name and paths"), "{err}");
    }

    #[test]
    fn the_description_gets_a_mark() {
        let (dir, a) = fixture();
        draft_description(&a, "A shop.\n\nIt sells books.", "m").unwrap();
        let snap = a.snapshot().unwrap();
        assert_eq!(
            snap.doc["project"]["description"],
            "A shop.\n\nIt sells books."
        );
        assert_eq!(snap.doc["project"]["descriptionDrafted"]["by"], "m");
        let text = std::fs::read_to_string(dir.path().join("atlas.yaml")).unwrap();
        assert!(
            text.contains("  # The description follows.\n  description: |-\n"),
            "{text}"
        );
    }
}
