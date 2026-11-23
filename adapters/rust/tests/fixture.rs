//! Runs the adapter on the invented lending library in `testdata/fixture` and
//! checks its output three ways: byte for byte against the golden file,
//! against `schema/atlas.schema.json`, and against named facts that read as
//! sentences.
//!
//! The run needs the pinned nightly toolchain. Without it, the tests say so
//! and pass, except under CI (`CI` set), where they fail. After a deliberate
//! change, rewrite the golden file with
//! `UPDATE_GOLDEN=1 cargo test -p acu-adapter-rust`, and read the diff.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use acu_adapter_rust::extract;
use acu_adapter_rust::request::{PluginConfig, Request};
use acu_adapter_rust::toolchain::NIGHTLY;
use serde_json::{json, Value};

/// The adapter's folder.
fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Reports whether the pinned nightly, with rust-analyzer, is installed.
fn toolchain_ready() -> bool {
    let ok = |args: &[&str]| {
        Command::new("rustup")
            .args(["run", NIGHTLY])
            .args(args)
            .output()
            .is_ok_and(|o| o.status.success())
    };
    ok(&["rustdoc", "--version"]) && ok(&["rust-analyzer", "--version"])
}

/// Runs the adapter on the fixture once, and keeps the document as JSON and
/// as text. Returns `None` when the toolchain is missing outside CI.
fn fixture() -> Option<&'static (Value, String)> {
    static RUN: OnceLock<Option<(Value, String)>> = OnceLock::new();
    RUN.get_or_init(|| {
        if !toolchain_ready() {
            assert!(
                std::env::var_os("CI").is_none(),
                "the toolchain {NIGHTLY} with rust-analyzer is missing; the CI image must install it"
            );
            eprintln!("fixture: the toolchain {NIGHTLY} is missing, so these tests check nothing; install it to run them");
            return None;
        }
        let root = here().join("testdata/fixture");
        let req = Request {
            protocol: 1,
            root: root.to_string_lossy().into_owned(),
            entry_points: vec![
                PluginConfig {
                    plugin: "axum".into(),
                    options: json!({ "prefixes": [] }),
                },
                PluginConfig {
                    plugin: "clap".into(),
                    options: Value::Null,
                },
            ],
            ..Request::default()
        };
        let mut doc = extract::run(req).expect("the adapter reads the fixture");
        doc.meta.generated = "2000-01-01T00:00:00Z".into();
        let text = serde_json::to_string_pretty(&doc).unwrap() + "\n";
        Some((serde_json::from_str(&text).unwrap(), text))
    })
    .as_ref()
}

#[test]
fn output_matches_the_golden_file() {
    let Some((_, got)) = fixture() else { return };
    let golden = here().join("testdata/golden/fixture.json");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&golden, got).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&golden).unwrap_or_else(|e| {
        panic!(
            "{e}: run UPDATE_GOLDEN=1 cargo test -p acu-adapter-rust to create {}",
            golden.display()
        )
    });
    if got != &want {
        let (g, w): (Vec<&str>, Vec<&str>) = (got.lines().collect(), want.lines().collect());
        for (i, (a, b)) in g.iter().zip(&w).enumerate() {
            assert_eq!(
                a,
                b,
                "the output differs from {} at line {} (run UPDATE_GOLDEN=1 cargo test -p acu-adapter-rust after a deliberate change)",
                golden.display(),
                i + 1
            );
        }
        panic!(
            "the output differs from {} in length: {} lines, want {}",
            golden.display(),
            g.len(),
            w.len()
        );
    }
}

#[test]
fn output_follows_the_schema() {
    let Some((doc, _)) = fixture() else { return };
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/atlas.schema.json"),
    )
    .unwrap();
    let schema: Value = serde_json::from_str(&text).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(doc)
        .map(|e| format!("{}: {e}", e.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "the output breaks the schema:\n{}",
        errors.join("\n")
    );
}

/// Finds one symbol by ID.
fn symbol<'a>(doc: &'a Value, id: &str) -> &'a Value {
    doc["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == id)
        .unwrap_or_else(|| panic!("the map has no symbol {id}"))
}

/// Reports whether the map holds one edge.
fn has_edge(doc: &Value, from: &str, to: &str, kind: &str) -> bool {
    doc["edges"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["from"] == from && e["to"] == to && e["kind"] == kind)
}

