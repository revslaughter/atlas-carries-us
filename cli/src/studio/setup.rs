//! The setup calls of `acu studio`: list folders to pick from, propose a
//! first atlas.yaml for a folder, open a project (and write its new config),
//! and clone a repository over HTTPS. The folder picker sees only the
//! browse root and the clone workspace; names that start with a dot stay
//! hidden.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use super::{edit, fail, Reply, Studio};

/// The most folders that one listing returns.
const MAX_FOLDERS: usize = 500;

/// Folders that hold build output or downloaded packages, never a project.
const SKIP: &[&str] = &[
    "node_modules",
    "target",
    "vendor",
    "dist",
    "build",
    "__pycache__",
    "venv",
];

/// Returns the user's home folder, from `HOME` or `USERPROFILE`.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Returns the default clone workspace: `acu/repos` in the user's cache
/// folder.
pub fn default_workspace() -> Option<PathBuf> {
    // The XDG rules say to ignore a relative XDG_CACHE_HOME.
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home().map(|h| h.join(".cache")))?;
    Some(cache.join("acu").join("repos"))
}

/// Resolves a folder from a request and checks that it lies inside the
/// browse root or the clone workspace.
fn allowed(s: &Studio, raw: Option<&str>) -> Result<PathBuf, Reply> {
    let root = s.browse_root.clone().ok_or_else(|| {
        fail(
            StatusCode::FORBIDDEN,
            "acu serve picks no folders; start acu studio to pick one",
        )
    })?;
    let p = match raw.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => PathBuf::from(r),
        None => root.clone(),
    };
    if !p.is_absolute() {
        return Err(fail(
            StatusCode::BAD_REQUEST,
            "give an absolute folder path",
        ));
    }
    let p = p.canonicalize().map_err(|e| {
        fail(
            StatusCode::NOT_FOUND,
            format!("find {}: {e}; pick another folder", p.display()),
        )
    })?;
    let inside = p.starts_with(&root)
        || s.workspace
            .as_ref()
            .and_then(|w| w.canonicalize().ok())
            .is_some_and(|w| p.starts_with(w));
    if !inside {
        return Err(fail(
            StatusCode::FORBIDDEN,
            format!(
                "{} lies outside {}; start acu studio with --root to pick folders elsewhere",
                p.display(),
                root.display()
            ),
        ));
    }
    if !p.is_dir() {
        return Err(fail(
            StatusCode::BAD_REQUEST,
            format!("{} is a file, not a folder; pick a folder", p.display()),
        ));
    }
    Ok(p)
}

/// Reports whether a folder name stays out of listings and detection.
fn hidden(name: &str) -> bool {
    name.starts_with('.') || SKIP.contains(&name)
}

/// `POST /api/folders`: lists the folders inside one folder, and marks
/// those that hold an atlas.yaml or a git repository.
pub fn folders(s: &Studio, args: &Map<String, Value>) -> Reply {
    let dir = match allowed(s, args.get("path").and_then(Value::as_str)) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let root = s.browse_root.clone().unwrap_or_default();
    let rd = match std::fs::read_dir(&dir) {
        Ok(r) => r,
        Err(e) => {
            return fail(
                StatusCode::FORBIDDEN,
                format!("read {}: {e}; pick another folder", dir.display()),
            )
        }
    };
    // Read at most one entry past the limit: a folder with millions of
    // entries must not cost the studio its memory.
    let mut list: Vec<(String, PathBuf)> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
        .filter(|(n, _)| !hidden(n))
        .take(MAX_FOLDERS + 1)
        .collect();
    list.sort();
    let more = list.len() > MAX_FOLDERS;
    list.truncate(MAX_FOLDERS);
    let entries: Vec<Value> = list
        .iter()
        .map(|(n, p)| {
            json!({ "name": n, "path": p.display().to_string(),
                    "config": p.join("atlas.yaml").is_file(), "git": p.join(".git").exists() })
        })
        .collect();
    let parent = dir
        .parent()
        .filter(|_| dir != root)
        .filter(|p| p.starts_with(&root))
        .map(|p| p.display().to_string());
    (
        StatusCode::OK,
        json!({
            "path": dir.display().to_string(),
            "parent": parent,
            "config": dir.join("atlas.yaml").is_file(),
            "git": dir.join(".git").exists(),
            "folders": entries,
            "more": more,
        }),
    )
}

/// One language that a folder seems to hold: the adapter and the folder it
/// reads, relative to the project folder.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Found {
    /// The adapter's name.
    pub adapter: &'static str,
    /// The folder, relative to the project; "." for the project itself.
    pub root: String,
}

/// What a folder seems to hold: its languages and the entry-point plugins
/// that its dependencies suggest.
#[derive(Debug, Default)]
pub struct Detected {
    /// The languages, at most one per adapter and folder.
    pub languages: Vec<Found>,
    /// The plugins, sorted.
    pub plugins: Vec<&'static str>,
}

