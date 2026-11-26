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
        // The TypeScript adapter reads JavaScript too, so these serve both.
        "express" | "hono" | "sveltekit" => Some("typescript"),
        "axum" | "clap" => Some("rust"),
        "flask" | "fastapi" | "django" | "click" | "typer" | "python-main" => Some("python"),
        _ => None,
    }
}

/// Facts about the folder an adapter reads.
pub struct Source {
    /// The absolute folder that the adapter reads.
    pub root: PathBuf,
    /// The repository's root; paths in the output start there.
    pub repo_root: PathBuf,
    /// The commit that the folder holds, when git knows it.
    pub commit: Option<String>,
}

/// Runs git in a folder and returns its trimmed output, or `None` when git
/// fails.
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

/// Finds the folder that one language entry reads, its repository root and
/// its commit.
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
/// acu-adapter-<name> beside acu itself, and, inside this repository, the
/// adapter's source. Beside acu, acu-adapter-<name> can be a program or a
/// folder that holds a Node package; acu runs a package with `node` and the
/// entry point that the package names (see [`beside_acu`]).
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
    // `cargo build --workspace` puts the Rust adapter beside acu. The Docker
    // `export` stage puts the Go, Rust and TypeScript adapters there, the
    // TypeScript one as a folder.
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| exe_dir(&p)) {
        if let Some(c) = beside_acu(&dir, &lang.adapter)? {
            return Ok(c);
        }
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
    if lang.adapter == "typescript" && dev.join("package.json").exists() {
        let main = dev.join("dist").join("src").join("main.js");
        if !main.is_file() {
            bail!(
                "the TypeScript adapter in {} has no build yet; run `npm ci && npm run build -w adapters/typescript`, then run acu again",
                dev.display()
            );
        }
        let mut c = Command::new("node");
        c.arg(main);
        return Ok(c);
    }
    if lang.adapter == "rust" && dev.join("Cargo.toml").exists() {
        let mut c = Command::new("cargo");
        c.args([
            "run",
            "--quiet",
            "--release",
            "-p",
            "acu-adapter-rust",
            "--manifest-path",
        ])
        .arg(dev.join("Cargo.toml"))
        .arg("--");
        return Ok(c);
    }
    if lang.adapter == "python" && dev.join("acu_adapter_python").is_dir() {
        let mut c = Command::new("python3");
        c.args(["-m", "acu_adapter_python"]).env("PYTHONPATH", &dev);
        return Ok(c);
    }
    bail!(
        "no program runs the {name} adapter. Install {exe} on the PATH ({hint}), \
         set {var}, or give `command` in atlas.yaml",
        name = lang.adapter,
        hint = install_hint(&lang.adapter),
    )
}

/// Returns the folder that holds the program at `exe`. It follows symbolic
/// links, so an `acu` that a link on the PATH starts still finds the
/// adapters beside the real program: macOS reports the link's own path. When
/// the path does not resolve, it takes the path as given. On Windows it
/// takes the path as given too, because a resolved path there starts with
/// `\\?\`, which Node and Python may not read.
fn exe_dir(exe: &Path) -> Option<PathBuf> {
    let real = if cfg!(windows) {
        exe.to_path_buf()
    } else {
        std::fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf())
    };
    real.parent().map(Path::to_path_buf)
}

/// Returns the command that runs a Python program: the first of `python3`
/// and `python` on the PATH. On Windows the launcher `py -3` comes first,
/// because Windows may hold a `python3` that only opens the Microsoft Store.
/// With none on the PATH it returns the first, so the start fails with a
/// message that names it.
fn python_command() -> Command {
    let order: &[&[&str]] = if cfg!(windows) {
        &[&["py", "-3"], &["python"], &["python3"]]
    } else {
        &[&["python3"], &["python"]]
    };
    let pick = order
        .iter()
        .find(|c| on_path(c[0]))
        .copied()
        .unwrap_or(order[0]);
    let mut c = Command::new(pick[0]);
    c.args(&pick[1..]);
    c
}

