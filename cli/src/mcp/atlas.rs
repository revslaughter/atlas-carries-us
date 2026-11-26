//! The live map that `acu serve` and `acu mcp` share: the current atlas.json,
//! where it came from, the folders that the read tools may see, and the one
//! path by which a write tool changes atlas.yaml.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::SystemTime;

use anyhow::{bail, Context, Result};
use serde_json::Value;

use super::read::Lookup;
use crate::assemble::{self, Index};
use crate::config;
use crate::AdapterRun;

/// One state of the map, with the lookup tables that the read tools use.
pub struct Snapshot {
    /// The checked atlas.json.
    pub doc: Value,
    /// The same document as JSON bytes, for the viewer.
    pub bytes: Vec<u8>,
    /// acu's own tables: IDs, units and names.
    pub index: Index,
    /// The read tools' tables: edges, members and entry points.
    pub lookup: Lookup,
    /// The warnings that acu reported when it assembled this state.
    pub warnings: Vec<String>,
}

impl Snapshot {
    /// Builds the tables over a checked document.
    pub fn new(doc: Value, warnings: Vec<String>) -> Snapshot {
        let bytes = serde_json::to_vec(&doc).unwrap_or_default();
        let index = Index::new(&doc);
        let lookup = Lookup::new(&doc);
        Snapshot {
            doc,
            bytes,
            index,
            lookup,
            warnings,
        }
    }
}

/// Where the map comes from.
enum Origin {
    /// A config: acu keeps the adapters' merged output and applies the
    /// config to it again after each write.
    Config {
        /// The canonical path of atlas.yaml; the only file a write changes.
        path: PathBuf,
        /// The adapters' output.
        run: AdapterRun,
    },
    /// A finished atlas.json, read again when its time stamp changes.
    File {
        /// The file.
        path: PathBuf,
        /// The time stamp of the copy in memory.
        stamp: Mutex<Option<SystemTime>>,
    },
}

/// The live map: its origin, its current state, and its limits.
pub struct Atlas {
    /// Where the map comes from.
    origin: Origin,
    /// The current state.
    current: RwLock<Arc<Snapshot>>,
    /// The canonical folders that the read tools may read files from.
    roots: Vec<PathBuf>,
    /// True when the write tools stay off.
    read_only: bool,
    /// Keeps two writes from editing atlas.yaml at once.
    writing: Mutex<()>,
}

/// The result of a write: what acu did, and the warnings of the new map.
pub struct Written {
    /// The file that acu wrote.
    pub path: PathBuf,
    /// The warnings that acu reported on the new map.
    pub warnings: Vec<String>,
    /// How acu edited the file: in place, or by writing a section again.
    pub how: &'static str,
}

impl Atlas {
    /// Opens a map from the adapters' output and a config; the result must
    /// pass the same check as `acu extract`.
    pub fn from_config(path: &Path, run: AdapterRun, read_only: bool) -> Result<Atlas> {
        let path = path
            .canonicalize()
            .with_context(|| format!("find {}", path.display()))?;
        let loaded = config::load(&path)?;
        let snap = assemble_checked(&run, &loaded.config)
            .map_err(|e| anyhow::anyhow!("{e}; fix atlas.yaml and run again"))?;
        let mut roots: Vec<PathBuf> = run
            .repo_roots
            .iter()
            .filter_map(|r| r.canonicalize().ok())
            .collect();
        roots.dedup();
        Ok(Atlas {
            origin: Origin::Config { path, run },
            current: RwLock::new(Arc::new(snap)),
            roots,
            read_only,
            writing: Mutex::new(()),
        })
    }

    /// Opens a finished atlas.json; source paths resolve against `root`, and
    /// the write tools stay off, because no config stands behind the file.
    pub fn from_file(path: &Path, root: &Path) -> Result<Atlas> {
        let (doc, stamp) = read_file(path)?;
        let root = root
            .canonicalize()
            .with_context(|| format!("find {}", root.display()))?;
        Ok(Atlas {
            origin: Origin::File {
                path: path.to_path_buf(),
                stamp: Mutex::new(stamp),
            },
            current: RwLock::new(Arc::new(Snapshot::new(doc, Vec::new()))),
            roots: vec![root],
            read_only: true,
            writing: Mutex::new(()),
        })
    }

