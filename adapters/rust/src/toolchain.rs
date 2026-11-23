//! The pinned nightly toolchain, and the runs of cargo, rustdoc and
//! rust-analyzer that the adapter makes with it.
//!
//! rustdoc writes JSON only on a nightly toolchain, and its format changes
//! between nightlies. The adapter pins one nightly, [`NIGHTLY`], whose format
//! the `rustdoc-types` crate in `Cargo.toml` reads. rust-analyzer comes from
//! the same toolchain. The adapter itself builds on the stable toolchain; it
//! picks the nightly for each child process through `RUSTUP_TOOLCHAIN`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

use crate::scip;

/// The nightly toolchain that writes the rustdoc JSON format that
/// `rustdoc-types` reads. Keep it in step with `RUST_NIGHTLY` in the
/// Dockerfile; `scripts/check-pins.sh` checks that the two agree.
pub const NIGHTLY: &str = "nightly-2026-11-01";

/// The command that installs the pinned toolchain with what the adapter needs.
pub fn install_hint(toolchain: &str) -> String {
    format!(
        "rustup toolchain install {toolchain} --profile minimal --component rust-src,rust-analyzer"
    )
}

/// What `cargo metadata` says about a workspace.
#[derive(Debug, Deserialize)]
pub struct Metadata {
    /// Every package in the build, the workspace's own and its dependencies.
    pub packages: Vec<Package>,
    /// The IDs of the workspace's own packages.
    pub workspace_members: Vec<String>,
    /// The folder that holds the workspace's `Cargo.toml`.
    pub workspace_root: PathBuf,
    /// The workspace's target folder.
    pub target_directory: PathBuf,
}

/// One package, as `cargo metadata` describes it.
#[derive(Debug, Clone, Deserialize)]
pub struct Package {
    /// The package's name, such as "grep-searcher".
    pub name: String,
    /// The package's version.
    pub version: String,
    /// Cargo's ID for the package.
    pub id: String,
    /// The package's `Cargo.toml`.
    pub manifest_path: PathBuf,
    /// The package's targets: its library, binaries, tests and so on.
    pub targets: Vec<Target>,
}

/// One target of a package.
#[derive(Debug, Clone, Deserialize)]
pub struct Target {
    /// The target's name, such as "rg" or "grep_searcher".
    pub name: String,
    /// The target's kinds, such as "lib", "bin" or "proc-macro".
    pub kind: Vec<String>,
    /// The target's root source file.
    pub src_path: PathBuf,
}

impl Target {
    /// Reports whether the target builds a library, a binary or a procedural
    /// macro: the targets that the map holds.
    pub fn mapped(&self) -> bool {
        self.kind.iter().any(|k| {
            matches!(
                k.as_str(),
                "lib" | "rlib" | "dylib" | "cdylib" | "staticlib" | "proc-macro" | "bin"
            )
        })
    }

    /// Reports whether the target builds a binary.
    pub fn is_bin(&self) -> bool {
        self.kind.iter().any(|k| k == "bin")
    }

    /// Reports whether the target builds a procedural macro.
    pub fn is_proc_macro(&self) -> bool {
        self.kind.iter().any(|k| k == "proc-macro")
    }

    /// The crate name that rustc gives the target: its name with `_` for `-`.
    pub fn crate_name(&self) -> String {
        self.name.replace('-', "_")
    }
}

/// Runs the tools of one toolchain with one set of build settings.
pub struct Tools {
    /// The rustup toolchain, such as [`NIGHTLY`].
    pub toolchain: String,
    /// The folder for build output.
    pub target_dir: PathBuf,
    /// The Cargo features to switch on.
    pub features: Vec<String>,
    /// Switches on every feature.
    pub all_features: bool,
}

impl Tools {
    /// Makes a command that runs on the toolchain.
    fn command(&self, program: &str, dir: &Path) -> Command {
        let mut c = Command::new(program);
        c.env("RUSTUP_TOOLCHAIN", &self.toolchain).current_dir(dir);
        c
    }

    /// Adds the feature flags to a cargo command.
    fn feature_args(&self, c: &mut Command) {
        if self.all_features {
            c.arg("--all-features");
        } else if !self.features.is_empty() {
            c.arg("--features").arg(self.features.join(","));
        }
    }

