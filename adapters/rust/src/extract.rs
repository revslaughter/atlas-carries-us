//! Builds an Atlas Carries Us document from rustdoc's JSON output and
//! rust-analyzer's SCIP index.
//!
//! [`run`] reads the workspace with `cargo metadata`, runs rustdoc on each
//! library, binary and procedural-macro target, and indexes the workspace
//! with rust-analyzer. The extraction then runs in passes:
//!
//! 1. Declarations, from rustdoc: crates, modules, files, items and their
//!    members. Each item's types keep rustdoc's IDs for now.
//! 2. Resolution: every rustdoc ID turns into a symbol ID, across crates, so
//!    types link, docs link, and the type edges appear.
//! 3. References, from SCIP: calls, imports between modules, and tests.
//! 4. Entry points, from the plugins.
//!
//! `docs/adapters.md` records the decisions and the limits.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use rustdoc_types::{
    Attribute, Crate, GenericBound, Id, Impl, Item, ItemEnum, ItemKind, StructKind, VariantKind,
    Visibility,
};

use crate::docs;
use crate::glob::Globs;
use crate::plugins;
use crate::request::Request;
use crate::schema::{
    AdapterInfo, Container, Doc, Document, Edge, EntryPoint, Location, Meta, Param, Signature,
    Symbol, TestUse, Tool, TypeText, VERSION,
};
use crate::scip;
use crate::toolchain::{self, Package, Target, Tools};
use crate::types::{abi_name, header_words, Printer};

/// This adapter's release.
pub const ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The limits of every run, for the output's notes.
pub const NOTES: &[&str] = &[
    "A reference to a function inside another function's body counts as a call, even where the code passes the function on without calling it.",
    "A call through a trait object or a generic bound points at the trait's method.",
    "Calls inside code that a macro writes may count for the wrong function, or not at all.",
    "Items under #[cfg(test)], and code for other platforms or features, do not appear.",
];

/// The packages of the standard library, which never count as outside imports.
const STD_PACKAGES: &[&str] = &["std", "core", "alloc", "proc_macro", "test"];

/// One crate that the map holds: a library, binary or procedural macro.
pub struct CrateInfo {
    /// The crate's key in IDs: its crate name, plus `@bin` for a binary
    /// whose name a library already takes.
    pub key: String,
    /// The Cargo package's name.
    pub package: String,
    /// The target's name.
    pub target: String,
    /// Whether the target builds a binary.
    pub is_bin: bool,
    /// The package's folder, from the repository root.
    pub dir: String,
    /// The descriptors that start the target's SCIP symbols: "lib",
    /// "bin/<name>" or "proc-macro".
    pub scip_root: String,
    /// rustdoc's output for the crate.
    pub krate: Crate,
    /// Maps a rustdoc ID of this crate to its symbol or container ID.
    pub ids: HashMap<Id, String>,
}

/// The span of a callable, for finding the function that holds a reference.
#[derive(Debug, Clone)]
pub struct Span {
    /// The file, from the repository root.
    pub file: String,
    /// The first position: line and column from 0.
    pub start: (u32, u32),
    /// The position just after the end.
    pub end: (u32, u32),
}

impl Span {
    /// Reports whether a position lies inside the span.
    pub fn contains(&self, line: u32, col: u32) -> bool {
        (line, col) >= self.start && (line, col) < self.end
    }
}

/// A use of a rustdoc ID that waits for pass 2, when every ID has a symbol.
enum Pending {
    /// An edge from a symbol to each item that a list of rustdoc IDs names.
    Edges {
        /// The crate whose IDs these are.
        krate: usize,
        /// The user.
        from: String,
        /// The relation.
        kind: &'static str,
        /// The rustdoc IDs of the things used.
        targets: Vec<Id>,
    },
    /// A doc comment, whose links resolve through the item's link table.
    Doc {
        /// The crate whose item this is.
        krate: usize,
        /// The rustdoc ID of the item that carries the doc.
        item: Id,
        /// The symbols and containers that take the doc.
        owners: Vec<String>,
    },
}

/// What a module needs while the walk declares its items.
#[derive(Clone)]
struct ModCtx {
    /// The module's path in IDs: the crate key and the module path.
    qual: String,
    /// The container that the module's top-level items name as their parent.
    item_parent: String,
    /// The module's own file.
    file: String,
}

/// The state of one run.
pub struct Extractor {
    /// The request as it arrived.
    pub req: Request,
    /// The absolute repository root.
    pub repo_root: PathBuf,
    /// The absolute folder that the request names.
    pub root: PathBuf,
    /// The Cargo workspace root.
    pub ws_root: PathBuf,
    /// Globs of folders and files to keep.
    include: Globs,
    /// Globs of folders and files to leave out.
    exclude: Globs,
    /// The crates that the map holds.
    pub crates: Vec<CrateInfo>,
    /// The containers so far, by ID.
    pub conts: BTreeMap<String, Container>,
    /// The symbols so far, by ID.
    pub syms: BTreeMap<String, Symbol>,
    /// The edges so far, by user, thing used and kind.
    edges: BTreeMap<(String, String, String), Edge>,
    /// The entry points that the plugins found.
    pub entries: Vec<EntryPoint>,
    /// The tests of each symbol.
    tests: BTreeMap<String, BTreeSet<TestUse>>,
    /// Extra limits of this run, for the output's notes.
    pub notes: Vec<String>,
    /// The uses that wait for pass 2.
    pending: Vec<Pending>,
    /// Maps a crate name and an item's path (`lending::domain::Book`) to its
    /// symbol ID, so that one crate's types link to another's.
    by_path: HashMap<String, String>,
    /// The span of each symbol, from rustdoc.
    pub spans: HashMap<String, Span>,
    /// The callables of each file, for the calls pass.
    callables: HashMap<String, Vec<(Span, String)>>,
    /// Maps a SCIP symbol to the symbol or container ID it names.
    pub by_scip: HashMap<String, String>,
    /// The module of each file module's file.
    file_module: HashMap<String, String>,
    /// The inline modules of each file, with their ranges from SCIP.
    inline_modules: HashMap<String, Vec<(scip::Range, String)>>,
    /// The derive and trait facts that the clap plugin reads: for each type,
    /// the outside traits it implements, as `crate::Trait`.
    pub outside_traits: HashMap<String, BTreeSet<String>>,
    /// The impl IDs that a type's own list already declared, by crate.
    seen_impls: HashSet2,
    /// The members of trait implementations, with the crate and the trait's
    /// rustdoc ID: a member without a doc takes the trait member's doc.
    doc_fallbacks: Vec<(String, usize, Id)>,
    /// The SCIP index.
    pub index: scip::Index,
    /// The workspace's own package names.
    pub workspace_packages: BTreeSet<String>,
}

/// A set of (crate, rustdoc ID) pairs.
type HashSet2 = std::collections::HashSet<(usize, Id)>;

