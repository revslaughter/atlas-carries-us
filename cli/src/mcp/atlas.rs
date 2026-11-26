//! The live map that `acu serve`, `acu studio` and `acu mcp` share: the
//! current atlas.json, where it came from, the folders that the read tools
//! may see, and the one path by which a write tool or the editor changes
//! atlas.yaml. [`Live`] holds the map that a server shows now; the studio
//! swaps in a new one after each extraction.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::SystemTime;

use anyhow::{bail, Context, Result};
use serde_json::Value;

use super::read::{Lookup, Sources};
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
    /// The files that the read tools may read.
    sources: Sources,
    /// Why the write tools stay off, or `None` when the command line lets
    /// them work.
    read_only: Option<&'static str>,
    /// Keeps two writes of this process from editing atlas.yaml at once.
    /// Another process that writes the file at the same time can still
    /// overwrite a draft; the last rename wins.
    writing: Mutex<()>,
}

/// The slot that holds the map a server shows now. `acu serve` and `acu mcp`
/// fill it once; `acu studio` starts with it empty and fills it again after
/// each extraction, so every request reads the newest map.
pub struct Live {
    /// The map on show, if any.
    slot: RwLock<Option<Arc<Atlas>>>,
}

/// What a request gets while the studio holds no map yet.
pub const NO_PROJECT: &str =
    "acu: the studio holds no project yet; open one in the browser, then call again";

impl Live {
    /// Makes a slot that holds one map.
    pub fn of(atlas: Arc<Atlas>) -> Live {
        Live {
            slot: RwLock::new(Some(atlas)),
        }
    }

    /// Makes an empty slot.
    pub fn empty() -> Live {
        Live {
            slot: RwLock::new(None),
        }
    }

    /// Returns the map on show, or the reason none shows.
    pub fn get(&self) -> Result<Arc<Atlas>, String> {
        self.slot
            .read()
            .map_err(|_| format!("acu: {BROKEN}"))?
            .clone()
            .ok_or_else(|| NO_PROJECT.to_string())
    }

    /// Puts a new map on show.
    pub fn set(&self, atlas: Arc<Atlas>) {
        // A lock that a panic broke still holds a usable value; replace it.
        match self.slot.write() {
            Ok(mut g) => *g = Some(atlas),
            Err(p) => *p.into_inner() = Some(atlas),
        }
    }
}