/// Reads a small text file, or returns nothing.
fn small(p: &Path) -> String {
    use std::io::Read;
    // A plain file only, never followed through a link: a repository could
    // point a marker file at /dev/zero or at a file outside it. The read
    // stops at 1 MB whatever the file claims.
    let plain = std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_file());
    if !plain {
        return String::new();
    }
    let mut out = String::new();
    if let Ok(f) = std::fs::File::open(p) {
        let _ = f.take(1 << 20).read_to_string(&mut out);
    }
    out
}

/// Reports whether a manifest names a dependency as a whole word: `click`
/// matches `click>=8` but not `clickhouse-driver`.
pub fn names(text: &str, dep: &str) -> bool {
    let part = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    text.match_indices(dep).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + dep.len()..].chars().next();
        !before.is_some_and(part) && !after.is_some_and(part)
    })
}

/// Looks at one folder's marker files and adds what they show.
fn look(dir: &Path, rel: &str, d: &mut Detected) {
    let has = |f: &str| dir.join(f).is_file();
    let mut add = |adapter: &'static str| {
        let f = Found {
            adapter,
            root: rel.to_string(),
        };
        // A folder inside one that the same adapter reads adds nothing.
        let covered = d.languages.iter().any(|x| {
            x.adapter == adapter && (x.root == "." || rel.starts_with(&format!("{}/", x.root)))
        });
        if !covered && !d.languages.contains(&f) {
            d.languages.push(f);
        }
    };
    let mut plug = |text: &str, pairs: &[(&str, &'static str)]| {
        for (needle, plugin) in pairs {
            if names(text, needle) && !d.plugins.contains(plugin) {
                d.plugins.push(plugin);
            }
        }
    };
    if has("go.mod") {
        let t = small(&dir.join("go.mod"));
        plug(
            &t,
            &[
                ("github.com/go-chi/chi", "go-chi"),
                ("github.com/spf13/cobra", "cobra"),
            ],
        );
        // Most Go servers use net/http, so acu proposes its plugin always.
        plug("net/http", &[("net/http", "go-nethttp")]);
        add("go");
    }
    if has("Cargo.toml") {
        let t = small(&dir.join("Cargo.toml"));
        plug(&t, &[("axum", "axum"), ("clap", "clap")]);
        add("rust");
    }
    if has("package.json") || has("tsconfig.json") {
        let t = small(&dir.join("package.json"));
        plug(
            &t,
            &[
                ("\"express\"", "express"),
                ("\"hono\"", "hono"),
                ("\"@sveltejs/kit\"", "sveltekit"),
            ],
        );
        add("typescript");
    }
    let py_markers = [
        "pyproject.toml",
        "setup.py",
        "setup.cfg",
        "requirements.txt",
    ];
    let any_py = std::fs::read_dir(dir).is_ok_and(|rd| {
        rd.filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().ends_with(".py"))
    });
    if py_markers.iter().any(|m| has(m)) || any_py {
        let mut t = String::new();
        for m in py_markers {
            t.push_str(&small(&dir.join(m)).to_lowercase());
        }
        plug(
            &t,
            &[
                ("flask", "flask"),
                ("fastapi", "fastapi"),
                ("django", "django"),
                ("click", "click"),
                ("typer", "typer"),
            ],
        );
        add("python");
    }
}

/// Finds the languages in a folder and in its folders one level down, as a
/// monorepo often holds them.
pub fn detect(dir: &Path) -> Detected {
    let mut d = Detected::default();
    look(dir, ".", &mut d);
    let mut subs: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
                .filter(|(n, _)| !hidden(n))
                .take(200)
                .collect()
        })
        .unwrap_or_default();
    subs.sort();
    for (n, p) in &subs {
        look(p, n, &mut d);
    }
    d.languages
        .sort_by(|a, b| (a.adapter, &a.root).cmp(&(b.adapter, &b.root)));
    d.plugins.sort();
    d
}

/// Reads `owner/name` and the link preset from the `origin` remote in a
/// folder's `.git/config`, when it points at GitHub or GitLab. It reads the
/// file as text and runs no git, because the person has not yet said that
/// they trust the folder.
fn remote(dir: &Path) -> Option<(String, &'static str)> {
    let cfg = dir.join(".git").join("config");
    if !cfg.is_file() {
        return None;
    }
    origin_url(&small(&cfg)).and_then(|u| repo_of(&u))
}

