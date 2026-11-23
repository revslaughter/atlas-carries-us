//! The acu-adapter-rust command: reads a Cargo workspace and writes an Atlas
//! Carries Us document (schema version 1) as JSON.
//!
//! The acu command runs it with a request on standard input (protocol 1, see
//! `docs/adapters.md`). For a run by hand, pass flags instead:
//!
//! ```text
//! acu-adapter-rust --root path/to/workspace --plugin axum --plugin clap --out atlas.json
//! ```

use std::io::{Read, Write};
use std::process::ExitCode;

use acu_adapter_rust::extract::{self, ADAPTER_VERSION, NOTES};
use acu_adapter_rust::request::{PluginConfig, Request};
use acu_adapter_rust::toolchain::NIGHTLY;

/// The help text, with the limits that every run's notes repeat.
fn help() -> String {
    let mut s = format!(
        "acu-adapter-rust {ADAPTER_VERSION}: reads a Cargo workspace with rustdoc's JSON output and
rust-analyzer's SCIP index, and writes an Atlas Carries Us document.

Without --root, the request comes as JSON on standard input (protocol 1).

Flags:
  --root <dir>        the workspace or package to read
  --repo-root <dir>   the folder that paths start at (default: --root)
  --include <glob>    package folders and files to keep (repeatable)
  --exclude <glob>    package folders and files to leave out (repeatable)
  --package <name>    a package to map (repeatable; default: every member)
  --toolchain <name>  the rustup toolchain to run (default: {NIGHTLY})
  --plugin <name>     an entry-point plugin: axum or clap (repeatable)
  --out <file>        the output file (default: standard output)
  --indent            indent the JSON
  --version           print the version

Limits:
"
    );
    for n in NOTES {
        s.push_str("  - ");
        s.push_str(n);
        s.push('\n');
    }
    s
}

/// Reads the flags of a run by hand into a request, or returns `None` when
/// the request should come from standard input.
fn flags(args: &[String]) -> Result<(Option<Request>, Option<String>, bool), String> {
    let mut req = Request {
        protocol: 1,
        ..Request::default()
    };
    let mut out = None;
    let mut indent = false;
    let mut by_hand = false;
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        let mut value = || {
            i += 1;
            args.get(i)
                .cloned()
                .ok_or(format!("the flag {flag} needs a value"))
        };
        match flag {
            "--root" => {
                req.root = value()?;
                by_hand = true;
            }
            "--repo-root" => req.repo_root = value()?,
            "--include" => req.include.push(value()?),
            "--exclude" => req.exclude.push(value()?),
            "--package" => req.options.packages.push(value()?),
            "--toolchain" => req.options.toolchain = Some(value()?),
            "--plugin" => req.entry_points.push(PluginConfig {
                plugin: value()?,
                options: serde_json::Value::Null,
            }),
            "--out" => out = Some(value()?),
            "--indent" => indent = true,
            other => {
                return Err(format!(
                    "the flag {other} is unknown; run with --help for the list"
                ))
            }
        }
        i += 1;
    }
    Ok((by_hand.then_some(req), out, indent))
}

/// Reads the request, runs the extraction, and writes the document. A
/// summary line goes to standard error, so standard output carries data only.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{}", help());
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--version") {
        println!("acu-adapter-rust {ADAPTER_VERSION}");
        return ExitCode::SUCCESS;
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("acu-adapter-rust: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Does the work of `main`, so that every failure takes one path.
fn run(args: &[String]) -> anyhow::Result<()> {
    let (by_hand, out, indent) = flags(args).map_err(anyhow::Error::msg)?;
    let req = match by_hand {
        Some(r) => r,
        None => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            let r: Request = serde_json::from_str(&buf)
                .map_err(|e| anyhow::anyhow!("read the request from standard input: {e}"))?;
            if r.protocol != 1 {
                anyhow::bail!(
                    "the request speaks protocol {}; this adapter speaks protocol 1",
                    r.protocol
                );
            }
            r
        }
    };
    let mut doc = extract::run(req)?;
    doc.meta.generated = now();
    let json = if indent {
        serde_json::to_string_pretty(&doc)?
    } else {
        serde_json::to_string(&doc)?
    };
    match out {
        Some(f) => std::fs::write(&f, json + "\n")?,
        None => {
            let mut w = std::io::stdout().lock();
            w.write_all(json.as_bytes())?;
            w.write_all(b"\n")?;
        }
    }
    eprintln!(
        "acu-adapter-rust: {} containers, {} symbols, {} edges, {} entry points",
        doc.containers.len(),
        doc.symbols.len(),
        doc.edges.len(),
        doc.entry_points.len()
    );
    Ok(())
}

/// The time now, RFC 3339, in UTC, to the second.
fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}