/// Names one state of atlas.yaml: a hash of its text. The editor sends it
/// back with each save, so acu can refuse a save that would overwrite an
/// edit made meanwhile in another window or in a text editor.
pub fn revision(text: &str) -> String {
    // FNV-1a, 64 bits: stable across runs and platforms.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Why a save failed: the file changed since the editor read it, or the
/// change itself fails.
#[derive(Debug)]
pub enum SaveError {
    /// atlas.yaml changed since the given revision.
    Stale(String),
    /// The change fails the schema, the check or the file system.
    Failed(String),
}

/// The result of a write: what acu did, and the warnings of the new map.
pub struct Written {
    /// The file that acu wrote.
    pub path: PathBuf,
    /// The warnings that acu reported on the new map.
    pub warnings: Vec<String>,
    /// How acu edited the file: in place, or by writing a section again.
    pub how: &'static str,
    /// The revision of the text that acu wrote, taken while acu still held
    /// the file, so no later writer can slip in between.
    pub revision: String,
}

/// Why the write tools stay off under `--read-only`.
pub const READ_ONLY: &str = "acu runs with --read-only; start it without the flag to draft text";

/// Why the write tools stay off when `acu serve` listens on an address that
/// other computers can reach.
pub const REMOTE: &str = "acu listens on an address that other computers can reach; start it with --allow-remote-writes to draft text";

/// The message of a lock that a panic in another thread left broken.
const BROKEN: &str = "a lock broke after an earlier failure; restart acu";

impl Atlas {
    /// Opens a map from the adapters' output and a config; the result must
    /// pass the same check as `acu extract`. `read_only` says why the write
    /// tools stay off, or holds `None` to let them work.
    pub fn from_config(
        path: &Path,
        run: AdapterRun,
        read_only: Option<&'static str>,
    ) -> Result<Atlas> {
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
        roots.sort();
        roots.dedup();
        Ok(Atlas {
            origin: Origin::Config { path, run },
            current: RwLock::new(Arc::new(snap)),
            sources: Sources::new(roots, None),
            read_only,
            writing: Mutex::new(()),
        })
    }

    /// Opens a finished atlas.json. Source paths resolve against `root`:
    /// the folder that `--root` names, else the current folder, which
    /// `root_given` false marks so that a failed read can say how to pass
    /// `--root`. The write tools stay off, because no config stands behind
    /// the file.
    pub fn from_file(path: &Path, root: &Path, root_given: bool) -> Result<Atlas> {
        let (doc, stamp) = read_file(path)?;
        let root = root
            .canonicalize()
            .with_context(|| format!("find {}", root.display()))?;
        let hint = (!root_given).then(|| {
            format!(
                "acu serves a finished atlas.json and reads files from the current folder, {}; start it with --root and the folder of the mapped repository",
                root.display()
            )
        });
        Ok(Atlas {
            origin: Origin::File {
                path: path.to_path_buf(),
                stamp: Mutex::new(stamp),
            },
            current: RwLock::new(Arc::new(Snapshot::new(doc, Vec::new()))),
            sources: Sources::new(vec![root], hint),
            read_only: Some(
                "acu serves a finished atlas.json, so no config stands behind it; start acu with -c atlas.yaml to draft text",
            ),
            writing: Mutex::new(()),
        })
    }

    /// The canonical path of atlas.yaml, when a config stands behind the
    /// map.
    pub fn config_path(&self) -> Option<&Path> {
        match &self.origin {
            Origin::Config { path, .. } => Some(path),
            Origin::File { .. } => None,
        }
    }

    /// Reads atlas.yaml as it stands on disk now, with its revision.
    pub fn config_text(&self) -> Result<(String, String), String> {
        let path = self
            .config_path()
            .ok_or("acu: no config stands behind this map; start acu with -c atlas.yaml")?;
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("acu: read {}: {e}; check the file", path.display()))?;
        let rev = revision(&text);
        Ok((text, rev))
    }

    /// Changes atlas.yaml as [`Atlas::write`] does, but only while the file
    /// still has the revision that the editor read. A save over a newer
    /// file fails with [`SaveError::Stale`] and changes nothing.
    pub fn save(
        &self,
        base: &str,
        edit: impl FnOnce(&str) -> Result<(String, &'static str), String>,
    ) -> Result<Written, SaveError> {
        let stale = std::cell::Cell::new(None);
        let out = self.write(|text| {
            let now = revision(text);
            if now != base {
                stale.set(Some(now));
                return Err("stale".into());
            }
            edit(text)
        });
        match (out, stale.into_inner()) {
            (Ok(w), _) => Ok(w),
            (Err(_), Some(now)) => Err(SaveError::Stale(format!(
                "acu: atlas.yaml changed on disk since you opened it (now revision {now}); reload the editor, then make the change again"
            ))),
            (Err(e), None) => Err(SaveError::Failed(e)),
        }
    }

    /// Reports whether the write tools work.
    pub fn writable(&self) -> bool {
        self.read_only.is_none()
    }

    /// Says why the write tools stay off, or returns `None` when they work.
    pub fn why_read_only(&self) -> Option<&'static str> {
        self.read_only
    }

    /// The files that the read tools may read.
    pub fn sources(&self) -> &Sources {
        &self.sources
    }

    /// Returns the current state. For a finished atlas.json, it reads the
    /// file again first when the file has changed.
    pub fn snapshot(&self) -> Result<Arc<Snapshot>, String> {
        if let Origin::File { path, stamp } = &self.origin {
            let now = std::fs::metadata(path)
                .and_then(|m| m.modified())
                .map_err(|e| format!("read {}: {e}", path.display()))?;
            let mut held = stamp.lock().map_err(|_| BROKEN.to_string())?;
            if *held != Some(now) {
                let (doc, t) = read_file(path).map_err(|e| format!("{e:#}"))?;
                *self.current.write().map_err(|_| BROKEN.to_string())? =
                    Arc::new(Snapshot::new(doc, Vec::new()));
                *held = t;
            }
        }
        Ok(self.current.read().map_err(|_| BROKEN.to_string())?.clone())
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
            return Err("no config stands behind this map; start acu with -c atlas.yaml".into());
        };
        let _guard = self.writing.lock().map_err(|_| BROKEN.to_string())?;
        // Write only the config file itself, and only inside its own folder.
        let target = path
            .canonicalize()
            .map_err(|e| format!("find {}: {e}; restart acu", path.display()))?;
        if &target != path {
            return Err(format!(
                "{} now points somewhere else; acu writes only the atlas.yaml it started with, so restart acu",
                path.display()
            ));
        }
        let text = std::fs::read_to_string(&target)
            .map_err(|e| format!("read {}: {e}; check the file", target.display()))?;
        let (new_text, how) = edit(&text)?;
        let value: Value = serde_yaml_ng::from_str(&new_text)
            .map_err(|e| format!("the new atlas.yaml does not parse, so acu wrote nothing: {e}"))?;
        let problems =
            config::validate(&value, config::CONFIG_SCHEMA).map_err(|e| e.to_string())?;
        if !problems.is_empty() {
            return Err(format!(
                "the change breaks the config schema, so acu wrote nothing: {}",
                problems.join("; ")
            ));
        }
        let cfg: config::Config = serde_json::from_value(value)
            .map_err(|e| format!("the change breaks the config, so acu wrote nothing: {e}"))?;
        let snap = assemble_checked(run, &cfg)
            .map_err(|e| format!("the change breaks the check, so acu wrote nothing: {e}"))?;
        write_atomic(&target, &new_text)?;
        let warnings = snap.warnings.clone();
        *self.current.write().map_err(|_| BROKEN.to_string())? = Arc::new(snap);
        Ok(Written {
            path: target,
            warnings,
            how,
            revision: revision(&new_text),
        })
    }

    /// Applies atlas.yaml as it stands on disk to the adapters' output
    /// again, and shows the result. The studio calls it right after it puts
    /// a new map on show, so a change that reached the file while the
    /// adapters ran shows too.
    pub fn refresh(&self) -> Result<(), String> {
        let Origin::Config { path, run } = &self.origin else {
            return Ok(());
        };
        let _guard = self.writing.lock().map_err(|_| BROKEN.to_string())?;
        let loaded = config::load(path).map_err(|e| format!("{e:#}"))?;
        let snap = assemble_checked(run, &loaded.config)?;
        *self.current.write().map_err(|_| BROKEN.to_string())? = Arc::new(snap);
        Ok(())
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

/// Counts the temporary files that this process makes, so their names
/// differ.
static DRAFTS: AtomicU64 = AtomicU64::new(0);

/// Writes a file through a new temporary file beside it, so a reader never
/// sees half a file. The temporary file gets a name of its own (process ID
/// and a counter) and opens only when nothing sits at that name, so a
/// second process or a symbolic link there cannot redirect the write.
fn write_atomic(target: &Path, text: &str) -> Result<(), String> {
    let dir = target.parent().unwrap_or(Path::new("."));
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "atlas.yaml".into());
    let mut tries = 0;
    let (tmp, mut file) = loop {
        let n = DRAFTS.fetch_add(1, Ordering::Relaxed);
        let tmp = dir.join(format!(".{name}.acu-draft-{}-{n}", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(f) => break (tmp, f),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && tries < 16 => tries += 1,
            Err(e) => {
                return Err(format!(
                    "create a temporary file in {}: {e}; check that acu may write there",
                    dir.display()
                ))
            }
        }
    };
    let done = file
        .write_all(text.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("write {}: {e}", tmp.display()));
    drop(file);
    if let Err(e) = done {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Ok(meta) = std::fs::metadata(target) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!(
            "replace {}: {e}; check that acu may write there",
            target.display()
        )
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
        let atlas = Atlas::from_config(&dir.path().join("atlas.yaml"), run, None).unwrap();
        (dir, atlas)
    }

    #[test]
    fn a_save_over_a_newer_file_is_refused() {
        let (dir, atlas) = fixture();
        let (text, rev) = atlas.config_text().unwrap();
        assert_eq!(rev, revision(FIXTURE_CONFIG));
        std::fs::write(
            dir.path().join("atlas.yaml"),
            text.replace("Shop", "Shop 2"),
        )
        .unwrap();
        let err = atlas
            .save(&rev, |t| Ok((t.to_string(), "test")))
            .err()
            .unwrap();
        assert!(matches!(err, SaveError::Stale(_)), "{err:?}");
        let (_, now) = atlas.config_text().unwrap();
        let w = atlas
            .save(&now, |t| Ok((t.replace("Shop 2", "Shop 3"), "test")))
            .unwrap();
        assert_eq!(w.how, "test");
        assert_eq!(w.revision, atlas.config_text().unwrap().1);
        assert_eq!(atlas.snapshot().unwrap().doc["project"]["name"], "Shop 3");
        // A change on disk shows after a refresh.
        let text = std::fs::read_to_string(dir.path().join("atlas.yaml")).unwrap();
        std::fs::write(
            dir.path().join("atlas.yaml"),
            text.replace("Shop 3", "Shop 4"),
        )
        .unwrap();
        atlas.refresh().unwrap();
        assert_eq!(atlas.snapshot().unwrap().doc["project"]["name"], "Shop 4");
    }

    #[test]
    fn a_live_slot_starts_empty_and_takes_a_map() {
        let live = Live::empty();
        assert_eq!(live.get().err().unwrap(), NO_PROJECT);
        let (_dir, atlas) = fixture();
        live.set(Arc::new(atlas));
        assert!(live.get().unwrap().writable());
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
    fn a_served_file_stays_read_only_and_reloads_when_it_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("atlas.json");
        std::fs::write(&path, serde_json::to_vec(&fixture_doc()).unwrap()).unwrap();
        let atlas = Atlas::from_file(&path, dir.path(), true).unwrap();
        assert!(!atlas.writable());
        assert!(atlas.why_read_only().unwrap().contains("-c atlas.yaml"));
        assert_eq!(
            atlas.snapshot().unwrap().doc["symbols"]
                .as_array()
                .unwrap()
                .len(),
            7
        );
        let mut doc = fixture_doc();
        doc["symbols"].as_array_mut().unwrap().pop();
        // Make sure the time stamp moves, even on a coarse file system.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
        let later = SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert_eq!(
            atlas.snapshot().unwrap().doc["symbols"]
                .as_array()
                .unwrap()
                .len(),
            6
        );
    }

    #[test]
    fn a_write_never_touches_a_file_at_its_temporary_name() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("atlas.yaml");
        std::fs::write(&target, "old\n").unwrap();
        // Something already sits at the next temporary name.
        let n = DRAFTS.load(Ordering::Relaxed);
        let taken = dir
            .path()
            .join(format!(".atlas.yaml.acu-draft-{}-{n}", std::process::id()));
        std::fs::write(&taken, "keep me\n").unwrap();
        write_atomic(&target, "new\n").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new\n");
        assert_eq!(std::fs::read_to_string(&taken).unwrap(), "keep me\n");
        let left: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains("acu-draft") && !taken.ends_with(n))
            .collect();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn read_only_refuses_writes() {
        let (dir, _) = fixture();
        let run = AdapterRun {
            merged: fixture_doc(),
            commit: None,
            repo_roots: vec![dir.path().to_path_buf()],
        };
        let atlas =
            Atlas::from_config(&dir.path().join("atlas.yaml"), run, Some(READ_ONLY)).unwrap();
        assert!(!atlas.writable());
        let err = atlas.write(|t| Ok((t.to_string(), "test"))).err().unwrap();
        assert!(err.contains("--read-only"), "{err}");
    }
}