/// Reads the workspace that the request names and returns the document.
pub fn run(req: Request) -> Result<Document> {
    let root = PathBuf::from(&req.root)
        .canonicalize()
        .with_context(|| format!("find the root {}", req.root))?;
    let repo_root = if req.repo_root.is_empty() {
        root.clone()
    } else {
        PathBuf::from(&req.repo_root)
            .canonicalize()
            .with_context(|| format!("find the repository root {}", req.repo_root))?
    };
    let mut tools = Tools {
        toolchain: req
            .options
            .toolchain
            .clone()
            .unwrap_or_else(|| toolchain::NIGHTLY.to_string()),
        target_dir: PathBuf::new(),
        features: req.options.features.clone(),
        all_features: req.options.all_features,
    };
    let meta = tools.metadata(&root)?;
    tools.target_dir = match &req.options.target_dir {
        Some(d) => root.join(d),
        None => meta.target_directory.join("acu"),
    };
    let ws_root = meta
        .workspace_root
        .canonicalize()
        .unwrap_or(meta.workspace_root.clone());
    let mut ex = Extractor {
        include: Globs::new(&req.include),
        exclude: Globs::new(&req.exclude),
        req,
        repo_root,
        root,
        ws_root,
        crates: Vec::new(),
        conts: BTreeMap::new(),
        syms: BTreeMap::new(),
        edges: BTreeMap::new(),
        entries: Vec::new(),
        tests: BTreeMap::new(),
        notes: Vec::new(),
        pending: Vec::new(),
        by_path: HashMap::new(),
        spans: HashMap::new(),
        callables: HashMap::new(),
        by_scip: HashMap::new(),
        file_module: HashMap::new(),
        inline_modules: HashMap::new(),
        outside_traits: HashMap::new(),
        seen_impls: HashSet2::new(),
        doc_fallbacks: Vec::new(),
        index: scip::Index::default(),
        workspace_packages: BTreeSet::new(),
    };
    let plugins = plugins::Plugins::new(&ex.req.entry_points)?;
    let selected = ex.select(&meta)?;
    for (package, target, key) in &selected {
        eprintln!(
            "acu-adapter-rust: rustdoc on {} ({})",
            package.name, target.name
        );
        let krate = tools.rustdoc(package, target)?;
        let scip_root = if target.is_bin() {
            format!("bin/{}", target.crate_name())
        } else if target.is_proc_macro() {
            "proc-macro".to_string()
        } else {
            "lib".to_string()
        };
        ex.crates.push(CrateInfo {
            key: key.clone(),
            package: package.name.clone(),
            target: target.name.clone(),
            is_bin: target.is_bin(),
            dir: ex.rel(package.manifest_path.parent().unwrap_or(Path::new(""))),
            scip_root,
            krate,
            ids: HashMap::new(),
        });
    }
    for ci in 0..ex.crates.len() {
        ex.declare_crate(ci);
    }
    for ci in 0..ex.crates.len() {
        ex.leftover_impls(ci);
    }
    ex.resolve();
    eprintln!(
        "acu-adapter-rust: rust-analyzer scip on {}",
        ex.ws_root.display()
    );
    ex.index = tools.scip(&ex.ws_root)?;
    ex.references();
    plugins.run(&mut ex);
    Ok(ex.document())
}

/// Shortens a text to at most 80 characters, never cutting a character in two.
fn shorten(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() <= 80 {
        return s.to_string();
    }
    let cut: String = s.chars().take(79).collect();
    format!("{cut}…")
}

/// Reports whether a function's first parameter is `self`.
fn has_self(f: &rustdoc_types::Function) -> bool {
    f.sig.inputs.first().is_some_and(|(n, _)| n == "self")
}

