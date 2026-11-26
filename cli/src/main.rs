//! acu: the Atlas Carries Us command.
//!
//! 1. `acu extract` runs the language adapters, merges their output, applies
//!    atlas.yaml, checks the result and writes atlas.json.
//! 2. `acu build` writes a static site: the viewer plus the data.
//! 3. `acu serve` runs the viewer locally, with an MCP server at `/mcp`.
//! 4. `acu mcp` runs the same MCP server over standard input and output.
//! 5. `acu check` reports config and data problems without writing anything.

mod adapter;
mod assemble;
mod config;
mod mcp;
mod site;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use serde_json::Value;

/// The command line of acu.
#[derive(Parser)]
#[command(
    name = "acu",
    version,
    about = "Turn a code base and its doc comments into an interactive map."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

/// The five things acu does.
#[derive(Subcommand)]
enum Cmd {
    /// Run the adapters and write atlas.json.
    Extract {
        #[command(flatten)]
        input: ConfigArg,
        /// Output file.
        #[arg(long, short, default_value = "atlas.json")]
        out: PathBuf,
        /// Indent the JSON.
        #[arg(long)]
        pretty: bool,
    },
    /// Write a static site (viewer and data) to a folder.
    Build {
        #[command(flatten)]
        input: Input,
        /// Output folder.
        #[arg(long, short, default_value = "site")]
        out: PathBuf,
    },
    /// Serve the viewer on this computer, with an MCP server at /mcp.
    Serve {
        #[command(flatten)]
        input: Input,
        /// Address to listen on.
        #[arg(long, default_value = "127.0.0.1:4321")]
        addr: SocketAddr,
        /// Turn off the MCP tools that write atlas.yaml.
        #[arg(long)]
        read_only: bool,
    },
    /// Run the MCP server over standard input and output, for desktop clients.
    Mcp {
        #[command(flatten)]
        input: Input,
        /// Turn off the MCP tools that write atlas.yaml.
        #[arg(long)]
        read_only: bool,
    },
    /// Check the config and the data, and report problems.
    Check {
        #[command(flatten)]
        input: Input,
    },
}

/// The config flag of `extract`.
#[derive(Args)]
struct ConfigArg {
    /// The project config.
    #[arg(long, short, default_value = "atlas.yaml")]
    config: PathBuf,
}

/// Where `build`, `serve`, `mcp` and `check` get the data: a config to
/// extract, or a finished atlas.json.
#[derive(Args)]
struct Input {
    /// The project config. acu runs the adapters to make the data.
    #[arg(long, short, conflicts_with = "data")]
    config: Option<PathBuf>,
    /// A finished atlas.json, used as it stands.
    #[arg(long, short)]
    data: Option<PathBuf>,
}

/// Parses the command line and runs it; a failure exits with status 1.
fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("acu: {e:#}");
        std::process::exit(1);
    }
}

/// Runs the command that the command line names.
fn run(cli: Cli) -> Result<()> {
    match cli.cmd {
        Cmd::Extract { input, out, pretty } => {
            let doc = extract(&input.config)?;
            let bytes = if pretty {
                serde_json::to_vec_pretty(&doc)?
            } else {
                serde_json::to_vec(&doc)?
            };
            if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("create {}", dir.display()))?;
            }
            std::fs::write(&out, bytes).with_context(|| format!("write {}", out.display()))?;
            summary(&doc, &out.display().to_string());
            Ok(())
        }
        Cmd::Build { input, out } => {
            let data = load_input(&input)?;
            let n = site::build(&out, &data)?;
            eprintln!("acu: wrote {n} files to {}", out.display());
            Ok(())
        }
        Cmd::Serve {
            input,
            addr,
            read_only,
        } => {
            let atlas = Arc::new(open_atlas(&input, read_only)?);
            tokio::runtime::Runtime::new()?.block_on(site::serve(addr, atlas))
        }
        Cmd::Mcp { input, read_only } => {
            let atlas = Arc::new(open_atlas(&input, read_only)?);
            tokio::runtime::Runtime::new()?.block_on(mcp::serve_stdio(atlas))
        }
        Cmd::Check { input } => {
            let doc = match (&input.config, &input.data) {
                (_, Some(d)) => read_json(d)?,
                (c, None) => extract_unchecked(c.as_deref().unwrap_or(Path::new("atlas.yaml")))?.0,
            };
            let rep = assemble::check(&doc);
            rep.print();
            if !rep.errors.is_empty() {
                bail!("{} problems found", rep.errors.len());
            }
            summary(&doc, "the check");
            eprintln!("acu: no problems found");
            Ok(())
        }
    }
}

/// Reads a JSON file.
fn read_json(p: &Path) -> Result<Value> {
    let b = std::fs::read(p).with_context(|| format!("read {}", p.display()))?;
    serde_json::from_slice(&b)
        .with_context(|| format!("{} holds JSON that does not parse", p.display()))
}

