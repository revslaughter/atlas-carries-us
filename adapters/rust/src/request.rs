//! The request that acu sends on standard input (protocol 1, see
//! `docs/adapters.md`), and the options that only the Rust adapter reads.

use serde::Deserialize;

/// What the acu command asks the adapter to read.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Request {
    /// The protocol version; this adapter speaks 1.
    pub protocol: u32,
    /// The repository root; every path in the output starts there.
    pub repo_root: String,
    /// The folder to read: a Cargo workspace or package.
    pub root: String,
    /// Globs of package folders and source files to keep; an empty list keeps all.
    pub include: Vec<String>,
    /// Globs of package folders and source files to leave out.
    pub exclude: Vec<String>,
    /// The settings that only this adapter reads.
    pub options: Options,
    /// The entry-point plugins to switch on.
    pub entry_points: Vec<PluginConfig>,
}

/// The settings that only the Rust adapter reads.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    /// The rustup toolchain that runs rustdoc and rust-analyzer. The default
    /// names the pinned nightly of `toolchain.rs`.
    pub toolchain: Option<String>,
    /// The folder for build output. The default is `acu` inside the
    /// workspace's target folder.
    pub target_dir: Option<String>,
    /// The packages to map, by name. An empty list maps every workspace member.
    pub packages: Vec<String>,
    /// Cargo features to switch on.
    pub features: Vec<String>,
    /// Switches on every feature of every package.
    pub all_features: bool,
}

/// Switches on one entry-point plugin.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PluginConfig {
    /// The plugin's name: "axum" or "clap".
    pub plugin: String,
    /// The plugin's options, as atlas.yaml writes them.
    pub options: serde_json::Value,
}