#[test]
fn named_facts_hold() {
    let Some((doc, _)) = fixture() else { return };
    let edges = [
        (
            "rs:lending::library#Library.borrow",
            "rs:lending::rules#may_borrow",
            "calls",
        ),
        (
            "rs:lending::library#Library.borrow",
            "rs:lending::store#Store.find",
            "calls",
        ),
        ("rs:lendd#main", "rs:lendd#serve", "calls"),
        (
            "rs:lendd#import",
            "rs:lending::library#Library.new",
            "calls",
        ),
        (
            "rs:lending::store::memory#MemoryStore",
            "rs:lending::store#Store",
            "implements",
        ),
        (
            "rs:lending::store#impl Shelved for Vec<Book>",
            "rs:lending::store#Shelved",
            "implements",
        ),
        (
            "rs:lending::store#Store",
            "rs:lending::store#Counted",
            "extends",
        ),
        (
            "rs:lending::domain#Book.id",
            "rs:lending::domain#BookId",
            "field_type",
        ),
        (
            "rs:lending::domain#LoanError.NotFound",
            "rs:lending::domain#BookId",
            "field_type",
        ),
        (
            "rs:lending::library#Library.borrow",
            "rs:lending::domain#Member",
            "param_type",
        ),
        (
            "rs:lendd::http#list_books",
            "rs:lending::domain#Book",
            "return_type",
        ),
        ("rs:mod:lendd::http", "rs:mod:lending::domain", "imports"),
    ];
    for (from, to, kind) in edges {
        assert!(
            has_edge(doc, from, to, kind),
            "{from} should have a {kind} edge to {to}"
        );
    }

    let book = symbol(doc, "rs:lending::domain#Book");
    assert_eq!(book["languageKind"], "struct");
    assert_eq!(book["annotations"][0], "#[derive(Clone, Debug)]");
    assert_eq!(book["satisfies"], json!(["Clone", "Debug", "Display"]));
    assert_eq!(
        symbol(doc, "rs:lending::domain#Book.Display::fmt")["name"],
        "fmt"
    );
    assert_eq!(
        symbol(doc, "rs:lending::domain#LoanError.OnLoan")["type"]["text"],
        "{ book: BookId, due: u32 }"
    );
    assert_eq!(
        symbol(doc, "rs:lending::library#Library.store")["languageKind"],
        "method"
    );
    assert_eq!(
        symbol(doc, "rs:lending::rules#VERSION")["parent"],
        "rs:mod:lending::rules"
    );
    assert_eq!(
        symbol(doc, "rs:lending_macros#Describe")["languageKind"],
        "derive macro"
    );
    assert_eq!(
        symbol(doc, "rs:lending::store::memory#MemoryStore.Store::find")["doc"]["summary"],
        "Finds a book by its identity.",
        "a trait implementation's member without a doc takes the trait member's doc"
    );
    assert_eq!(
        symbol(doc, "rs:lending#rules_version")["doc"]["deprecated"],
        "read MAX_LOAN_DAYS instead"
    );
    let lib_doc = &symbol(doc, "rs:lending::library#Library")["doc"]["summary"];
    assert_eq!(
        lib_doc, "Lends books from a store, by the [lending rules](sym:rs:mod:lending::rules).",
        "an intra-doc link to a module becomes a link to its container"
    );
    assert!(
        doc["containers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "rs:crate:lending@bin"),
        "a binary whose name a library takes gains @bin"
    );

    let entries: Vec<(String, String)> = doc["entryPoints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["id"].as_str().unwrap().to_string(),
                e["handler"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let want = [
        ("http:GET /books", "rs:lendd::http#list_books"),
        ("http:POST /books", "rs:lendd::http#add_book"),
        ("http:DELETE /books/{id}", "rs:lendd::http#remove_book"),
        ("http:ANY /api/search", "rs:lendd::http#search"),
        ("http:POST /admin/reindex", "rs:lendd::http#reindex"),
        ("http:GET /admin/ping", "rs:lendd::http#router"),
        ("http:GET /health", "rs:lendd::http#health"),
        ("cli:lendd serve", "rs:lendd#serve"),
        ("cli:lendd import-books", "rs:lendd#import"),
        ("cli:lendd report count", "rs:lendd#report_count"),
        ("cli:lendd report overdue", "rs:lendd#main"),
    ];
    for (id, handler) in want {
        assert!(
            entries.iter().any(|(i, h)| i == id && h == handler),
            "the entry point {id} should have the handler {handler}; found {entries:?}"
        );
    }

    let tests = &doc["tests"]["rs:lending::store::memory#MemoryStore.new"];
    assert_eq!(tests[0]["name"], "store::memory::tests::finds_a_saved_book");
}