/// Finds the program for an adapter in the folder that holds acu. It returns
/// `None` when nothing named acu-adapter-<name> sits there. A file runs as
/// a program. A file acu-adapter-<name>.pyz, a Python zip application, runs
/// with Python (see [`python_command`]); the release for Windows ships the
/// Python adapter in that form. A folder must hold a Node package: acu runs `node` with the
/// entry point from the package's `bin` field, or with `dist/src/main.js`
/// when `bin` names none. It fails when the folder holds no package.json or
/// no entry point.
fn beside_acu(acu_dir: &Path, adapter: &str) -> Result<Option<Command>> {
    let exe = format!("acu-adapter-{adapter}");
    let path = acu_dir.join(&exe);
    if path.is_file() || path.with_extension("exe").is_file() {
        return Ok(Some(Command::new(path)));
    }
    let pyz = acu_dir.join(format!("{exe}.pyz"));
    if pyz.is_file() {
        let mut c = python_command();
        c.arg(pyz);
        return Ok(Some(c));
    }
    if !path.is_dir() {
        return Ok(None);
    }
    let manifest = path.join("package.json");
    if !manifest.is_file() {
        bail!(
            "the folder {} holds no package.json; put the {adapter} adapter's package there or remove the folder, then run acu again",
            path.display()
        );
    }
    let text = std::fs::read_to_string(&manifest)
        .with_context(|| format!("read {}", manifest.display()))?;
    let package: Value = serde_json::from_str(&text)
        .with_context(|| format!("read {} as JSON", manifest.display()))?;
    let main = path.join(package_bin(&package, &exe).unwrap_or("dist/src/main.js"));
    if !main.is_file() {
        bail!(
            "the {adapter} adapter in {} has no entry point {}; copy the adapter's build (dist/src) into the folder, then run acu again",
            path.display(),
            main.display()
        );
    }
    let mut c = Command::new("node");
    c.arg(main);
    Ok(Some(c))
}

/// Reads the entry point for a program from a package's `bin` field. The
/// field holds one path, or a map from program name to path; a map without
/// the program's name gives its only entry, if it has one.
fn package_bin<'a>(package: &'a Value, exe: &str) -> Option<&'a str> {
    match package.get("bin")? {
        Value::String(s) => Some(s),
        Value::Object(map) => map
            .get(exe)
            .or_else(|| (map.len() == 1).then(|| map.values().next()).flatten())
            .and_then(Value::as_str),
        _ => None,
    }
}

/// How to install the program of a built-in adapter.
fn install_hint(adapter: &str) -> &'static str {
    match adapter {
        "go" => "go install github.com/revslaughter/atlas-carries-us/adapters/go/cmd/acu-adapter-go@latest",
        "typescript" => "npm install --global ./adapters/typescript from a checkout, after npm run build -w adapters/typescript",
        "rust" => "cargo install --path adapters/rust from a checkout",
        "python" => "acu-adapter-python, or acu-adapter-python.pyz on Windows, from a release archive beside acu; or a launcher that runs python3 -m acu_adapter_python with adapters/python on PYTHONPATH",
        _ => "see the adapter's own docs",
    }
}