/// Finds the `url` of `[remote "origin"]` in the text of a git config. It
/// does not follow `include` lines; a remote set there gives no repository,
/// and the person can add `repo` by hand.
pub fn origin_url(text: &str) -> Option<String> {
    let mut inside = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t.replace(' ', "") == "[remote\"origin\"]";
        } else if inside {
            if let Some((k, v)) = t.split_once('=') {
                // Git reads keys without regard to case.
                if k.trim().eq_ignore_ascii_case("url") {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

/// Turns a remote URL into `owner/name` and a link preset.
pub fn repo_of(url: &str) -> Option<(String, &'static str)> {
    for (host, preset) in [("github.com", "github"), ("gitlab.com", "gitlab")] {
        let rest = url
            .strip_prefix(&format!("https://{host}/"))
            .or_else(|| url.strip_prefix(&format!("git@{host}:")))
            .or_else(|| url.strip_prefix(&format!("ssh://git@{host}/")));
        if let Some(r) = rest {
            let r = r.trim_end_matches('/').trim_end_matches(".git");
            let ok = r.split('/').count() >= 2
                && r.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c));
            if ok {
                return Some((r.to_string(), preset));
            }
        }
    }
    None
}

/// Writes a text as a YAML string that always parses as itself.
fn q(s: &str) -> String {
    Value::String(s.to_string()).to_string()
}

/// Writes the first atlas.yaml for a folder from what it holds.
pub fn proposal(dir: &Path, d: &Detected) -> String {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "My project".into());
    let mut y = String::new();
    y.push_str("# atlas.yaml: what acu reads and how the map presents it.\n");
    y.push_str("# acu studio wrote this first version; docs/config.md explains each field.\n");
    y.push_str("version: 1\nproject:\n");
    y.push_str(&format!("  name: {}\n", q(&name)));
    if let Some((repo, preset)) = remote(dir) {
        y.push_str(&format!("  repo: {}\n  links: {preset}\n", q(&repo)));
    }
    y.push_str("languages:\n");
    if d.languages.is_empty() {
        y.push_str(
            "  # acu found no language here; name one, such as go, typescript, rust or python.\n",
        );
        y.push_str("  - adapter: go\n");
    }
    for l in &d.languages {
        y.push_str(&format!("  - adapter: {}\n", l.adapter));
        if l.root != "." {
            y.push_str(&format!("    root: {}\n", q(&l.root)));
        }
    }
    let langs: Vec<&str> = d.languages.iter().map(|l| l.adapter).collect();
    let plugins: Vec<&&str> = d
        .plugins
        .iter()
        .filter(|p| crate::adapter::plugin_adapter(p).is_some_and(|a| langs.contains(&a)))
        .collect();
    if !plugins.is_empty() {
        y.push_str("entryPoints:\n");
        for p in plugins {
            y.push_str(&format!("  - plugin: {p}\n"));
        }
    }
    y
}

/// Returns the folder's atlas.yaml path, and refuses one that is a
/// symbolic link: a cloned repository could point it at any file, and the
/// editor's saves would then change that file.
fn config_in(dir: &Path) -> Result<PathBuf, Reply> {
    let file = dir.join("atlas.yaml");
    if std::fs::symlink_metadata(&file).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(fail(
            StatusCode::FORBIDDEN,
            format!(
                "{} is a symbolic link; replace it with a file, then pick the folder again",
                file.display()
            ),
        ));
    }
    Ok(file)
}

/// `POST /api/propose`: returns the folder's atlas.yaml when it has one,
/// else a proposal, with the languages that acu found.
pub fn propose(s: &Studio, args: &Map<String, Value>) -> Reply {
    let dir = match allowed(s, args.get("path").and_then(Value::as_str)) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let file = match config_in(&dir) {
        Ok(f) => f,
        Err(r) => return r,
    };
    let d = detect(&dir);
    let langs: Vec<Value> = d
        .languages
        .iter()
        .map(|l| json!({ "adapter": l.adapter, "root": l.root }))
        .collect();
    let (exists, text) = if file.is_file() {
        (true, small(&file))
    } else {
        (false, proposal(&dir, &d))
    };
    (
        StatusCode::OK,
        json!({
            "path": dir.display().to_string(),
            "exists": exists,
            "config": text,
            "languages": langs,
            "plugins": d.plugins,
        }),
    )
}

/// Checks a new config's text: it must parse, pass the config schema, and
/// read as a config.
pub fn check_text(text: &str) -> Result<(), String> {
    let v: Value = serde_yaml_ng::from_str(text)
        .map_err(|e| format!("acu: the config does not parse: {e}; fix it and try again"))?;
    let problems = crate::config::validate(&v, crate::config::CONFIG_SCHEMA)
        .map_err(|e| format!("acu: {e}"))?;
    if !problems.is_empty() {
        return Err(format!(
            "acu: the config breaks the schema: {}",
            problems.join("; ")
        ));
    }
    serde_json::from_value::<crate::config::Config>(v)
        .map_err(|e| format!("acu: the config does not read: {e}"))?;
    Ok(())
}

