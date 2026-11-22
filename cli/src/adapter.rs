//! Runs language adapters. Each adapter gets one JSON request on standard
//! input (protocol 1) and writes one schema-v1 document to standard output.
//! docs/adapters.md describes the protocol.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::config::{EntryPointPlugin, Language};

/// The adapter that runs each built-in entry-point plugin.
pub fn plugin_adapter(plugin: &str) -> Option<&'static str> {
    match plugin {
        "go-chi" | "go-nethttp" | "cobra" => Some("go"),
        _ => None,
    }
}

/// Facts about the folder an adapter reads.
pub struct Source {
    pub root: PathBuf,
    pub repo_root: PathBuf,
    pub commit: Option<String>,
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn source(config_dir: &Path, lang: &Language) -> Result<Source> {
    let root = config_dir.join(lang.root.as_deref().unwrap_or("."));
    let root = root
        .canonicalize()
        .with_context(|| format!("find the {} root {}", lang.adapter, root.display()))?;
    let repo_root = git(&root, &["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .unwrap_or_else(|| root.clone());
    let commit = git(&root, &["rev-parse", "HEAD"]);
    Ok(Source {
        root,
        repo_root,
        commit,
    })
}

/// Picks the program that runs an adapter, in this order: the config's
/// `command`, the ACU_ADAPTER_<NAME> variable, acu-adapter-<name> on PATH,
/// and, inside this repository, the adapter's source.
fn command_for(lang: &Language) -> Result<Command> {
    if let Some(cmd) = &lang.command {
        if cmd.is_empty() {
            bail!(
                "the {} entry in languages has an empty command",
                lang.adapter
            );
        }
        let mut c = Command::new(&cmd[0]);
        c.args(&cmd[1..]);
        return Ok(c);
    }
    let var = format!(
        "ACU_ADAPTER_{}",
        lang.adapter.to_uppercase().replace('-', "_")
    );
    if let Ok(v) = std::env::var(&var) {
        let parts: Vec<&str> = v.split_whitespace().collect();
        if !parts.is_empty() {
            let mut c = Command::new(parts[0]);
            c.args(&parts[1..]);
            return Ok(c);
        }
    }
    let exe = format!("acu-adapter-{}", lang.adapter);
    if on_path(&exe) {
        return Ok(Command::new(exe));
    }
    let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("adapters")
        .join(&lang.adapter);
    if lang.adapter == "go" && dev.join("go.mod").exists() {
        let mut c = Command::new("go");
        c.args(["run", "./cmd/acu-adapter-go"]).current_dir(dev);
        return Ok(c);
    }
    bail!(
        "no program runs the {name} adapter. Install it (for Go: go install \
         github.com/revslaughter/atlas-carries-us/adapters/go/cmd/acu-adapter-go@latest), \
         set {var}, or give `command` in atlas.yaml",
        name = lang.adapter
    )
}

fn on_path(exe: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|p| {
        let f = p.join(exe);
        f.is_file() || f.with_extension("exe").is_file()
    })
}

/// Runs one adapter and returns its document.
pub fn run(lang: &Language, src: &Source, plugins: &[&EntryPointPlugin]) -> Result<Value> {
    let entry_points: Vec<Value> = plugins
        .iter()
        .map(|p| json!({ "plugin": p.plugin, "options": p.options.clone().unwrap_or(json!({})) }))
        .collect();
    let request = json!({
        "protocol": 1,
        "repoRoot": src.repo_root,
        "root": src.root,
        "include": lang.include,
        "exclude": lang.exclude,
        "tests": lang.tests,
        "options": lang.options.clone().unwrap_or(json!({})),
        "entryPoints": entry_points,
    });
    let mut cmd = command_for(lang)?;
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("start the {} adapter", lang.adapter))?;
    child
        .stdin
        .take()
        .context("open the adapter's standard input")?
        .write_all(request.to_string().as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("the {} adapter failed ({})", lang.adapter, out.status);
    }
    serde_json::from_slice(&out.stdout)
        .with_context(|| format!("read the {} adapter's output as JSON", lang.adapter))
}