/// Reports whether a program of this name sits in a folder on the `PATH`.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_plugins_name_their_adapter() {
        assert_eq!(plugin_adapter("go-chi"), Some("go"));
        assert_eq!(plugin_adapter("express"), Some("typescript"));
        assert_eq!(plugin_adapter("hono"), Some("typescript"));
        assert_eq!(plugin_adapter("sveltekit"), Some("typescript"));
        assert_eq!(plugin_adapter("axum"), Some("rust"));
        assert_eq!(plugin_adapter("clap"), Some("rust"));
        assert_eq!(plugin_adapter("flask"), Some("python"));
        assert_eq!(plugin_adapter("python-main"), Some("python"));
        assert_eq!(plugin_adapter("rails"), None);
    }

    /// Makes an empty folder for one test; it goes away when dropped.
    fn folder() -> tempfile::TempDir {
        tempfile::tempdir().expect("make a temporary folder")
    }

    /// Writes a file and the folders above it.
    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// Lists a command's program and arguments as strings.
    fn parts(c: &Command) -> Vec<String> {
        std::iter::once(c.get_program())
            .chain(c.get_args())
            .map(|s| s.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_program_beside_acu_runs_itself() {
        let dir = folder();
        let exe = dir.path().join("acu-adapter-go");
        write(&exe, "");
        let c = beside_acu(dir.path(), "go").unwrap().unwrap();
        assert_eq!(parts(&c), vec![exe.display().to_string()]);
    }

    #[test]
    fn a_package_beside_acu_runs_its_bin_with_node() {
        let dir = folder();
        let pkg = dir.path().join("acu-adapter-typescript");
        write(
            &pkg.join("package.json"),
            r#"{"bin": {"acu-adapter-typescript": "lib/run.js"}}"#,
        );
        write(&pkg.join("lib/run.js"), "");
        let c = beside_acu(dir.path(), "typescript").unwrap().unwrap();
        assert_eq!(
            parts(&c),
            vec![
                "node".to_string(),
                pkg.join("lib/run.js").display().to_string()
            ]
        );
    }

    #[test]
    fn a_package_without_bin_runs_dist_src_main() {
        let dir = folder();
        let pkg = dir.path().join("acu-adapter-typescript");
        write(&pkg.join("package.json"), r#"{"name": "x"}"#);
        write(&pkg.join("dist/src/main.js"), "");
        let c = beside_acu(dir.path(), "typescript").unwrap().unwrap();
        assert_eq!(
            parts(&c),
            vec![
                "node".to_string(),
                pkg.join("dist/src/main.js").display().to_string()
            ]
        );
    }

    #[test]
    fn a_package_without_an_entry_point_fails() {
        let dir = folder();
        let pkg = dir.path().join("acu-adapter-typescript");
        write(
            &pkg.join("package.json"),
            r#"{"bin": {"acu-adapter-typescript": "dist/src/main.js"}}"#,
        );
        let err = beside_acu(dir.path(), "typescript").unwrap_err();
        assert!(err.to_string().contains("has no entry point"), "{err}");
    }

    #[test]
    fn a_folder_without_a_package_fails() {
        let dir = folder();
        std::fs::create_dir(dir.path().join("acu-adapter-typescript")).unwrap();
        let err = beside_acu(dir.path(), "typescript").unwrap_err();
        assert!(err.to_string().contains("holds no package.json"), "{err}");
    }

    #[test]
    fn a_zip_application_beside_acu_runs_with_python() {
        let dir = folder();
        let pyz = dir.path().join("acu-adapter-python.pyz");
        write(&pyz, "");
        let c = beside_acu(dir.path(), "python").unwrap().unwrap();
        let p = parts(&c);
        assert!(
            ["py", "python", "python3"].contains(&p[0].as_str()),
            "{p:?}"
        );
        assert_eq!(p.last().unwrap(), &pyz.display().to_string());
    }

    #[test]
    fn a_program_beside_acu_wins_over_a_zip_application() {
        let dir = folder();
        let exe = dir.path().join("acu-adapter-python");
        write(&exe, "");
        write(&dir.path().join("acu-adapter-python.pyz"), "");
        let c = beside_acu(dir.path(), "python").unwrap().unwrap();
        assert_eq!(parts(&c), vec![exe.display().to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_acu_finds_the_folder_of_the_real_program() {
        let dir = folder();
        let real = dir.path().join("real").join("acu");
        write(&real, "");
        let link_dir = dir.path().join("bin");
        std::fs::create_dir(&link_dir).unwrap();
        std::os::unix::fs::symlink(&real, link_dir.join("acu")).unwrap();
        let want = std::fs::canonicalize(dir.path().join("real")).unwrap();
        assert_eq!(exe_dir(&link_dir.join("acu")), Some(want));
    }

    #[test]
    fn a_path_that_does_not_resolve_keeps_its_folder() {
        let dir = folder();
        let missing = dir.path().join("gone").join("acu");
        assert_eq!(exe_dir(&missing), Some(dir.path().join("gone")));
    }

    #[test]
    fn nothing_beside_acu_gives_none() {
        let dir = folder();
        assert!(beside_acu(dir.path(), "typescript").unwrap().is_none());
    }

    #[test]
    fn bin_reads_a_path_or_a_map() {
        let exe = "acu-adapter-typescript";
        assert_eq!(package_bin(&json!({"bin": "a.js"}), exe), Some("a.js"));
        let both = json!({"bin": {"acu-adapter-typescript": "b.js", "other": "c.js"}});
        assert_eq!(package_bin(&both, exe), Some("b.js"));
        let one = json!({"bin": {"other": "c.js"}});
        assert_eq!(package_bin(&one, exe), Some("c.js"));
        let two = json!({"bin": {"one": "c.js", "two": "d.js"}});
        assert_eq!(package_bin(&two, exe), None);
        assert_eq!(package_bin(&json!({}), exe), None);
    }
}
