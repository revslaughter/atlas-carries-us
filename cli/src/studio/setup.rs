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
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".cache")))?;
    Some(cache.join("acu").join("repos"))
}

/// Resolves a folder from a request and checks that it lies inside the
/// browse root or the clone workspace.
fn allowed(s: &Studio, raw: Option<&str>) -> Result<PathBuf, Reply> {
    let root = s
        .browse_root
        .clone()
        .ok_or_else(|| fail(StatusCode::FORBIDDEN, "only acu studio picks folders"))?;
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
            format!("{} holds no folder", p.display()),
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
    let mut list: Vec<(String, PathBuf)> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
        .filter(|(n, _)| !hidden(n))
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
    match std::fs::metadata(p) {
        Ok(m) if m.len() < 1 << 20 => std::fs::read_to_string(p).unwrap_or_default(),
        _ => String::new(),
    }
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
            if text.contains(needle) && !d.plugins.contains(plugin) {
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
                .collect()
        })
        .unwrap_or_default();
    subs.sort();
    for (n, p) in subs.iter().take(200) {
        look(p, n, &mut d);
    }
    d.languages
        .sort_by(|a, b| (a.adapter, &a.root).cmp(&(b.adapter, &b.root)));
    d.plugins.sort();
    d
}

/// Reads `owner/name` and the link preset from a folder's git remote, when
/// it points at GitHub or GitLab.
fn remote(dir: &Path) -> Option<(String, &'static str)> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    repo_of(String::from_utf8_lossy(&out.stdout).trim())
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

/// `POST /api/propose`: returns the folder's atlas.yaml when it has one,
/// else a proposal, with the languages that acu found.
pub fn propose(s: &Studio, args: &Map<String, Value>) -> Reply {
    let dir = match allowed(s, args.get("path").and_then(Value::as_str)) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let file = dir.join("atlas.yaml");
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
        return fail(StatusCode::FORBIDDEN, "only acu studio opens projects");
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
    let file = dir.join("atlas.yaml");
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
            // create_new: never replace a file that appeared meanwhile.
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&file)
                .map_err(|e| format!("acu: create {}: {e}", file.display()))?;
            f.write_all(t.as_bytes())
                .map_err(|e| format!("acu: write {}: {e}", file.display()))?;
            log(&format!("acu: wrote {}", file.display()));
        }
        edit::open_config(st, &file, log)?;
        Ok(Some(dir.display().to_string()))
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

/// Waits for a child up to a limit, and kills it after; returns whether it
/// succeeded and what it wrote to standard error.
fn wait_limited(mut child: std::process::Child, limit: std::time::Duration) -> (bool, String) {
    use std::io::Read;
    let mut err = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = err.as_mut() {
            let _ = e.take(1 << 20).read_to_string(&mut s);
        }
        s
    });
    let start = std::time::Instant::now();
    let ok = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st.success(),
            Ok(None) if start.elapsed() < limit => {
                std::thread::sleep(std::time::Duration::from_millis(100))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    (ok, reader.join().unwrap_or_default())
}

/// `POST /api/clone`: clones a repository into the workspace as a job, with
/// depth 1 and no submodules. A repository cloned before opens as it is.
pub fn clone(s: &Arc<Studio>, args: &Map<String, Value>) -> Reply {
    let Some(ws) = s.workspace.clone() else {
        return fail(StatusCode::FORBIDDEN, "only acu studio clones repositories");
    };
    let target = match clone_target(args.get("url").and_then(Value::as_str).unwrap_or("")) {
        Ok(t) => t,
        Err(e) => return fail(StatusCode::BAD_REQUEST, e),
    };
    s.start_job("clone", move |_, log| {
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
        std::fs::create_dir_all(dest.parent().unwrap_or(&ws))
            .map_err(|e| format!("acu: create {}: {e}", ws.display()))?;
        log(&format!(
            "acu: cloning {} into {}",
            target.url,
            dest.display()
        ));
        let child = Command::new("git")
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
                "--",
                &target.url,
            ])
            .arg(&dest)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("acu: start git: {e}; install git and try again"))?;
        let (ok, err) = wait_limited(child, CLONE_LIMIT);
        for line in err.lines().filter(|l| !l.trim().is_empty()) {
            log(line.trim());
        }
        if !ok {
            let _ = std::fs::remove_dir_all(&dest);
            return Err(format!(
                "acu: git could not clone {}; check the address and your network",
                target.url
            ));
        }
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