    /// Reports whether the write tools work: a config stands behind the map
    /// and `--read-only` is off.
    pub fn writable(&self) -> bool {
        !self.read_only && matches!(self.origin, Origin::Config { .. })
    }

    /// Says why the write tools stay off, or returns `None` when they work.
    pub fn why_read_only(&self) -> Option<&'static str> {
        match self.origin {
            Origin::File { .. } => Some(
                "acu serves a finished atlas.json, so no config stands behind it; start acu with -c atlas.yaml to draft text",
            ),
            Origin::Config { .. } if self.read_only => {
                Some("acu runs with --read-only; start it without the flag to draft text")
            }
            Origin::Config { .. } => None,
        }
    }

    /// The canonical folders that the read tools may read from.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Returns the current state. For a finished atlas.json, it reads the
    /// file again first when the file has changed.
    pub fn snapshot(&self) -> Result<Arc<Snapshot>, String> {
        if let Origin::File { path, stamp } = &self.origin {
            let now = std::fs::metadata(path)
                .and_then(|m| m.modified())
                .map_err(|e| format!("read {}: {e}", path.display()))?;
            let mut held = stamp.lock().map_err(|_| "a lock broke".to_string())?;
            if *held != Some(now) {
                let (doc, t) = read_file(path).map_err(|e| format!("{e:#}"))?;
                *self
                    .current
                    .write()
                    .map_err(|_| "a lock broke".to_string())? =
                    Arc::new(Snapshot::new(doc, Vec::new()));
                *held = t;
            }
        }
        Ok(self
            .current
            .read()
            .map_err(|_| "a lock broke".to_string())?
            .clone())
    }

    /// Changes atlas.yaml through `edit`, which gets the file's text and
    /// returns the new text. The new config must pass the config schema and
    /// the same check as `acu extract`; else acu writes nothing and returns
    /// the problems. On success the served map shows the change at once.
    pub fn write(
        &self,
        edit: impl FnOnce(&str) -> Result<(String, &'static str), String>,
    ) -> Result<Written, String> {
        if let Some(why) = self.why_read_only() {
            return Err(why.to_string());
        }
        let Origin::Config { path, run } = &self.origin else {
            return Err("no config stands behind this map".into());
        };
        let _guard = self
            .writing
            .lock()
            .map_err(|_| "a lock broke".to_string())?;
        // Write only the config file itself, and only inside its own folder.
        let target = path
            .canonicalize()
            .map_err(|e| format!("find {}: {e}", path.display()))?;
        if &target != path {
            return Err(format!(
                "{} now points somewhere else; acu writes only the atlas.yaml it started with",
                path.display()
            ));
        }
        let text = std::fs::read_to_string(&target)
            .map_err(|e| format!("read {}: {e}", target.display()))?;
        let (new_text, how) = edit(&text)?;
        let value: Value = serde_yaml_ng::from_str(&new_text)
            .map_err(|e| format!("the new atlas.yaml does not parse: {e}"))?;
        let problems =
            config::validate(&value, config::CONFIG_SCHEMA).map_err(|e| e.to_string())?;
        if !problems.is_empty() {
            return Err(format!(
                "the draft breaks the config schema, so acu wrote nothing: {}",
                problems.join("; ")
            ));
        }
        let cfg: config::Config = serde_json::from_value(value)
            .map_err(|e| format!("the draft breaks the config, so acu wrote nothing: {e}"))?;
        let snap = assemble_checked(run, &cfg)
            .map_err(|e| format!("the draft breaks the check, so acu wrote nothing: {e}"))?;
        write_atomic(&target, &new_text)?;
        let warnings = snap.warnings.clone();
        *self
            .current
            .write()
            .map_err(|_| "a lock broke".to_string())? = Arc::new(snap);
        Ok(Written {
            path: target,
            warnings,
            how,
        })
    }
}