/// `POST /api/open`: opens a folder's project as a job. The body must say
/// `trust: true`, because the adapters run the project's build tools. With
/// `config`, it first writes that text as the folder's new atlas.yaml; it
/// never replaces a file that exists.
pub fn open(s: &Arc<Studio>, args: &Map<String, Value>) -> Reply {
    if s.browse_root.is_none() {
        return fail(
            StatusCode::FORBIDDEN,
            "acu serve opens no other project; start acu studio to open one",
        );
    }
    let dir = match allowed(s, args.get("path").and_then(Value::as_str)) {
        Ok(d) => d,
        Err(r) => return r,
    };
    if args.get("trust") != Some(&Value::Bool(true)) {
        return fail(
            StatusCode::PRECONDITION_REQUIRED,
            "extraction runs the project's own build tools (Rust build scripts and procedural macros, Go's toolchain); confirm that you trust this code, then send trust: true",
        );
    }
    let file = match config_in(&dir) {
        Ok(f) => f,
        Err(r) => return r,
    };
    let text = args.get("config").and_then(Value::as_str).map(String::from);
    match (&text, file.is_file()) {
        (Some(_), true) => {
            return fail(
                StatusCode::CONFLICT,
                format!(
                    "{} exists already; open it without a new config",
                    file.display()
                ),
            )
        }
        (None, false) => {
            return fail(
                StatusCode::NOT_FOUND,
                format!(
                    "{} holds no atlas.yaml; send a config to write one",
                    dir.display()
                ),
            )
        }
        (Some(t), false) => {
            if let Err(e) = check_text(t) {
                return fail(StatusCode::UNPROCESSABLE_ENTITY, e);
            }
        }
        (None, true) => {}
    }
    s.start_job("open", move |st, log| {
        if let Some(t) = text {
            create_new(&file, &t)?;
            log(&format!("acu: wrote {}", file.display()));
        }
        edit::open_config(st, &file, log)?;
        Ok(Some(dir.display().to_string()))
    })
}

/// Writes a new file in one step and never replaces one: the text goes to
/// a temporary file first, which then gets the final name through a hard
/// link, which fails when that name exists. A crash leaves no half file.
pub fn create_new(file: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    let dir = file.parent().unwrap_or(Path::new("."));
    let tmp = dir.join(format!(".atlas.yaml.acu-new-{}", std::process::id()));
    let done = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        match std::fs::hard_link(&tmp, file) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(e),
            // A file system without hard links (exFAT, some network shares):
            // write the final name directly, still never over a file.
            Err(_) => {
                let mut f = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(file)?;
                f.write_all(text.as_bytes())?;
                f.sync_all()
            }
            Ok(()) => Ok(()),
        }?;
        // Make the new name itself durable.
        #[cfg(unix)]
        std::fs::File::open(dir)?.sync_all()?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&tmp);
    done.map_err(|e| {
        format!(
            "acu: create {}: {e}; check that the folder is writable and holds no atlas.yaml",
            file.display()
        )
    })
}

/// A repository to clone: its URL, and the folder in the workspace that
/// receives it.
#[derive(Debug, PartialEq, Eq)]
pub struct CloneTarget {
    /// The checked HTTPS URL.
    pub url: String,
    /// The host, then the path, without `.git`, such as
    /// `github.com/go-chi/chi`.
    pub rel: String,
}

/// Checks a repository URL: HTTPS only, a plain host name, and a path of
/// plain names. Anything else (another protocol, a user name or password,
/// a query, `..`, a leading `-`) is refused, so git sees no option and no
/// other transport.
pub fn clone_target(url: &str) -> Result<CloneTarget, String> {
    let url = url.trim();
    let bad = || {
        "acu: give an HTTPS address of a repository, such as https://github.com/go-chi/chi"
            .to_string()
    };
    if url.len() > 300 {
        return Err(bad());
    }
    let rest = url.strip_prefix("https://").ok_or_else(bad)?;
    let (host, path) = rest.split_once('/').ok_or_else(bad)?;
    let host_ok = !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !host.starts_with(['-', '.']);
    if !host_ok {
        return Err(bad());
    }
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let parts: Vec<&str> = path.split('/').collect();
    let part_ok = |p: &&str| {
        !p.is_empty()
            && !p.starts_with(['.', '-'])
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    };
    if parts.len() < 2 || parts.len() > 6 || !parts.iter().all(part_ok) {
        return Err(bad());
    }
    Ok(CloneTarget {
        url: format!("https://{host}/{path}.git"),
        rel: format!("{}/{}", host.to_ascii_lowercase(), parts.join("/")),
    })
}

/// How long a clone may take before acu stops it.
const CLONE_LIMIT: std::time::Duration = std::time::Duration::from_secs(600);

/// The oldest git that acu clones with: 2.45.1 fixed a code execution in
/// clone (CVE-2024-32002).
const GIT_MIN: (u32, u32, u32) = (2, 45, 1);

/// Reads the three numbers of `git version 2.47.1` and similar text.
pub fn git_version(text: &str) -> Option<(u32, u32, u32)> {
    let v = text.trim().strip_prefix("git version ")?;
    let mut it = v
        .split(|c: char| !c.is_ascii_digit())
        .map(|p| p.parse::<u32>().ok());
    Some((it.next()??, it.next()??, it.next().flatten().unwrap_or(0)))
}