    /// Runs a command, and fails with the end of its standard error when it fails.
    fn run(&self, mut c: Command, what: &str) -> Result<Output> {
        let out = c.output().map_err(|e| {
            anyhow!(
                "could not start {what} ({e}); install the toolchain with `{}`, then run acu again",
                install_hint(&self.toolchain)
            )
        })?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let tail: Vec<&str> = err.lines().rev().take(20).collect();
            let tail: Vec<&str> = tail.into_iter().rev().collect();
            let hint = if err.contains("is not installed")
                || err.contains("not installed for the toolchain")
            {
                format!(
                    "install the toolchain with `{}`, then run acu again",
                    install_hint(&self.toolchain)
                )
            } else {
                "fix the error above, then run acu again".to_string()
            };
            bail!(
                "{what} failed ({}):\n{}\n{hint}",
                out.status,
                tail.join("\n")
            );
        }
        Ok(out)
    }

    /// Reads the workspace that holds `root` with `cargo metadata`.
    pub fn metadata(&self, root: &Path) -> Result<Metadata> {
        let mut c = self.command("cargo", root);
        c.args(["metadata", "--format-version", "1"]);
        self.feature_args(&mut c);
        let out = self.run(c, "cargo metadata")?;
        serde_json::from_slice(&out.stdout).context("read the output of cargo metadata")
    }

    /// Runs rustdoc on one target and reads its JSON output.
    pub fn rustdoc(&self, package: &Package, target: &Target) -> Result<rustdoc_types::Crate> {
        let dir = package.manifest_path.parent().unwrap_or(Path::new("."));
        let mut c = self.command("cargo", dir);
        c.arg("rustdoc")
            .arg("--manifest-path")
            .arg(&package.manifest_path);
        if target.is_bin() {
            c.args(["--bin", &target.name]);
        } else {
            c.arg("--lib");
        }
        self.feature_args(&mut c);
        c.arg("--target-dir").arg(&self.target_dir);
        c.args([
            "--quiet",
            "--",
            "-Z",
            "unstable-options",
            "--output-format",
            "json",
            "--document-private-items",
            "--cap-lints",
            "allow",
        ]);
        self.run(c, &format!("rustdoc on {} ({})", package.name, target.name))?;
        let file = self
            .target_dir
            .join("doc")
            .join(format!("{}.json", target.crate_name()));
        let bytes = std::fs::read(&file)
            .with_context(|| format!("read rustdoc's output {}", file.display()))?;
        self.parse(&bytes, &file)
    }

    /// Parses rustdoc JSON, and fails with the toolchain to use when its
    /// format differs from the one that `rustdoc-types` reads.
    fn parse(&self, bytes: &[u8], file: &Path) -> Result<rustdoc_types::Crate> {
        /// The one field that every format version keeps.
        #[derive(Deserialize)]
        struct Version {
            /// The format's version.
            format_version: u32,
        }
        let want = rustdoc_types::FORMAT_VERSION;
        match serde_json::from_slice::<rustdoc_types::Crate>(bytes) {
            Ok(k) if k.format_version == want => Ok(k),
            other => {
                let got = serde_json::from_slice::<Version>(bytes)
                    .map(|v| v.format_version)
                    .ok();
                if let Some(got) = got.filter(|g| *g != want) {
                    bail!(
                        "rustdoc on the toolchain {} writes JSON format {got}, and this adapter reads format {want}; use the toolchain {NIGHTLY} (option toolchain), or install it with `{}`",
                        self.toolchain,
                        install_hint(NIGHTLY)
                    );
                }
                other
                    .map_err(anyhow::Error::from)
                    .with_context(|| format!("read rustdoc's output {}", file.display()))
            }
        }
    }

    /// Indexes the workspace with rust-analyzer and reads the SCIP index.
    pub fn scip(&self, workspace_root: &Path) -> Result<scip::Index> {
        std::fs::create_dir_all(&self.target_dir)
            .with_context(|| format!("make the folder {}", self.target_dir.display()))?;
        let out_file = self.target_dir.join("acu.scip");
        let config_file = self.target_dir.join("acu-rust-analyzer.json");
        let features = if self.all_features {
            serde_json::json!("all")
        } else {
            serde_json::json!(self.features)
        };
        let config = serde_json::json!({
            "cargo": { "targetDir": self.target_dir.join("rust-analyzer"), "features": features }
        });
        std::fs::write(&config_file, config.to_string())
            .with_context(|| format!("write {}", config_file.display()))?;
        let mut c = self.command("rust-analyzer", workspace_root);
        c.arg("scip")
            .arg(workspace_root)
            .arg("--output")
            .arg(&out_file)
            .arg("--config-path")
            .arg(&config_file);
        self.run(c, "rust-analyzer scip")?;
        let bytes = std::fs::read(&out_file)
            .with_context(|| format!("read the SCIP index {}", out_file.display()))?;
        scip::decode(&bytes)
    }
}