/// Applies a config to the adapters' output and runs the check of
/// `acu extract`; any error comes back as one line.
fn assemble_checked(run: &AdapterRun, cfg: &config::Config) -> Result<Snapshot, String> {
    let (doc, mut rep) = crate::apply_config(run, cfg);
    let check = assemble::check(&doc);
    rep.errors.extend(check.errors);
    rep.warnings.extend(check.warnings);
    if !rep.errors.is_empty() {
        return Err(format!(
            "{} problems: {}",
            rep.errors.len(),
            rep.errors.join("; ")
        ));
    }
    Ok(Snapshot::new(doc, rep.warnings))
}

/// Reads and parses an atlas.json, with its time stamp.
fn read_file(path: &Path) -> Result<(Value, Option<SystemTime>)> {
    let stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let doc: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("{} holds JSON that does not parse", path.display()))?;
    if doc.get("version").and_then(Value::as_u64) != Some(1) {
        bail!("{} holds no atlas.json of version 1", path.display());
    }
    Ok((doc, stamp))
}

/// Writes a file through a temporary file beside it, so a reader never sees
/// half a file.
fn write_atomic(target: &Path, text: &str) -> Result<(), String> {
    let dir = target.parent().unwrap_or(Path::new("."));
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "atlas.yaml".into());
    let tmp = dir.join(format!(".{name}.acu-draft"));
    std::fs::write(&tmp, text).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    if let Ok(meta) = std::fs::metadata(target) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("replace {}: {e}", target.display())
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;

    /// A small invented document: two Go-like units, a type with methods,
    /// an interface, calls and one entry point.
    pub fn fixture_doc() -> Value {
        json!({
            "version": 1,
            "meta": { "schemaVersion": 1, "tool": {"name": "t", "version": "0"}, "generated": "2000-01-01T00:00:00Z",
                      "languages": ["go"], "adapters": [{"name": "go", "version": "0", "language": "go", "root": ""}] },
            "containers": [
                {"id": "go:pkg:shop", "kind": "package", "languageKind": "package", "name": "shop", "path": "shop", "language": "go", "unit": true,
                 "doc": {"raw": "Package shop sells books.", "format": "godoc", "summary": "Package shop sells books.", "description": []}},
                {"id": "go:pkg:store", "kind": "package", "languageKind": "package", "name": "store", "path": "store", "language": "go", "unit": true},
                {"id": "go:file:shop/shop.go", "kind": "file", "languageKind": "file", "name": "shop.go", "path": "shop/shop.go", "parent": "go:pkg:shop"},
                {"id": "go:file:store/store.go", "kind": "file", "languageKind": "file", "name": "store.go", "path": "store/store.go", "parent": "go:pkg:store"}
            ],
            "symbols": [
                {"id": "go:shop#Service", "kind": "struct", "languageKind": "struct", "name": "Service", "parent": "go:file:shop/shop.go", "exported": true,
                 "location": {"path": "shop/shop.go", "line": 3, "endLine": 5},
                 "doc": {"raw": "Service sells books.", "format": "godoc", "summary": "Service sells books.", "description": [{"type": "paragraph", "text": "Service sells books from a \\`Store\\`."}]}},
                {"id": "go:shop#Service.Buy", "kind": "method", "languageKind": "method", "name": "Buy", "parent": "go:shop#Service", "exported": true,
                 "location": {"path": "shop/shop.go", "line": 7, "endLine": 9}, "signature": {"params": [], "results": [], "text": "func (s *Service) Buy(id string) error"},
                 "doc": {"raw": "Buy sells one book.", "format": "godoc", "summary": "Buy sells one book.", "description": []}},
                {"id": "go:shop#Run", "kind": "function", "languageKind": "func", "name": "Run", "parent": "go:file:shop/shop.go", "exported": true,
                 "location": {"path": "shop/shop.go", "line": 11, "endLine": 13}},
                {"id": "go:store#Store", "kind": "interface", "languageKind": "interface", "name": "Store", "parent": "go:file:store/store.go", "exported": true,
                 "location": {"path": "store/store.go", "line": 1, "endLine": 3}},
                {"id": "go:store#Store.Get", "kind": "method", "languageKind": "method", "name": "Get", "parent": "go:store#Store", "exported": true,
                 "location": {"path": "store/store.go", "line": 2}},
                {"id": "go:store#Mem", "kind": "struct", "languageKind": "struct", "name": "Mem", "parent": "go:file:store/store.go", "exported": true,
                 "location": {"path": "store/store.go", "line": 5}},
                {"id": "go:store#Mem.Get", "kind": "method", "languageKind": "method", "name": "Get", "parent": "go:store#Mem", "exported": true,
                 "location": {"path": "store/store.go", "line": 6}}
            ],
            "edges": [
                {"from": "go:shop#Run", "to": "go:shop#Service.Buy", "kind": "calls", "confidence": "exact", "line": 12},
                {"from": "go:shop#Service.Buy", "to": "go:store#Store.Get", "kind": "calls", "confidence": "exact", "line": 8},
                {"from": "go:store#Mem", "to": "go:store#Store", "kind": "implements", "confidence": "exact"},
                {"from": "go:shop#Service.Buy", "to": "go:store#Store", "kind": "param_type", "confidence": "exact"}
            ],
            "entryPoints": [
                {"id": "http:POST /buy", "kind": "http", "name": "POST /buy", "method": "POST", "path": "/buy", "handler": "go:shop#Service.Buy",
                 "registeredAt": {"path": "shop/shop.go", "line": 12}, "plugin": "go-nethttp"}
            ],
            "tests": {"go:shop#Service.Buy": [{"name": "TestBuy", "location": {"path": "shop/shop_test.go", "line": 4}}]}
        })
    }

    /// The config that goes with the fixture, with comments that a write
    /// must keep.
    pub const FIXTURE_CONFIG: &str = "# The shop example. Keep this comment.\n\
version: 1\n\
project:\n  name: Shop\n  # The description follows.\n  description: |\n    A shop that sells books.\n\
languages:\n  - adapter: go\n\
regions:\n  # The selling side.\n  - id: selling\n    name: Selling\n    paths: [shop]\n    color: 1\n  - id: storage\n    name: Storage\n    paths: [store]\n\
tour:\n  - title: Start\n    open: { view: overview }\n    text: Two units.\n  # The second step.\n  - title: Buying\n    open: { view: symbol, name: Service.Buy }\n    text: Buy sells a book.\n";

    /// Writes the fixture's config and source into a temporary folder and
    /// opens a writable map on it.
    pub fn fixture() -> (tempfile::TempDir, Atlas) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("atlas.yaml"), FIXTURE_CONFIG).unwrap();
        std::fs::create_dir_all(dir.path().join("shop")).unwrap();
        std::fs::write(
            dir.path().join("shop/shop.go"),
            "package shop\n\n// Service sells books.\ntype Service struct{}\n\n// Buy sells one book.\nfunc (s *Service) Buy(id string) error {\n\treturn nil\n}\n\nfunc Run() {\n\t(&Service{}).Buy(\"1\")\n}\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("outside.txt"), "secret\n").unwrap();
        let run = AdapterRun {
            merged: fixture_doc(),
            commit: None,
            repo_roots: vec![dir.path().to_path_buf()],
        };
        let atlas = Atlas::from_config(&dir.path().join("atlas.yaml"), run, false).unwrap();
        (dir, atlas)
    }

    #[test]
    fn a_broken_write_changes_nothing() {
        let (dir, atlas) = fixture();
        let before = std::fs::read_to_string(dir.path().join("atlas.yaml")).unwrap();
        let err = atlas
            .write(|t| Ok((t.replace("name: Service.Buy", "name: Nothing.Here"), "test")))
            .err()
            .unwrap();
        assert!(err.contains("breaks the check"), "{err}");
        let after = std::fs::read_to_string(dir.path().join("atlas.yaml")).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn read_only_refuses_writes() {
        let (dir, _) = fixture();
        let run = AdapterRun {
            merged: fixture_doc(),
            commit: None,
            repo_roots: vec![dir.path().to_path_buf()],
        };
        let atlas = Atlas::from_config(&dir.path().join("atlas.yaml"), run, true).unwrap();
        assert!(!atlas.writable());
        let err = atlas.write(|t| Ok((t.to_string(), "test"))).err().unwrap();
        assert!(err.contains("--read-only"), "{err}");
    }
}