/// Returns the data that `build` shows: a checked atlas.json, or the result
/// of `extract` on the config.
fn load_input(input: &Input) -> Result<Vec<u8>> {
    match (&input.config, &input.data) {
        (_, Some(d)) => {
            let doc = read_json(d)?;
            let rep = assemble::check(&doc);
            rep.print();
            if !rep.errors.is_empty() {
                bail!("{} does not pass the check", d.display());
            }
            Ok(serde_json::to_vec(&doc)?)
        }
        (c, None) => Ok(serde_json::to_vec(&extract(
            c.as_deref().unwrap_or(Path::new("atlas.yaml")),
        )?)?),
    }
}

/// Opens the map that `serve` and `mcp` show: a finished atlas.json, read
/// again whenever the file changes, or the checked result of the adapters
/// and the config, which the MCP write tools can change.
fn open_atlas(input: &Input, read_only: bool) -> Result<mcp::Atlas> {
    if let Some(d) = &input.data {
        // Report problems but serve the file anyway, as `serve -d` always
        // has: a later `acu extract` may fix it, and the page reads it again.
        assemble::check(&read_json(d)?).print();
        let root = std::env::current_dir().context("find the current folder")?;
        return mcp::Atlas::from_file(d, &root);
    }
    let path = input
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from("atlas.yaml"));
    let loaded = config::load(&path)?;
    let run = run_adapters(&loaded)?;
    mcp::Atlas::from_config(&path, run, read_only)
}

/// What the adapters found: their merged document before acu applies the
/// config, the commit they read, and the repository roots that their paths
/// start from.
pub struct AdapterRun {
    /// The merged adapter documents.
    pub merged: Value,
    /// The commit that the first adapter read, when git knows it.
    pub commit: Option<String>,
    /// The repository root of each language, without repeats.
    pub repo_roots: Vec<PathBuf>,
}

/// Applies a config to a copy of the merged document and stamps the time.
pub fn apply_config(run: &AdapterRun, cfg: &config::Config) -> (Value, assemble::Report) {
    let mut doc = run.merged.clone();
    let ctx = assemble::Context {
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        generated: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        commit: run.commit.clone(),
    };
    let rep = assemble::apply(&mut doc, cfg, &ctx);
    (doc, rep)
}

/// Runs the adapters and applies the config, without the final check.
fn extract_unchecked(config_path: &Path) -> Result<(Value, assemble::Report)> {
    let loaded = config::load(config_path)?;
    let run = run_adapters(&loaded)?;
    Ok(apply_config(&run, &loaded.config))
}

/// Runs each language's adapter with its plugins and merges the documents.
fn run_adapters(loaded: &config::Loaded) -> Result<AdapterRun> {
    let cfg = &loaded.config;
    for e in &cfg.entry_points {
        if e.adapter.is_none() && adapter::plugin_adapter(&e.plugin).is_none() {
            bail!(
                "entry-point plugin {}: acu does not know which adapter runs it; add `adapter:`",
                e.plugin
            );
        }
    }
    let mut docs = Vec::new();
    let mut commit = None;
    let mut repo_roots: Vec<PathBuf> = Vec::new();
    for lang in &cfg.languages {
        let src = adapter::source(&loaded.dir, lang)?;
        if commit.is_none() {
            commit = src.commit.clone();
        }
        if !repo_roots.contains(&src.repo_root) {
            repo_roots.push(src.repo_root.clone());
        }
        let plugins: Vec<_> = cfg
            .entry_points
            .iter()
            .filter(|e| {
                e.adapter.as_deref().or(adapter::plugin_adapter(&e.plugin))
                    == Some(lang.adapter.as_str())
            })
            .collect();
        eprintln!(
            "acu: running the {} adapter on {}",
            lang.adapter,
            src.root.display()
        );
        docs.push(adapter::run(lang, &src, &plugins)?);
    }
    Ok(AdapterRun {
        merged: assemble::merge(docs)?,
        commit,
        repo_roots,
    })
}

/// Runs the adapters, applies the config and checks the result; any error
/// stops it.
fn extract(config_path: &Path) -> Result<Value> {
    let (doc, mut rep) = extract_unchecked(config_path)?;
    let check = assemble::check(&doc);
    rep.errors.extend(check.errors);
    rep.warnings.extend(check.warnings);
    rep.print();
    if !rep.errors.is_empty() {
        bail!(
            "{} problems found; fix them and run again",
            rep.errors.len()
        );
    }
    Ok(doc)
}

/// Writes one line that counts what a document holds.
fn summary(doc: &Value, target: &str) {
    let n = |k: &str| doc.get(k).and_then(Value::as_array).map_or(0, Vec::len);
    eprintln!(
        "acu: {target}: {} containers, {} symbols, {} edges, {} entry points, {} regions, {} tour steps",
        n("containers"),
        n("symbols"),
        n("edges"),
        n("entryPoints"),
        n("regions"),
        n("tour")
    );
}
