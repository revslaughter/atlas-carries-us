//! acu: the Atlas Carries Us command.
//!
//! 1. `acu extract` runs the language adapters, merges their output, applies
//!    atlas.yaml, checks the result and writes atlas.json.
//! 2. `acu build` writes a static site: the viewer plus the data.
//! 3. `acu serve` runs the viewer locally.
//! 4. `acu check` reports config and data problems without writing anything.

mod adapter;
mod assemble;
mod config;
mod site;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use serde_json::Value;

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
    /// Serve the viewer on this computer.
    Serve {
        #[command(flatten)]
        input: Input,
        /// Address to listen on.
        #[arg(long, default_value = "127.0.0.1:4321")]
        addr: SocketAddr,
    },
    /// Check the config and the data, and report problems.
    Check {
        #[command(flatten)]
        input: Input,
    },
}

#[derive(Args)]
struct ConfigArg {
    /// The project config.
    #[arg(long, short, default_value = "atlas.yaml")]
    config: PathBuf,
}

#[derive(Args)]
struct Input {
    /// The project config. acu runs the adapters to make the data.
    #[arg(long, short, conflicts_with = "data")]
    config: Option<PathBuf>,
    /// A finished atlas.json, used as it stands.
    #[arg(long, short)]
    data: Option<PathBuf>,
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("acu: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.cmd {
        Cmd::Extract { input, out, pretty } => {
            let doc = extract(&input.config)?;
            let bytes = if pretty {
                serde_json::to_vec_pretty(&doc)?
            } else {
                serde_json::to_vec(&doc)?
            };
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
        Cmd::Serve { input, addr } => {
            let src = match (&input.config, &input.data) {
                (_, Some(d)) => site::DataSource::File(d.clone()),
                _ => site::DataSource::Memory(load_input(&input)?),
            };
            tokio::runtime::Runtime::new()?.block_on(site::serve(addr, src))
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

fn read_json(p: &Path) -> Result<Value> {
    let b = std::fs::read(p).with_context(|| format!("read {}", p.display()))?;
    serde_json::from_slice(&b)
        .with_context(|| format!("{} holds JSON that does not parse", p.display()))
}

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

/// Runs the adapters and applies the config, without the final check.
fn extract_unchecked(config_path: &Path) -> Result<(Value, assemble::Report)> {
    let loaded = config::load(config_path)?;
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
    for lang in &cfg.languages {
        let src = adapter::source(&loaded.dir, lang)?;
        if commit.is_none() {
            commit = src.commit.clone();
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
    let mut doc = assemble::merge(docs)?;
    let ctx = assemble::Context {
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        generated: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        commit,
    };
    let rep = assemble::apply(&mut doc, cfg, &ctx);
    Ok((doc, rep))
}

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