impl Extractor {
    /// Picks the packages and targets that the map holds, and gives each
    /// crate its key.
    fn select<'m>(
        &mut self,
        meta: &'m toolchain::Metadata,
    ) -> Result<Vec<(&'m Package, &'m Target, String)>> {
        let members: BTreeSet<&str> = meta.workspace_members.iter().map(String::as_str).collect();
        let mut packages: Vec<&Package> = meta
            .packages
            .iter()
            .filter(|p| members.contains(p.id.as_str()))
            .collect();
        packages.sort_by(|a, b| a.name.cmp(&b.name));
        self.workspace_packages = packages.iter().map(|p| p.name.clone()).collect();
        let wanted = &self.req.options.packages;
        for w in wanted {
            if !packages.iter().any(|p| &p.name == w) {
                let known: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();
                bail!(
                    "the workspace has no package named {w} (known: {})",
                    known.join(", ")
                );
            }
        }
        let mut out = Vec::new();
        for p in packages {
            if !wanted.is_empty() && !wanted.contains(&p.name) {
                continue;
            }
            let dir = self.rel(p.manifest_path.parent().unwrap_or(Path::new("")));
            if self.exclude.matches(&dir) {
                continue;
            }
            let mut targets: Vec<&Target> = p.targets.iter().filter(|t| t.mapped()).collect();
            targets.sort_by_key(|t| (t.is_bin(), t.name.clone()));
            if !self.include.is_empty()
                && !self.include.matches(&dir)
                && !targets
                    .iter()
                    .any(|t| self.include.matches(&self.rel(&t.src_path)))
            {
                continue;
            }
            for t in targets {
                out.push((p, t, t.crate_name()));
            }
        }
        let libs: BTreeSet<String> = out
            .iter()
            .filter(|x| !x.1.is_bin())
            .map(|x| x.2.clone())
            .collect();
        for x in out.iter_mut() {
            if x.1.is_bin() && libs.contains(&x.2) {
                x.2.push_str("@bin");
            }
        }
        if out.is_empty() {
            bail!(
                "no package under {} matches the request; check include, exclude and the option packages",
                self.root.display()
            );
        }
        Ok(out)
    }

    /// Writes a path relative to the repository root, with forward slashes.
    pub fn rel(&self, p: &Path) -> String {
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.ws_root.join(p)
        };
        let abs = abs.canonicalize().unwrap_or(abs);
        let r = abs.strip_prefix(&self.repo_root).unwrap_or(&abs);
        r.to_string_lossy().replace('\\', "/")
    }

    /// Reports whether the request keeps a source file.
    pub fn keeps(&self, file: &str) -> bool {
        (self.include.is_empty() || self.include.matches(file)) && !self.exclude.matches(file)
    }

    /// Records a container, keeping the first one of an ID.
    fn add_cont(&mut self, c: Container) {
        self.conts.entry(c.id.clone()).or_insert(c);
    }

    /// Records a symbol and returns the ID it got. A second symbol of one ID
    /// gains `@<file>:<line>`.
    fn add_sym(&mut self, mut s: Symbol) -> String {
        if self.syms.contains_key(&s.id) {
            s.id = format!(
                "{}@{}:{}",
                s.id,
                s.location.path.rsplit('/').next().unwrap_or(""),
                s.location.line
            );
        }
        let id = s.id.clone();
        self.syms.insert(id.clone(), s);
        id
    }

    /// Records one use; a repeat raises the count and keeps the first line.
    pub fn add_edge(&mut self, from: &str, to: &str, kind: &str, line: u32) {
        let e = self
            .edges
            .entry((from.to_string(), to.to_string(), kind.to_string()))
            .or_insert_with(|| Edge {
                from: from.to_string(),
                to: to.to_string(),
                kind: kind.to_string(),
                confidence: "exact".into(),
                line,
                count: 0,
            });
        e.count += 1;
        if line > 0 && (e.line == 0 || line < e.line) {
            e.line = line;
        }
    }

    /// Records that a test uses a symbol.
    pub fn add_test(&mut self, id: &str, test: TestUse) {
        self.tests.entry(id.to_string()).or_default().insert(test);
    }

    /// Turns a rustdoc span into a location and a span.
    fn locate(
        &self,
        span: Option<&rustdoc_types::Span>,
        fallback: &str,
    ) -> (Location, Option<Span>) {
        match span {
            Some(s) if !s.filename.is_absolute() || s.filename.starts_with(&self.ws_root) => {
                let file = self.rel(&s.filename);
                let loc = Location {
                    path: file.clone(),
                    line: s.begin.0 as u32,
                    end_line: if s.end.0 > s.begin.0 {
                        s.end.0 as u32
                    } else {
                        0
                    },
                };
                let sp = Span {
                    file,
                    start: (
                        (s.begin.0 as u32).saturating_sub(1),
                        (s.begin.1 as u32).saturating_sub(1),
                    ),
                    end: (
                        (s.end.0 as u32).saturating_sub(1),
                        (s.end.1 as u32).saturating_sub(1),
                    ),
                };
                (loc, Some(sp))
            }
            _ => (
                Location {
                    path: fallback.to_string(),
                    line: 1,
                    end_line: 0,
                },
                None,
            ),
        }
    }

    /// Declares one crate: its container, its modules, and their items.
    fn declare_crate(&mut self, ci: usize) {
        let (key, root_id) = (self.crates[ci].key.clone(), self.crates[ci].krate.root);
        let root = &self.crates[ci].krate.index[&root_id];
        let _ = root;
        let dir = self.crates[ci].dir.clone();
        let crate_id = format!("rs:crate:{key}");
        self.add_cont(Container {
            id: crate_id.clone(),
            kind: "package".into(),
            language_kind: "crate".into(),
            name: key.clone(),
            path: dir,
            parent: String::new(),
            language: "rust".into(),
            unit: false,
            doc: None,
            external_imports: Vec::new(),
        });
        self.pending.push(Pending::Doc {
            krate: ci,
            item: root_id,
            owners: vec![crate_id],
        });
        // The path of each local item, so that other crates' IDs resolve.
        let local: Vec<(Id, String)> = self.crates[ci]
            .krate
            .paths
            .iter()
            .filter(|(_, s)| s.crate_id == 0)
            .map(|(id, s)| (*id, s.path.join("::")))
            .collect();
        self.walk_module(ci, root_id, None);
        let ids = &self.crates[ci].ids;
        let name = self.crates[ci].target.replace('-', "_");
        for (id, path) in local {
            if let Some(sym) = ids.get(&id) {
                let rest = path.split_once("::").map(|x| x.1).unwrap_or("");
                let k = if rest.is_empty() {
                    name.clone()
                } else {
                    format!("{name}::{rest}")
                };
                if !self.crates[ci].is_bin {
                    self.by_path.entry(k).or_insert_with(|| sym.clone());
                }
            }
        }
    }

    /// Declares one module and everything in it, then its child modules.
    fn walk_module(&mut self, ci: usize, mid: Id, parent: Option<&ModCtx>) {
        let item = self.crates[ci].krate.index[&mid].clone();
        let ItemEnum::Module(m) = &item.inner else {
            return;
        };
        let key = self.crates[ci].key.clone();
        let (loc, _) = self.locate(
            item.span.as_ref(),
            parent.map(|p| p.file.as_str()).unwrap_or(""),
        );
        let name = item.name.clone().unwrap_or_default();
        let qual = match parent {
            None => key.clone(),
            Some(p) => format!("{}::{name}", p.qual),
        };
        let id = format!("rs:mod:{qual}");
        let inline = parent.is_some_and(|p| p.file == loc.path);
        let file = loc.path.clone();
        let kept = self.keeps(&file);
        let mut item_parent = id.clone();
        if kept {
            self.add_cont(Container {
                id: id.clone(),
                kind: "module".into(),
                language_kind: "module".into(),
                name: qual.clone(),
                path: file.clone(),
                parent: format!("rs:crate:{key}"),
                language: "rust".into(),
                unit: true,
                doc: None,
                external_imports: Vec::new(),
            });
            let mut owners = vec![id.clone()];
            if !inline {
                let mut fid = format!("rs:file:{file}");
                if self.conts.get(&fid).is_some_and(|c| c.parent != id) {
                    fid = format!("{fid}@{key}");
                }
                self.add_cont(Container {
                    id: fid.clone(),
                    kind: "file".into(),
                    language_kind: "file".into(),
                    name: file.rsplit('/').next().unwrap_or("").to_string(),
                    path: file.clone(),
                    parent: id.clone(),
                    language: "rust".into(),
                    unit: false,
                    doc: None,
                    external_imports: Vec::new(),
                });
                self.file_module
                    .entry(file.clone())
                    .or_insert_with(|| id.clone());
                owners.push(fid.clone());
                item_parent = fid;
            }
            self.pending.push(Pending::Doc {
                krate: ci,
                item: mid,
                owners,
            });
            self.crates[ci].ids.insert(mid, id.clone());
        }
        let ctx = ModCtx {
            qual,
            item_parent,
            file,
        };
        let mut items: Vec<Item> = m
            .items
            .iter()
            .filter_map(|i| self.crates[ci].krate.index.get(i).cloned())
            .collect();
        items.sort_by_key(|i| {
            i.span
                .as_ref()
                .map(|s| (s.filename.clone(), s.begin))
                .unwrap_or_default()
        });
        let mut children = Vec::new();
        for it in items {
            if matches!(it.inner, ItemEnum::Module(_)) {
                children.push(it.id);
            } else if kept {
                self.declare_item(ci, &it, &ctx);
            }
        }
        for c in children {
            self.walk_module(ci, c, Some(&ctx));
        }
    }

    /// Makes the symbol of one item with what every symbol holds; the caller
    /// fills the rest.
    #[allow(clippy::too_many_arguments)]
    fn base(
        &mut self,
        item: &Item,
        id: String,
        kind: &str,
        language_kind: &str,
        parent: &str,
        exported: bool,
        fallback: &str,
    ) -> Symbol {
        let (location, _) = self.locate(item.span.as_ref(), fallback);
        Symbol {
            id,
            kind: kind.into(),
            language_kind: language_kind.into(),
            name: item.name.clone().unwrap_or_default(),
            parent: parent.into(),
            language: "rust".into(),
            exported,
            location,
            doc: None,
            type_params: String::new(),
            signature: None,
            type_text: None,
            value: String::new(),
            modifiers: Vec::new(),
            annotations: attributes(&item.attrs),
            satisfies: Vec::new(),
        }
    }

    /// Records a symbol, its rustdoc ID and its doc, and returns its final ID.
    fn finish(&mut self, ci: usize, item: &Item, s: Symbol) -> String {
        let callable = matches!(s.kind.as_str(), "function" | "method");
        let id = self.add_sym(s);
        if let (_, Some(sp)) = self.locate(item.span.as_ref(), "") {
            if callable {
                self.callables
                    .entry(sp.file.clone())
                    .or_default()
                    .push((sp.clone(), id.clone()));
            }
            self.spans.insert(id.clone(), sp);
        }
        self.crates[ci].ids.insert(item.id, id.clone());
        self.pending.push(Pending::Doc {
            krate: ci,
            item: item.id,
            owners: vec![id.clone()],
        });
        id
    }

    /// Declares one top-level item of a module.
    fn declare_item(&mut self, ci: usize, item: &Item, m: &ModCtx) {
        let Some(name) = item.name.clone() else {
            return;
        };
        let id = format!("rs:{}#{name}", m.qual);
        let exported = matches!(item.visibility, Visibility::Public);
        let parent = m.item_parent.clone();
        let mut printer = Printer::default();
        match &item.inner {
            ItemEnum::Struct(st) => {
                let mut s = self.base(item, id, "struct", "struct", &parent, exported, &m.file);
                s.type_params = printer.generics(&st.generics);
                let sid = self.finish(ci, item, s);
                let fields: Vec<(Option<Id>, String)> = match &st.kind {
                    StructKind::Unit => vec![],
                    StructKind::Tuple(f) => f
                        .iter()
                        .enumerate()
                        .map(|(i, x)| (*x, i.to_string()))
                        .collect(),
                    StructKind::Plain { fields, .. } => {
                        fields.iter().map(|f| (Some(*f), String::new())).collect()
                    }
                };
                // Methods first: a getter keeps the plain ID when a field shares its name.
                self.declare_impls(ci, &sid, item.id, &st.impls, exported, &m.file);
                self.declare_fields(ci, &sid, &fields, &m.file);
            }
            ItemEnum::Union(u) => {
                let mut s = self.base(item, id, "struct", "union", &parent, exported, &m.file);
                s.type_params = printer.generics(&u.generics);
                let sid = self.finish(ci, item, s);
                let fields: Vec<(Option<Id>, String)> =
                    u.fields.iter().map(|f| (Some(*f), String::new())).collect();
                self.declare_impls(ci, &sid, item.id, &u.impls, exported, &m.file);
                self.declare_fields(ci, &sid, &fields, &m.file);
            }
            ItemEnum::Enum(en) => {
                let mut s = self.base(item, id, "enum", "enum", &parent, exported, &m.file);
                s.type_params = printer.generics(&en.generics);
                let sid = self.finish(ci, item, s);
                for v in &en.variants {
                    self.declare_variant(ci, &sid, *v, exported, &m.file);
                }
                self.declare_impls(ci, &sid, item.id, &en.impls, exported, &m.file);
            }
            ItemEnum::Trait(tr) => {
                let mut s = self.base(item, id, "trait", "trait", &parent, exported, &m.file);
                s.type_params = printer.generics(&tr.generics);
                if tr.is_unsafe {
                    s.modifiers.push("unsafe".into());
                }
                if tr.is_auto {
                    s.modifiers.push("auto".into());
                }
                if !tr.bounds.is_empty() {
                    let text = printer.bounds(&tr.bounds);
                    s.type_text = Some(TypeText {
                        text,
                        refs: placeholders(ci, &printer.ids),
                    });
                }
                let sid = self.finish(ci, item, s);
                let supers: Vec<Id> = tr
                    .bounds
                    .iter()
                    .filter_map(|b| match b {
                        GenericBound::TraitBound { trait_, .. } => Some(trait_.id),
                        _ => None,
                    })
                    .collect();
                self.pending.push(Pending::Edges {
                    krate: ci,
                    from: sid.clone(),
                    kind: "extends",
                    targets: supers,
                });
                for it in &tr.items {
                    self.declare_member(ci, &sid, *it, "", exported, &m.file);
                }
            }
            ItemEnum::TraitAlias(ta) => {
                let mut s = self.base(
                    item,
                    id,
                    "type_alias",
                    "trait alias",
                    &parent,
                    exported,
                    &m.file,
                );
                s.type_params = printer.generics(&ta.generics);
                let text = printer.bounds(&ta.params);
                s.type_text = Some(TypeText {
                    text,
                    refs: placeholders(ci, &printer.ids),
                });
                self.finish(ci, item, s);
            }
            ItemEnum::Function(f) => {
                let mut s = self.base(item, id, "function", "function", &parent, exported, &m.file);
                self.fill_function(ci, &mut s, f, &name);
                if abi_name(&f.header.abi).is_some() && !f.has_body {
                    s.language_kind = "foreign function".into();
                }
                let sid = self.finish(ci, item, s);
                self.signature_edges(ci, &sid, f);
            }
            ItemEnum::TypeAlias(ta) => {
                let mut s = self.base(
                    item,
                    id,
                    "type_alias",
                    "type alias",
                    &parent,
                    exported,
                    &m.file,
                );
                s.type_params = printer.generics(&ta.generics);
                let mut p = Printer::default();
                let text = p.ty(&ta.type_);
                s.type_text = Some(TypeText {
                    text,
                    refs: placeholders(ci, &p.ids),
                });
                self.finish(ci, item, s);
            }
            ItemEnum::Constant { type_, const_ } => {
                let mut s = self.base(item, id, "constant", "constant", &parent, exported, &m.file);
                let (text, ids) = Printer::one(type_);
                s.type_text = Some(TypeText {
                    text,
                    refs: placeholders(ci, &ids),
                });
                s.value = shorten(if const_.expr == "_" {
                    const_.value.as_deref().unwrap_or("")
                } else {
                    &const_.expr
                });
                self.finish(ci, item, s);
            }
            ItemEnum::Static(st) => {
                let mut s = self.base(item, id, "variable", "static", &parent, exported, &m.file);
                let (text, ids) = Printer::one(&st.type_);
                s.type_text = Some(TypeText {
                    text,
                    refs: placeholders(ci, &ids),
                });
                s.value = shorten(&st.expr);
                if st.is_mutable {
                    s.modifiers.push("mut".into());
                }
                if st.is_unsafe {
                    s.modifiers.push("unsafe".into());
                }
                self.finish(ci, item, s);
            }
            ItemEnum::Macro(_) => {
                let s = self.base(item, id, "macro", "macro", &parent, exported, &m.file);
                self.finish(ci, item, s);
            }
            ItemEnum::ProcMacro(pm) => {
                let lk = match pm.kind {
                    rustdoc_types::MacroKind::Bang => "function-like macro",
                    rustdoc_types::MacroKind::Attr => "attribute macro",
                    rustdoc_types::MacroKind::Derive => "derive macro",
                };
                let s = self.base(item, id, "macro", lk, &parent, exported, &m.file);
                self.finish(ci, item, s);
            }
            ItemEnum::ExternType => {
                let s = self.base(item, id, "type", "extern type", &parent, exported, &m.file);
                self.finish(ci, item, s);
            }
            _ => {}
        }
    }

    /// Declares the fields of a struct or union. A tuple field takes its
    /// index as its name.
    fn declare_fields(
        &mut self,
        ci: usize,
        owner: &str,
        fields: &[(Option<Id>, String)],
        file: &str,
    ) {
        for (fid, index) in fields {
            let Some(fid) = fid else { continue };
            let Some(item) = self.crates[ci].krate.index.get(fid).cloned() else {
                continue;
            };
            let ItemEnum::StructField(t) = &item.inner else {
                continue;
            };
            let name = if index.is_empty() {
                item.name.clone().unwrap_or_default()
            } else {
                index.clone()
            };
            let exported = matches!(item.visibility, Visibility::Public);
            let mut s = self.base(
                &item,
                format!("{owner}.{name}"),
                "field",
                "field",
                owner,
                exported,
                file,
            );
            s.name = name;
            let (text, ids) = Printer::one(t);
            s.type_text = Some(TypeText {
                text,
                refs: placeholders(ci, &ids),
            });
            let sid = self.finish(ci, &item, s);
            self.pending.push(Pending::Edges {
                krate: ci,
                from: sid,
                kind: "field_type",
                targets: ids,
            });
        }
    }

    /// Declares one variant of an enum. Its fields give its type text; they
    /// make no symbols of their own.
    fn declare_variant(&mut self, ci: usize, owner: &str, vid: Id, exported: bool, file: &str) {
        let Some(item) = self.crates[ci].krate.index.get(&vid).cloned() else {
            return;
        };
        let ItemEnum::Variant(v) = &item.inner else {
            return;
        };
        let name = item.name.clone().unwrap_or_default();
        let mut s = self.base(
            &item,
            format!("{owner}.{name}"),
            "constant",
            "variant",
            owner,
            exported,
            file,
        );
        let mut p = Printer::default();
        let field = |p: &mut Printer, id: &Id, named: bool| -> Option<String> {
            let f = self.crates[ci].krate.index.get(id)?;
            let ItemEnum::StructField(t) = &f.inner else {
                return None;
            };
            let ty = p.ty(t);
            Some(if named {
                format!("{}: {ty}", f.name.clone().unwrap_or_default())
            } else {
                ty
            })
        };
        let text = match &v.kind {
            VariantKind::Plain => String::new(),
            VariantKind::Tuple(fs) => {
                let parts: Vec<String> = fs
                    .iter()
                    .flatten()
                    .filter_map(|f| field(&mut p, f, false))
                    .collect();
                format!("({})", parts.join(", "))
            }
            VariantKind::Struct { fields, .. } => {
                let parts: Vec<String> = fields
                    .iter()
                    .filter_map(|f| field(&mut p, f, true))
                    .collect();
                format!("{{ {} }}", parts.join(", "))
            }
        };
        if !text.is_empty() {
            s.type_text = Some(TypeText {
                text,
                refs: placeholders(ci, &p.ids),
            });
        }
        if let Some(d) = &v.discriminant {
            s.value = shorten(&d.expr);
        }
        let sid = self.finish(ci, &item, s);
        self.pending.push(Pending::Edges {
            krate: ci,
            from: sid,
            kind: "field_type",
            targets: p.ids,
        });
    }

    /// Declares one item of a trait or an impl block as a member: a method,
    /// an associated function, an associated type or an associated constant.
    /// `trait_label` names the trait of a trait implementation, or is empty.
    fn declare_member(
        &mut self,
        ci: usize,
        owner: &str,
        mid: Id,
        trait_label: &str,
        owner_exported: bool,
        file: &str,
    ) -> Option<String> {
        let item = self.crates[ci].krate.index.get(&mid).cloned()?;
        let name = item.name.clone()?;
        let path = if trait_label.is_empty() {
            format!("{owner}.{name}")
        } else {
            format!("{owner}.{trait_label}::{name}")
        };
        let exported = match item.visibility {
            Visibility::Public => true,
            Visibility::Default => owner_exported,
            _ => false,
        };
        match &item.inner {
            ItemEnum::Function(f) => {
                let lk = if has_self(f) {
                    "method"
                } else {
                    "associated function"
                };
                let mut s = self.base(&item, path, "method", lk, owner, exported, file);
                self.fill_function(ci, &mut s, f, &name);
                let sid = self.finish(ci, &item, s);
                self.signature_edges(ci, &sid, f);
                Some(sid)
            }
            ItemEnum::AssocConst { type_, value, .. } => {
                let mut s = self.base(
                    &item,
                    path,
                    "constant",
                    "associated constant",
                    owner,
                    exported,
                    file,
                );
                let (text, ids) = Printer::one(type_);
                s.type_text = Some(TypeText {
                    text,
                    refs: placeholders(ci, &ids),
                });
                s.value = value.as_deref().map(shorten).unwrap_or_default();
                Some(self.finish(ci, &item, s))
            }
            ItemEnum::AssocType {
                generics,
                bounds,
                type_,
                ..
            } => {
                let mut s = self.base(
                    &item,
                    path,
                    "type_alias",
                    "associated type",
                    owner,
                    exported,
                    file,
                );
                let mut p = Printer::default();
                s.type_params = p.generics(generics);
                let text = match type_ {
                    Some(t) => p.ty(t),
                    None => p.bounds(bounds),
                };
                if !text.is_empty() {
                    s.type_text = Some(TypeText {
                        text,
                        refs: placeholders(ci, &p.ids),
                    });
                }
                Some(self.finish(ci, &item, s))
            }
            _ => None,
        }
    }

    /// Fills a callable's signature and modifiers.
    fn fill_function(
        &mut self,
        ci: usize,
        s: &mut Symbol,
        f: &rustdoc_types::Function,
        name: &str,
    ) {
        let mut p = Printer::default();
        let generics = p.generics(&f.generics);
        let mut params = Vec::new();
        let mut written = Vec::new();
        for (pname, t) in &f.sig.inputs {
            written.push(p.param(pname, t));
            if pname == "self" {
                continue;
            }
            let (text, ids) = Printer::one(t);
            params.push(Param {
                name: pname.clone(),
                type_text: text,
                refs: placeholders(ci, &ids),
            });
        }
        let mut results = Vec::new();
        let mut text = format!(
            "{}fn {name}{generics}({})",
            header_words(&f.header),
            written.join(", ")
        );
        if let Some(o) = &f.sig.output {
            let (t, ids) = Printer::one(o);
            text.push_str(" -> ");
            text.push_str(&t);
            results.push(Param {
                name: String::new(),
                type_text: t,
                refs: placeholders(ci, &ids),
            });
        }
        text.push_str(&p.where_clause(&f.generics));
        s.signature = Some(Signature {
            type_params: generics,
            params,
            results,
            text,
        });
        let h = &f.header;
        if h.is_const {
            s.modifiers.push("const".into());
        }
        if h.is_async {
            s.modifiers.push("async".into());
        }
        if h.is_unsafe {
            s.modifiers.push("unsafe".into());
        }
        if let Some(abi) = abi_name(&h.abi) {
            s.modifiers.push(format!("extern \"{abi}\""));
        }
    }

    /// Queues the type edges of a callable: the types its parameters and its
    /// result name. `Self` adds no edge.
    fn signature_edges(&mut self, ci: usize, from: &str, f: &rustdoc_types::Function) {
        let mut params = Vec::new();
        for (n, t) in &f.sig.inputs {
            if n != "self" {
                params.extend(Printer::one(t).1);
            }
        }
        self.pending.push(Pending::Edges {
            krate: ci,
            from: from.to_string(),
            kind: "param_type",
            targets: params,
        });
        if let Some(o) = &f.sig.output {
            self.pending.push(Pending::Edges {
                krate: ci,
                from: from.to_string(),
                kind: "return_type",
                targets: Printer::one(o).1,
            });
        }
    }

    /// Declares the impl blocks of a type: their items become the type's
    /// members, and their traits its `implements` edges or `satisfies` list.
    /// Derived implementations add no members; their traits go to one
    /// `#[derive(…)]` annotation.
    fn declare_impls(
        &mut self,
        ci: usize,
        owner: &str,
        owner_rid: Id,
        impls: &[Id],
        exported: bool,
        file: &str,
    ) {
        let mut list: Vec<Item> = impls
            .iter()
            .filter_map(|i| self.crates[ci].krate.index.get(i).cloned())
            .filter(|i| i.crate_id == 0)
            .collect();
        list.sort_by_key(|i| {
            i.span
                .as_ref()
                .map(|s| (s.filename.clone(), s.begin))
                .unwrap_or_default()
        });
        let mut derives = BTreeSet::new();
        let mut satisfies = BTreeSet::new();
        for item in list {
            let ItemEnum::Impl(imp) = &item.inner else {
                continue;
            };
            if imp.is_synthetic
                || imp.blanket_impl.is_some()
                || imp.is_negative
                || !names(&imp.for_, owner_rid)
            {
                continue;
            }
            self.seen_impls.insert((ci, item.id));
            let derived = item
                .attrs
                .iter()
                .any(|a| matches!(a, Attribute::AutomaticallyDerived));
            let Some(tr) = &imp.trait_ else {
                for m in &imp.items {
                    self.declare_member(ci, owner, *m, "", exported, file);
                }
                continue;
            };
            let mut p = Printer::default();
            let label = p.path(tr);
            let short = label.rsplit("::").next().unwrap_or(&label).to_string();
            let short = if let Some(i) = label.find('<') {
                let head = &label[..i];
                format!(
                    "{}{}",
                    head.rsplit("::").next().unwrap_or(head),
                    &label[i..]
                )
            } else {
                short
            };
            let marker = matches!(short.as_str(), "StructuralPartialEq" | "StructuralEq");
            if marker {
                continue;
            }
            if let Some(name) = self.outside_trait(ci, tr.id) {
                self.outside_traits
                    .entry(owner.to_string())
                    .or_default()
                    .insert(name);
            }
            if self.is_local(ci, tr.id) {
                self.pending.push(Pending::Edges {
                    krate: ci,
                    from: owner.to_string(),
                    kind: "implements",
                    targets: vec![tr.id],
                });
            } else {
                satisfies.insert(short.clone());
            }
            if derived {
                derives.insert(tr.path.rsplit("::").next().unwrap_or(&tr.path).to_string());
                continue;
            }
            for m in &imp.items {
                if let Some(sid) = self.declare_member(ci, owner, *m, &short, exported, file) {
                    self.doc_fallbacks.push((sid, ci, tr.id));
                }
            }
        }
        if let Some(s) = self.syms.get_mut(owner) {
            if !derives.is_empty() {
                let list: Vec<String> = derives.into_iter().collect();
                s.annotations
                    .insert(0, format!("#[derive({})]", list.join(", ")));
            }
            s.satisfies = satisfies.into_iter().collect();
        }
    }

    /// Reports whether a rustdoc ID names an item of a crate that the map
    /// holds: this crate, or another library of the workspace.
    fn is_local(&self, ci: usize, id: Id) -> bool {
        let k = &self.crates[ci].krate;
        if k.index.get(&id).is_some_and(|i| i.crate_id == 0) {
            return true;
        }
        let Some(s) = k.paths.get(&id) else {
            return false;
        };
        if s.crate_id == 0 {
            return true;
        }
        let name = k
            .external_crates
            .get(&s.crate_id)
            .map(|c| c.name.as_str())
            .unwrap_or("");
        self.crates
            .iter()
            .any(|c| !c.is_bin && c.target.replace('-', "_") == name)
    }

    /// Names a trait of another crate as `<crate>::<Trait>`, such as
    /// `clap_builder::Parser`, for the plugins.
    fn outside_trait(&self, ci: usize, id: Id) -> Option<String> {
        let k = &self.crates[ci].krate;
        let s = k.paths.get(&id)?;
        if s.crate_id == 0 {
            return None;
        }
        let krate = &k.external_crates.get(&s.crate_id)?.name;
        Some(format!("{krate}::{}", s.path.last()?))
    }

    /// Declares the impl blocks of the crate's traits that no local type
    /// owns: an impl of a local trait for an outside or generic type becomes
    /// an item of its own, named as written.
    fn leftover_impls(&mut self, ci: usize) {
        let mut found: Vec<(Id, Item)> = Vec::new();
        for (tid, item) in &self.crates[ci].krate.index {
            if item.crate_id != 0 {
                continue;
            }
            if let ItemEnum::Trait(tr) = &item.inner {
                for i in &tr.implementations {
                    if self.seen_impls.contains(&(ci, *i)) {
                        continue;
                    }
                    if let Some(imp) = self.crates[ci].krate.index.get(i) {
                        found.push((*tid, imp.clone()));
                    }
                }
            }
        }
        found.sort_by_key(|(_, i)| {
            i.span
                .as_ref()
                .map(|s| (s.filename.clone(), s.begin))
                .unwrap_or_default()
        });
        for (tid, item) in found {
            let ItemEnum::Impl(imp) = &item.inner else {
                continue;
            };
            if imp.is_synthetic || imp.blanket_impl.is_some() || item.crate_id != 0 {
                continue;
            }
            let Some(trait_id) = self.crates[ci].ids.get(&tid).cloned() else {
                continue;
            };
            let Some(span) = &item.span else { continue };
            let file = self.rel(&span.filename);
            if !self.keeps(&file) {
                continue;
            }
            let Some(m) = self.module_of_file(&file) else {
                continue;
            };
            let mut p = Printer::default();
            let name = impl_name(&mut p, imp);
            let qual = m.trim_start_matches("rs:mod:").to_string();
            let parent = self
                .conts
                .values()
                .find(|c| c.kind == "file" && c.parent == m && c.path == file)
                .map(|c| c.id.clone())
                .unwrap_or(m.clone());
            let exported = self.syms.get(&trait_id).is_some_and(|s| s.exported);
            let mut s = self.base(
                &item,
                format!("rs:{qual}#{name}"),
                "namespace",
                "impl",
                &parent,
                exported,
                &file,
            );
            s.name = name;
            s.type_params = p.generics(&imp.generics);
            let sid = self.add_sym(s);
            self.add_edge(&sid, &trait_id, "implements", 0);
            for it in &imp.items {
                self.declare_member(ci, &sid, *it, "", exported, &file);
            }
        }
    }

    /// Finds the module whose own file a path names.
    fn module_of_file(&self, file: &str) -> Option<String> {
        self.file_module.get(file).cloned()
    }

    /// Turns a rustdoc ID of a crate into a symbol or container ID: the
    /// crate's own items first, then the items of the workspace's other
    /// libraries, by their path.
    pub fn resolve_id(&self, ci: usize, id: Id) -> Option<String> {
        let c = &self.crates[ci];
        if let Some(s) = c.ids.get(&id) {
            return Some(s.clone());
        }
        let sum = c.krate.paths.get(&id)?;
        if sum.crate_id == 0 {
            return None;
        }
        let krate = &c.krate.external_crates.get(&sum.crate_id)?.name;
        let rest = sum
            .path
            .iter()
            .skip(1)
            .cloned()
            .collect::<Vec<_>>()
            .join("::");
        let key = if rest.is_empty() {
            krate.clone()
        } else {
            format!("{krate}::{rest}")
        };
        self.by_path.get(&key).cloned().or_else(|| {
            if matches!(sum.kind, ItemKind::Module) {
                let m = format!("rs:mod:{key}");
                self.conts.contains_key(&m).then_some(m)
            } else {
                None
            }
        })
    }

    /// Pass 2: turns the rustdoc IDs in types into symbol IDs, makes the
    /// type edges, and normalises the docs.
    fn resolve(&mut self) {
        let mut syms = std::mem::take(&mut self.syms);
        for s in syms.values_mut() {
            if let Some(t) = &mut s.type_text {
                t.refs = self.resolve_refs(&t.refs);
            }
            if let Some(sig) = &mut s.signature {
                for p in sig.params.iter_mut().chain(sig.results.iter_mut()) {
                    p.refs = self.resolve_refs(&p.refs);
                }
            }
        }
        self.syms = syms;
        let pending = std::mem::take(&mut self.pending);
        for p in pending {
            match p {
                Pending::Edges {
                    krate,
                    from,
                    kind,
                    targets,
                } => {
                    let mut seen = BTreeSet::new();
                    for t in targets {
                        if let Some(to) = self.resolve_id(krate, t) {
                            if seen.insert(to.clone()) && self.syms.contains_key(&to) {
                                self.add_edge(&from, &to, kind, 0);
                            }
                        }
                    }
                }
                Pending::Doc {
                    krate,
                    item,
                    owners,
                } => {
                    let Some(it) = self.crates[krate].krate.index.get(&item) else {
                        continue;
                    };
                    let deprecated = it
                        .deprecation
                        .as_ref()
                        .map(|d| d.note.clone().unwrap_or_default());
                    let links = &it.links;
                    let resolve = |target: &str| -> Option<String> {
                        let id = links.get(target)?;
                        Some(
                            self.resolve_id(krate, *id)
                                .map(|s| format!("sym:{s}"))
                                .or_else(|| self.external_url(krate, *id))
                                .unwrap_or_default(),
                        )
                    };
                    let doc =
                        docs::normalise(it.docs.as_deref().unwrap_or(""), &resolve, deprecated);
                    for o in owners {
                        self.set_doc(&o, doc.clone());
                    }
                }
            }
        }
        self.inherit_docs();
    }

    /// Finds the page of an item of another crate in its published docs,
    /// such as `https://docs.rs/tower/0.5.2/tower/trait.Service.html`. Members
    /// of a type get no link, because the path does not say the type's kind.
    fn external_url(&self, ci: usize, id: Id) -> Option<String> {
        let k = &self.crates[ci].krate;
        let sum = k.paths.get(&id)?;
        let root = k
            .external_crates
            .get(&sum.crate_id)?
            .html_root_url
            .as_ref()?;
        let (name, dirs) = sum.path.split_last()?;
        if dirs
            .last()
            .is_some_and(|d| d.starts_with(|c: char| c.is_uppercase()))
        {
            return None;
        }
        let page = match sum.kind {
            ItemKind::Module => return Some(format!("{root}{}/index.html", sum.path.join("/"))),
            ItemKind::Struct => "struct",
            ItemKind::Enum => "enum",
            ItemKind::Union => "union",
            ItemKind::Trait => "trait",
            ItemKind::TraitAlias => "traitalias",
            ItemKind::Function => "fn",
            ItemKind::TypeAlias => "type",
            ItemKind::Constant => "constant",
            ItemKind::Static => "static",
            ItemKind::Macro => "macro",
            ItemKind::ProcAttribute => "attr",
            ItemKind::ProcDerive => "derive",
            ItemKind::Primitive => "primitive",
            _ => return None,
        };
        let root = if root.ends_with('/') {
            root.clone()
        } else {
            format!("{root}/")
        };
        Some(format!("{root}{}/{page}.{name}.html", dirs.join("/")))
    }

    /// Gives each member of a trait implementation that has no doc of its
    /// own the doc of the trait's member, as rustdoc's pages do.
    fn inherit_docs(&mut self) {
        for (sid, ci, tid) in std::mem::take(&mut self.doc_fallbacks) {
            if self.syms.get(&sid).is_none_or(|s| s.doc.is_some()) {
                continue;
            }
            let Some(t) = self.resolve_id(ci, tid) else {
                continue;
            };
            let name = self.syms[&sid].name.clone();
            let doc = self
                .syms
                .get(&format!("{t}.{name}"))
                .and_then(|m| m.doc.clone());
            if let Some(s) = self.syms.get_mut(&sid) {
                s.doc = doc;
            }
        }
    }

    /// Gives a symbol or container its doc.
    fn set_doc(&mut self, owner: &str, doc: Option<Doc>) {
        if let Some(s) = self.syms.get_mut(owner) {
            s.doc = doc;
        } else if let Some(c) = self.conts.get_mut(owner) {
            c.doc = doc;
        }
    }

    /// Turns placeholder refs into symbol IDs, in order, without repeats.
    fn resolve_refs(&self, refs: &[String]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for r in refs {
            let Some((ci, id)) = parse_placeholder(r) else {
                continue;
            };
            if let Some(s) = self.resolve_id(ci, Id(id)) {
                if !out.contains(&s) {
                    out.push(s);
                }
            }
        }
        out
    }

    /// Finds the unit that holds a symbol or container.
    pub fn unit_of(&self, id: &str) -> Option<String> {
        let mut cur = id.to_string();
        for _ in 0..64 {
            if let Some(c) = self.conts.get(&cur) {
                if c.unit {
                    return Some(cur);
                }
                cur = c.parent.clone();
            } else {
                cur = self.syms.get(&cur)?.parent.clone();
            }
        }
        None
    }

    /// The module that holds a position in a file: the innermost inline
    /// module around it, else the module whose file it is.
    pub fn module_at(&self, file: &str, line: u32, col: u32) -> Option<String> {
        if let Some(list) = self.inline_modules.get(file) {
            let best = list
                .iter()
                .filter(|(r, _)| r.contains(line, col))
                .min_by_key(|(r, _)| (r.end_line - r.start_line, r.end_col));
            if let Some((_, m)) = best {
                return Some(m.clone());
            }
        }
        self.file_module.get(file).cloned()
    }

    /// The innermost callable whose span holds a position.
    pub fn callable_at(&self, file: &str, line: u32, col: u32) -> Option<&str> {
        self.callables
            .get(file)?
            .iter()
            .filter(|(sp, _)| sp.contains(line, col))
            .min_by_key(|(sp, _)| (sp.end.0 - sp.start.0, sp.end.1))
            .map(|(_, id)| id.as_str())
    }

    /// Pass 3: joins SCIP's symbols to the map's, then reads every reference
    /// for calls, imports and tests.
    fn references(&mut self) {
        let docs = std::mem::take(&mut self.index.documents);
        let files: Vec<String> = docs
            .iter()
            .map(|d| self.rel(Path::new(&d.relative_path)))
            .collect();
        let pkg_roots: HashMap<(String, String), String> = self
            .crates
            .iter()
            .map(|c| ((c.package.clone(), c.scip_root.clone()), c.key.clone()))
            .collect();
        // Inline modules, and the join of SCIP symbols to the map's.
        for (d, file) in docs.iter().zip(&files) {
            let mut defs: Vec<&scip::Occurrence> =
                d.occurrences.iter().filter(|o| o.is_definition()).collect();
            defs.sort_by_key(|o| o.range);
            for o in &defs {
                let Some(sym) = scip::parse_symbol(&o.symbol) else {
                    continue;
                };
                if sym.descriptors.last().is_some_and(|x| x.suffix == '/') {
                    let root: Vec<String> = sym.descriptors[..sym.root_len()]
                        .iter()
                        .map(|x| x.name.clone())
                        .collect();
                    let Some(key) = pkg_roots.get(&(sym.package.clone(), root.join("/"))) else {
                        continue;
                    };
                    let path = sym.module_path();
                    let mid = if path.is_empty() {
                        format!("rs:mod:{key}")
                    } else {
                        format!("rs:mod:{key}::{}", path.join("::"))
                    };
                    if self.conts.contains_key(&mid) {
                        self.by_scip.entry(o.symbol.clone()).or_insert(mid.clone());
                        if let Some(enc) = o.enclosing {
                            if (enc.start_line, enc.start_col) != (0, 0) {
                                self.inline_modules
                                    .entry(file.clone())
                                    .or_default()
                                    .push((enc, mid));
                            }
                        }
                    }
                }
            }
        }
        let mut by_file: HashMap<&str, Vec<(String, Span)>> = HashMap::new();
        for (id, sp) in &self.spans {
            if self.syms.contains_key(id) {
                by_file
                    .entry(sp.file.as_str())
                    .or_default()
                    .push((id.clone(), sp.clone()));
            }
        }
        let mut joins = Vec::new();
        for (d, file) in docs.iter().zip(&files) {
            let Some(ours) = by_file.get(file.as_str()) else {
                continue;
            };
            let mut ours = ours.clone();
            // Smaller spans first, so that a member claims its own name before
            // an owner whose span holds it.
            ours.sort_by_key(|(id, sp)| (sp.end.0 - sp.start.0, sp.start, id.clone()));
            let mut taken = BTreeSet::new();
            for (id, sp) in ours {
                let name = &self.syms[&id].name;
                let hit = d
                    .occurrences
                    .iter()
                    .filter(|o| o.is_definition())
                    .find(|o| {
                        !taken.contains(&o.symbol)
                            && sp.contains(o.range.start_line, o.range.start_col)
                            && scip::parse_symbol(&o.symbol).is_some_and(|s| s.name() == name)
                    });
                if let Some(o) = hit {
                    taken.insert(o.symbol.clone());
                    joins.push((o.symbol.clone(), id));
                }
            }
        }
        for (s, id) in joins {
            self.by_scip.entry(s).or_insert(id);
        }
        // Calls, imports and outside crates.
        let mut externals: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (d, file) in docs.iter().zip(&files) {
            if !self.file_module.contains_key(file) {
                continue;
            }
            for o in &d.occurrences {
                if o.is_definition() || o.symbol.starts_with("local ") {
                    continue;
                }
                if o.symbol.ends_with('/') {
                    // A module's name in a path or a `mod` line: the item
                    // after it, if any, makes the import.
                    continue;
                }
                let (line, col) = (o.range.start_line, o.range.start_col);
                let from_mod = self.module_at(file, line, col);
                if let Some(target) = self.by_scip.get(&o.symbol).cloned() {
                    let callee = self
                        .syms
                        .get(&target)
                        .is_some_and(|s| matches!(s.kind.as_str(), "function" | "method"));
                    if callee {
                        if let Some(caller) = self.callable_at(file, line, col).map(str::to_string)
                        {
                            self.add_edge(&caller, &target, "calls", line + 1);
                        }
                    }
                    if let (Some(a), Some(b)) = (from_mod.clone(), self.unit_of(&target)) {
                        if a != b {
                            self.add_edge(&a, &b, "imports", 0);
                        }
                    }
                } else if let (Some(a), Some(sym)) = (from_mod, scip::parse_symbol(&o.symbol)) {
                    if !STD_PACKAGES.contains(&sym.package.as_str())
                        && !self.workspace_packages.contains(&sym.package)
                    {
                        externals.entry(a).or_default().insert(sym.package.clone());
                    }
                }
            }
        }
        for (m, set) in externals {
            if let Some(c) = self.conts.get_mut(&m) {
                c.external_imports = set.into_iter().collect();
            }
        }
        self.find_tests(&docs, &files);
        self.index.documents = docs;
    }

    /// Finds the tests: functions with an attribute named `test`, such as
    /// `#[test]` or `#[tokio::test]`. Each reference in a test's body records
    /// the test against the symbol it names.
    fn find_tests(&mut self, docs: &[scip::Document], files: &[String]) {
        for (d, file) in docs.iter().zip(files) {
            if !self.keeps(file) {
                continue;
            }
            let refs: Vec<&scip::Occurrence> = d
                .occurrences
                .iter()
                .filter(|o| !o.is_definition())
                .collect();
            for def in d.occurrences.iter().filter(|o| o.is_definition()) {
                let Some(enc) = def.enclosing else { continue };
                let Some(sym) = scip::parse_symbol(&def.symbol) else {
                    continue;
                };
                if sym.descriptors.last().is_none_or(|x| x.suffix != '(') {
                    continue;
                }
                let is_test = refs.iter().any(|r| {
                    enc.contains(r.range.start_line, r.range.start_col)
                        && r.range < def.range
                        && scip::parse_symbol(&r.symbol).is_some_and(|s| {
                            s.name() == "test"
                                && s.descriptors
                                    .last()
                                    .is_some_and(|x| matches!(x.suffix, ':' | '!'))
                        })
                });
                if !is_test {
                    continue;
                }
                let mut parts = sym.module_path();
                parts.push(sym.name().to_string());
                let test = TestUse {
                    name: parts.join("::"),
                    location: Location {
                        path: file.clone(),
                        line: def.range.start_line + 1,
                        end_line: enc.end_line + 1,
                    },
                };
                for r in &refs {
                    if r.range > def.range && enc.contains(r.range.start_line, r.range.start_col) {
                        if let Some(id) = self.by_scip.get(&r.symbol).cloned() {
                            if self.syms.contains_key(&id) {
                                self.add_test(&id, test.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    /// Writes the document: every list sorted, refs and edges kept only when
    /// both ends exist.
    fn document(mut self) -> Document {
        let root = self.rel(&self.root.clone());
        let mut notes: Vec<String> = NOTES.iter().map(|s| s.to_string()).collect();
        notes.extend(self.notes.iter().cloned());
        let known: BTreeSet<String> = self.syms.keys().cloned().collect();
        for s in self.syms.values_mut() {
            if let Some(t) = &mut s.type_text {
                t.refs.retain(|r| known.contains(r));
            }
            if let Some(sig) = &mut s.signature {
                for p in sig.params.iter_mut().chain(sig.results.iter_mut()) {
                    p.refs.retain(|r| known.contains(r));
                }
            }
        }
        let has = |id: &str| known.contains(id) || self.conts.contains_key(id);
        let edges: Vec<Edge> = self
            .edges
            .values()
            .filter(|e| has(&e.from) && has(&e.to))
            .cloned()
            .map(|mut e| {
                if e.count == 1 {
                    e.count = 0;
                }
                e
            })
            .collect();
        let mut seen = BTreeSet::new();
        let mut entries = Vec::new();
        for mut e in self.entries.drain(..) {
            if !seen.insert(e.id.clone()) {
                e.id = format!(
                    "{}@{}:{}",
                    e.id,
                    e.registered_at.path.rsplit('/').next().unwrap_or(""),
                    e.registered_at.line
                );
            }
            entries.push(e);
        }
        entries.sort_by(|a, b| (&a.registered_at, &a.id).cmp(&(&b.registered_at, &b.id)));
        let tests = self
            .tests
            .into_iter()
            .filter(|(k, _)| known.contains(k))
            .map(|(k, v)| {
                let mut l: Vec<TestUse> = v.into_iter().collect();
                l.sort_by(|a, b| {
                    (&a.location.path, a.location.line, &a.name).cmp(&(
                        &b.location.path,
                        b.location.line,
                        &b.name,
                    ))
                });
                (k, l)
            })
            .collect();
        Document {
            version: VERSION,
            meta: Meta {
                schema_version: VERSION,
                tool: Tool {
                    name: "acu-adapter-rust".into(),
                    version: ADAPTER_VERSION.into(),
                },
                generated: String::new(),
                commit: String::new(),
                languages: vec!["rust".into()],
                adapters: vec![AdapterInfo {
                    name: "rust".into(),
                    version: ADAPTER_VERSION.into(),
                    language: "rust".into(),
                    root: if root.is_empty() || root == "." {
                        String::new()
                    } else {
                        root
                    },
                    notes,
                }],
            },
            containers: self.conts.into_values().collect(),
            symbols: self.syms.into_values().collect(),
            edges,
            entry_points: entries,
            tests,
        }
    }
}

/// Writes the attributes that a reader cares about, as written.
fn attributes(attrs: &[Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter_map(|a| match a {
            Attribute::NonExhaustive => Some("#[non_exhaustive]".into()),
            Attribute::MustUse { reason: None } => Some("#[must_use]".into()),
            Attribute::MustUse { reason: Some(r) } => Some(format!("#[must_use = {r:?}]")),
            Attribute::MacroExport => Some("#[macro_export]".into()),
            Attribute::NoMangle => Some("#[no_mangle]".into()),
            Attribute::ExportName(n) => Some(format!("#[export_name = {n:?}]")),
            Attribute::Repr(r) => {
                let mut parts = vec![match r.kind {
                    rustdoc_types::ReprKind::Rust => "Rust",
                    rustdoc_types::ReprKind::C => "C",
                    rustdoc_types::ReprKind::Transparent => "transparent",
                    rustdoc_types::ReprKind::Simd => "simd",
                }
                .to_string()];
                if let Some(i) = &r.int {
                    parts.push(i.clone());
                }
                if let Some(a) = r.align {
                    parts.push(format!("align({a})"));
                }
                if let Some(p) = r.packed {
                    parts.push(format!("packed({p})"));
                }
                if parts.len() > 1 && parts[0] == "Rust" {
                    parts.remove(0);
                }
                Some(format!("#[repr({})]", parts.join(", ")))
            }
            _ => None,
        })
        .collect()
}

/// Reports whether an impl's self type names one item: the item itself, or
/// a reference or pointer to it.
fn names(t: &rustdoc_types::Type, id: Id) -> bool {
    match t {
        rustdoc_types::Type::ResolvedPath(p) => p.id == id,
        rustdoc_types::Type::BorrowedRef { type_, .. }
        | rustdoc_types::Type::RawPointer { type_, .. } => names(type_, id),
        _ => false,
    }
}

/// Names an impl block as written: `impl Shelved for Vec<Book>`.
fn impl_name(p: &mut Printer, imp: &Impl) -> String {
    let generics = p.generics(&imp.generics);
    let for_ = p.ty(&imp.for_);
    match &imp.trait_ {
        Some(t) => format!("impl{generics} {} for {for_}", p.path(t)),
        None => format!("impl{generics} {for_}"),
    }
}

/// Writes rustdoc IDs as placeholders that pass 2 turns into symbol IDs.
fn placeholders(ci: usize, ids: &[Id]) -> Vec<String> {
    ids.iter().map(|id| format!("\u{1}{ci}:{}", id.0)).collect()
}

/// Reads a placeholder back into a crate index and a rustdoc ID.
fn parse_placeholder(s: &str) -> Option<(usize, u32)> {
    let rest = s.strip_prefix('\u{1}')?;
    let (a, b) = rest.split_once(':')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}