/// Checks that the installed git is 2.45.1 or later.
fn git_new_enough() -> Result<(), String> {
    let out = Command::new("git")
        .arg("--version")
        .output()
        .map_err(|e| format!("acu: start git: {e}; install git and try again"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    match git_version(&text) {
        Some(v) if v >= GIT_MIN => Ok(()),
        _ => Err(format!(
            "acu: {} is too old to clone safely; install git 2.45.1 or later",
            text.trim()
        )),
    }
}

/// Waits for a child up to a limit, and stops it and its own children
/// after. Meanwhile it hands each line of the child's standard error to
/// `log`; of git's progress lines, which end in a carriage return, it keeps
/// only the finished ones. Returns whether the child succeeded.
fn wait_limited(
    mut child: std::process::Child,
    limit: std::time::Duration,
    log: &(dyn Fn(&str) + Sync),
) -> bool {
    use std::io::{BufRead, BufReader};
    let err = child.stderr.take();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    std::thread::scope(|scope| {
        if let Some(err) = err {
            scope.spawn(move || {
                let mut r = BufReader::new(err);
                let mut buf = Vec::new();
                loop {
                    buf.clear();
                    // Split on both line ends: git redraws progress with \r.
                    let n = r.fill_buf().map(|b| b.len()).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    match r.read_until(b'\n', &mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            for part in String::from_utf8_lossy(&buf).split(['\r', '\n']) {
                                let t = part.trim();
                                if !t.is_empty() && (!t.contains('%') || t.contains("done")) {
                                    log(t);
                                }
                            }
                        }
                    }
                }
                let _ = done_tx.send(());
            });
        } else {
            let _ = done_tx.send(());
        }
        let start = std::time::Instant::now();
        let ok = loop {
            match child.try_wait() {
                Ok(Some(st)) => break st.success(),
                Ok(None) if start.elapsed() < limit => {
                    std::thread::sleep(std::time::Duration::from_millis(100))
                }
                _ => {
                    log("acu: git took too long, so acu stopped it");
                    kill_group(&mut child);
                    let _ = child.wait();
                    break false;
                }
            }
        };
        // A grandchild that still holds the pipe must not keep acu waiting.
        if done_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .is_err()
        {
            kill_group(&mut child);
        }
        ok
    })
}

/// Puts a command in a process group of its own, so [`kill_group`] can stop
/// it with every process it starts.
fn own_group(cmd: &mut Command) -> &mut Command {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd
}

/// Stops a child and, on Unix, every process in its group.
fn kill_group(child: &mut std::process::Child) {
    kill_pgid(child.id());
    let _ = child.kill();
}

/// Stops every process in the group that a child led. A group keeps its
/// number while any member lives. Once none lives, the system may give the
/// number to a new process, so acu signals a group only while it tracks a
/// running clone; a short window stays between the clone's end and the
/// moment acu stops tracking it. acu runs the system's `kill` program
/// rather than call the C library's `kill`, because acu holds no `unsafe`
/// code and a foreign call needs it.
pub fn kill_pgid(pid: u32) {
    #[cfg(unix)]
    {
        // The system's own program where it sits at a usual place; else,
        // as on NixOS, the `kill` that the PATH names.
        let program = ["/bin/kill", "/usr/bin/kill"]
            .into_iter()
            .find(|p| Path::new(p).is_file())
            .unwrap_or("kill");
        let _ = Command::new(program)
            .args(["-s", "KILL", "--", &format!("-{pid}")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(not(unix))]
    let _ = pid;
}

/// Checks the folders from the workspace down to `parent`: none may form a
/// symbolic link or hold a git repository, and the real path must stay in
/// the workspace. Else a repository cloned before could carry a link that
/// sends a later clone anywhere the user can write.
fn inside_workspace(ws: &Path, parent: &Path) -> Result<(), String> {
    let refuse = |why: String| {
        Err(format!(
            "acu: {why}, so acu refuses to clone there; remove it, or clone another address"
        ))
    };
    let rel = parent.strip_prefix(ws).unwrap_or(parent);
    let mut at = ws.to_path_buf();
    for part in rel.components() {
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(m) if m.file_type().is_symlink() => {
                return refuse(format!("{} is a symbolic link", at.display()))
            }
            Ok(_) if at.join(".git").exists() => {
                return refuse(format!("{} holds a git repository", at.display()))
            }
            _ => {}
        }
    }
    if let (Ok(w), Ok(p)) = (ws.canonicalize(), parent.canonicalize()) {
        if !p.starts_with(&w) {
            return refuse(format!(
                "{} leads outside {}",
                parent.display(),
                w.display()
            ));
        }
    }
    Ok(())
}

/// Names the staging folder of a clone: the final name with a leading dot,
/// so the folder picker hides it and no repository name (which never starts
/// with a dot) can collide with it.
fn staging_for(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    dest.with_file_name(format!(".{name}.acu-partial"))
}

/// Ends a clone: on success, gives the staging folder its final name; on
/// failure, removes it, with the host and owner folders that stay empty,
/// and returns `why`.
fn finish_clone(
    ok: bool,
    staging: &Path,
    dest: &Path,
    ws: &Path,
    why: String,
) -> Result<(), String> {
    if ok {
        return std::fs::rename(staging, dest).map_err(|e| {
            format!(
                "acu: name the clone {}: {e}; move {} there by hand, or remove it and clone again",
                dest.display(),
                staging.display()
            )
        });
    }
    let _ = std::fs::remove_dir_all(staging);
    let mut up = dest.parent();
    while let Some(p) = up.filter(|p| p.starts_with(ws) && *p != ws) {
        if std::fs::remove_dir(p).is_err() {
            break;
        }
        up = p.parent();
    }
    Err(why)
}

/// `POST /api/clone`: clones a repository into the workspace as a job, with
/// depth 1 and no submodules. A repository cloned before opens as it is.
pub fn clone(s: &Arc<Studio>, args: &Map<String, Value>) -> Reply {
    let Some(ws) = s.workspace.clone() else {
        return fail(
            StatusCode::FORBIDDEN,
            "acu serve clones no repository; start acu studio to clone one",
        );
    };
    let target = match clone_target(args.get("url").and_then(Value::as_str).unwrap_or("")) {
        Ok(t) => t,
        Err(e) => return fail(StatusCode::BAD_REQUEST, e),
    };
    s.start_job("clone", move |st, log| {
        let dest = ws.join(&target.rel);
        if dest.join(".git").exists() {
            log(&format!(
                "acu: {} holds a clone already; acu opens it as it is",
                dest.display()
            ));
            return Ok(Some(dest.display().to_string()));
        }
        git_new_enough()?;
        if dest.exists() {
            return Err(format!(
                "acu: {} exists but holds no clone; remove it, then try again",
                dest.display()
            ));
        }
        // git writes into a staging folder beside the final one, which gets
        // its name only after git succeeds. A clone that Ctrl-C or a crash
        // stopped halfway never sits at the final name, so it never opens
        // later as a good one.
        let staging = staging_for(&dest);
        if staging.exists() {
            std::fs::remove_dir_all(&staging).map_err(|e| {
                format!(
                    "acu: remove the half clone {}: {e}; remove it by hand, then clone again",
                    staging.display()
                )
            })?;
        }
        let parent = dest.parent().unwrap_or(&ws).to_path_buf();
        inside_workspace(&ws, &parent)?;
        std::fs::create_dir_all(&parent).map_err(|e| {
            format!(
                "acu: create {}: {e}; check that acu may write there",
                parent.display()
            )
        })?;
        inside_workspace(&ws, &parent)?;
        log(&format!(
            "acu: cloning {} into {}",
            target.url,
            dest.display()
        ));
        let mut cmd = Command::new("git");
        let child = own_group(&mut cmd)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ALLOW_PROTOCOL", "https")
            .env("GIT_PROTOCOL_FROM_USER", "0")
            .args([
                "-c",
                "protocol.allow=never",
                "-c",
                "protocol.https.allow=always",
                "-c",
                "core.hooksPath=/dev/null",
                "clone",
                "--depth",
                "1",
                "--single-branch",
                "--no-tags",
                "--no-recurse-submodules",
                "--progress",
                "--",
                &target.url,
            ])
            .arg(&staging)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn();
        let child = match child {
            Ok(c) => c,
            Err(e) => {
                let why = format!("acu: start git: {e}; install git and try again");
                return finish_clone(false, &staging, &dest, &ws, why).map(|_| None);
            }
        };
        // Ctrl-C reaches acu but not git's own group; acu stops it then.
        st.set_child(Some(child.id()));
        let ok = wait_limited(child, CLONE_LIMIT, log);
        st.set_child(None);
        let why = format!(
            "acu: git could not clone {} into {}; {}",
            target.url,
            dest.display(),
            "check the address and your network, then try again"
        );
        finish_clone(ok, &staging, &dest, &ws, why)?;
        log("acu: the clone finished");
        Ok(Some(dest.display().to_string()))
    })
}

#[cfg(test)]
mod tests {
    use super::super::{Mode, Studio};
    use super::*;
    use crate::mcp::Live;

    /// Writes a file and the folders above it.
    fn write(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    /// Makes a studio whose browse root is a new temporary folder.
    fn studio_in() -> (tempfile::TempDir, Arc<Studio>) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let s = Arc::new(Studio::new(
            Arc::new(Live::empty()),
            Mode::Studio,
            "127.0.0.1:4321".parse().unwrap(),
            Some(root.join("code")),
            Some(root.join("ws")),
        ));
        std::fs::create_dir_all(root.join("code")).unwrap();
        (dir, s)
    }

    #[test]
    fn detection_finds_languages_and_plugins_in_a_monorepo() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(
            &d.join("go.mod"),
            "module x\nrequire github.com/go-chi/chi/v5 v5.0.0\n",
        );
        write(
            &d.join("web/package.json"),
            "{\"dependencies\":{\"hono\":\"4\"}}",
        );
        write(&d.join("web/sub/package.json"), "{}");
        write(&d.join("tools/gen.py"), "print(1)\n");
        write(&d.join("node_modules/x/package.json"), "{}");
        write(&d.join(".hidden/Cargo.toml"), "");
        let got = detect(d);
        let langs: Vec<(&str, &str)> = got
            .languages
            .iter()
            .map(|l| (l.adapter, l.root.as_str()))
            .collect();
        assert_eq!(
            langs,
            vec![("go", "."), ("python", "tools"), ("typescript", "web")]
        );
        assert_eq!(got.plugins, vec!["go-chi", "go-nethttp", "hono"]);
        let text = proposal(d, &got);
        check_text(&text).unwrap();
        assert!(
            text.contains("  - adapter: typescript\n    root: \"web\"\n"),
            "{text}"
        );
        assert!(text.contains("  - plugin: hono\n"), "{text}");
    }

    #[test]
    fn an_empty_folder_still_gets_a_valid_proposal() {
        let dir = tempfile::tempdir().unwrap();
        let text = proposal(dir.path(), &detect(dir.path()));
        check_text(&text).unwrap();
    }

    #[test]
    fn dependencies_match_whole_names() {
        assert!(names("click>=8\n", "click"));
        assert!(!names("clickhouse-driver\n", "click"));
        assert!(names("\"hono\": \"4\"", "\"hono\""));
        assert!(!names("axum-extra = 1", "axum"));
        assert!(names("axum = \"0.8\"", "axum"));
    }

    #[test]
    fn the_origin_url_comes_from_the_config_text() {
        let text = "[core]\n\tbare = false\n[remote \"up\"]\n\turl = https://x/y\n[remote \"origin\"]\n\turl = git@github.com:a/b.git\n";
        assert_eq!(origin_url(text).as_deref(), Some("git@github.com:a/b.git"));
        assert_eq!(origin_url("[core]\n"), None);
    }

    #[test]
    fn remotes_become_repo_names() {
        assert_eq!(
            repo_of("https://github.com/go-chi/chi.git"),
            Some(("go-chi/chi".into(), "github"))
        );
        assert_eq!(
            repo_of("git@gitlab.com:group/sub/proj.git"),
            Some(("group/sub/proj".into(), "gitlab"))
        );
        assert_eq!(repo_of("https://example.com/a/b"), None);
    }

    #[test]
    fn clone_addresses_are_checked() {
        let t = clone_target("https://github.com/go-chi/chi").unwrap();
        assert_eq!(t.url, "https://github.com/go-chi/chi.git");
        assert_eq!(t.rel, "github.com/go-chi/chi");
        for bad in [
            "http://github.com/a/b",
            "git@github.com:a/b.git",
            "ext::sh -c touch% /tmp/x",
            "file:///etc",
            "https://user:pw@github.com/a/b",
            "https://github.com/a/../b",
            "https://github.com/-u/b",
            "https://github.com/a",
            "https://github.com/a/b?x=1",
            "https://-oops/a/b",
            "--upload-pack=touch /tmp/x",
        ] {
            assert!(clone_target(bad).is_err(), "{bad}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_slow_child_stops_at_the_limit_with_its_children() {
        let mut cmd = Command::new("sh");
        let child = own_group(&mut cmd)
            .args(["-c", "echo started >&2; sleep 30 & sleep 30"])
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let seen = std::sync::Mutex::new(Vec::<String>::new());
        let t = std::time::Instant::now();
        let ok = wait_limited(child, std::time::Duration::from_millis(300), &|l: &str| {
            seen.lock().unwrap().push(l.to_string())
        });
        assert!(!ok);
        assert!(t.elapsed() < std::time::Duration::from_secs(10));
        let seen = seen.into_inner().unwrap();
        assert_eq!(seen[0], "started");
        assert!(seen.iter().any(|l| l.contains("too long")), "{seen:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_clone_never_lands_through_a_link_or_inside_a_clone() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(ws.join("evil.example/evil/a/.git")).unwrap();
        std::fs::create_dir_all(dir.path().join("outside")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("outside"), ws.join("evil.example/link"))
            .unwrap();
        let err = inside_workspace(&ws, &ws.join("evil.example/link/x")).unwrap_err();
        assert!(err.contains("symbolic link"), "{err}");
        let err = inside_workspace(&ws, &ws.join("evil.example/evil/a/sub")).unwrap_err();
        assert!(err.contains("git repository"), "{err}");
        assert!(inside_workspace(&ws, &ws.join("github.com/owner")).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn marker_files_never_follow_links_or_read_without_end() {
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/dev/zero", dir.path().join("setup.cfg")).unwrap();
        assert_eq!(small(&dir.path().join("setup.cfg")), "");
        write(&dir.path().join("a.py"), "print(1)\n");
        let d = detect(dir.path());
        assert_eq!(d.languages.len(), 1);
    }

    #[test]
    fn a_clone_takes_its_name_only_after_success() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().to_path_buf();
        let dest = ws.join("github.com/owner/repo");
        let staging = staging_for(&dest);
        assert!(staging.ends_with(".repo.acu-partial"));
        assert!(hidden(&staging.file_name().unwrap().to_string_lossy()));
        // A failure removes the staging folder and the empty owner folders.
        write(&staging.join(".git/HEAD"), "x");
        assert_eq!(
            finish_clone(false, &staging, &dest, &ws, "acu: why".into()),
            Err("acu: why".into())
        );
        assert!(!staging.exists() && !ws.join("github.com").exists());
        // A success renames it.
        write(&staging.join(".git/HEAD"), "x");
        finish_clone(true, &staging, &dest, &ws, String::new()).unwrap();
        assert!(dest.join(".git/HEAD").is_file() && !staging.exists());
    }

    #[test]
    fn git_versions_read() {
        assert_eq!(git_version("git version 2.47.1\n"), Some((2, 47, 1)));
        assert_eq!(
            git_version("git version 2.39.3 (Apple Git-146)"),
            Some((2, 39, 3))
        );
        assert_eq!(git_version("git version 2.50"), Some((2, 50, 0)));
        assert!(git_version("git version 2.45.0").unwrap() < GIT_MIN);
        assert_eq!(git_version("nonsense"), None);
    }

    #[test]
    fn the_picker_stays_inside_its_root() {
        let (dir, s) = studio_in();
        let root = dir.path().canonicalize().unwrap();
        write(&root.join("code/app/atlas.yaml"), "version: 1\n");
        std::fs::create_dir_all(root.join("code/.secret")).unwrap();
        let (code, v) = folders(&s, &Map::new());
        assert_eq!(code, StatusCode::OK, "{v}");
        assert_eq!(v["folders"].as_array().unwrap().len(), 1);
        assert_eq!(v["folders"][0]["config"], true);
        assert!(v["parent"].is_null());
        let mut a = Map::new();
        a.insert("path".into(), json!(root.display().to_string()));
        assert_eq!(folders(&s, &a).0, StatusCode::FORBIDDEN);
        a.insert(
            "path".into(),
            json!(root.join("code/app/..").join("..").display().to_string()),
        );
        assert_eq!(folders(&s, &a).0, StatusCode::FORBIDDEN);
        a.insert("path".into(), json!("relative/path"));
        assert_eq!(folders(&s, &a).0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn a_new_config_never_replaces_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("atlas.yaml");
        create_new(&f, "a\n").unwrap();
        assert!(create_new(&f, "b\n").is_err());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "a\n");
        let left = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(left, 1);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_config_is_refused() {
        let (dir, s) = studio_in();
        let root = dir.path().canonicalize().unwrap();
        write(&root.join("outside.yaml"), "version: 1\n");
        std::fs::create_dir_all(root.join("code/app")).unwrap();
        std::os::unix::fs::symlink(root.join("outside.yaml"), root.join("code/app/atlas.yaml"))
            .unwrap();
        let mut a = Map::new();
        a.insert(
            "path".into(),
            json!(root.join("code/app").display().to_string()),
        );
        let (code, v) = propose(&s, &a);
        assert_eq!(code, StatusCode::FORBIDDEN, "{v}");
        a.insert("trust".into(), json!(true));
        assert_eq!(open(&s, &a).0, StatusCode::FORBIDDEN);
    }

    #[test]
    fn open_never_replaces_a_config() {
        let (dir, s) = studio_in();
        let root = dir.path().canonicalize().unwrap();
        write(&root.join("code/app/atlas.yaml"), "version: 1\n");
        let mut a = Map::new();
        a.insert(
            "path".into(),
            json!(root.join("code/app").display().to_string()),
        );
        a.insert("config".into(), json!("version: 1\n"));
        assert_eq!(open(&s, &a).0, StatusCode::PRECONDITION_REQUIRED);
        a.insert("trust".into(), json!(true));
        assert_eq!(open(&s, &a).0, StatusCode::CONFLICT);
        a.insert(
            "path".into(),
            json!(root.join("code").display().to_string()),
        );
        a.insert("config".into(), json!("version: 2\n"));
        assert_eq!(open(&s, &a).0, StatusCode::UNPROCESSABLE_ENTITY);
        a.remove("config");
        assert_eq!(open(&s, &a).0, StatusCode::NOT_FOUND);
    }
}
